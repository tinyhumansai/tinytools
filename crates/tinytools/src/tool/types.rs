//! The tool trait itself.

use std::any::Any;

use async_trait::async_trait;
use serde_json::Value;

use crate::call::{ToolCallOptions, ToolTimeout};
use crate::classification::{ToolCategory, ToolExposure, ToolScope};
use crate::context::ToolRunContext;
use crate::naming::{context_detail_from_args, humanize_tool_name};
use crate::permission::PermissionLevel;
use crate::result::ToolResult;
use crate::spec::ToolSpec;

/// A capability an agent can invoke.
///
/// Everything beyond [`Self::name`], [`Self::description`],
/// [`Self::parameters_schema`] and [`Self::execute`] has a default, so the
/// smallest useful tool is four short methods. The rest of the trait is
/// **declaration**: a tool states what privilege it needs, whether it reaches
/// outside the machine, how long it may run, and how it should read in a
/// timeline. A host reads those declarations and decides what to allow.
///
/// That split is the point. A tool never enforces policy on itself — it
/// describes itself accurately and the host enforces.
///
/// # The defaults are not uniformly safe — four of them are permissive
///
/// Two defaults are genuinely cautious: [`Self::is_concurrency_safe`] is
/// `false`, so nothing is dispatched in parallel unless a tool says it is safe,
/// and [`Self::timeout_policy`] inherits the host's bound rather than opting
/// out of it. **Four are permissive**, and a tool author who assumes otherwise
/// ships a hole:
///
/// - **[`Self::external_effect`] defaults to `false`.** A tool that sends an
///   email, posts a message or fires a webhook and does *not* override it is
///   declaring that it has no outside effect, and a host honouring that
///   declaration will route it **past** its approval gate. This default exists
///   because most tools genuinely are local and the alternative would prompt on
///   every file read — but it means **an effectful tool MUST override it**.
///   There is no way for this crate to detect the omission: a missing override
///   and an honest `false` are the same bytes.
/// - **[`Self::max_result_size_chars`] defaults to `None`**, meaning no cap. A
///   chatty tool takes the host's global handling, if it has any.
/// - **[`Self::permission_level`] defaults to
///   [`PermissionLevel::ReadOnly`]**, not [`PermissionLevel::None`], because
///   most tools genuinely read — but a writing tool must say so.
/// - **[`Self::scope`] defaults to [`ToolScope::All`]**, the *widest* setting:
///   the tool is offered to the autonomous agent loop, the CLI and RPC alike. A
///   tool that should only ever be driven deliberately by a human has to say
///   [`ToolScope::CliRpcOnly`]; leaving the default hands it to the loop.
///
/// If you are reviewing a `Tool` impl, those four are what to check for
/// absence. The rest are safe to leave alone.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Canonical tool name, used in model function calling.
    fn name(&self) -> &str;

    /// Human- and model-readable description of what the tool does.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's arguments.
    fn parameters_schema(&self) -> Value;

    /// Runs the tool.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the tool could not run at all. A tool that ran and
    /// decided no returns `Ok` with
    /// [`ToolResult::error`][crate::ToolResult::error] instead, so the model
    /// sees the reason and can adapt.
    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult>;

    /// Runs the tool with caller-supplied options.
    ///
    /// The default forwards to [`Self::execute`], so a tool that does not care
    /// about options needs no change. Override to honour
    /// [`ToolCallOptions::prefer_markdown`].
    ///
    /// # Errors
    ///
    /// As [`Self::execute`].
    async fn execute_with_options(
        &self,
        args: Value,
        _options: ToolCallOptions,
    ) -> anyhow::Result<ToolResult> {
        self.execute(args).await
    }

    /// Runs the tool with the caller's run context.
    ///
    /// The default forwards to [`Self::execute_with_options`], so a tool stays
    /// context-agnostic unless it needs to know where it is running — the
    /// isolated-workspace case being the common one.
    ///
    /// # Errors
    ///
    /// As [`Self::execute`].
    async fn execute_with_context(
        &self,
        args: Value,
        options: ToolCallOptions,
        context: Option<&dyn ToolRunContext>,
    ) -> anyhow::Result<ToolResult> {
        let _ = context;
        self.execute_with_options(args, options).await
    }

    /// Whether this tool can produce a markdown rendering when
    /// [`ToolCallOptions::prefer_markdown`] is set.
    ///
    /// A tool that overrides [`Self::execute_with_options`] to honour the flag
    /// should override this too: it is what lets a host attribute the token
    /// saving to the right tool.
    fn supports_markdown(&self) -> bool {
        false
    }

    /// Privilege this tool requires.
    ///
    /// For a tool exposing several actions at different privileges, return the
    /// **minimum** any action needs, so the tool is not statically blocked on a
    /// caller that could legitimately run its read-only half. The per-call
    /// level is [`Self::permission_level_with_args`].
    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    /// Argument-aware [`Self::permission_level`].
    ///
    /// A host calls *this* at the enforcement point, so a tool with mixed
    /// actions (`list` versus `create`) should override here. The default
    /// defers to the argument-less answer.
    fn permission_level_with_args(&self, _args: &Value) -> PermissionLevel {
        self.permission_level()
    }

    /// Where this tool may be executed.
    fn scope(&self) -> ToolScope {
        ToolScope::All
    }

    /// Which belt this tool belongs to.
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    /// Where this tool is exposed to the model.
    ///
    /// Defaults to [`ToolExposure::Direct`] so a tool that has not considered
    /// the question behaves exactly as it did before this method existed.
    /// Override it on a tool whose schema is large relative to how often the
    /// model reaches for it — that is the trade this is here to make, and the
    /// host's own budget report is the place to find the candidates.
    fn exposure(&self) -> ToolExposure {
        ToolExposure::Direct
    }

    /// Whether two concurrent invocations are safe to run in parallel within a
    /// single model turn.
    ///
    /// Read-only tools touching no shared mutable state should return `true`; a
    /// host can then dispatch a batch of reads together instead of serially.
    /// Tools that mutate the workspace, write to disk, or talk to a service
    /// that throttles by caller keep the default `false`.
    ///
    /// The arguments are supplied so a tool can refine the answer per call — a
    /// generic shell could allow parallel `ls` and refuse parallel installs —
    /// but most tools ignore them.
    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        false
    }

    /// Whether this tool produces an externally observable side effect: an
    /// outbound message, an email, a calendar write, a webhook.
    ///
    /// A host routes such calls through its approval gate before
    /// [`Self::execute`] runs. Local file writes and memory writes stay `false`
    /// — they are reversible inside the user's own machine.
    ///
    /// **This default fails open.** `false` means "no approval needed", so a
    /// tool that reaches outside the machine and forgets to override this is
    /// silently exempted from the gate. Overriding it is the tool author's
    /// responsibility; nothing here can infer it.
    fn external_effect(&self) -> bool {
        false
    }

    /// Argument-aware [`Self::external_effect`].
    ///
    /// A host calls *this* at the gate decision point, so a tool whose
    /// classification depends on its arguments should override here rather than
    /// the argument-less variant.
    fn external_effect_with_args(&self, _args: &Value) -> bool {
        self.external_effect()
    }

    /// Per-tool cap on the character length of the result body sent back to the
    /// model.
    ///
    /// Set this on tools whose output is *bounded but unpredictable* — a shell,
    /// a fetch. Leave it unset where callers genuinely want the whole thing, as
    /// with a file read: truncating those hides data the caller asked for. When
    /// `None`, the host's global handling applies.
    fn max_result_size_chars(&self) -> Option<usize> {
        None
    }

    /// How the host should bound this invocation in wall-clock time.
    fn timeout_policy(&self, _args: &Value) -> ToolTimeout {
        ToolTimeout::Inherit
    }

    /// Host-defined metadata this tool carries, for a host that needs to
    /// recognise its own tool kinds through a `dyn Tool`.
    ///
    /// Erased rather than typed because the answer is *host* policy — a pack
    /// registry handle, a generated-tool provenance record — and this crate has
    /// no business naming either. A host downcasts to its own type; every other
    /// tool returns `None` and pays nothing.
    fn host_extension(&self) -> Option<&(dyn Any + Send + Sync)> {
        None
    }

    /// Host-defined per-call metadata, for policy that depends on the
    /// arguments.
    ///
    /// Erased for the same reason as [`Self::host_extension`], but returned
    /// owned because it is derived from the call rather than held by the tool.
    fn host_call_extension(&self, _args: &Value) -> Option<Box<dyn Any + Send + Sync>> {
        None
    }

    /// The full declaration to register with a model.
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }

    /// Short verb phrase describing this call for an activity timeline —
    /// "Reading file", "Running command".
    ///
    /// The default title-cases [`Self::name`]. Dynamic and integration tools
    /// override with a curated phrase so a row never reads as raw
    /// `snake_case`.
    fn display_label(&self, _args: &Value) -> Option<String> {
        Some(humanize_tool_name(self.name()))
    }

    /// The specific argument for this call — the path, address, command or
    /// query — shown after [`Self::display_label`], so a row reads
    /// `Read(src/main.rs)`.
    ///
    /// The default pulls the most relevant common argument, which is right for
    /// nearly every tool. Override when the meaningful argument sits under an
    /// unusual key.
    fn display_detail(&self, args: &Value) -> Option<String> {
        context_detail_from_args(args)
    }
}
