//! Metadata-first built-in and custom chunk discovery.

mod builtins;
mod types;
mod validation;

use crate::{
    CodeError, Language,
    analysis::bounded_text,
    error::{enforce_limit, invalid},
    revision,
};
use std::collections::BTreeMap;
pub use types::{
    ChunkManifest, ChunkParameter, ChunkSlot, ChunkSummary, ChunkTarget, ParameterKind,
};
pub(crate) use validation::identifier;

#[derive(Debug, Clone)]
struct Entry {
    manifest: ChunkManifest,
    source: Option<&'static str>,
}

/// A bounded catalog whose search results never expose template bodies.
#[derive(Debug, Clone, Default)]
pub struct ChunkCatalog {
    entries: BTreeMap<String, Entry>,
}

impl ChunkManifest {
    /// Check metadata, defaults, portable paths and bounded list sizes.
    ///
    /// # Errors
    /// Refuses invalid paths, duplicate names, unknown kinds or exceeded bounds.
    pub fn validate(&self) -> Result<(), CodeError> {
        validation::validate(self)
    }

    /// Revision of validated, normalized metadata; template content is separate.
    ///
    /// # Errors
    /// Refuses invalid metadata or serialization failure.
    pub fn revision(&self) -> Result<String, CodeError> {
        self.validate()?;
        let serialized = serde_json::to_string(self).map_err(|error| invalid(error.to_string()))?;
        Ok(revision(&serialized))
    }
}

impl ChunkCatalog {
    /// Create an empty catalog for caller-supplied manifests.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the twelve shipped patterns without filesystem access.
    ///
    /// # Errors
    /// Refuses inconsistent built-in metadata.
    pub fn builtin() -> Result<Self, CodeError> {
        let manifests: Vec<ChunkManifest> =
            serde_json::from_str(include_str!("starter-manifests.json"))
                .map_err(|error| invalid(error.to_string()))?;
        let mut catalog = Self::new();
        for manifest in manifests {
            let source = builtins::source(&manifest.id);
            manifest.validate()?;
            catalog
                .entries
                .insert(manifest.id.clone(), Entry { manifest, source });
        }
        Ok(catalog)
    }

    /// Insert validated custom metadata without granting access to built-in source.
    ///
    /// # Errors
    /// Refuses invalid manifests, duplicate IDs or catalogs above 512 entries.
    pub fn insert(&mut self, manifest: ChunkManifest) -> Result<(), CodeError> {
        manifest.validate()?;
        if self.entries.contains_key(&manifest.id) {
            return Err(invalid("duplicate chunk ID"));
        }
        enforce_limit("catalog entries", self.entries.len() + 1, 512)?;
        self.entries.insert(
            manifest.id.clone(),
            Entry {
                manifest,
                source: None,
            },
        );
        Ok(())
    }

    /// Inspect one metadata record.
    ///
    /// # Errors
    /// Refuses oversized or unknown IDs.
    pub fn manifest(&self, id: &str) -> Result<&ChunkManifest, CodeError> {
        enforce_limit("chunk ID bytes", id.len(), 128)?;
        self.entries
            .get(id)
            .map(|entry| &entry.manifest)
            .ok_or_else(|| CodeError::UnknownChunk(id.to_owned()))
    }

    /// Alias for metadata inspection.
    ///
    /// # Errors
    /// Refuses oversized or unknown IDs.
    pub fn inspect(&self, id: &str) -> Result<&ChunkManifest, CodeError> {
        self.manifest(id)
    }

    /// Return source only for entries loaded by [`Self::builtin`].
    #[must_use]
    pub fn builtin_source(&self, id: &str) -> Option<&'static str> {
        self.entries.get(id).and_then(|entry| entry.source)
    }

    /// Search bounded metadata terms with deterministic score/ID ordering.
    ///
    /// Query text is capped at 1024 bytes and 32 terms; at most 50 hits return.
    #[must_use]
    pub fn search(
        &self,
        query: &str,
        language: Option<Language>,
        limit: usize,
    ) -> Vec<ChunkSummary> {
        let query = bounded_text(query, 1024).0.to_ascii_lowercase();
        let terms: Vec<_> = query.split_whitespace().take(32).collect();
        let mut hits: Vec<_> = self
            .entries
            .values()
            .filter(|entry| language.is_none_or(|language| language == entry.manifest.language))
            .filter_map(|entry| {
                let manifest = &entry.manifest;
                let searchable = format!(
                    "{} {} {}",
                    manifest.id,
                    manifest.summary,
                    manifest.tags.join(" ")
                )
                .to_ascii_lowercase();
                let score = terms
                    .iter()
                    .filter(|term| searchable.contains(**term))
                    .count();
                (terms.is_empty() || score > 0).then_some((score, manifest))
            })
            .collect();
        hits.sort_by(|(score_a, a), (score_b, b)| {
            score_b.cmp(score_a).then_with(|| a.id.cmp(&b.id))
        });
        hits.into_iter()
            .take(limit.min(50))
            .map(|(_, manifest)| ChunkSummary {
                id: manifest.id.clone(),
                summary: manifest.summary.clone(),
                language: manifest.language,
                target: manifest.target,
                tags: manifest.tags.clone(),
            })
            .collect()
    }
}
