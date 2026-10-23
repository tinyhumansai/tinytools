//! Bounded LRU source snapshots and reusable incremental syntax trees.

mod edit;
mod types;

use crate::{Analysis, CodeError, Language, analysis, error::enforce_limit};
use std::collections::{HashMap, VecDeque};
use tree_sitter::{Parser, Query, Tree};
pub use types::{CacheStats, CacheStatus, CodeLimits};

#[derive(Debug)]
struct CachedSource {
    key: String,
    language: Language,
    source: String,
    tree: Tree,
}

/// A pure syntax engine with bounded, per-key retained source snapshots.
#[derive(Debug, Default)]
pub struct CodeEngine {
    limits: CodeLimits,
    cache: VecDeque<CachedSource>,
    queries: HashMap<Language, Query>,
    stats: CacheStats,
}

/// Compute the content revision used for source, manifest and template guards.
#[must_use]
pub fn revision(source: &str) -> String {
    blake3::hash(source.as_bytes()).to_hex().to_string()
}

impl CodeEngine {
    /// Create an engine with explicitly bounded resources.
    ///
    /// # Errors
    /// Returns an error for zero required budgets or budgets above hard caps.
    pub fn new(limits: CodeLimits) -> Result<Self, CodeError> {
        limits.validate()?;
        Ok(Self {
            limits,
            ..Self::default()
        })
    }

    /// Active resource budgets.
    #[must_use]
    pub fn limits(&self) -> CodeLimits {
        self.limits
    }

    /// Current usage and cumulative parser reuse counters.
    #[must_use]
    pub fn cache_stats(&self) -> CacheStats {
        self.stats
    }

    /// Drop retained source and trees, preserving cumulative counters.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.stats.entries = 0;
        self.stats.source_bytes = 0;
    }

    /// Analyze one caller-keyed snapshot with an explicit grammar.
    ///
    /// # Errors
    /// Refuses oversized source/keys or unavailable compiled grammar/query data.
    /// Invalid syntax is returned as diagnostics, not as an operation error.
    pub fn analyze(
        &mut self,
        key: &str,
        language: Language,
        source: &str,
    ) -> Result<Analysis, CodeError> {
        enforce_limit("cache key bytes", key.len(), 4096)?;
        enforce_limit("source bytes", source.len(), self.limits.max_source_bytes)?;
        let (tree, status) = self.cached_parse(key, language, source)?;
        let query = match self.queries.entry(language) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(
                Query::new(&language.grammar(), language.query())
                    .map_err(|error| CodeError::Parser(error.to_string()))?,
            ),
        };
        Ok(analysis::analyze(
            &tree,
            query,
            language,
            source,
            self.limits,
            status,
        ))
    }

    /// Validate complete source without retaining it in the cache.
    ///
    /// # Errors
    /// Refuses source beyond the configured budget or any missing/error node.
    pub fn validate(&self, language: Language, source: &str) -> Result<(), CodeError> {
        let tree = self.parse(language, source)?;
        if tree.root_node().has_error() {
            Err(CodeError::InvalidSyntax { language })
        } else {
            Ok(())
        }
    }

    pub(crate) fn parse(&self, language: Language, source: &str) -> Result<Tree, CodeError> {
        enforce_limit("source bytes", source.len(), self.limits.max_source_bytes)?;
        parse(language, source, None)
    }

    fn cached_parse(
        &mut self,
        key: &str,
        language: Language,
        source: &str,
    ) -> Result<(Tree, CacheStatus), CodeError> {
        let position = self
            .cache
            .iter()
            .position(|entry| entry.key == key && entry.language == language);
        let previous = position.and_then(|index| self.cache.remove(index));
        let (tree, status) = if let Some(mut previous) = previous {
            self.stats.source_bytes -= previous.source.len();
            if previous.source == source {
                self.stats.hits += 1;
                (previous.tree, CacheStatus::Cached)
            } else {
                previous
                    .tree
                    .edit(&edit::difference(&previous.source, source));
                self.stats.incremental_parses += 1;
                (
                    parse(language, source, Some(&previous.tree))?,
                    CacheStatus::Incremental,
                )
            }
        } else {
            self.stats.cold_parses += 1;
            (parse(language, source, None)?, CacheStatus::Fresh)
        };
        if self.limits.max_cache_entries > 0
            && self.limits.max_cache_bytes > 0
            && source.len() <= self.limits.max_cache_bytes
        {
            while self.cache.len() >= self.limits.max_cache_entries
                || self.stats.source_bytes + source.len() > self.limits.max_cache_bytes
            {
                if let Some(oldest) = self.cache.pop_front() {
                    self.stats.source_bytes -= oldest.source.len();
                    self.stats.evictions += 1;
                }
            }
            self.stats.source_bytes += source.len();
            self.cache.push_back(CachedSource {
                key: key.to_owned(),
                language,
                source: source.to_owned(),
                tree: tree.clone(),
            });
        }
        self.stats.entries = self.cache.len();
        Ok((tree, status))
    }
}

fn parse(language: Language, source: &str, old: Option<&Tree>) -> Result<Tree, CodeError> {
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar())
        .map_err(|error| CodeError::Parser(error.to_string()))?;
    parser
        .parse(source, old)
        .ok_or_else(|| CodeError::Parser("parsing returned no tree".to_owned()))
}
