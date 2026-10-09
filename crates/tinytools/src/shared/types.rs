//! The owned handle onto a shared tool.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::call::{ToolCallOptions, ToolInjectedArgument, ToolTimeout};
use crate::classification::{ToolCategory, ToolScope};
use crate::context::ToolRunContext;
use crate::permission::PermissionLevel;
use crate::policy::ToolPolicy;
use crate::result::ToolResult;
use crate::spec::ToolSpec;
use crate::tool::{Tool, ToolExposure};

/// One shared tool, owned as a `Box<dyn Tool>` for as long as a caller needs
/// it.
///
/// # Every method, deliberately
///
/// [`Tool`] has four required methods and twenty-two defaulted ones. A wrapper
/// implementing only the four would compile and silently answer the defaults
/// for the tool it wraps — a [`PermissionLevel::Write`] tool would read as
/// read-only, an admission gate would see the wrong
/// [`external_effect`][Tool::external_effect], and a hidden tool would
/// advertise itself. Nothing in the type system catches that, so every method
/// is forwarded explicitly and the module's tests pin each one.
pub struct SharedTool(Arc<dyn Tool>);

// `dyn Tool` is not `Debug`, and the name is the only part of a tool worth
// printing anyway — a belt in a log line should read as its names.
impl std::fmt::Debug for SharedTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedTool").field(&self.0.name()).finish()
    }
}

impl SharedTool {
    /// Wraps one shared tool.
    #[must_use]
    pub fn new(inner: Arc<dyn Tool>) -> Self {
        Self(inner)
    }
}

#[async_trait]
impl Tool for SharedTool {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn description(&self) -> &str {
        self.0.description()
    }

    fn parameters_schema(&self) -> Value {
        self.0.parameters_schema()
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        self.0.execute(args).await
    }

    async fn execute_with_options(
        &self,
        args: Value,
        options: ToolCallOptions,
    ) -> anyhow::Result<ToolResult> {
        self.0.execute_with_options(args, options).await
    }

    async fn execute_with_context(
        &self,
        args: Value,
        options: ToolCallOptions,
        context: Option<&dyn ToolRunContext>,
    ) -> anyhow::Result<ToolResult> {
        self.0.execute_with_context(args, options, context).await
    }

    fn policy(&self) -> ToolPolicy {
        self.0.policy()
    }

    fn injected_arguments(&self) -> Vec<ToolInjectedArgument> {
        self.0.injected_arguments()
    }

    fn supports_markdown(&self) -> bool {
        self.0.supports_markdown()
    }

    fn permission_level(&self) -> PermissionLevel {
        self.0.permission_level()
    }

    fn permission_level_with_args(&self, args: &Value) -> PermissionLevel {
        self.0.permission_level_with_args(args)
    }

    fn scope(&self) -> ToolScope {
        self.0.scope()
    }

    fn category(&self) -> ToolCategory {
        self.0.category()
    }

    fn exposure(&self) -> ToolExposure {
        self.0.exposure()
    }

    fn family(&self) -> Option<&str> {
        self.0.family()
    }

    fn tags(&self) -> Vec<String> {
        self.0.tags()
    }

    fn indirect_target(&self, args: &Value) -> Option<crate::rules::IndirectCall> {
        self.0.indirect_target(args)
    }

    fn is_concurrency_safe(&self, args: &Value) -> bool {
        self.0.is_concurrency_safe(args)
    }

    fn external_effect(&self) -> bool {
        self.0.external_effect()
    }

    fn external_effect_with_args(&self, args: &Value) -> bool {
        self.0.external_effect_with_args(args)
    }

    fn max_result_size_chars(&self) -> Option<usize> {
        self.0.max_result_size_chars()
    }

    fn timeout_policy(&self, args: &Value) -> ToolTimeout {
        self.0.timeout_policy(args)
    }

    fn host_extension(&self) -> Option<&(dyn Any + Send + Sync)> {
        self.0.host_extension()
    }

    fn host_call_extension(&self, args: &Value) -> Option<Box<dyn Any + Send + Sync>> {
        self.0.host_call_extension(args)
    }

    fn spec(&self) -> ToolSpec {
        self.0.spec()
    }

    fn display_label(&self, args: &Value) -> Option<String> {
        self.0.display_label(args)
    }

    fn display_detail(&self, args: &Value) -> Option<String> {
        self.0.display_detail(args)
    }

    fn return_direct(&self) -> bool {
        self.0.return_direct()
    }
}
