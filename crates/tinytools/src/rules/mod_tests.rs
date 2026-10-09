//! Behaviour of the rule engine: matching, precedence, surfaces, layering,
//! indirect targets and the serde representation.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{
    PermissionLevel, Tool, ToolCategory, ToolExposure, ToolPolicy, ToolResult, ToolSideEffects,
};

fn ctx() -> RuleContext {
    RuleContext::new()
}

fn layer(value: Value) -> ToolRules {
    serde_json::from_value(value).expect("rules parse")
}

fn decide(rules: &ToolRules, subject: &ToolSubject, surface: Surface) -> RuleDecision {
    rules.evaluate(subject, &ctx(), surface, None)
}

// ── glob ──────────────────────────────────────────────────────────────────

#[test]
fn glob_star_and_question_mark() {
    assert!(glob_matches("mcp_*", "mcp_github_issue_abc123"));
    assert!(glob_matches("*", ""));
    assert!(glob_matches("*", "anything"));
    assert!(glob_matches("a?c", "abc"));
    assert!(!glob_matches("a?c", "ac"));
    assert!(glob_matches("*_issue_*", "mcp_github_issue_abc123"));
    assert!(glob_matches("a*b*c", "aXXbYYbZZc"));
    assert!(!glob_matches("a*b*c", "aXXbYY"));
    assert!(glob_matches("**", "x"));
}

#[test]
fn glob_is_ascii_case_insensitive() {
    assert!(glob_matches("gmail_*", "GMAIL_SEND_EMAIL"));
    assert!(glob_matches("GMAIL_SEND_EMAIL", "gmail_send_email"));
}

#[test]
fn empty_glob_matches_only_empty() {
    assert!(glob_matches("", ""));
    assert!(!glob_matches("", "x"));
    assert!(!glob_matches("x", ""));
}

// ── matching ──────────────────────────────────────────────────────────────

#[test]
fn empty_layer_admits_everything() {
    let rules = ToolRules::allow_all();
    let decision = decide(&rules, &ToolSubject::named("shell"), Surface::Call);
    assert_eq!(decision, RuleDecision::allow());
    assert!(rules.is_permissive());
}

#[test]
fn deny_beats_allow_inside_a_layer() {
    let rules = layer(json!({
        "default": "deny",
        "rules": [
            { "effect": "allow", "match": { "name": "*" } },
            { "id": "no-shell", "effect": "deny", "match": { "name": "shell" }, "reason": "no shells" },
        ],
    }));
    let decision = decide(&rules, &ToolSubject::named("shell"), Surface::Call);
    assert!(!decision.callable && !decision.visible);
    let by = decision.blocked_by.expect("blocked");
    assert_eq!(by.rule, Some(1));
    assert_eq!(by.id.as_deref(), Some("no-shell"));
    assert!(decide(&rules, &ToolSubject::named("file_read"), Surface::Call).callable);
}

#[test]
fn default_deny_needs_an_allow() {
    let rules = ToolRules::from_allow_deny(["file_*"], Vec::<String>::new());
    assert!(decide(&rules, &ToolSubject::named("file_read"), Surface::Catalog).visible);
    let refused = decide(&rules, &ToolSubject::named("shell"), Surface::Catalog);
    assert!(!refused.visible);
    assert_eq!(refused.blocked_by.expect("blocked").rule, None);
}

#[test]
fn from_allow_deny_with_empty_allow_is_a_denylist() {
    let rules = ToolRules::from_allow_deny(Vec::<String>::new(), ["spawn_*"]);
    assert_eq!(rules.default, DefaultEffect::Allow);
    assert!(!decide(&rules, &ToolSubject::named("spawn_subagent"), Surface::Call).callable);
    assert!(decide(&rules, &ToolSubject::named("shell"), Surface::Call).callable);
}

#[test]
fn hide_keeps_a_tool_callable() {
    let rules = layer(json!({ "rules": [ { "effect": "hide", "match": { "tags": "pack:*" } } ] }));
    let subject = ToolSubject::named("gmail_send").with_tag("pack:gmail");
    assert!(!decide(&rules, &subject, Surface::Catalog).visible);
    assert!(!decide(&rules, &subject, Surface::Search).visible);
    assert!(decide(&rules, &subject, Surface::Call).callable);
    assert!(decide(&rules, &ToolSubject::named("shell"), Surface::Catalog).visible);
}

