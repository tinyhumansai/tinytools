# TinyTools Agent Protocols

`tinytools-agent` owns the model-facing protocol around a tool declaration:
tool-call parsing, P-Format signatures, XML and native dialects, catalogue and
result-block rendering, and safe transcript replay.

It consumes `tinytools::ToolSpec` and never executes a tool. Permission checks,
approvals, sandboxing, timeouts, provider requests, and progress reporting
remain responsibilities of the consuming harness or host.

The primary API is:

- root parsing and P-Format helpers;
- `dialect::ToolDialect` and the XML, P-Format, and native implementations;
- provider-neutral transcript, call, result, and message block types.

The crate has no dependency on TinyAgents or an inference/provider runtime.
The optional `tracing` feature emits diagnostics for parser recovery and
transcript repair decisions.
