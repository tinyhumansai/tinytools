# Agent Tool Protocols

## Purpose

TinyTools separates two reusable concerns:

- `tinytools` describes callable tools and their results;
- `tinytools-agent` translates those declarations and results to and from the
  protocols models use during an agent loop.

## Ownership

`tinytools-agent` owns tolerant call parsing, P-Format registry construction,
XML/P-Format/native dialects, catalogue and result-block rendering, and
provider-neutral transcript replay repair.

It does not own tool execution, permission or approval policy, sandboxing,
timeouts, provider transports, or an agent loop.

## Dependency boundary

The crate consumes `tinytools::ToolSpec`. It must not depend on TinyAgents or a
provider runtime. A harness maps its message and response types onto the thin
`DialectResponse`, `TranscriptEntry`, and `DialectMessage` types.

## Compatibility

TinyAgents may re-export this API from its historical
`tinyagents_harness::tool_calling` module. That is a compatibility adapter, not
the implementation owner.
