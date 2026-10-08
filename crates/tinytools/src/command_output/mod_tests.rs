//! Tests for command-failure rendering.

use super::*;

#[test]
fn failure_surfaces_exit_code_and_both_streams() {
    let rendered = render_command_failure(Some(7), "the-stdout-line", "the-stderr-line");
    assert!(
        rendered.contains("exit code 7"),
        "exit code missing: {rendered}"
    );
    assert!(
        rendered.contains("the-stdout-line"),
        "stdout dropped on failure: {rendered}"
    );
    assert!(
        rendered.contains("the-stderr-line"),
        "stderr dropped on failure: {rendered}"
    );
}

#[test]
fn failure_keeps_stdout_even_when_stderr_present() {
    // The exact regression: the old `if stderr.is_empty() { stdout } else
    // { stderr }` formatting threw stdout away whenever stderr existed.
    let rendered = render_command_failure(Some(1), "diagnostic-on-stdout", "error-on-stderr");
    assert!(rendered.contains("diagnostic-on-stdout"));
    assert!(rendered.contains("error-on-stderr"));
}

#[test]
fn exit_127_hints_missing_command_or_dependency() {
    let rendered = render_command_failure(Some(127), "", "pytest: command not found");
    assert!(rendered.contains("exit code 127"));
    assert!(
        rendered.to_lowercase().contains("command not found"),
        "127 should hint at a missing command/dependency: {rendered}"
    );
}

#[test]
fn exit_126_hints_permission_or_sandbox() {
    let rendered = render_command_failure(Some(126), "", "permission denied");
    assert!(rendered.contains("exit code 126"));
    assert!(
        rendered.to_lowercase().contains("sandbox")
            || rendered.to_lowercase().contains("permission denied"),
        "126 should hint at a permission/sandbox wall: {rendered}"
    );
}

#[test]
fn ordinary_failure_code_gets_no_hint() {
    let rendered = render_command_failure(Some(1), "", "boom");
    // No editorialising for a generic application failure.
    assert!(rendered.contains("exit code 1"));
    assert!(!rendered.contains("command not found"));
    assert!(!rendered.contains("sandbox"));
}

#[test]
fn signal_termination_has_no_exit_code() {
    let rendered = render_command_failure(None, "", "");
    assert!(rendered.contains("terminated by a signal"));
    assert!(rendered.contains("no output was captured"));
}

#[test]
fn sandbox_negative_exit_code_maps_to_signal() {
    assert_eq!(sandbox_exit_code(-1), None);
    assert_eq!(sandbox_exit_code(0), Some(0));
    assert_eq!(sandbox_exit_code(7), Some(7));
}

/// A host that wraps commands in `set -o pipefail` turns the commonest idiom
/// in an agent's toolkit -- a large output piped into an early-closing reader
/// -- into a failed command. The code alone does not say the requested output
/// arrived, so the hint has to.
#[test]
fn exit_141_explains_that_sigpipe_is_usually_not_a_failure() {
    let rendered = render_command_failure(Some(141), "first-five-lines", "");
    assert!(rendered.contains("exit code 141"));
    assert!(rendered.contains("SIGPIPE"));
    assert!(
        rendered.contains("head"),
        "141 should name the idiom that causes it: {rendered}"
    );
    assert!(
        rendered.contains("treat it as data, not as an error"),
        "141 should say the output is usable: {rendered}"
    );
    assert!(
        rendered.contains("first-five-lines"),
        "the accepted output must still be shown: {rendered}"
    );
}

/// 137 and 143 are the two ways a long agent run loses a process to the host
/// rather than to its own exit, and they need opposite responses from 141:
/// the output is gone or partial, and an unchanged retry repeats the kill.
#[test]
fn killed_and_terminated_codes_hint_at_the_host_not_the_command() {
    let killed = render_command_failure(Some(137), "", "");
    assert!(killed.contains("SIGKILL"));
    assert!(
        killed.contains("out-of-memory") && killed.contains("timeout"),
        "137 should name both usual causes: {killed}"
    );
    let terminated = render_command_failure(Some(143), "half-the-output", "");
    assert!(terminated.contains("SIGTERM"));
    assert!(
        terminated.contains("partial"),
        "143 should warn the output is incomplete: {terminated}"
    );
}

#[test]
fn exit_139_hints_a_crash_rather_than_a_bad_invocation() {
    let rendered = render_command_failure(Some(139), "", "");
    assert!(rendered.contains("SIGSEGV"));
    assert!(
        rendered.contains("not in how it was invoked"),
        "139 should steer away from re-tuning the flags: {rendered}"
    );
}

/// The signal hints must not leak into an ordinary application failure, and a
/// code that merely looks adjacent (140, 142, 138) is not a convention.
#[test]
fn adjacent_signal_codes_are_not_editorialised() {
    for code in [138, 140, 142, 2] {
        let rendered = render_command_failure(Some(code), "", "boom");
        assert!(
            !rendered.contains("SIG"),
            "exit {code} is not a known convention: {rendered}"
        );
    }
}
