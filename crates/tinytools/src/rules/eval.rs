//! Evaluating rules against a subject.

use serde_json::Value;

use crate::tool::Tool;

use super::subject::ToolSubject;
use super::types::{
    ApprovalDirective, DefaultEffect, Patterns, RuleContext, RuleDecision, RuleEffect, RuleRef,
    Surface, ToolMatcher, ToolRule, ToolRuleSet, ToolRules,
};

/// Whether `matcher` matches `subject`. `args` is the call's arguments on the
/// call surface and `None` elsewhere; `optimistic_args` makes an argument
/// match count as satisfied when there are no arguments to check.
fn matcher_matches(
    matcher: &ToolMatcher,
    subject: &ToolSubject,
    args: Option<&Value>,
    optimistic_args: bool,
) -> bool {
    let name_ok = matcher
        .name
        .as_ref()
        .is_none_or(|patterns| patterns.matches(&subject.name));
    let family_ok = matcher.family.as_ref().is_none_or(|patterns| {
        subject
            .family
            .as_deref()
            .is_some_and(|family| patterns.matches(family))
    });
    let tags_ok = matcher
        .tags
        .as_ref()
        .is_none_or(|patterns| patterns.matches_any(subject.tags.iter().map(String::as_str)));
    let category_ok = matcher.category.as_ref().is_none_or(|categories| {
        subject
            .category
            .is_some_and(|category| categories.contains(&category))
    });
    let exposure_ok = matcher.exposure.as_ref().is_none_or(|exposures| {
        subject
            .exposure
            .is_some_and(|exposure| exposures.contains(&exposure))
    });
    let at_least_ok = matcher
        .permission_at_least
        .is_none_or(|floor| subject.permission.is_some_and(|level| level >= floor));
    let at_most_ok = matcher
        .permission_at_most
        .is_none_or(|ceiling| subject.permission.is_some_and(|level| level <= ceiling));
    let effects_ok = matcher.side_effects.as_ref().is_none_or(|wanted| {
        subject
            .side_effects
            .is_some_and(|declared| wanted.iter().any(|effect| effect.declared_by(&declared)))
    });
    let arg_ok = matcher.arg.as_ref().is_none_or(|arg| match args {
        Some(args) => arg.matches(args),
        None => optimistic_args,
    });
    name_ok
        && family_ok
        && tags_ok
        && category_ok
        && exposure_ok
        && at_least_ok
        && at_most_ok
        && effects_ok
        && arg_ok
}

fn context_matches(
    when: &std::collections::BTreeMap<String, Patterns>,
    context: &RuleContext,
) -> bool {
    when.iter().all(|(key, patterns)| {
        context
            .get(key)
            .is_some_and(|value| patterns.matches(value))
    })
}

impl ToolRule {
    /// Whether this rule matches `subject` in `context` on `surface`.
    ///
    /// `args` is the call's arguments and is only consulted on
    /// [`Surface::Call`]. Elsewhere an argument condition cannot be decided,
    /// so it is read in the direction that never over-restricts a listing:
    /// an `allow` with an argument condition still lists the tool (some call
    /// of it may be allowed), while every other effect does not apply until
    /// there is a call to check. The call itself is then decided exactly.
    #[must_use]
    pub fn matches(
        &self,
        subject: &ToolSubject,
        context: &RuleContext,
        surface: Surface,
        args: Option<&Value>,
    ) -> bool {
        if !self.applies_on(surface) || !context_matches(&self.when, context) {
            return false;
        }
        let args = if surface == Surface::Call { args } else { None };
        let optimistic = self.effect == RuleEffect::Allow && surface != Surface::Call;
        if !matcher_matches(&self.matcher, subject, args, optimistic) {
            return false;
        }
        // The carve-out is read pessimistically for the rule: an `except`
        // with an argument condition carves nothing out until a call proves
        // it applies.
        !self
            .except
            .as_ref()
            .is_some_and(|except| matcher_matches(except, subject, args, false))
    }
}

impl ToolRules {
    /// An empty layer that admits everything.
    #[must_use]
    pub fn allow_all() -> Self {
        Self::default()
    }

