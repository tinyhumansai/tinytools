# Chunk catalog

Manifests contain typed metadata separately from template source. Paths must be portable and relative; filesystem containment belongs to the adapter. Catalog insertion validates bounded metadata and rejects duplicate IDs. Slot and forbidden-node constraints must name visible, named nodes from the selected grammar; unknown names are errors. Search is deterministic and metadata-only. Only built-in entries expose embedded source.
