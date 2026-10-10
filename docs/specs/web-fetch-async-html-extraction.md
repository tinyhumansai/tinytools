# Asynchronous HTML extraction in `web_fetch`

## Status

Accepted behavior, introduced with `AsyncHtmlExtractor` and
`WebFetchTool::new_async`.

## Context

Some hosts provide HTML detection and Markdown conversion through a separate
service or module. `web_fetch` must support that work without depending on a
particular extractor or runtime implementation.

## Contract

- `AsyncHtmlExtractor` is supplied by the host and provides asynchronous,
  fallible HTML detection and Markdown conversion.
- `WebFetchTool::new_async` uses that extractor for HTML responses. The
  synchronous `new` constructor remains available for local extractors.
- The configured fetch timeout bounds each asynchronous extractor operation
  independently: detection has its own deadline and conversion has its own
  deadline. The operations are not given a shared cumulative budget.
- `None` and zero timeouts use the host's `HttpLimits::timeout_secs` default.
  The same resolved value configures HTTP and both extractor deadlines.
- Extractor failures and timeouts propagate as tool errors. The tool does not
  retry, fall back to another extractor, or return the unconverted markup as
  successful output.
- The `max_bytes` output cap applies after extraction to returned Markdown.
  For raw responses, it applies to the returned response body.
- Converted markup has a separate built-in 8 MiB UTF-8-safe input ceiling
  before extraction. When this ceiling truncates a response, `web_fetch`
  includes `markup_truncated_at=8388608B` in its result header. Raising
  `max_bytes` does not raise this extractor-input ceiling.
- URL validation, domain policy, HTTP status handling, and raw-response
  behavior are otherwise shared with the synchronous constructor.

## Operational limits

The per-operation timeout bounds elapsed time but does not replace host-side
network and resource policy. The built-in markup ceiling bounds extractor
input; hosts that require a smaller budget should enforce it at the network
boundary.

## Related plan

See [the implementation plan](../plans/web-fetch-async-html-extraction.md).
