//! Unit tests for the `Tool` trait: its defaults, how a host recovers tool
//! metadata through the erased trait object, and how display helpers pull
//! context out of a call's arguments.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::unnecessary_literal_bound)]

use std::any::Any;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::Tool;
use crate::{
    PermissionLevel, ToolCallOptions, ToolCategory, ToolDisplay, ToolInjectedArgument, ToolPolicy,
    ToolResult, ToolRunContext, ToolRuntime, ToolScope, ToolTimeout,
};

/// A tool implementing only the four required methods, so every default is
/// exercised as written.
struct DummyTool;

#[async_trait]
impl Tool for DummyTool {
    fn name(&self) -> &str {
        "dummy_tool"
    }

    fn description(&self) -> &str {
        "A deterministic test tool"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "value": { "type": "string" } }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let text = args
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Ok(ToolResult::success(text))
    }
}

#[test]
fn spec_is_built_from_the_tool_metadata_and_schema() {
    let spec = DummyTool.spec();
    assert_eq!(spec.name, "dummy_tool");
    assert_eq!(spec.description, "A deterministic test tool");
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["properties"]["value"]["type"], "string");
}

#[tokio::test]
async fn execute_returns_the_expected_output() {
    let result = DummyTool
        .execute(json!({ "value": "hello-tool" }))
        .await
        .expect("the tool runs");
    assert!(!result.is_error);
    assert_eq!(result.output(), "hello-tool");
}

#[tokio::test]
async fn the_options_and_context_overloads_default_through_to_execute() {
    let with_options = DummyTool
        .execute_with_options(json!({ "value": "a" }), ToolCallOptions::prefer_markdown())
        .await
        .expect("the tool runs");
    assert_eq!(with_options.output(), "a");

    let with_context = DummyTool
        .execute_with_context(json!({ "value": "b" }), ToolCallOptions::default(), None)
        .await
        .expect("the tool runs");
    assert_eq!(with_context.output(), "b");
}

#[test]
fn the_declaration_defaults_are_the_conservative_answer() {
    let tool = DummyTool;
    assert_eq!(tool.permission_level(), PermissionLevel::ReadOnly);
    assert_eq!(
        tool.permission_level_with_args(&Value::Null),
        PermissionLevel::ReadOnly
    );
    assert_eq!(tool.scope(), ToolScope::All);
    assert_eq!(tool.category(), ToolCategory::System);
    assert!(!tool.supports_markdown());
    assert!(!tool.is_concurrency_safe(&Value::Null));
    assert!(!tool.external_effect());
    assert!(!tool.external_effect_with_args(&Value::Null));
    assert!(tool.max_result_size_chars().is_none());
    assert_eq!(tool.timeout_policy(&Value::Null), ToolTimeout::Inherit);
    assert!(tool.host_extension().is_none());
    assert!(tool.host_call_extension(&Value::Null).is_none());
    assert_eq!(tool.policy(), ToolPolicy::default());
    assert!(tool.injected_arguments().is_empty());
}

#[tokio::test]
async fn tools_declare_injected_argument_sources_without_exposing_values() {
    struct Injected;

    #[async_trait]
    impl Tool for Injected {
        fn name(&self) -> &str {
            "injected"
        }

        fn description(&self) -> &str {
            "Uses host-owned arguments"
        }

        fn parameters_schema(&self) -> Value {
            json!({ "type": "object" })
        }

        async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::success("ok"))
        }

        fn injected_arguments(&self) -> Vec<ToolInjectedArgument> {
            vec![
                ToolInjectedArgument::host("account_id"),
                ToolInjectedArgument::tool_call_id("call_id"),
            ]
        }
    }

    let tool = Injected;
    assert_eq!(tool.name(), "injected");
    assert_eq!(tool.description(), "Uses host-owned arguments");
    assert_eq!(tool.parameters_schema(), json!({ "type": "object" }));
    let result = tool.execute(Value::Null).await.expect("the tool runs");
    assert_eq!(result.output(), "ok");

    let declarations = tool.injected_arguments();
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].name, "account_id");
    assert_eq!(declarations[1].name, "call_id");
}

/// A tool whose complete declaration lives in the canonical policy vocabulary.
struct DeclaredTool;

#[async_trait]
impl Tool for DeclaredTool {
    fn name(&self) -> &str {
        "declared_tool"
    }

    fn description(&self) -> &str {
        "A tool with a policy declaration"
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success("done"))
    }

    fn policy(&self) -> ToolPolicy {
        ToolPolicy::read_only()
            .with_runtime(ToolRuntime {
                timeout_ms: Some(250),
                ..ToolRuntime::default()
            })
            .with_display(ToolDisplay::label("Inspect workspace").with_detail("repository"))
    }
}

