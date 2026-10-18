#!/usr/bin/env python3
"""Split crates/jcode-tui/src/tui/backend.rs into 6 sibling files under
crates/jcode-tui/src/tui/backend/.

Line ranges were extracted by `grep -n "^\s*pub async fn|^\s*async fn|..."` in
the previous step. See the "Map all top-level functions" output for the
authoritative line table.

This is a one-shot decomposition tool; it is deleted in the same commit as
the file split."""
import os
import sys
from pathlib import Path

ROOT = Path("crates/jcode-tui/src/tui/backend.rs")
OUT = Path("crates/jcode-tui/src/tui/backend")
OUT.mkdir(parents=True, exist_ok=True)

src = ROOT.read_text()
lines = src.splitlines(keepends=True)


def slice_lines(start, end):
    """Return 1-indexed inclusive line range as a string (no trailing newline
    adjustment)."""
    return "".join(lines[start - 1:end])


# ----- debug.rs -----
# 1-22: imports (debug.rs only needs `crate::message::ToolCall` + serde)
# 24-113: DebugEvent
# 115-123: DebugMessage
debug_content = (
    "//! Debug-socket events broadcast by local harnesses.\n"
    "//!\n"
    "//! The full-event [`DebugEvent`] and simplified [`DebugMessage`] payload used\n"
    "//! by the local-harness debug socket. Kept as a small sibling file (~120L)\n"
    "//! so the TUI's other transport types don't have to drag debug-only fields\n"
    "//! (like `Vec<ToolCall>`-bearing snapshots) into their public surface.\n"
    "\n"
    "use crate::message::ToolCall;\n"
    "use serde::{Deserialize, Serialize};\n"
    "\n"
    + slice_lines(24, 123)
)
(OUT / "debug.rs").write_text(debug_content)
print(f"debug.rs: {len(debug_content.splitlines())}L")

# ----- mod.rs -----
# 1-22: imports (full set; sibling files re-use these via `use super::*`)
# 126-304: BackendEvent, RemoteDisconnectReason, RemoteRead, LineOutcome,
#          BackendInfo, RemoteConnection struct, constants, helper,
#          trait RemoteEventState, struct ReplayRemoteState
# 306-421: impl RemoteConnection (connect, connect_with_session)
# 1341-1406: impl RemoteConnection (writer, session_id, dummy,
#            take_dummy_peer, next_request_id_for_test, set_session_id,
#            has_loaded_history, has_buffered_inbound_frame,
#            mark_history_loaded)
# 1448-1492: impl RemoteEventState for RemoteConnection
# 1494-1534: impl RemoteEventState for ReplayRemoteState
mod_doc = (
    "//! Orchestrator for the `backend/` module.\n"
    "//!\n"
    "//! This file used to be a 1996-line `backend.rs`. Per AGENTS.md's 500-line\n"
    "//! hard cap, it is decomposed into 6 sibling files in this directory. The\n"
    "//! `mod.rs` is the only file that owns the public surface of the module -\n"
    "//! every type, function, and impl method that callers depend on is re-exported\n"
    "//! from here. The other 5 sibling files are organized by concern:\n"
    "//!\n"
    "//! - [`debug`] - debug-socket events (`DebugEvent`, `DebugMessage`).\n"
    "//! - [`protocol`] - read pipeline (socket -> `RemoteRead`), write pipeline\n"
    "//!   (`send_request` + interrupt trigger), and the `RemoteEventState` trait\n"
    "//!   plumbing for tool-diff tracking.\n"
    "//! - [`actions`] - content / identity / model-selection requests.\n"
    "//! - [`controls`] - operator / UI plumbing requests (features, compaction,\n"
    "//!   transcript, cancel, auth).\n"
    "//! - [`tests`] - `#[cfg(test)] mod tests` (was 460+ lines, the bulk of the\n"
    "//!   regression coverage).\n"
    "//!\n"
    "//! This orchestrator still owns the `RemoteConnection` struct definition and\n"
    "//! its public lifecycle methods (`connect`, `connect_with_session`, `dummy`,\n"
    "//! `take_dummy_peer`, `writer`, session id accessors, history-load marker),\n"
    "//! since those are the natural cohesion boundary that all sibling files hook\n"
    "//! into via `impl RemoteConnection` blocks. Rust allows impl blocks to be\n"
    "//! split across files in the same module, so each sibling file holds the\n"
    "//! `impl` for its own concern without duplicating struct fields.\n"
    "\n"
    "mod actions;\n"
    "mod controls;\n"
    "mod debug;\n"
    "mod protocol;\n"
    "\n"
    "#[cfg(test)]\n"
    "mod tests;\n"
    "\n"
    "pub use debug::{DebugEvent, DebugMessage};\n"
    "\n"
)
mod_content = (
    mod_doc
    + slice_lines(8, 18)  # imports only (sibling files use `use super::*`)
    + "\n"
    + slice_lines(126, 304)  # BackendEvent, RemoteDisconnectReason, RemoteRead,
                              # LineOutcome, BackendInfo, RemoteConnection struct,
                              # constants, helper, trait, ReplayRemoteState
    + "\n"
    + slice_lines(306, 421)  # impl RemoteConnection (connect, connect_with_session)
    + "\n"
    + slice_lines(1341, 1406)  # impl RemoteConnection (writer, dummy, etc.)
    + "\n"
    + slice_lines(1448, 1492)  # impl RemoteEventState for RemoteConnection
    + "\n"
    + slice_lines(1494, 1534)  # impl RemoteEventState for ReplayRemoteState
)
(OUT / "mod.rs").write_text(mod_content)
print(f"mod.rs: {len(mod_content.splitlines())}L")