#[test]
fn rules_apply_only_on_their_surfaces() {
    let rules = layer(
        json!({ "rules": [ { "effect": "deny", "on": ["search"], "match": { "name": "x" } } ] }),
    );
    let subject = ToolSubject::named("x");
    assert!(decide(&rules, &subject, Surface::Catalog).visible);
    assert!(!decide(&rules, &subject, Surface::Search).visible);
    assert!(decide(&rules, &subject, Surface::Call).callable);
}

#[test]
fn except_carves_out_of_a_rule() {
    let rules = layer(json!({ "rules": [
        { "effect": "deny", "match": { "name": "mcp_*" }, "except": { "family": "github" } },
    ] }));
    let github = ToolSubject::named("mcp_github_issue_1").with_family("github");
    let slack = ToolSubject::named("mcp_slack_post_1").with_family("slack");
    let unknown = ToolSubject::named("mcp_bare_1");
    assert!(decide(&rules, &github, Surface::Call).callable);
    assert!(!decide(&rules, &slack, Surface::Call).callable);
    assert!(!decide(&rules, &unknown, Surface::Call).callable);
}

#[test]
fn unknown_attributes_do_not_match() {
    let rules = layer(json!({ "rules": [
        { "effect": "deny", "match": { "permission_at_least": "Execute" } },
    ] }));
    // A bare-name subject has no permission: the deny cannot apply.
    assert!(decide(&rules, &ToolSubject::named("shell"), Surface::Call).callable);
    let known = ToolSubject::named("shell").with_permission(PermissionLevel::Execute);
    assert!(!decide(&rules, &known, Surface::Call).callable);
}

#[test]
fn permission_bounds_category_exposure_and_effects() {
    let subject = ToolSubject {
        name: "pay".into(),
        family: None,
        tags: Vec::new(),
        category: Some(ToolCategory::Workflow),
        exposure: Some(ToolExposure::Deferred),
        permission: Some(PermissionLevel::Write),
        side_effects: Some(ToolSideEffects {
            payment: true,
            ..ToolSideEffects::default()
        }),
    };
    let denies = |matcher: Value| {
        let rules = layer(json!({ "rules": [ { "effect": "deny", "match": matcher } ] }));
        !decide(&rules, &subject, Surface::Call).callable
    };
    assert!(denies(json!({ "permission_at_most": "Write" })));
    assert!(!denies(json!({ "permission_at_most": "ReadOnly" })));
    assert!(denies(json!({ "permission_at_least": "Write" })));
    assert!(denies(json!({ "category": ["skill"] })));
    assert!(!denies(json!({ "category": ["system"] })));
    assert!(denies(json!({ "exposure": ["deferred"] })));
    assert!(!denies(json!({ "exposure": ["direct", "hidden"] })));
    assert!(denies(
        json!({ "side_effects": ["destructive", "payment"] })
    ));
    assert!(!denies(json!({ "side_effects": ["network"] })));
    assert!(!denies(json!({ "family": "*" })));
    assert!(denies(
        json!({ "name": ["x", "p*"], "category": ["skill"] })
    ));
}

#[test]
fn when_matches_the_context() {
    let rules = layer(json!({ "rules": [
        { "effect": "deny", "match": { "name": "shell" }, "when": { "channel": "telegram*" } },
    ] }));
    let shell = ToolSubject::named("shell");
    let telegram = RuleContext::new().with("channel", "telegram");
    let web = RuleContext::new().with("channel", "web");
    assert!(
        !rules
            .evaluate(&shell, &telegram, Surface::Call, None)
            .callable
    );
    assert!(rules.evaluate(&shell, &web, Surface::Call, None).callable);
    assert!(rules.evaluate(&shell, &ctx(), Surface::Call, None).callable);
}

// ── arguments ─────────────────────────────────────────────────────────────

