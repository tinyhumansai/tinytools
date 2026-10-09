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
    #[must_use]
    pub fn of(tool: &dyn Tool) -> Self {
        Self {
            name: tool.name().to_string(),
            family: tool.family().map(str::to_string),
            tags: tool.tags(),
            category: Some(tool.category()),
            exposure: Some(tool.exposure()),
            permission: Some(tool.permission_level()),
            side_effects: Some(tool.policy().side_effects),
        }
    }

    /// [`Self::of`], with the permission level `tool` declares for `args`.
    #[must_use]
    pub fn of_call(tool: &dyn Tool, args: &Value) -> Self {
        let mut subject = Self::of(tool);
        subject.permission = Some(tool.permission_level_with_args(args));
        subject
    }
}
