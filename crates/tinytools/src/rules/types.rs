//! The declarative shapes of a rule set: what a rule matches, what it does,
//! and on which surfaces.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::classification::ToolCategory;
use crate::permission::PermissionLevel;
use crate::policy::ToolSideEffects;
use crate::tool::ToolExposure;

use super::glob::glob_matches;

/// One or more glob patterns; a value matches when any pattern does.
///
/// Serializes as a bare string when it holds exactly one pattern and as a list
/// otherwise, so `name = "mcp_*"` and `name = ["mcp_*", "composio_*"]` both
/// read naturally in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patterns(pub Vec<String>);

impl Patterns {
    /// A single-pattern set.
    #[must_use]
    pub fn one(pattern: impl Into<String>) -> Self {
        Self(vec![pattern.into()])
    }

    /// Whether any pattern matches `text`.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        self.0.iter().any(|pattern| glob_matches(pattern, text))
    }

    /// Whether any pattern matches any of `texts`.
    #[must_use]
    pub fn matches_any<'a>(&self, texts: impl IntoIterator<Item = &'a str>) -> bool {
        texts.into_iter().any(|text| self.matches(text))
    }
}

impl<S: Into<String>> FromIterator<S> for Patterns {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        Self(iter.into_iter().map(Into::into).collect())
    }
}

impl Serialize for Patterns {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [single] => serializer.serialize_str(single),
            many => many.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Patterns {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(String),
            Many(Vec<String>),
        }
        Ok(match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(one) => Self(vec![one]),
            OneOrMany::Many(many) => Self(many),
        })
    }
}

/// What a matching rule does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEffect {
    /// Admit the tool. Only meaningful when something would otherwise refuse
    /// it: a rule set whose [`ToolRules::default`] is [`DefaultEffect::Deny`].
    Allow,
    /// Refuse the tool on the rule's surfaces: not listed, not searchable,
    /// not callable. A deny beats every other effect.
    Deny,
    /// Keep the tool off the catalogue and out of search, but leave it
    /// callable by name or through an indirect route a host provides.
    Hide,
    /// Admit the call only after explicit approval.
    RequireApproval,
    /// Waive approval for the call. Loses to [`Self::RequireApproval`].
    AutoApprove,
}

/// What a rule set does with a tool no `allow` rule matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultEffect {
    /// Admit it. Rules only subtract.
    #[default]
    Allow,
    /// Refuse it. The rule set is an allowlist.
    Deny,
}

/// Where a decision is being made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The tool schemas sent to the model up front.
    Catalog,
    /// On-demand discovery: a deferred catalogue, a tool-search index and the
    /// results it returns.
    Search,
    /// Admission of a concrete call, with its arguments.
    Call,
}

impl Surface {
    /// Every surface, in evaluation-independent order.
    pub const ALL: [Self; 3] = [Self::Catalog, Self::Search, Self::Call];
}

/// One declared side effect, named as in [`ToolSideEffects`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    /// [`ToolSideEffects::read_only`].
    ReadOnly,
    /// [`ToolSideEffects::writes_files`].
    WritesFiles,
    /// [`ToolSideEffects::network`].
    Network,
    /// [`ToolSideEffects::installs_dependencies`].
    InstallsDependencies,
    /// [`ToolSideEffects::destructive`].
    Destructive,
    /// [`ToolSideEffects::external_service`].
    ExternalService,
    /// [`ToolSideEffects::payment`].
    Payment,
}

impl SideEffect {
    /// Whether `effects` declares this effect.
    #[must_use]
    pub fn declared_by(self, effects: &ToolSideEffects) -> bool {
        match self {
            Self::ReadOnly => effects.read_only,
            Self::WritesFiles => effects.writes_files,
            Self::Network => effects.network,
            Self::InstallsDependencies => effects.installs_dependencies,
            Self::Destructive => effects.destructive,
            Self::ExternalService => effects.external_service,
            Self::Payment => effects.payment,
        }
    }
}

/// A match on one argument of a call, addressed by JSON pointer.
///
/// Strings match as themselves; numbers and booleans match their JSON text;
/// anything else (absent, null, objects, arrays) never matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgMatcher {
    /// RFC 6901 pointer into the call arguments, e.g. `/action`. A missing
    /// leading `/` is supplied.
    pub pointer: String,
    /// Patterns the addressed value must match.
    pub value: Patterns,
}

