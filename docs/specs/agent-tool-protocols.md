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
| Format vocabulary, dispatcher-name selection and native/XML fallback rule | `tinytools-agent::dialect` |
| Configured dispatcher choice, provider capability lookup, call-id minting, argument validation policy, execution, unknown-tool policy, re-prompt nudges | the host (`tinyagents-harness`) |

A format-specific string — a DSML marker, a Kimi sentinel, a Harmony channel
token — belongs in exactly one grammar file under
`crates/tinytools-agent/src/parse/grammar/`. A consumer that finds itself
matching one is looking at a bug to fix here.

## Dependency boundary

`tinytools-agent` depends on `tinytools`, `regex`, `serde`, `serde_json` and
optionally `tracing`. It never depends on an inference runtime, a transport,
or TinyAgents. `tinyinference-llm` and `tinyagents-harness` depend on it.

## Text dialects

Three text dialects share one parser and one result envelope and differ only
in what the model reads and how it spells a call:

| Dialect | Catalogue | Call | Registry |
| --- | --- | --- | --- |
| `XmlDialect` | full JSON schemas, embedded in the protocol block | `<tool_call>{"name":…,"arguments":{…}}</tool_call>` | no |
| `PFormatDialect` | `name[0\|<a>\|1\|<b>]` signatures in the prompt's tool section | `<tool_call>name[0\|value]</tool_call>` | yes |
| `CodeDialect` | `def name(a: str, b: int = None) -> str` or `function name(a: string, b?: number): string;` signatures in the prompt's tool section | `<tool_call>name(a="value")</tool_call>` or `name({a: "value"})` | yes |

The code dialect exists for small, code-trained models: a function signature
is both shorter than a schema and a form the model has written millions of
times, where P-Format is a syntax it has never seen. Its grammar accepts
literals only, binds positional arguments in the catalogue's order (required
first, then optional alphabetically — the same `from_schema` order P-Format
uses), unpacks a single object argument when every key is a declared
parameter, and refuses whole bodies that contain anything but calls to known
tools. A top-level ```` ```python ```` fence remains an example everywhere in
this crate; only the `<tool_call>` tag and the ```` ```tool_call ```` fence
carry calls.

## Compatibility

`parse_tool_calls`, `parse_tool_calls_with_pformat`, `parse_tool_call_value`,
`parse_tool_calls_from_json_value`, `extract_json_values`,
`parse_arguments_value`, `parse_glm_style_tool_calls` and the `dialect` API
keep their signatures. `ParsedToolCall` gained a `source: CallSource` field
and a `new` / `native` constructor; construct it through those. It later
gained `termination: Termination` (`Terminated` from both constructors,
`EndOfText` for a Harmony payload read to the end of the text, and
`ClosersBalanced` when the JSON repair ladder had to append the closers the
arguments were missing) with a `may_be_truncated()` accessor, so a host can
refuse exactly the calls an output-limit stop may have cut off.
`repair::json::recover_object_tracked` / `recover_whole_object_tracked`
return that same `closers_balanced` signal alongside the object.
`CallSource` and `Termination` are `#[non_exhaustive]`; `dialect::ToolCallFormat` is not, and
gained `Python` and `TypeScript`, so a consumer matching it exhaustively
must add the arms when it takes this version.

## Shared selection and legacy prompt compatibility

The host supplies a dispatcher name and native-tool capability to
ToolCallFormat::resolve. Explicit native, XML, P-Format, Python and TypeScript
choices remain fixed; automatic and unknown names select native when supported
and XML otherwise. Compact/code modes require explicit selection. Hosts retain
ownership of configuration, capability discovery and their defaults; the enum's
existing PFormat default does not impose that default on a host.

The dialect module also owns recognition and heuristic stripping of unexecuted
XML/JSON tool-call text. Marker-free responses remain byte-identical. The
stripper removes XML spans and marker lines, without parsing multiline JSON.

XmlDialect::description_protocol preserves the legacy protocol bytes for hosts
whose catalogues contain descriptions and whose schemas arrive separately.
Full-schema instructions remain unchanged. TinyAgents owns the description
catalogue's framing and verifies the complete frozen channel-prompt output.

Implementation sequence: [shared dialect ownership](../plans/shared-dialect-ownership.md).