# ----- protocol.rs -----
# 423-560: interrupt_request_log_fields, send_request_with_interrupt_trigger,
#          send_request, send_request_detached
# 1142-1339: next_event, take_buffered_line, classify_protocol_line
# 1408-1446: handle_tool_*, get_tool_input, clear_pending,
#            call_output_tokens_seen, reset_call_output_tokens_seen
proto_doc = (
    "//! Read and write pipelines for the remote protocol.\n"
    "//!\n"
    "//! - **Read pipeline**: `next_event`, `take_buffered_line`,\n"
    "//!   `classify_protocol_line` turn a `ReadHalf` socket into a stream of\n"
    "//!   `RemoteRead` values while staying cancellation-safe across `select!`\n"
    "//!   peers.\n"
    "//! - **Write pipeline**: `send_request_with_interrupt_trigger`,\n"
    "//!   `send_request`, `send_request_detached` write JSON-request frames to\n"
    "//!   the shared writer and tag interrupt-class requests with their UI\n"
    "//!   trigger so server-side logs can correlate them.\n"
    "//! - **Tool-diff state machine**: `handle_tool_*`, `get_tool_input`,\n"
    "//!   `clear_pending`, `call_output_tokens_seen`,\n"
    "//!   `reset_call_output_tokens_seen` forward into the\n"
    "//!   `RemoteDiffTracker`.\n"
    "//!\n"
    "//! Kept as a sibling of `mod.rs` so the framing / classification logic is\n"
    "//! not buried inside the orchestrator that owns the connection state.\n"
    "\n"
    "use super::{\n"
    "    DETACHED_REQUEST_TIMEOUT, LineOutcome, MAX_STRAY_REMOTE_PROTOCOL_LINES,\n"
    "    READ_BUFFER_RETAIN_CAPACITY, READ_BUFFER_SHRINK_THRESHOLD,\n"
    "    RemoteConnection, RemoteDisconnectReason, RemoteRead,\n"
    "    remote_protocol_frame_exceeds_limit,\n"
    "};\n"
    "use crate::protocol::{Request, ServerEvent};\n"
    "use anyhow::Result;\n"
    "use std::time::Instant;\n"
    "use tokio::io::AsyncWriteExt;\n"
    "\n"
)
proto_content = (
    proto_doc
    + slice_lines(423, 560)
    + "\n"
    + slice_lines(1142, 1339)
    + "\n"
    + slice_lines(1408, 1446)
)
(OUT / "protocol.rs").write_text(proto_content)
print(f"protocol.rs: {len(proto_content.splitlines())}L")

