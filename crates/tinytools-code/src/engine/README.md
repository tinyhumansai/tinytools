# Parser cache

`CodeEngine` stores bounded source snapshots and trees by caller key and language. Unchanged input reuses a tree; changed input receives a minimal UTF-8-safe edit. LRU eviction measures source bytes, not native parser memory. Grammar/query data is compiled and no I/O occurs.
