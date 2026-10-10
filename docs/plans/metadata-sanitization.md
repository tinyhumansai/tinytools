# Implement bounded metadata sanitization

Spec: [Sanitization of untrusted metadata](../specs/metadata-sanitization.md).

## Steps

1. Add tests for repeated fence removal, tokens formed across a removed token,
   UTF-8 preservation, and the exact suffix-size boundary.
2. Scan into an output suffix stack, removing any complete fence token at the
   current suffix and rechecking after removal. This preserves arbitrarily
   deep prefixes formed across earlier removals while doing constant work per
   input character for the fixed token catalog.
3. Preserve the documented control-character and UTF-8 truncation behavior,
   including a suffix when the cap exactly equals the suffix length.
4. Run formatting, Clippy, build, and the complete workspace test suite.
