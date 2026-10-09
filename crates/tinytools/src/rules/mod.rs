//! Declarative allow / deny / hide / approval rules over tools.
//!
//! # Why this exists
//!
//! A host restricts tools from many places — an agent definition's
//! allowlist, a channel's permission ceiling, an MCP server's tool filter, a
//! connector's curated actions, a user's toggles — and each used to carry its
//! own exact-name matcher, applied on whichever surface its author thought
//! of. A tool denied by middleware still turned up in tool search; a
//! wildcard worked in one list and not the next.
//!
//! This module is the shared vocabulary for those decisions. The rules are
//! data: the host writes them from its own configuration and threat model,
//! and the harness evaluates them at every surface a tool can reach the
//! model on — the catalogue, search, and the call itself. Nothing here
//! chooses a policy; it only gives every policy the same words.
//!
//! # Shape
//!
//! - A [`ToolRule`] has an [`RuleEffect`], the [`Surface`]s it applies on, a
//!   [`ToolMatcher`] (name, family and tag globs; category; exposure;
//!   permission bounds; declared side effects; one argument), an optional
//!   `except` carve-out and `when` context conditions.
//! - A [`ToolRules`] layer holds rules and a default. Inside a layer effects
//!   combine without regard to order: `deny` beats everything, `hide` only
//!   removes from listings, `require_approval` beats `auto_approve`.
//! - A [`ToolRuleSet`] stacks layers from independent sources; every layer
//!   must admit, so adding one only ever narrows.
//! - A [`ToolSubject`] is what is evaluated: built from a live tool, or by
//!   hand when a host only has a schema.
//!
//! # Example
//!
//! ```
//! use tinytools::{RuleContext, Surface, ToolRules, ToolRuleSet, ToolSubject};
//!
//! let rules: ToolRules = serde_json::from_value(serde_json::json!({
//!     "name": "config",
//!     "rules": [
//!         {
//!             "id": "no-mcp",
//!             "effect": "deny",
//!             "match": { "name": "mcp_*" },
//!             "except": { "family": "github" },
//!             "reason": "Only the GitHub server is approved.",
//!         },
//!         { "effect": "hide", "match": { "name": "gmail_*" }, "on": ["catalog", "search"] },
//!     ],
//! }))?;
//! let set = ToolRuleSet::from(rules);
//! let context = RuleContext::new();
//!
//! let slack = ToolSubject::named("mcp_slack_post_1a2b3c").with_family("slack");
//! let github = ToolSubject::named("mcp_github_issue_4d5e6f").with_family("github");
//! assert!(!set.visible(&slack, &context, Surface::Catalog));
//! assert!(set.visible(&github, &context, Surface::Catalog));
//!
//! // Hidden from listings, still callable.
//! let send = ToolSubject::named("GMAIL_SEND_EMAIL");
//! let decision = set.evaluate(&send, &context, Surface::Call, None);
//! assert!(!set.visible(&send, &context, Surface::Search));
//! assert!(decision.callable);
//! # Ok::<(), serde_json::Error>(())
//! ```

mod eval;
mod glob;
mod subject;
mod types;

pub use glob::glob_matches;
pub use subject::ToolSubject;
pub use types::{
    ApprovalDirective, ArgMatcher, DefaultEffect, Patterns, RuleContext, RuleDecision, RuleEffect,
    RuleRef, SideEffect, Surface, ToolMatcher, ToolRule, ToolRuleSet, ToolRules,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
