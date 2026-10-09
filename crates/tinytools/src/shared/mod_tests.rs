//! What [`SharedTool`] must not change about the tool it wraps.
//!
//! Twenty-two of `Tool`'s methods carry defaults, so a wrapper that forgot one
//! would compile and answer for its inner tool with the wrong value. The tool
//! below overrides every defaulted method with a non-default answer, so a
//! wrapper that dropped any of them reports the default and fails here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::unnecessary_literal_bound)]

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{SharedTool, owned_belt, share_belt};
use crate::{
    PermissionLevel, Tool, ToolCallOptions, ToolCategory, ToolExposure, ToolInjectedArgument,
    ToolPolicy, ToolResult, ToolRunContext, ToolScope, ToolSpec, ToolTimeout,
};

/// The host extension [`Opinionated`] carries, so a test can downcast it.
#[derive(Debug, PartialEq)]
struct Marker(u32);

static MARKER: Marker = Marker(7);

/// A tool whose every defaulted answer differs from the trait default.
#[derive(Default)]
struct Opinionated {
    executions: AtomicUsize,
}

#[async_trait]
impl Tool for Opinionated {
    fn name(&self) -> &str {
        "opinionated"
    }

    fn description(&self) -> &str {
        "answers nothing by default"
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": { "x": { "type": "string" } } })
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(ToolResult::success("plain"))
    }

    async fn execute_with_options(
        &self,
        _args: Value,
        options: ToolCallOptions,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success(format!(
            "options markdown={}",
            options.prefer_markdown
        )))
    }

    async fn execute_with_context(
        &self,
        _args: Value,
        _options: ToolCallOptions,
        context: Option<&dyn ToolRunContext>,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success(format!(
            "context present={}",
            context.is_some()
        )))
    }

    fn policy(&self) -> ToolPolicy {
        let mut policy = ToolPolicy::default();
        policy.display.label = Some("policy label".into());
        policy
    }

    fn injected_arguments(&self) -> Vec<ToolInjectedArgument> {
        vec![ToolInjectedArgument::host("account_id")]
    }

    fn supports_markdown(&self) -> bool {
        true
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Write
    }

    fn permission_level_with_args(&self, _args: &Value) -> PermissionLevel {
        PermissionLevel::Dangerous
    }

    fn scope(&self) -> ToolScope {
        ToolScope::CliRpcOnly
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workflow
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::Hidden
    }

    fn family(&self) -> Option<&str> {
        Some("opinions")
    }

    fn tags(&self) -> Vec<String> {
        vec!["pack:opinions".into()]
    }

    fn indirect_target(&self, args: &Value) -> Option<crate::ToolSubject> {
        args.get("x")?.as_str().map(crate::ToolSubject::named)
    }

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        true
    }

    fn external_effect(&self) -> bool {
        true
    }

    fn external_effect_with_args(&self, args: &Value) -> bool {
        args.get("send").is_some()
    }

    fn max_result_size_chars(&self) -> Option<usize> {
        Some(17)
    }

    fn timeout_policy(&self, _args: &Value) -> ToolTimeout {
        ToolTimeout::Millis(250)
    }

    fn host_extension(&self) -> Option<&(dyn Any + Send + Sync)> {
        Some(&MARKER)
    }

    fn host_call_extension(&self, _args: &Value) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(Marker(9)))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "opinionated".into(),
            description: "a curated spec".into(),
            parameters: json!({}),
        }
    }

    fn display_label(&self, _args: &Value) -> Option<String> {
        Some("Having opinions".into())
    }

    fn display_detail(&self, _args: &Value) -> Option<String> {
        Some("about everything".into())
    }

    fn return_direct(&self) -> bool {
        true
    }
}

/// A trivial run context, so the context-forwarding path can be observed.
struct Run;

impl ToolRunContext for Run {}

fn wrapped() -> SharedTool {
    SharedTool::new(Arc::new(Opinionated::default()))
}

#[test]
fn the_wrapper_is_the_tool_it_wraps() {
    let tool = wrapped();
    assert_eq!(tool.name(), "opinionated");
    assert_eq!(tool.description(), "answers nothing by default");
    assert_eq!(
        tool.parameters_schema()["properties"]["x"]["type"],
        "string"
    );
    assert_eq!(tool.spec().description, "a curated spec");
}

/// An admission gate reads these. A wrapper answering the default would
/// quietly widen what a tool is allowed to do.
#[test]
fn the_wrapper_does_not_soften_what_a_gate_reads() {
    let tool = wrapped();
    let args = json!({ "send": true });
    assert_eq!(tool.permission_level(), PermissionLevel::Write);
    assert_eq!(
        tool.permission_level_with_args(&args),
        PermissionLevel::Dangerous
    );
    assert!(tool.external_effect());
    assert!(tool.external_effect_with_args(&args));
    assert!(!tool.external_effect_with_args(&json!({})));
    assert_eq!(tool.scope(), ToolScope::CliRpcOnly);
    assert_eq!(tool.category(), ToolCategory::Workflow);
    assert_eq!(tool.policy().display.label.as_deref(), Some("policy label"));
    assert_eq!(tool.injected_arguments().len(), 1);
}