# ----- actions.rs -----
# 562-854: send_message + variants, clear, reload, resume_all_sessions,
#          invalidate_openai_usage, request_history, request_model_catalog,
#          resume_session, get_compacted_history, rewind, rewind_undo,
#          cycle_model, refresh_models, set_model, set_route_selection,
#          set_subagent_model, run_subagent, set_premium_mode,
#          set_reasoning_effort, set_service_tier, set_transport
actions_doc = (
    "//! Session content / identity / model-selection requests.\n"
    "//!\n"
    "//! These are the operator-driven mutations to a session: posting messages\n"
    "//! and inline images, clearing, reloading, resuming, requesting history /\n"
    "//! model catalog, history rewinds, model selection, route selection, model\n"
    "//! selection for subagents, premium mode, reasoning effort, service tier,\n"
    "//! transport.\n"
    "//!\n"
    "//! Separated from `controls.rs` so the session-payload surface can evolve\n"
    "//! independently from the UI plumbing in `controls.rs`.\n"
    "\n"
    "use super::RemoteConnection;\n"
    "use crate::protocol::Request;\n"
    "use anyhow::Result;\n"
    "\n"
    "impl RemoteConnection {\n"
)
actions_content = (
    actions_doc
    + slice_lines(562, 854)
    + "}\n"
)
(OUT / "actions.rs").write_text(actions_content)
print(f"actions.rs: {len(actions_content.splitlines())}L")

# ----- controls.rs -----
# 856-1126: set_feature, set_compaction_mode, set_session_saved,
#           rename_session, send_transcript, send_input_shell,
#           send_stdin_response, cancel*, background_tool, soft_interrupt*,
#           split, transfer, compact, trigger_memory_extraction,
#           notify_auth_changed*, switch_anthropic_account,
#           switch_openai_account, send_client_debug_response
controls_doc = (
    "//! Control-plane requests on [`super::RemoteConnection`].\n"
    "//!\n"
    "//! Operator/UI plumbing requests that aren't session content: feature toggles,\n"
    "//! compaction / saved-state, transcript injection, shell + stdin plumbing,\n"
    "//! cancel / interrupt / background / handoff, auth account switching, and\n"
    "//! client debug responses.\n"
    "//!\n"
    "//! These are deliberately separated from `actions.rs` (which holds content /\n"
    "//! identity / model selection) so that session-payload changes and UI\n"
    "//! plumbing can evolve independently.\n"
    "\n"
    "use super::RemoteConnection;\n"
    "use crate::protocol::{AuthChanged, FeatureToggle, Request};\n"
    "use anyhow::Result;\n"
    "\n"
    "impl RemoteConnection {\n"
)
controls_content = (
    controls_doc
    + slice_lines(856, 1126)
    + "}\n"
)
(OUT / "controls.rs").write_text(controls_content)
print(f"controls.rs: {len(controls_content.splitlines())}L")

# ----- tests.rs -----
# 1536-1996: all tests
tests_doc = (
    "//! Tests for [`super::RemoteConnection`] and its protocol plumbing.\n"
    "//!\n"
    "//! These were originally a `#[cfg(test)] mod tests` block inside the\n"
    "//! 1996-line `backend.rs` file; they live here as a sibling so each file in\n"
    "//! the `backend/` directory stays under the AGENTS.md 500-line hard cap.\n"
    "//!\n"
    "//! The tests rely on the test-only `dummy()` constructor (which produces a\n"
    "//! in-memory socketpair) and on direct access to private fields such as\n"
    "//! `_dummy_peer` and `next_request_id`. That access is preserved because this\n"
    "//! module is a child of `backend`, so the parent's private items remain\n"
    "//! in-scope via `use super::*;`.\n"
    "\n"
    "use super::*;\n"
    "use std::time::Duration;\n"
    "\n"
)
tests_content = (
    tests_doc
    + slice_lines(1536, 1996)
)
(OUT / "tests.rs").write_text(tests_content)
print(f"tests.rs: {len(tests_content.splitlines())}L")

print("\nDone. Next: `git rm backend.rs` and stage the 6 new files.")
