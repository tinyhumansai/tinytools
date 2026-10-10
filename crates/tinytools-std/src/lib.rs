//! Host-independent building blocks for agent tools.
//!
//! `tinytools` is the vocabulary a tool is written against and deliberately
//! carries no behavior. This crate holds the small, reusable mechanisms that
//! several hosts' tools share, none of which encodes a host's policy:
//!
//! - [`file_state`] — cross-agent read/write stamps and per-path locks, so a
//!   sibling agent's edit is noticed before a stale overwrite.
//! - [`filesystem`] — file read/write/edit/patch, search, git and check-runner
//!   tools, gated by a host-implemented [`filesystem::FsGate`].
//! - [`network`] — `http_request`, `web_fetch`, `curl` and `pushover`, gated by a
//!   host-implemented [`network::NetGate`].
//! - [`url_guard`] — URL validation with SSRF checks, plus DNS resolution
//!   that returns the vetted addresses for the caller to pin its connection to.
//! - [`detect_tools`] — `PATH` probing and the read-only `detect_tools` tool.
//! - [`sanitize`] — lexical sanitization and truncation for untrusted metadata.
//!
//! # Example
//!
//! Probe `PATH` directly, or hand the host the read-only tool that does the
//! same for a model:
//!
//! ```
//! use tinytools::{PermissionLevel, Tool};
//! use tinytools_std::detect_tools::{DetectToolsTool, find_on_path};
//!
//! // A missing binary is `None`, never an error.
//! assert_eq!(find_on_path("definitely-not-a-real-binary-7f3a"), None);
//!
//! let tool = DetectToolsTool::new();
//! assert_eq!(tool.name(), "detect_tools");
//! assert_eq!(tool.permission_level(), PermissionLevel::ReadOnly);
//! ```

pub mod detect_tools;
pub mod file_state;
pub mod filesystem;
pub mod network;
pub mod sanitize;
pub mod url_guard;