impl ArgMatcher {
    /// Whether `args` carries a matching value at [`Self::pointer`].
    #[must_use]
    pub fn matches(&self, args: &Value) -> bool {
        let found = if self.pointer.starts_with('/') || self.pointer.is_empty() {
            args.pointer(&self.pointer)
        } else {
            args.pointer(&format!("/{}", self.pointer))
        };
        match found {
            Some(Value::String(text)) => self.value.matches(text),
            Some(value @ (Value::Number(_) | Value::Bool(_))) => {
                self.value.matches(&value.to_string())
            }
            _ => false,
        }
    }
}

/// What a rule matches about a tool. Every field that is set must match; an
/// empty matcher matches every tool.
///
/// A field that names an attribute the subject does not know (a permission
/// level on a subject built from a bare schema, say) does not match. That is
/// fail-open for a `deny` and fail-closed for an `allow`, which is why a host
/// should evaluate against the full tool wherever it has one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolMatcher {
    /// Tool name globs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Patterns>,
    /// Globs over the tool's family: a toolpack, a connector toolkit, an MCP
    /// server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<Patterns>,
    /// Globs over host-assigned tags; matches when any tag matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Patterns>,
    /// Categories, any of which matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Vec<ToolCategory>>,
    /// Exposures, any of which matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<Vec<ToolExposure>>,
    /// Matches tools requiring at least this permission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_at_least: Option<PermissionLevel>,
    /// Matches tools requiring at most this permission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_at_most: Option<PermissionLevel>,
    /// Declared side effects, any of which matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<Vec<SideEffect>>,
    /// An argument match. Only a call has arguments; see
    /// [`ToolRules::evaluate`](super::ToolRules::evaluate) for how such a rule
    /// reads on the catalogue and search surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg: Option<ArgMatcher>,
}

/// One rule: an effect, the surfaces it applies on, and what it matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRule {
    /// Stable identifier, reported in decisions and refusal messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// What the rule does.
    pub effect: RuleEffect,
    /// Surfaces the rule applies on. Empty means every surface.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on: Vec<Surface>,
    /// What the rule matches.
    #[serde(default, rename = "match")]
    pub matcher: ToolMatcher,
    /// Carve-out: a tool this matches is not matched by the rule, so
    /// "deny `mcp_*` except the github server" is one rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub except: Option<ToolMatcher>,
    /// Context attributes that must all be present and match, e.g.
    /// `channel = "telegram"`. Empty means any context.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, Patterns>,
    /// Why the rule exists, for audit logs and model-facing refusals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ToolRule {
    /// A rule with `effect` matching every tool on every surface.
    #[must_use]
    pub fn new(effect: RuleEffect) -> Self {
        Self {
            id: None,
            effect,
            on: Vec::new(),
            matcher: ToolMatcher::default(),
            except: None,
            when: BTreeMap::new(),
            reason: None,
        }
    }

    /// A rule with `effect` matching tool names against `patterns`.
    #[must_use]
    pub fn names<S: Into<String>>(effect: RuleEffect, patterns: impl IntoIterator<Item = S>) -> Self {
        let mut rule = Self::new(effect);
        rule.matcher.name = Some(patterns.into_iter().collect());
        rule
    }

    /// Sets [`Self::id`].
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets [`Self::reason`].
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Replaces [`Self::matcher`].
    #[must_use]
    pub fn matching(mut self, matcher: ToolMatcher) -> Self {
        self.matcher = matcher;
        self
    }

    /// Sets [`Self::except`].
    #[must_use]
    pub fn except(mut self, matcher: ToolMatcher) -> Self {
        self.except = Some(matcher);
        self
    }

    /// Restricts the rule to `surfaces`.
    #[must_use]
    pub fn on(mut self, surfaces: impl IntoIterator<Item = Surface>) -> Self {
        self.on = surfaces.into_iter().collect();
        self
    }

    /// Adds a context condition.
    #[must_use]
    pub fn when(mut self, key: impl Into<String>, patterns: Patterns) -> Self {
        self.when.insert(key.into(), patterns);
        self
    }

    /// Whether the rule applies on `surface`.
    #[must_use]
    pub fn applies_on(&self, surface: Surface) -> bool {
        self.on.is_empty() || self.on.contains(&surface)
    }
}

