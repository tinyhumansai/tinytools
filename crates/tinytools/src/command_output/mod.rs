//! Shared output formatting for shell-family command tools (`shell`,
//! `node_exec`, `npm_exec`).
//!
//! # Why this exists
//!
//! Each shell-family tool previously formatted a FAILED command as
//! `if stderr.is_empty() { stdout } else { stderr }` with no exit code. That
//! lost the two signals the agent most needs to recover:
//!
//!   1. **stdout was dropped whenever stderr was non-empty.** Compilers, test
//!      runners and linters routinely write diagnostics to stdout; the model
//!      saw only stderr and lost half the failure context.
//!   2. **the exit code was never surfaced.** The model could not tell a `127`
//!      (command / dependency not found) or a `126` (permission denied — often a
//!      sandbox restriction) from a generic `1`, so it could not recognise an
//!      un-retryable wall and re-ran the identical command. A host's
//!      repeated-failure circuit breaker (a repeated-failure circuit breaker) still bounds that loop, but
//!      only after a few wasted iterations and with a generic halt message,
//!      because the root-cause signal had already been thrown away.
//!
//! This module surfaces the exit code AND both streams on failure, and appends a
//! short hint for the well-known dependency/sandbox exit codes so the agent can
//! adapt (install/declare the dependency, request escalation, or report the
//! blocker) instead of retrying blindly. The success path is intentionally left
//! as raw stdout so existing callers that parse a command's output (e.g. `pwd`)
//! keep working unchanged.

use crate::ToolResult;

/// Hint appended after the exit code for the exit statuses whose meaning is a
/// shell convention rather than an application's own choice, so the agent can
/// tell "cannot succeed on retry" from "did not actually fail" without
/// guessing. Empty for every other code: an ordinary application failure is
/// never editorialised.
///
/// The `128 + N` codes matter because a host that wraps commands in
/// `set -o pipefail` reports the *pipeline's* signal death, so the most common
/// idiom in an agent's toolkit -- piping a large output into an early-closing
/// reader like `head` -- arrives as a failed command with a bare `141` and no
/// indication that the requested output was in fact delivered in full.
fn exit_code_hint(code: i32) -> &'static str {
    match code {
        127 => {
            " — command not found: a required executable or dependency is \
                 missing or not on PATH. Install/declare it, use an available \
                 alternative, or report the blocker — do NOT re-run the same command"
        }
        126 => {
            " — permission denied or not executable: often a sandbox \
                 restriction. This will not succeed on retry — report the blocker \
                 or request escalation instead of repeating the command"
        }
        141 => {
            " — SIGPIPE: a reader closed the pipe before the writer finished, \
                 which is what `| head`, `| grep -q` and `| sed -n '1,Np'` do \
                 once they have what they asked for. Under `set -o pipefail` \
                 that surfaces as a failed pipeline. The output above is \
                 whatever the reader accepted, and is usually complete for what \
                 was requested — treat it as data, not as an error, and only \
                 re-run without the early-closing reader if you need the rest"
        }
        137 => {
            " — SIGKILL: the process was killed rather than exiting, most often \
                 by the out-of-memory killer or a hard timeout. Retrying the \
                 same command unchanged will be killed again; reduce what it \
                 holds in memory, process the input in parts, or raise the limit"
        }
        143 => {
            " — SIGTERM: the process was asked to stop before it finished, \
                 usually by a timeout or a shutdown. Any output above is \
                 partial. Re-run with a longer timeout or less work per call"
        }
        139 => {
            " — SIGSEGV: the process crashed. This is a fault in the program or \
                 its input, not in how it was invoked, so the same command will \
                 crash again — change the input or use a different tool"
        }
        _ => "",
    }
}

/// Render a finished command's exit status + captured streams into the text the
/// model sees on FAILURE. Never drops a non-empty stream; always states the exit
/// code (or that the process was terminated by a signal).
#[must_use]
pub fn render_command_failure(exit_code: Option<i32>, stdout: &str, stderr: &str) -> String {
    let mut out = match exit_code {
        Some(code) => format!("Command failed (exit code {code}{})", exit_code_hint(code)),
        None => "Command failed (terminated by a signal — no exit code)".to_string(),
    };
    let stdout = stdout.trim_end();
    let stderr = stderr.trim_end();
    if !stdout.is_empty() {
        out.push_str("\n[stdout]\n");
        out.push_str(stdout);
    }
    if !stderr.is_empty() {
        out.push_str("\n[stderr]\n");
        out.push_str(stderr);
    }
    if stdout.is_empty() && stderr.is_empty() {
        out.push_str("\n(no output was captured on stdout or stderr)");
    }
    out
}

/// A `ToolResult::error` carrying [`render_command_failure`]. The single failure
/// constructor shared by every shell-family tool, on both the native and the
/// sandboxed execution path, so the surfaced shape can't drift between them.
#[must_use]
pub fn command_failure(exit_code: Option<i32>, stdout: &str, stderr: &str) -> ToolResult {
    ToolResult::error(render_command_failure(exit_code, stdout, stderr))
}

/// Normalise a sandbox backend's `exit_code` into the `Option<i32>` the
/// formatter expects. The sandbox layer uses `-1` as its sentinel for
/// "terminated / no real exit code" (for example a sandbox `exec` result);
/// map any negative value to `None` so it renders as a signal termination rather
/// than the literal `exit code -1`.
#[must_use]
pub fn sandbox_exit_code(code: i32) -> Option<i32> {
    if code < 0 { None } else { Some(code) }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
