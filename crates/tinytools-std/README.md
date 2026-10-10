# tinytools-std

Host-independent building blocks for agent tools, extracted from OpenHuman.

| Module | What it is |
| --- | --- |
| `file_state` | Process-wide read/write stamps so parallel agents detect stale or partial reads before overwriting a file. The host decides whether the guard is on (`init_global(enabled)`); read tools pass `record_read` an `Instant` captured before their I/O. |
| `url_guard` | URL validation with SSRF checks for outbound network tools. `validate_url_with_dns_check` returns a `ValidatedUrl` whose vetted `addrs` the caller must pin its HTTP client to (e.g. `reqwest`'s `resolve_to_addrs`); re-resolving the hostname reopens DNS rebinding. |
| `filesystem` | The file and repository tools: `file_read`, `file_write`, `edit_file`, `apply_patch`, `grep`, `glob`, `list_files`, `csv_export`, `read_diff`, `git_operations`, `run_linter`, `run_tests`, `update_memory_md`, `image_info`, `read_workspace_state`. Every tool that can touch a path takes an `Arc<dyn FsGate>`: the tool does the I/O, the host's gate (autonomy, workspace boundary, approvals, action budget) decides whether it may. |
| `network` | The network tools: `http_request`, `web_fetch`, `curl`, `pushover`. Every tool takes an `Arc<dyn NetGate>`: the tool does the I/O, the host's gate (autonomy, action budget, approval, privacy mode, proxy) decides whether it may and how. Two smaller seams keep host behavior out: `PaymentHook` answers a `402 Payment Required` for `http_request`, and `HtmlExtractor` converts pages to Markdown for `web_fetch`. `WebFetchTool::new_async` accepts a fallible `AsyncHtmlExtractor` for transforms supplied by a remote service or loadable module; detection and extraction are awaited with the configured request timeout applied to each operation, and failures propagate without a local retry. Both constructors extract before applying the output cap, and skip extraction for raw or explicitly non-HTML responses. Names, descriptions and schemas are pinned by `src/network/fixtures/`. |
| `detect_tools` | `find_on_path` and the read-only `detect_tools` tool. |

No enforcement of host policy lives here; the crate only supplies mechanisms. The `filesystem` tools' name, description and JSON Schema are pinned by the fixtures in `src/filesystem/fixtures/`.
