//! Parser and retained-source budgets, with observable cache outcomes.

use crate::{
    CodeError,
    error::{enforce_limit, invalid},
};
use serde::{Deserialize, Serialize};

/// Resource bounds applied before parsing or expanding source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeLimits {
    /// Maximum bytes in one source or rendered result.
    pub max_source_bytes: usize,
    /// Maximum cached parse trees; zero disables caching.
    pub max_cache_entries: usize,
    /// Maximum retained source bytes; excludes native tree allocations.
    pub max_cache_bytes: usize,
    /// Maximum symbols and references, each independently bounded.
    pub max_records: usize,
    /// Maximum syntax diagnostics.
    pub max_diagnostics: usize,
}

impl Default for CodeLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 1_048_576,
            max_cache_entries: 32,
            max_cache_bytes: 8_388_608,
            max_records: 2048,
            max_diagnostics: 64,
        }
    }
}

impl CodeLimits {
    pub(crate) fn validate(self) -> Result<(), CodeError> {
        enforce_limit("source budget", self.max_source_bytes, 16_777_216)?;
        enforce_limit("cache entry budget", self.max_cache_entries, 1024)?;
        enforce_limit("cache byte budget", self.max_cache_bytes, 134_217_728)?;
        enforce_limit("record budget", self.max_records, 65_536)?;
        enforce_limit("diagnostic budget", self.max_diagnostics, 4096)?;
        if self.max_source_bytes == 0 || self.max_records == 0 || self.max_diagnostics == 0 {
            return Err(invalid(
                "source, record and diagnostic budgets must be positive",
            ));
        }
        Ok(())
    }
}

/// How this particular analysis obtained its parse tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    /// No reusable cached tree was available.
    Fresh,
    /// Source was identical to its cached snapshot.
    Cached,
    /// A prior tree was edited and incrementally reparsed.
    Incremental,
}

/// Cumulative cache counters plus current retained-source usage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStats {
    /// Currently retained entries.
    pub entries: usize,
    /// Currently retained UTF-8 source bytes.
    pub source_bytes: usize,
    /// Unchanged-source hits.
    pub hits: u64,
    /// Incremental reparses.
    pub incremental_parses: u64,
    /// Parses without a prior tree.
    pub cold_parses: u64,
    /// Entries evicted to meet budgets.
    pub evictions: u64,
}
