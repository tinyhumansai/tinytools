# Implement asynchronous HTML extraction in `web_fetch`

Spec: [Asynchronous HTML extraction in `web_fetch`](../specs/web-fetch-async-html-extraction.md).

## Steps

1. Define a host-owned `AsyncHtmlExtractor` trait with fallible asynchronous
   detection and conversion methods. Keep the existing synchronous trait.
2. Add an asynchronous provider variant and `WebFetchTool::new_async`, applying
   the resolved timeout to each extractor operation and preserving existing
   domain, status, raw-body, and output-cap behavior.
3. Add deterministic tests for successful remote extraction, detection and
   conversion failures, and timeout behavior. Verify raw responses bypass the
   extractor and the output cap remains post-extraction.
4. Document the public API and accepted timeout, failure, and byte-cap
   semantics in crate and module documentation.
5. Run formatting, Clippy, build, and the complete workspace test suite.
