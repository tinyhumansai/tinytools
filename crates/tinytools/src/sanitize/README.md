# Shared metadata sanitization

`tinytools::sanitize` provides lexical text helpers usable without a runtime or
MCP module: `strip_control_chars`, `strip_instruction_fences`,
`truncate_utf8_safe` and their combined `sanitize_for_llm` pipeline.

The implementation and fixtures moved from TinyMCP without changing processing:
newline/tab preservation, case-insensitive instruction-fence removal, UTF-8 byte
bounds and the ellipsis suffix remain identical. Callers supply byte limits.
TinyMCP retains its description/title caps and its display behavior; hosts may
use this library for generic tool, skill and capability metadata independently
of optional MCP support. Semantic injection detection and approvals remain host
policy. No dependencies were added.