#[tokio::test]
async fn policy_declaration_drives_the_default_timeout_and_display_metadata() {
    let tool = DeclaredTool;
    assert_eq!(tool.name(), "declared_tool");
    assert_eq!(tool.description(), "A tool with a policy declaration");
    assert_eq!(tool.parameters_schema(), json!({ "type": "object" }));
    let result = tool.execute(Value::Null).await.expect("the tool runs");
    assert_eq!(result.output(), "done");
    assert!(tool.policy().classified);
    assert_eq!(tool.timeout_policy(&Value::Null), ToolTimeout::Millis(250));
    assert_eq!(
        tool.display_label(&Value::Null).as_deref(),
        Some("Inspect workspace")
    );
    assert_eq!(
        tool.display_detail(&Value::Null).as_deref(),
        Some("repository")
    );
}

#[test]
fn display_defaults_humanize_the_name_and_pull_a_context_argument() {
    let tool = DummyTool;
    assert_eq!(
        tool.display_label(&Value::Null).as_deref(),
        Some("Dummy Tool")
    );
    assert!(tool.display_detail(&Value::Null).is_none());
    assert_eq!(
        tool.display_detail(&json!({ "path": "src/main.rs" }))
            .as_deref(),
        Some("src/main.rs")
    );
}

/// A tool overriding the seams a host actually reaches for.
struct WorkspaceTool;

/// Host-defined metadata, standing in for whatever a real host attaches.
#[derive(Debug, PartialEq)]
struct HostTag(&'static str);

#[async_trait]
impl Tool for WorkspaceTool {
    fn name(&self) -> &str {
        "workspace_tool"
    }

    fn description(&self) -> &str {
        "Reports the root it was given"
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success("no workspace"))
    }

    async fn execute_with_context(
        &self,
        _args: Value,
        _options: ToolCallOptions,
        context: Option<&dyn ToolRunContext>,
    ) -> anyhow::Result<ToolResult> {
        match context.and_then(ToolRunContext::workspace_root) {
            Some(root) => Ok(ToolResult::success(root.display().to_string())),
            None => Ok(ToolResult::success("no workspace")),
        }
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Execute
    }

    fn external_effect(&self) -> bool {
        true
    }

    fn timeout_policy(&self, _args: &Value) -> ToolTimeout {
        ToolTimeout::Unbounded
    }

    fn host_extension(&self) -> Option<&(dyn Any + Send + Sync)> {
        static TAG: HostTag = HostTag("pack-registry");
        Some(&TAG)
    }

    fn host_call_extension(&self, _args: &Value) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(HostTag("per-call")))
    }
}

struct Isolated(PathBuf);

impl ToolRunContext for Isolated {
    fn workspace_root(&self) -> Option<&Path> {
        Some(&self.0)
    }
}

#[tokio::test]
async fn a_tool_reads_its_workspace_root_through_the_erased_context() {
    let context = Isolated(PathBuf::from("/tmp/worktree"));
    let direct = WorkspaceTool
        .execute(Value::Null)
        .await
        .expect("the tool runs");
    assert_eq!(direct.output(), "no workspace");
    let result = WorkspaceTool
        .execute_with_context(Value::Null, ToolCallOptions::default(), Some(&context))
        .await
        .expect("the tool runs");
    assert_eq!(result.output(), "/tmp/worktree");

    // Without a context the tool falls back rather than failing.
    let bare = WorkspaceTool
        .execute_with_context(Value::Null, ToolCallOptions::default(), None)
        .await
        .expect("the tool runs");
    assert_eq!(bare.output(), "no workspace");
}

#[test]
fn a_host_recovers_its_own_metadata_by_downcasting() {
    let tool = WorkspaceTool;
    let tagged = tool
        .host_extension()
        .and_then(|any| any.downcast_ref::<HostTag>());
    assert_eq!(tagged, Some(&HostTag("pack-registry")));

    let per_call = tool
        .host_call_extension(&Value::Null)
        .and_then(|any| any.downcast::<HostTag>().ok());
    assert_eq!(per_call.as_deref(), Some(&HostTag("per-call")));
}

#[test]
fn overridden_declarations_are_visible_through_a_trait_object() {
    let erased: &dyn Tool = &WorkspaceTool;
    assert_eq!(erased.name(), "workspace_tool");
    assert_eq!(erased.description(), "Reports the root it was given");
    assert_eq!(erased.parameters_schema(), json!({ "type": "object" }));
    assert_eq!(erased.permission_level(), PermissionLevel::Execute);
    assert!(erased.external_effect());
    assert_eq!(erased.timeout_policy(&Value::Null), ToolTimeout::Unbounded);
}
