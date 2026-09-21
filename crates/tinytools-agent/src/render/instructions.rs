//! The protocol block each dialect puts in the system prompt.
//!
//! One place, so the wording a model reads and the grammar the parser
//! expects cannot drift apart. The JSON block embeds its catalogue because the
//! schemas *are* the protocol for a model writing argument names by hand; the
//! P-Format and code blocks do not, because their signatures live in the
//! prompt's tool section next to the descriptions; the native block carries no
//! catalogue at all, because the request does.

use tinytools::ToolSpec;

use super::catalogue::render_json_catalogue;
use crate::codecall::CodeStyle;

/// The JSON-in-tag protocol block plus the full-schema catalogue.
#[must_use]
pub fn json_instructions(tools: &[ToolSpec]) -> String {
    let mut out = String::new();
    out.push_str("## Tool Use Protocol\n\n");
    out.push_str("To use a tool, wrap a JSON object in <tool_call></tool_call> tags:\n\n");
    out.push_str(
        "```\n<tool_call>\n{\"name\": \"tool_name\", \"arguments\": {\"param\": \"value\"}}\n</tool_call>\n```\n\n",
    );
    out.push_str("You may emit multiple tool calls in a single response. ");
    out.push_str("After execution, results appear in <tool_result> tags. ");
    out.push_str("Continue reasoning with the results until you can give a final answer.\n\n");
    out.push_str("### Available Tools\n\n");
    out.push_str(&render_json_catalogue(tools));
    out
}

/// The P-Format protocol block — protocol only, no catalogue.
#[must_use]
pub fn pformat_instructions() -> String {
    let mut out = String::new();
    out.push_str("## Tool Use Protocol\n\n");
    out.push_str(
        "Tool calls use **P-Format** (Parameter-Format): compact, slot-indexed, \
         pipe-delimited syntax wrapped in `<tool_call>` tags. ~80% cheaper on tokens \
         than JSON.\n\n",
    );
    out.push_str("```\n<tool_call>\nget_weather[0|London|1|metric]\n</tool_call>\n```\n\n");
    out.push_str(
        "**Rules:**\n\
         - Form: `name[index|value|index|value|...]`. Each value is preceded by the slot \
           number it fills, taken from that tool's `Call as:` signature in the `## Tools` \
           section above.\n\
         - **Send only the arguments you mean to send.** To pass just the third slot, \
           write `name[2|value]` — there are no empty slots to count.\n\
         - The signature shows each slot as `index|<name>`, e.g. \
           `search[0|<query>|1|<limit>]`. `<name>` is a placeholder: replace it with the \
           value, and do not send the name itself.\n\
         - Empty calls: `name[]` for zero-arg tools, or for a call sending no arguments.\n\
         - A call whose indices are missing, non-numeric, or not in the signature is \
           **rejected** — it will not run. Copy the numbers from the signature.\n\
         - Escapes inside argument values: `\\|` → `|`, `\\]` → `]`, `\\\\` → `\\`.\n\
         - You may emit multiple `<tool_call>` blocks in a single response. Each tag holds \
           exactly one call.\n\
         - After tool execution, results appear in `<tool_result>` tags. Continue reasoning \
           with the results until you can give a final answer.\n\
         - If you genuinely need a complex nested argument that p-format can't express, \
           you may fall back to the JSON form: \
           `<tool_call>{\"name\":\"...\",\"arguments\":{...}}</tool_call>`. Prefer p-format \
           for everything else.\n\n",
    );
    out
}

/// The native protocol block: behavioural guidance only.
#[must_use]
pub fn native_instructions() -> String {
    [
        "## Tool Use Protocol",
        "",
        "When a tool is needed, emit tool calls directly via the model's native tool-calling output.",
        "Do not only narrate intent (for example, avoid \"Let me check...\") without emitting the tool call.",
        "After tool results are provided, continue reasoning and then produce the final answer.",
        "",
    ]
    .join("\n")
}

/// The code-call protocol block — protocol only, no catalogue.
///
/// Short on purpose: the whole point of the dialect is that a code-trained
/// model already knows how to write a function call, so the block only has to
/// say where to put it and what a value may be.
#[must_use]
pub fn code_instructions(style: CodeStyle) -> String {
    let mut out = String::new();
    out.push_str("## Tool Use Protocol\n\n");
    match style {
        CodeStyle::Python => {
            out.push_str(
                "Call a tool by writing a Python function call inside `<tool_call>` tags, \
                 one call per line:\n\n",
            );
            out.push_str("```\n<tool_call>\nread_file(path=\"src/main.rs\", limit=20)\n</tool_call>\n```\n\n");
            out.push_str(
                "- Use the signatures in `## Tools`. Prefer keyword arguments; positional \
                 arguments follow the signature order.\n\
                 - Values are Python literals only: quoted strings, numbers, True/False/None, \
                 lists, dicts. Omit optional arguments you do not need.\n",
            );
        }
        CodeStyle::TypeScript => {
            out.push_str(
                "Call a tool by writing a function call inside `<tool_call>` tags, one call \
                 per line, passing the arguments as one object:\n\n",
            );
            out.push_str(
                "```\n<tool_call>\nread_file({path: \"src/main.rs\", limit: 20})\n</tool_call>\n```\n\n",
            );
            out.push_str(
                "- Use the signatures in `## Tools`. Keys are the parameter names; positional \
                 arguments in signature order also work.\n\
                 - Values are literals only: quoted strings, numbers, true/false/null, arrays, \
                 objects. Omit optional arguments you do not need.\n",
            );
        }
    }
    out.push_str(
        "- Write only calls inside the tags: no prose, no code fences. For several calls, \
         use one line each or one `<tool_call>` block each.\n\
         - After execution, results appear in `<tool_result>` tags. Continue reasoning with \
         the results until you can give a final answer.\n\n",
    );
    out
}
