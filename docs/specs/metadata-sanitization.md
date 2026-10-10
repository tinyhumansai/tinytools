# Sanitization of untrusted metadata

## Status

Accepted behavior for the shared `tinytools_std::sanitize` text pipeline.

## Contract

The pipeline accepts untrusted free-form tool, skill, and capability metadata
and returns text suitable for inclusion in an agent's tool-use context. It
applies these transformations in order:

1. Remove ASCII control characters except newline and tab.
2. Remove the known instruction-fence tokens listed in
   `INSTRUCTION_FENCE_TOKENS`, matching ASCII case-insensitively. Continue
   recognizing tokens formed when an earlier token is removed and its
   neighboring text joins.
3. Truncate at a UTF-8 character boundary to the caller's byte cap. If the
   input is shortened and the cap can hold the ellipsis suffix, include it
   within that cap. If the cap is smaller than the suffix, truncate without
   the suffix.

The fence-removal scan must have linear work in input length for the fixed
token catalog, including inputs containing many fence tokens. Sanitization is
lexical filtering; it does not detect semantic prompt injection or replace
host policy.

## Related plan

See [the implementation plan](../plans/metadata-sanitization.md).
