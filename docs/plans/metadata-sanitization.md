# Implement bounded metadata sanitization

Spec: [Sanitization of untrusted metadata](../specs/metadata-sanitization.md).

## Steps

1. Add tests for repeated fence removal, tokens formed across a removed token,
   UTF-8 preservation, and the exact suffix-size boundary.
2. Replace repeated whole-string rescans with a bounded pending buffer that
   detects tokens and preserves possible prefixes across removals.
3. Preserve the documented control-character and UTF-8 truncation behavior,
   including a suffix when the cap exactly equals the suffix length.
4. Run formatting, Clippy, build, and the complete workspace test suite.