#[test]
fn arg_rules_decide_calls_and_read_listings_safely() {
    let rules = layer(json!({
        "default": "deny",
        "rules": [
            { "effect": "allow", "match": { "name": "composio_execute", "arg": { "pointer": "/action", "value": "GMAIL_*" } } },
            { "effect": "deny", "match": { "arg": { "pointer": "action", "value": "*_DELETE_*" } } },
        ],
    }));
    let execute = ToolSubject::named("composio_execute");
    // Listings: the allow reads optimistically, the deny cannot apply yet.
    assert!(decide(&rules, &execute, Surface::Catalog).visible);
    assert!(decide(&rules, &execute, Surface::Search).visible);
    let call = |action: &str| {
        rules
            .evaluate(
                &execute,
                &ctx(),
                Surface::Call,
                Some(&json!({ "action": action })),
            )
            .callable
    };
    assert!(call("GMAIL_SEND_EMAIL"));
    assert!(!call("GMAIL_DELETE_EMAIL"));
    assert!(!call("SLACK_POST"));
    // A call with no arguments supplied cannot satisfy an argument allow.
    assert!(
        !rules
            .evaluate(&execute, &ctx(), Surface::Call, None)
            .callable
    );
}

#[test]
fn arg_matcher_reads_scalars_only() {
    let matcher = ArgMatcher {
        pointer: "/n".into(),
        value: Patterns::one("4*"),
    };
    assert!(matcher.matches(&json!({ "n": 42 })));
    assert!(!matcher.matches(&json!({ "n": [42] })));
    assert!(!matcher.matches(&json!({})));
    let flag = ArgMatcher {
        pointer: "/f".into(),
        value: Patterns::one("true"),
    };
    assert!(flag.matches(&json!({ "f": true })));
}

// ── approval ──────────────────────────────────────────────────────────────

#[test]
fn require_approval_beats_auto_approve() {
    let rules = layer(json!({ "rules": [
        { "effect": "auto_approve", "match": { "name": "*" } },
        { "effect": "require_approval", "match": { "name": "send_*" } },
    ] }));
    let send = decide(&rules, &ToolSubject::named("send_email"), Surface::Call);
    assert_eq!(send.approval, ApprovalDirective::Required);
    assert!(send.callable);
    let read = decide(&rules, &ToolSubject::named("read_file"), Surface::Call);
    assert_eq!(read.approval, ApprovalDirective::Waived);
    let none = decide(
        &ToolRules::allow_all(),
        &ToolSubject::named("x"),
        Surface::Call,
    );
    assert_eq!(none.approval, ApprovalDirective::Default);
}

#[test]
fn approval_directive_strictest_order() {
    use ApprovalDirective::{Default, Required, Waived};
    assert_eq!(Default.strictest(Waived), Waived);
    assert_eq!(Waived.strictest(Required), Required);
    assert_eq!(Default.strictest(Default), Default);
}

// ── layering ──────────────────────────────────────────────────────────────

#[test]
fn layers_intersect_allowlists() {
    let set = ToolRuleSet::new()
        .with_layer(
            ToolRules::from_allow_deny(["file_*", "shell"], Vec::<String>::new()).named("config"),
        )
        .with_layer(
            ToolRules::from_allow_deny(["file_*", "web_*"], Vec::<String>::new()).named("agent"),
        );
    let visible = |name: &str| set.visible(&ToolSubject::named(name), &ctx(), Surface::Catalog);
    assert!(visible("file_read"));
    assert!(!visible("shell"));
    assert!(!visible("web_fetch"));
    let refused = set.evaluate(
        &ToolSubject::named("web_fetch"),
        &ctx(),
        Surface::Call,
        None,
    );
    let by = refused.blocked_by.expect("blocked");
    assert_eq!((by.layer, by.layer_name.as_deref()), (0, Some("config")));
}

#[test]
fn permissive_layers_are_skipped() {
    let mut set = ToolRuleSet::new();
    set.push(ToolRules::allow_all());
    assert!(set.layers.is_empty());
    assert!(set.is_permissive());
    set.extend(ToolRuleSet::single(ToolRules::deny_all()));
    assert!(!set.is_permissive());
    assert!(!set.visible(&ToolSubject::named("x"), &ctx(), Surface::Catalog));
    assert!(ToolRuleSet::from(ToolRules::allow_all()).layers.is_empty());
}

