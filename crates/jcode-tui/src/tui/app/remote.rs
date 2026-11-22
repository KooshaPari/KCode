#![cfg_attr(test, allow(clippy::items_after_test_module))]

use super::{
    App, DisplayMessage, PendingReloadReconnectStatus, ProcessingStatus, RemoteResumeActivity,
    SendAction, ctrl_bracket_fallback_to_esc, input, parse_rate_limit_error,
    remote_notifications::present_swarm_notification, spawn_in_new_terminal,
};
use crate::bus::BusEvent;
use crate::message::ToolCall;
use crate::protocol::{ServerEvent, TranscriptMode};
use crate::tui::backend::{RemoteConnection, RemoteDisconnectReason, RemoteEventState, RemoteRead};
use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::{DefaultTerminal, Terminal, backend::Backend};
use std::time::{Duration, Instant};

mod input_dispatch;
mod key_handling;
mod queue_recovery;
mod reconnect;
mod server_event_handlers;
mod server_events;
mod session_persistence;
mod swarm_plan_core;
mod swarm_status_core;
mod version_ordering;
mod workspace;
#[cfg(test)]
pub(super) use key_handling::reload_stale_remote_server_before_update;
use queue_recovery::{
    recover_local_interleave_to_queue, recover_rejected_queued_continuation,
    recover_stranded_soft_interrupts, recover_undelivered_queued_continuation,
    retain_pending_ssh_soft_interrupts,
};
// Re-export for sibling modules and tests that access reconnect state and helpers
// through `super::remote::*` without reaching into private submodules directly.
#[allow(unused_imports)]
pub(super) use reconnect::{
    ConnectOutcome, PostConnectOutcome, ReloadReconnectHints, RemoteRunState, connect_with_retry,
    finalize_reload_reconnect, handle_post_connect, reload_handoff_active,
    should_allow_reconnect_takeover, should_use_same_session_fast_path,
};
use reconnect::{format_disconnect_reason, reconnect_status_message};
pub(super) use reconnect::has_unresolved_for_session;
pub(super) use reconnect::pause_after_protocol_error;
use session_persistence::{
    persist_remote_session_metadata, persist_replay_display_message, persist_swarm_plan_snapshot,
    persist_swarm_status_snapshot,
};
use workspace::{handle_workspace_command, handle_workspace_navigation_key};

// Re-export remote input dispatch helpers for sibling modules/tests.
#[allow(unused_imports)]
pub(super) use input_dispatch::{
    apply_remote_transcript_event, apply_transcript_event, begin_remote_send,
    begin_remote_split_launch, finish_remote_split_launch, history_matches_pending_startup_prompt,
    route_prepared_input_to_new_remote_session, stage_turn_for_remote_tick_loop,
    submit_prepared_remote_input, submit_remote_slash_input, submit_remote_voice_transcript,
    submit_voice_transcript,
};
pub(super) use key_handling::{
    handle_remote_char_input, handle_remote_key, handle_remote_key_event, send_interleave_now,
};
pub(super) use server_events::handle_server_event;

const CONNECTION_MESSAGE_TITLE: &str = "Connection";
const RELOAD_MARKER_MAX_AGE: Duration = Duration::from_secs(30);


#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;

include!("remote/loop_part_0.rs");
include!("remote/loop_part_1.rs");
include!("remote/loop_part_2.rs");
include!("remote/loop_part_3.rs");
include!("remote/loop_part_4.rs");
include!("remote/loop_part_5.rs");
include!("remote/loop_part_6.rs");
include!("remote/loop_part_7.rs");
include!("remote/loop_part_8.rs");
