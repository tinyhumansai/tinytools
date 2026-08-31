//! The vocabulary an agent tool is written against: the [`Tool`] trait, the
//! [`ToolResult`] it returns, and the classifications a host enforces around a
//! call.
//!
//! # Why this is its own crate
//!
//! Two crates need these types and neither can own them. An agent harness has
//! to name a tool's result to run a loop over it; a host application has to
//! name the same result to implement one. Before this crate existed, both
//! declared their own, and the conversions between them were written by hand at
//! every seam — which is how an error flag ends up inverted in one direction
//! and nothing catches it.
//!
//! So the vocabulary sits underneath both. A harness depends on this crate and
//! re-exports it, so `harness::ToolResult` and [`ToolResult`] are the
//! *same type*, not structural twins. A tool author depends on this crate alone
//! and compiles neither the harness nor the host.
//!
//! # What is here
//!
//! - [`tool`] — the [`Tool`] trait: four required methods, and a set of
//!   defaulted declarations describing what the tool needs and what it touches.
//! - [`result`] — [`ToolResult`] and [`ToolContent`], the MCP-shaped block list
//!   a tool hands back.
//! - [`spec`] — [`ToolSpec`], the declaration a model is shown.
//! - [`permission`] — [`PermissionLevel`], the privilege ladder.
//! - [`classification`] — [`ToolScope`], [`ToolCategory`] and [`ToolExposure`].
//! - [`call`] — [`ToolCallOptions`] and [`ToolTimeout`], the per-invocation
//!   inputs that are not arguments.
//! - [`context`] — [`ToolRunContext`], the narrow seam onto a live run.
//! - [`workspace`] — [`WorkspaceDescriptor`], the root a tool may touch.
//! - [`naming`] — rendering a call for a human.
//!
//! # What is deliberately not here
//!
//! **No enforcement.** Nothing in this crate checks a [`PermissionLevel`],
//! applies a [`ToolTimeout`], or decides whether an
//! [`external_effect`][Tool::external_effect] needs approval. A tool
//! *describes* itself and a host *decides*, because the decision depends on
//! that host's threat model, its configuration, and who is asking — none of
//! which generalize. Putting the check here would mean every host inherits one
//! host's policy.
//!
//! **No registry, no dispatch, no execution loop.** Those belong to whoever
//! owns the run.
//!
//! **No dependency on an agent harness.** The harness depends on this crate.
//! [`ToolRunContext`] exists precisely so a tool can read run-scoped facts
//! without this crate naming the harness type that carries them — see that
//! module for why the edge has to point one way.
//!
//! # Example
//!
//! ```
//! use tinytools::{PermissionLevel, Tool, ToolResult};
//!
//! struct Echo;
//!
//! #[async_trait::async_trait]
//! impl Tool for Echo {
//!     fn name(&self) -> &str {
//!         "echo"
//!     }
//!
//!     fn description(&self) -> &str {
//!         "Returns its input unchanged."
//!     }
//!
//!     fn parameters_schema(&self) -> serde_json::Value {
//!         serde_json::json!({
//!             "type": "object",
//!             "properties": { "text": { "type": "string" } },
//!             "required": ["text"],
//!         })
//!     }
//!
//!     async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
//!         let text = args.get("text").and_then(|v| v.as_str()).unwrap_or_default();
//!         Ok(ToolResult::success(text))
//!     }
//! }
//!
//! # tokio_test_shim(async {
//! let echo = Echo;
//! let out = echo.execute(serde_json::json!({ "text": "hi" })).await?;
//! assert_eq!(out.output(), "hi");
//!
//! // Declarations a host reads before it ever calls `execute`.
//! assert_eq!(echo.permission_level(), PermissionLevel::ReadOnly);
//! assert!(!echo.external_effect());
//! assert_eq!(echo.display_label(&serde_json::Value::Null).as_deref(), Some("Echo"));
//! # Ok::<(), anyhow::Error>(())
//! # });
//! # fn tokio_test_shim<F: std::future::Future<Output = Result<(), anyhow::Error>>>(f: F) {
//! #     tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f).unwrap();
//! # }
//! ```

pub mod call;
pub mod classification;
pub mod context;
pub mod naming;
pub mod permission;
pub mod result;
pub mod spec;
pub mod tool;
pub mod workspace;

pub use call::{ToolCallOptions, ToolTimeout};
pub use classification::{ToolCategory, ToolExposure, ToolScope};
pub use context::ToolRunContext;
pub use naming::{
    ContextDetailOptions, context_detail_from_args, context_detail_from_args_with,
    humanize_tool_name,
};
pub use permission::PermissionLevel;
pub use result::{ToolContent, ToolResult};
pub use spec::ToolSpec;
pub use tool::Tool;
pub use workspace::{SandboxMode, WorkspaceDescriptor};
