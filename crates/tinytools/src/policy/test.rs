//! Unit tests for the declarative tool-policy vocabulary and its wire shape.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::{
    ToolAccess, ToolDisplay, ToolPolicy, ToolReplay, ToolRuntime, ToolSideEffects, WorkspaceAccess,
};
use crate::{SandboxMode, ToolTimeout};

#[test]
fn default_policy_is_unclassified_and_declares_no_capabilities() {
    let policy = ToolPolicy::default();
    assert!(!policy.classified);
    assert_eq!(policy.access.workspace, WorkspaceAccess::None);
    assert!(!policy.access.approval_required);
    assert!(!policy.runtime.cancelable);
    assert!(!policy.has_side_effects());
    assert!(policy.display.is_empty());
}

#[test]
fn read_only_policy_is_a_classified_background_safe_baseline() {
    let policy = ToolPolicy::read_only();
    assert!(policy.classified);
    assert!(policy.side_effects.read_only);
    assert!(policy.runtime.idempotent);
    assert!(policy.runtime.cancelable);
    assert!(policy.access.background_safe);
    assert!(!policy.has_side_effects());
}

#[test]
fn builders_preserve_every_declaration_without_enforcing_it() {
    let side_effects = ToolSideEffects {
        network: true,
        external_service: true,
        ..ToolSideEffects::default()
    };
    let runtime = ToolRuntime {
        timeout_ms: Some(250),
        timeout: ToolTimeout::Millis(500),
        cancelable: true,
        sandbox: SandboxMode::Required,
        ..ToolRuntime::default()
    };
    let access = ToolAccess {
        workspace: WorkspaceAccess::Scoped,
        trusted_roots: vec!["/work/agent".into()],
        credentials: vec!["calendar".into()],
        approval_required: true,
        background_safe: false,
    };
    let display = ToolDisplay::label("Send calendar invite").with_detail("recipient");

    let policy = ToolPolicy::classified()
        .with_side_effects(side_effects)
        .with_runtime(runtime)
        .with_access(access)
        .with_display(display);

    assert!(policy.classified);
    assert!(policy.has_side_effects());
    assert_eq!(policy.runtime.timeout, ToolTimeout::Millis(500));
    assert_eq!(policy.access.workspace, WorkspaceAccess::Scoped);
    assert_eq!(
        policy.display.label.as_deref(),
        Some("Send calendar invite")
    );
    assert_eq!(policy.display.detail.as_deref(), Some("recipient"));
}

#[test]
fn approval_builder_marks_the_access_declaration() {
    let policy = ToolPolicy::classified().requiring_approval();
    assert!(policy.classified);
    assert!(policy.access.approval_required);
}

#[test]
fn policy_round_trips_through_its_stable_json_shape() {
    let policy = ToolPolicy::classified()
        .with_runtime(ToolRuntime {
            timeout: ToolTimeout::Millis(42),
            sandbox: SandboxMode::Required,
            ..ToolRuntime::default()
        })
        .with_access(ToolAccess {
            workspace: WorkspaceAccess::Any,
            ..ToolAccess::default()
        });
    let encoded = serde_json::to_value(&policy).expect("serializable");
    assert_eq!(
        encoded,
        serde_json::json!({
            "classified": true,
            "side_effects": {
                "read_only": false,
                "writes_files": false,
                "network": false,
                "installs_dependencies": false,
                "destructive": false,
                "external_service": false,
                "payment": false,
            },
            "runtime": {
                "timeout": { "mode": "millis", "timeout_ms": 42 },
                "idempotent": false,
                "cancelable": false,
                "sandbox": "required",
                "streaming": false,
                "replay": "never",
            },
            "access": {
                "workspace": "any",
                "approval_required": false,
                "background_safe": false,
            },
        })
    );
    let decoded: ToolPolicy = serde_json::from_value(encoded).expect("deserializable");
    assert_eq!(decoded, policy);
}

#[test]
fn fully_populated_policy_has_a_pinned_json_wire_shape() {
    let policy = ToolPolicy {
        classified: true,
        side_effects: ToolSideEffects {
            read_only: true,
            writes_files: true,
            network: true,
            installs_dependencies: true,
            destructive: true,
            external_service: true,
            payment: true,
        },
        runtime: ToolRuntime {
            timeout_ms: Some(125),
            timeout: ToolTimeout::Millis(250),
            max_retries: Some(3),
            idempotent: true,
            cancelable: true,
            sandbox: SandboxMode::Required,
            max_result_bytes: Some(8_192),
            streaming: true,
        },
        access: ToolAccess {
            workspace: WorkspaceAccess::Scoped,
            trusted_roots: vec!["/work/agent".into(), "/work/shared".into()],
            credentials: vec!["calendar".into(), "mail".into()],
            approval_required: true,
            background_safe: true,
        },
        display: ToolDisplay::new(Some("Send invite"), Some("recipient")),
    };
    let wire = serde_json::json!({
        "classified": true,
        "side_effects": {
            "read_only": true,
            "writes_files": true,
            "network": true,
            "installs_dependencies": true,
            "destructive": true,
            "external_service": true,
            "payment": true,
        },
        "runtime": {
            "timeout_ms": 125,
            "timeout": { "mode": "millis", "timeout_ms": 250 },
            "max_retries": 3,
            "idempotent": true,
            "cancelable": true,
            "sandbox": "required",
            "max_result_bytes": 8192,
            "streaming": true,
        },
        "access": {
            "workspace": "scoped",
            "trusted_roots": ["/work/agent", "/work/shared"],
            "credentials": ["calendar", "mail"],
            "approval_required": true,
            "background_safe": true,
        },
        "display": {
            "label": "Send invite",
            "detail": "recipient",
        },
    });

    assert_eq!(serde_json::to_value(&policy).expect("serializable"), wire);
    assert_eq!(
        serde_json::from_value::<ToolPolicy>(wire).expect("deserializable"),
        policy
    );
}
