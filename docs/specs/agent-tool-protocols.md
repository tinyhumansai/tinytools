# Agent Tool Protocols

## Purpose

`tinytools-agent` is the single owner of the model-facing tool protocol for
every TinyHumans consumer: TinyAgents (host level), TinyInference (provider
level), and any host that drives its own model loop. It renders what a model
reads, parses what a model writes, repairs what a model damaged, and replays a
transcript onto the wire. See `crates/tinytools-agent/README.md` for the
module map and the grammar table.

## Ownership

| Concern | Owner |
| --- | --- |
| Tool vocabulary (`Tool`, `ToolSpec`, `ToolResult`) | `tinytools` |
| Parsing, repair, streaming scrub, rendering, dialects | `tinytools-agent` |
| Provider wire translation (native `tools` / `tool_calls`, SSE assembly, reasoning side-channels) | `tinyinference-llm` |
| Dialect selection, call-id minting, argument validation policy, execution, unknown-tool policy, re-prompt nudges | the host (`tinyagents-harness`) |

A format-specific string — a DSML marker, a Kimi sentinel, a Harmony channel
token — belongs in exactly one grammar file under
`crates/tinytools-agent/src/parse/grammar/`. A consumer that finds itself
matching one is looking at a bug to fix here.

## Dependency boundary

`tinytools-agent` depends on `tinytools`, `regex`, `serde`, `serde_json` and
optionally `tracing`. It never depends on an inference runtime, a transport,
or TinyAgents. `tinyinference-llm` and `tinyagents-harness` depend on it.

## Compatibility

`parse_tool_calls`, `parse_tool_calls_with_pformat`, `parse_tool_call_value`,
`parse_tool_calls_from_json_value`, `extract_json_values`,
`parse_arguments_value`, `parse_glm_style_tool_calls` and the `dialect` API
keep their signatures. `ParsedToolCall` gained a `source: CallSource` field
and a `new` / `native` constructor; construct it through those.