#[test]
fn layer_approval_takes_the_strictest() {
    let set = ToolRuleSet::new()
        .with_layer(layer(
            json!({ "rules": [ { "effect": "auto_approve", "match": {} } ] }),
        ))
        .with_layer(layer(
            json!({ "rules": [ { "effect": "require_approval", "match": { "name": "x" } } ] }),
        ));
    let decision = set.evaluate(&ToolSubject::named("x"), &ctx(), Surface::Call, None);
    assert_eq!(decision.approval, ApprovalDirective::Required);
}

#[test]
fn a_hidden_tool_reports_the_hiding_rule_but_stays_callable() {
    let set = ToolRuleSet::new().with_layer(layer(
        json!({ "rules": [ { "id": "quiet", "effect": "hide", "match": { "name": "x" } } ] }),
    ));
    let decision = set.evaluate(&ToolSubject::named("x"), &ctx(), Surface::Catalog, None);
    assert!(!decision.admits(Surface::Catalog));
    assert!(decision.admits(Surface::Call));
    assert_eq!(
        decision.blocked_by.expect("hidden").id.as_deref(),
        Some("quiet")
    );
}

// ── tools and indirect targets ────────────────────────────────────────────

struct Execute;

#[async_trait]
impl Tool for Execute {
    fn name(&self) -> &'static str {
        "composio_execute"
    }
    fn description(&self) -> &'static str {
        "Runs a connector action."
    }
    fn parameters_schema(&self) -> Value {
        json!({ "type": "object" })
    }
    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success("ok"))
    }
    fn family(&self) -> Option<&str> {
        Some("composio")
    }
    fn tags(&self) -> Vec<String> {
        vec!["connector".into()]
    }
    fn permission_level_with_args(&self, args: &Value) -> PermissionLevel {
        if args["action"]
            .as_str()
            .is_some_and(|a| a.contains("DELETE"))
        {
            PermissionLevel::Dangerous
        } else {
            PermissionLevel::Write
        }
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::classified().with_side_effects(ToolSideEffects {
            external_service: true,
            ..ToolSideEffects::default()
        })
    }
    fn indirect_target(&self, args: &Value) -> Option<ToolSubject> {
        let action = args.get("action")?.as_str()?;
        let toolkit = action.split('_').next()?;
        Some(ToolSubject::named(action).with_family(toolkit.to_ascii_lowercase()))
    }
}

#[test]
fn subject_of_reads_the_tool_declarations() {
    let subject = ToolSubject::of(&Execute);
    assert_eq!(subject.name, "composio_execute");
    assert_eq!(subject.family.as_deref(), Some("composio"));
    assert_eq!(subject.tags, vec!["connector".to_string()]);
    assert_eq!(subject.category, Some(ToolCategory::System));
    assert_eq!(subject.exposure, Some(ToolExposure::Direct));
    assert_eq!(subject.permission, Some(PermissionLevel::ReadOnly));
    assert!(subject.side_effects.is_some_and(|e| e.external_service));
    let call = ToolSubject::of_call(&Execute, &json!({ "action": "GMAIL_DELETE_EMAIL" }));
    assert_eq!(call.permission, Some(PermissionLevel::Dangerous));
}

#[test]
fn evaluate_call_checks_the_indirect_target() {
    let set = ToolRuleSet::single(layer(json!({ "rules": [
        { "id": "no-gmail-delete", "effect": "deny", "match": { "name": "GMAIL_DELETE_*" } },
        { "effect": "require_approval", "match": { "family": "slack" } },
    ] })));
    let call = |action: &str| set.evaluate_call(&Execute, &ctx(), &json!({ "action": action }));
    let refused = call("GMAIL_DELETE_EMAIL");
    assert!(!refused.callable);
    assert_eq!(
        refused.blocked_by.expect("blocked").id.as_deref(),
        Some("no-gmail-delete")
    );
    assert!(call("GMAIL_SEND_EMAIL").callable);
    assert_eq!(call("SLACK_POST").approval, ApprovalDirective::Required);
    // No target in the arguments: only the dispatcher is evaluated.
    assert!(set.evaluate_call(&Execute, &ctx(), &json!({})).callable);
}

