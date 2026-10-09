//! What rules are evaluated against: the attributes of one tool.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::classification::ToolCategory;
use crate::permission::PermissionLevel;
use crate::policy::ToolSideEffects;
use crate::tool::{Tool, ToolExposure};

/// The attributes of a tool a rule can match.
///
/// Built from a live tool with [`Self::of`], which knows everything, or by
/// hand from whatever a host has at hand — a bare schema plus a family, a
/// target resolved from a dispatcher's arguments. Unknown attributes stay
/// `None`; see [`ToolMatcher`](super::ToolMatcher) for how that reads.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ToolSubject {
    /// The tool name.
    pub name: String,
    /// The tool's family, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Host-assigned tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The tool's category, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<ToolCategory>,
    /// The tool's exposure, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<ToolExposure>,
    /// The privilege the tool (or this call of it) requires, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionLevel>,
    /// The tool's declared side effects, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<ToolSideEffects>,
}

impl ToolSubject {
    /// A subject that knows only its name.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Sets [`Self::family`].
    #[must_use]
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    /// Adds a tag.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Sets [`Self::permission`].
    #[must_use]
    pub fn with_permission(mut self, permission: PermissionLevel) -> Self {
        self.permission = Some(permission);
        self
    }

    /// Everything `tool` declares about itself, with its argument-less
    /// permission level.
    ///
    /// A tool that declares an outside effect only through the older
    /// [`Tool::external_effect`] (its policy left unclassified) still reads as
    /// `external_service`, so a rule on that side effect matches it.
    #[must_use]
    pub fn of(tool: &dyn Tool) -> Self {
        let mut side_effects = tool.policy().side_effects;
        side_effects.external_service |= tool.external_effect();
        Self {
            name: tool.name().to_string(),
            family: tool.family().map(str::to_string),
            tags: tool.tags(),
            category: Some(tool.category()),
            exposure: Some(tool.exposure()),
            permission: Some(tool.permission_level()),
            side_effects: Some(side_effects),
        }
    }

    /// [`Self::of`], with the permission level and external effect `tool`
    /// declares for `args`.
    ///
    /// The per-call answer supersedes the argument-free one, so a composite
    /// that is conservatively external but refines a read-only call to
    /// `false` reads as such; an effect the tool's policy declares explicitly
    /// is kept either way.
    #[must_use]
    pub fn of_call(tool: &dyn Tool, args: &Value) -> Self {
        let mut subject = Self::of(tool);
        subject.permission = Some(tool.permission_level_with_args(args));
        let declared = tool.policy().side_effects.external_service;
        if let Some(effects) = subject.side_effects.as_mut() {
            effects.external_service = declared || tool.external_effect_with_args(args);
        }
        subject
    }
}

/// The tool a dispatcher's call actually reaches, and the arguments that
/// tool receives.
///
/// Returned by [`Tool::indirect_target`]. A dispatcher that wraps its
/// target's arguments in an envelope — `{"action": "...", "arguments": {...}}`
/// — reports the inner arguments here, so an argument-scoped rule written for
/// the target reads the same values whether the target is called directly or
/// through the dispatcher. `None` means the target receives the dispatcher's
/// own arguments unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IndirectCall {
    /// The target tool.
    pub target: ToolSubject,
    /// The arguments the target receives, when they differ from the
    /// dispatcher's.
    pub arguments: Option<Value>,
}

impl IndirectCall {
    /// A call of `target` with the dispatcher's own arguments.
    #[must_use]
    pub fn new(target: ToolSubject) -> Self {
        Self {
            target,
            arguments: None,
        }
    }

    /// Sets the arguments the target receives.
    #[must_use]
    pub fn with_arguments(mut self, arguments: Value) -> Self {
        self.arguments = Some(arguments);
        self
    }
}

impl From<ToolSubject> for IndirectCall {
    fn from(target: ToolSubject) -> Self {
        Self::new(target)
    }
}
