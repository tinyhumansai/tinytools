# Tool progress

- **Status:** Implemented
- **Owner:** Maintainers
- **Plan:** [`../plans/tool-progress.md`](../plans/tool-progress.md)

## Problem

A tool that runs for seconds or minutes (a build, a download, a sub-agent) can
say nothing through its `ToolResult` until it returns. Hosts want to show live
status without every tool depending on a particular transport.

## Goals

- A small, optional, transport-free vocabulary a tool uses to report status
  before it finishes.
- A seam on `ToolRunContext` that existing implementors and tools can ignore.

## Non-goals

- Throttling, buffering, persistence, or display of updates (host policy).
- Making progress part of the model-visible result.
- Cancellation or back-pressure.

## Proposed behavior

- `ToolProgress { message, fraction, partial }`: every field optional;
  `ToolProgress::message(..)`, `with_fraction`, `with_partial`, `is_empty`.
  Serialization omits absent fields.
- `fraction` is in `0.0..=1.0`. `with_fraction` clamps out-of-range values and
  treats `NaN` as unset. A value assigned directly to the public field is the
  host's to sanitize.
- `partial` is advisory output; only the final `ToolResult` is guaranteed to
  reach the model.
- `ProgressSink` is a cloneable, thread-safe wrapper over a closure. The
  default and `ProgressSink::noop()` discard every update.
- `ToolRunContext::report_progress(&self, ToolProgress)` has a default no-op
  body, so a tool may report unconditionally and a host that does not stream
  stays correct.

## Invariants and constraints

- Implementations must not block; a tool may call from a hot loop or a spawned
  task.
- Re-entrancy: a host that forwards updates to listeners must document that
  listeners may not call `report_progress` re-entrantly.
- Late updates: an update reported after the call returned is the host's to
  drop; tools must not rely on it landing.
- Adding the method is source-compatible for all existing `ToolRunContext`
  implementors.

## Acceptance criteria

- A bare context ignores progress without panicking.
- A streaming context receives updates through `&dyn ToolRunContext`.
- A sink delivers every update, in order, to its closure; a no-op sink does
  nothing.
- Fractions clamp; `NaN` leaves the field unset; the wire form omits absent
  fields.

## Open questions

None.