#[test]
fn evaluate_call_uses_the_argument_aware_permission() {
    let set = ToolRuleSet::single(layer(json!({ "rules": [
        { "effect": "deny", "match": { "permission_at_least": "Dangerous" } },
    ] })));
    assert!(
        !set.evaluate_call(&Execute, &ctx(), &json!({ "action": "X_DELETE" }))
            .callable
    );
    assert!(
        set.evaluate_call(&Execute, &ctx(), &json!({ "action": "X_SEND" }))
            .callable
    );
}

// ── refusal text and serde ────────────────────────────────────────────────

#[test]
fn refusal_names_the_rule() {
    let rules = layer(json!({ "name": "agent:researcher", "rules": [
        { "id": "no-shell", "effect": "deny", "match": { "name": "shell" }, "reason": "read-only agent" },
        { "effect": "deny", "match": { "name": "curl" } },
    ] }));
    let message = decide(&rules, &ToolSubject::named("shell"), Surface::Call).refusal("shell");
    assert_eq!(
        message,
        "Tool 'shell' is not permitted by tool rules (rule 'no-shell') in 'agent:researcher': read-only agent."
    );
    let anonymous = decide(&rules, &ToolSubject::named("curl"), Surface::Call).refusal("curl");
    assert!(anonymous.contains("(rule #1)"), "{anonymous}");
    let default = decide(
        &ToolRules::deny_all(),
        &ToolSubject::named("x"),
        Surface::Call,
    )
    .refusal("x");
    assert!(default.contains("(default deny)"), "{default}");
    assert_eq!(
        RuleDecision::allow().refusal("x"),
        "Tool 'x' is not permitted."
    );
}

#[test]
fn serde_round_trips_and_pins_the_wire_form() {
    let rules = ToolRules::from_allow_deny(["a"], ["b", "c"])
        .named("cfg")
        .with_rule(
            ToolRule::names(RuleEffect::Hide, ["x"])
                .with_id("h")
                .with_reason("why")
                .on([Surface::Catalog])
                .when("channel", Patterns::one("web")),
        )
        .with_rule(
            ToolRule::new(RuleEffect::Deny)
                .matching(ToolMatcher {
                    family: Some(Patterns::one("slack")),
                    ..ToolMatcher::default()
                })
                .except(ToolMatcher {
                    tags: Some(Patterns::one("safe")),
                    ..ToolMatcher::default()
                }),
        );
    let value = serde_json::to_value(&rules).expect("serialize");
    assert_eq!(value["default"], "deny");
    assert_eq!(
        value["rules"][0],
        json!({ "effect": "allow", "match": { "name": "a" } })
    );
    assert_eq!(value["rules"][1]["match"]["name"], json!(["b", "c"]));
    assert_eq!(
        value["rules"][2],
        json!({ "id": "h", "effect": "hide", "on": ["catalog"], "match": { "name": "x" }, "when": { "channel": "web" }, "reason": "why" })
    );
    let back: ToolRules = serde_json::from_value(value).expect("deserialize");
    assert_eq!(back, rules);

    let set = ToolRuleSet::single(rules);
    let wire = serde_json::to_value(&set).expect("serialize set");
    assert!(wire.is_array());
    assert_eq!(
        serde_json::from_value::<ToolRuleSet>(wire).expect("set"),
        set
    );
}

#[test]
fn unknown_matcher_fields_are_rejected() {
    let err = serde_json::from_value::<ToolRules>(json!({ "rules": [
        { "effect": "deny", "match": { "nmae": "x" } },
    ] }))
    .expect_err("typo rejected");
    assert!(err.to_string().contains("nmae"), "{err}");
}

#[test]
fn decision_serializes() {
    let decision = decide(
        &ToolRules::deny_all().named("n"),
        &ToolSubject::named("x"),
        Surface::Call,
    );
    let value = serde_json::to_value(&decision).expect("serialize");
    assert_eq!(value["callable"], false);
    assert_eq!(value["approval"], "default");
    assert_eq!(
        value["blocked_by"],
        json!({ "layer": 0, "layer_name": "n" })
    );
    let ctx_value = serde_json::to_value(RuleContext::new().with("channel", "web")).expect("ctx");
    assert_eq!(ctx_value, json!({ "channel": "web" }));
    let subject = serde_json::to_value(ToolSubject::named("x").with_tag("t")).expect("subject");
    assert_eq!(subject, json!({ "name": "x", "tags": ["t"] }));
}