/// One layer of rules with its own default.
///
/// Inside a layer, effects combine without regard to order: a `deny` beats
/// everything, `hide` only removes from the listing surfaces, and
/// `require_approval` beats `auto_approve`. A tool is admitted when no `deny`
/// matches and either an `allow` matches or the default is `allow`.
///
/// Layers from different sources stack in a [`ToolRuleSet`], where every
/// layer must admit — so adding a layer can only narrow.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolRules {
    /// Where this layer came from (`config`, `agent:researcher`), for
    /// decisions and refusal messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What happens to a tool no `allow` rule matched.
    pub default: DefaultEffect,
    /// The rules.
    pub rules: Vec<ToolRule>,
}

/// Layers of [`ToolRules`] that must all admit a tool.
///
/// This is how independent sources compose — global configuration, an agent
/// definition, a channel, a session overlay — without one widening another:
/// two allowlists intersect rather than union.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolRuleSet {
    /// The layers, in the order they were added.
    pub layers: Vec<ToolRules>,
}

/// Attributes of the situation a decision is made in: the channel, the
/// agent, the origin of the turn. Matched by [`ToolRule::when`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleContext {
    /// Attribute values by key.
    pub attributes: BTreeMap<String, String>,
}

impl RuleContext {
    /// An empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an attribute.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// The value of `key`, if set.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(String::as_str)
    }
}

/// Whether a call needs approval, as far as the rules say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDirective {
    /// No rule spoke; the host's own approval policy applies.
    #[default]
    Default,
    /// A rule requires approval.
    Required,
    /// A rule waives approval and none requires it.
    Waived,
}

impl ApprovalDirective {
    /// The stricter of two directives: required, then waived, then default.
    #[must_use]
    pub fn strictest(self, other: Self) -> Self {
        match (self, other) {
            (Self::Required, _) | (_, Self::Required) => Self::Required,
            (Self::Waived, _) | (_, Self::Waived) => Self::Waived,
            _ => Self::Default,
        }
    }
}

/// A pointer to the rule behind a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleRef {
    /// Index of the layer in its [`ToolRuleSet`] (0 for a lone [`ToolRules`]).
    pub layer: usize,
    /// The layer's [`ToolRules::name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_name: Option<String>,
    /// Index of the rule in its layer; `None` when the layer's default decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<usize>,
    /// The rule's [`ToolRule::id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The rule's [`ToolRule::reason`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The outcome of evaluating rules for one tool on one surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleDecision {
    /// Whether the tool may be listed (catalogue) or found (search).
    pub visible: bool,
    /// Whether the tool may be called.
    pub callable: bool,
    /// Whether the call needs approval.
    pub approval: ApprovalDirective,
    /// What refused the tool, when it was refused or hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<RuleRef>,
}

impl RuleDecision {
    /// A decision that admits everything and says nothing about approval.
    #[must_use]
    pub fn allow() -> Self {
        Self {
            visible: true,
            callable: true,
            approval: ApprovalDirective::Default,
            blocked_by: None,
        }
    }

    /// Whether the tool is admitted on `surface`: visible for the listing
    /// surfaces, callable for a call.
    #[must_use]
    pub fn admits(&self, surface: Surface) -> bool {
        match surface {
            Surface::Catalog | Surface::Search => self.visible,
            Surface::Call => self.callable,
        }
    }

    /// A one-line refusal a model can read: names the rule and its reason.
    #[must_use]
    pub fn refusal(&self, tool: &str) -> String {
        let Some(by) = &self.blocked_by else {
            return format!("Tool '{tool}' is not permitted.");
        };
        let mut message = format!("Tool '{tool}' is not permitted by tool rules");
        match (&by.id, by.rule) {
            (Some(id), _) => message.push_str(&format!(" (rule '{id}')")),
            (None, Some(index)) => message.push_str(&format!(" (rule #{index})")),
            (None, None) => message.push_str(" (default deny)"),
        }
        if let Some(layer) = &by.layer_name {
            message.push_str(&format!(" in '{layer}'"));
        }
        if let Some(reason) = &by.reason {
            message.push_str(": ");
            message.push_str(reason);
        }
        message.push('.');
        message
    }
}