    /// An empty layer that refuses everything.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            default: DefaultEffect::Deny,
            ..Self::default()
        }
    }

    /// The common legacy shape: an optional allowlist and a denylist of name
    /// globs. An empty `allow` admits everything not denied; a non-empty one
    /// makes this layer an allowlist.
    #[must_use]
    pub fn from_allow_deny<A, D>(allow: A, deny: D) -> Self
    where
        A: IntoIterator,
        A::Item: Into<String>,
        D: IntoIterator,
        D::Item: Into<String>,
    {
        let allow: Patterns = allow.into_iter().collect();
        let deny: Patterns = deny.into_iter().collect();
        let mut rules = Self::allow_all();
        if !allow.0.is_empty() {
            rules.default = DefaultEffect::Deny;
            rules
                .rules
                .push(ToolRule::names(RuleEffect::Allow, allow.0));
        }
        if !deny.0.is_empty() {
            rules.rules.push(ToolRule::names(RuleEffect::Deny, deny.0));
        }
        rules
    }

    /// Sets [`Self::name`].
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Appends a rule.
    #[must_use]
    pub fn with_rule(mut self, rule: ToolRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Whether this layer has no rules and admits everything, so a host can
    /// skip it.
    #[must_use]
    pub fn is_permissive(&self) -> bool {
        self.default == DefaultEffect::Allow && self.rules.is_empty()
    }

    /// Decides `subject` on `surface`. See [`ToolRules`] for how effects
    /// combine and [`ToolRule::matches`] for how argument conditions read
    /// off the call surface.
    #[must_use]
    pub fn evaluate(
        &self,
        subject: &ToolSubject,
        context: &RuleContext,
        surface: Surface,
        args: Option<&Value>,
    ) -> RuleDecision {
        self.evaluate_layer(0, subject, context, surface, args)
    }

    fn evaluate_layer(
        &self,
        layer: usize,
        subject: &ToolSubject,
        context: &RuleContext,
        surface: Surface,
        args: Option<&Value>,
    ) -> RuleDecision {
        let reference = |index: Option<usize>| RuleRef {
            layer,
            layer_name: self.name.clone(),
            rule: index,
            id: index.and_then(|i| self.rules[i].id.clone()),
            reason: index.and_then(|i| self.rules[i].reason.clone()),
        };
        let (mut deny, mut hide, mut allowed) = (None, None, false);
        let mut approval = ApprovalDirective::Default;
        for (index, rule) in self.rules.iter().enumerate() {
            if !rule.matches(subject, context, surface, args) {
                continue;
            }
            match rule.effect {
                RuleEffect::Allow => allowed = true,
                RuleEffect::Deny => {
                    deny.get_or_insert(index);
                }
                RuleEffect::Hide => {
                    hide.get_or_insert(index);
                }
                RuleEffect::RequireApproval => approval = ApprovalDirective::Required,
                RuleEffect::AutoApprove => {
                    approval = approval.strictest(ApprovalDirective::Waived);
                }
            }
        }

        if let Some(index) = deny {
            return RuleDecision {
                visible: false,
                callable: false,
                approval,
                blocked_by: Some(reference(Some(index))),
            };
        }
        if !allowed && self.default == DefaultEffect::Deny {
            return RuleDecision {
                visible: false,
                callable: false,
                approval,
                blocked_by: Some(reference(None)),
            };
        }
        RuleDecision {
            visible: hide.is_none(),
            callable: true,
            approval,
            blocked_by: hide.map(|index| reference(Some(index))),
        }
    }
}

impl ToolRuleSet {
    /// A set with no layers, which admits everything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A set holding one layer.
    #[must_use]
    pub fn single(layer: ToolRules) -> Self {
        Self {
            layers: vec![layer],
        }
    }

    /// Adds a layer. Adding can only narrow what the set admits.
    #[must_use]
    pub fn with_layer(mut self, layer: ToolRules) -> Self {
        self.push(layer);
        self
    }

    /// Adds a layer in place, skipping one that admits everything.
    pub fn push(&mut self, layer: ToolRules) {
        if !layer.is_permissive() {
            self.layers.push(layer);
        }
    }

    /// Adds every layer of `other`.
    pub fn extend(&mut self, other: ToolRuleSet) {
        for layer in other.layers {
            self.push(layer);
        }
    }

    /// Whether the set admits everything, so a host can skip evaluation.
    #[must_use]
    pub fn is_permissive(&self) -> bool {
        self.layers.iter().all(ToolRules::is_permissive)
    }

    /// Decides `subject` on `surface` against every layer: each must admit
    /// for the set to admit, the first refusal is reported, and approval
    /// takes the strictest directive any layer gave.
    #[must_use]
    pub fn evaluate(
        &self,
        subject: &ToolSubject,
        context: &RuleContext,
        surface: Surface,
        args: Option<&Value>,
    ) -> RuleDecision {
        let mut combined = RuleDecision::allow();
        for (index, layer) in self.layers.iter().enumerate() {
            let decision = layer.evaluate_layer(index, subject, context, surface, args);
            combined = combine(combined, decision);
        }
        combined
    }

    /// Whether `subject` may be listed on `surface` (catalogue or search).
    #[must_use]
    pub fn visible(&self, subject: &ToolSubject, context: &RuleContext, surface: Surface) -> bool {
        self.evaluate(subject, context, surface, None).visible
    }

    /// Decides a concrete call of `tool` with `args`.
    ///
    /// The tool is evaluated with its argument-aware permission level, and
    /// when it dispatches to another tool ([`Tool::indirect_target`]) the
    /// target is evaluated too: a rule against `GMAIL_DELETE_*` refuses a
    /// connector's generic execute tool aimed at it. Both must be callable,
    /// and approval takes the stricter directive.
    #[must_use]
    pub fn evaluate_call(
        &self,
        tool: &dyn Tool,
        context: &RuleContext,
        args: &Value,
    ) -> RuleDecision {
        let subject = ToolSubject::of_call(tool, args);
        let direct = self.evaluate(&subject, context, Surface::Call, Some(args));
        match tool.indirect_target(args) {
            Some(target) => {
                let indirect = self.evaluate(&target, context, Surface::Call, Some(args));
                combine(direct, indirect)
            }
            None => direct,
        }
    }
}

/// Folds two decisions: both must admit; the first refusal is kept.
fn combine(first: RuleDecision, second: RuleDecision) -> RuleDecision {
    let blocked_by = if !first.callable {
        first.blocked_by
    } else if !second.callable {
        second.blocked_by
    } else {
        first.blocked_by.or(second.blocked_by)
    };
    RuleDecision {
        visible: first.visible && second.visible,
        callable: first.callable && second.callable,
        approval: first.approval.strictest(second.approval),
        blocked_by,
    }
}

impl From<ToolRules> for ToolRuleSet {
    fn from(layer: ToolRules) -> Self {
        let mut set = Self::new();
        set.push(layer);
        set
    }
}