/// The catalogue reads these. A hidden tool that advertised itself through a
/// wrapper is the failure this module exists to prevent.
#[test]
fn the_wrapper_does_not_reveal_a_hidden_tool() {
    let tool = wrapped();
    assert_eq!(tool.exposure(), ToolExposure::Hidden);
    assert_eq!(tool.family(), Some("opinions"));
}

/// Tool rules read these: a wrapper that dropped them would let a tag-based
/// deny miss, and a dispatcher's real target escape its rules.
#[test]
fn the_wrapper_keeps_what_tool_rules_read() {
    let tool = wrapped();
    assert_eq!(tool.tags(), ["pack:opinions"]);
    assert_eq!(
        tool.indirect_target(&json!({ "x": "GMAIL_DELETE_EMAIL" })),
        Some(crate::ToolSubject::named("GMAIL_DELETE_EMAIL"))
    );
    let rules = crate::ToolRuleSet::single(crate::ToolRules::from_allow_deny(
        Vec::<String>::new(),
        ["*_delete_*"],
    ));
    let decision = rules.evaluate_call(
        &tool,
        &crate::RuleContext::new(),
        &json!({ "x": "GMAIL_DELETE_EMAIL" }),
    );
    assert!(!decision.callable);
}

/// Dispatch and result handling read these.
#[test]
fn the_wrapper_keeps_runtime_and_result_declarations() {
    let tool = wrapped();
    let args = json!({});
    assert!(tool.is_concurrency_safe(&args));
    assert_eq!(tool.timeout_policy(&args), ToolTimeout::Millis(250));
    assert_eq!(tool.max_result_size_chars(), Some(17));
    assert!(tool.return_direct());
    assert!(tool.supports_markdown());
    assert_eq!(
        tool.display_label(&args).as_deref(),
        Some("Having opinions")
    );
    assert_eq!(
        tool.display_detail(&args).as_deref(),
        Some("about everything")
    );
}

/// A host recognises its own tool kinds through these; a wrapper that hid
/// them would make every shared tool look foreign.
#[test]
fn the_wrapper_exposes_the_inner_host_extensions() {
    let tool = wrapped();
    let held = tool
        .host_extension()
        .and_then(|ext| ext.downcast_ref::<Marker>());
    assert_eq!(held, Some(&Marker(7)));
    let per_call = tool
        .host_call_extension(&json!({}))
        .and_then(|ext| ext.downcast::<Marker>().ok());
    assert_eq!(per_call.as_deref(), Some(&Marker(9)));
}

#[tokio::test]
async fn every_execute_entry_point_reaches_the_inner_override() {
    let tool = wrapped();
    let plain = tool.execute(json!({})).await.unwrap();
    assert_eq!(plain.output(), "plain");

    let options = ToolCallOptions {
        prefer_markdown: true,
    };
    let with_options = tool.execute_with_options(json!({}), options).await.unwrap();
    assert_eq!(with_options.output(), "options markdown=true");

    let with_context = tool
        .execute_with_context(json!({}), options, Some(&Run))
        .await
        .unwrap();
    assert_eq!(with_context.output(), "context present=true");
}

#[test]
fn debug_prints_the_tool_name() {
    assert_eq!(format!("{:?}", wrapped()), "SharedTool(\"opinionated\")");
}

/// The whole point: one shared instance, many owned handles. A belt minted
/// twice must not build the tool twice.
#[tokio::test]
async fn an_owned_belt_delegates_to_the_one_shared_instance() {
    let inner = Arc::new(Opinionated::default());
    let shared: Vec<Arc<dyn Tool>> = vec![inner.clone()];
    let first = owned_belt(&shared);
    let second = owned_belt(&shared);

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(Arc::strong_count(&inner), 4, "source, local, two handles");
    for belt in [first, second] {
        belt[0].execute(json!({})).await.unwrap();
    }
    assert_eq!(inner.executions.load(Ordering::SeqCst), 2);
}

#[test]
fn share_belt_keeps_every_tool_in_order() {
    let belt: Vec<Box<dyn Tool>> = vec![
        Box::new(Opinionated::default()),
        Box::new(SharedTool::new(Arc::new(Opinionated::default()))),
    ];
    let shared = share_belt(belt);
    assert_eq!(shared.len(), 2);
    assert!(shared.iter().all(|tool| tool.name() == "opinionated"));
}

#[test]
fn an_empty_belt_round_trips_empty() {
    assert!(owned_belt(&share_belt(Vec::new())).is_empty());
}
