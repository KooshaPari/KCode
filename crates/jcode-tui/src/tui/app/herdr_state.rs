//! Effective herdr agent state for this pane.
//!
//! A pane is only `idle` when *nothing* is still running on its behalf:
//! no turn in flight (see [`App::is_processing`]), no background task in the
//! `Running` state, and no live child/subagent session that this session
//! spawned. Reporting `idle` while a spawned subagent or a background `bash`
//! job is still working makes herdr show the pane as finished while work
//! continues, so every reporter (the CLI reporter and the socket reporter)
//! derives its state from [`App::herdr_agent_state`].
use super::App;
use crate::tui::herdr::AgentState;
use crate::tui::BackgroundTaskRowStatus;

/// Swarm lifecycle words that mean a member is no longer live. Anything else
/// (`ready`, `running`, `spawned`, `waiting`, ...) still counts as work, which
/// is the safe direction: a pane that lingers in `working` is a cosmetic
/// annoyance, whereas a pane that flips to `idle` mid-work hides real work.
const TERMINAL_SWARM_STATUSES: &[&str] = &[
    "completed",
    "complete",
    "done",
    "finished",
    "failed",
    "error",
    "stopped",
    "terminated",
    "cancelled",
    "canceled",
    "killed",
    "exited",
];

/// Whether a swarm member status still represents live work.
pub(super) fn swarm_member_is_live(status: &str) -> bool {
    let status = status.trim().to_ascii_lowercase();
    !TERMINAL_SWARM_STATUSES.contains(&status.as_str())
}

impl App {
    /// The state this pane should report: `Working` while a turn, a background
    /// task, or a spawned child session is still running; otherwise `Idle`.
    pub(super) fn herdr_agent_state(&self) -> AgentState {
        if self.is_processing() || self.has_running_background_task() || self.has_live_child_session()
        {
            AgentState::Working
        } else {
            AgentState::Idle
        }
    }

    fn has_running_background_task(&self) -> bool {
        self.background_task_rows_ref()
            .iter()
            .any(|row| row.status == BackgroundTaskRowStatus::Running)
    }

    /// One authoritative in-process source: the live swarm-member snapshot this
    /// session already receives for its inline swarm strip. Scoped to the
    /// subtree this session spawned via the `report_back_to_session_id` parent
    /// edge, so another session's children never keep this pane awake. No
    /// filesystem access and no session-store scan.
    fn has_live_child_session(&self) -> bool {
        let self_id = if self.is_remote {
            self.remote_session_id.as_deref()
        } else {
            Some(self.session.id.as_str())
        };
        let Some(self_id) = self_id else {
            return false;
        };
        super::tui_state::filter_inline_swarm_subtree(&self.remote_swarm_members, self_id)
            .iter()
            .any(|member| swarm_member_is_live(&member.status))
    }

    /// Report the effective state through the socket reporter, which is
    /// initialised at startup alongside the CLI reporter. Turn-end call sites
    /// use this instead of a hardcoded `Idle` so the two reporters agree.
    pub(super) fn report_herdr_socket_state(&self) {
        let state = match self.herdr_agent_state() {
            AgentState::Working => jcode_herdr::AgentState::Working,
            AgentState::Idle => jcode_herdr::AgentState::Idle,
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(crate::herdr::report_state(state));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{SwarmMemberRuntime, SwarmMemberStatus};

    fn member(id: &str, parent: Option<&str>, status: &str) -> SwarmMemberStatus {
        SwarmMemberStatus {
            session_id: id.to_string(),
            friendly_name: None,
            status: status.to_string(),
            detail: None,
            task_label: None,
            role: Some("agent".to_string()),
            is_headless: Some(true),
            live_attachments: Some(0),
            status_age_secs: Some(0),
            output_tail: None,
            report_back_to_session_id: parent.map(str::to_string),
            todo_progress: None,
            todo_items: Vec::new(),
            runtime: SwarmMemberRuntime::default(),
        }
    }

    #[test]
    fn processing_reports_working() {
        let mut app = crate::tui::app::tests::create_test_app();
        app.is_processing = true;
        assert_eq!(app.herdr_agent_state(), AgentState::Working);
    }

    #[test]
    fn running_background_task_after_turn_end_reports_working() {
        let mut app = crate::tui::app::tests::create_test_app();
        app.is_processing = false;
        app.upsert_running_background_task("task-1".into(), "bash".into(), None);
        assert_eq!(app.herdr_agent_state(), AgentState::Working);
    }

    #[test]
    fn finished_background_task_alone_reports_idle() {
        let mut app = crate::tui::app::tests::create_test_app();
        app.is_processing = false;
        app.upsert_running_background_task("task-1".into(), "bash".into(), None);
        app.finish_background_task(
            "task-1".into(),
            "bash".into(),
            BackgroundTaskRowStatus::Completed,
        );
        assert_eq!(app.herdr_agent_state(), AgentState::Idle);
    }

    #[test]
    fn live_child_session_after_turn_end_reports_working() {
        let mut app = crate::tui::app::tests::create_test_app();
        app.is_processing = false;
        let parent = app.session.id.clone();
        app.remote_swarm_members = vec![
            member("child-a", Some(&parent), "running"),
            member("grandchild", Some("child-a"), "ready"),
        ];
        assert_eq!(app.herdr_agent_state(), AgentState::Working);
    }

    #[test]
    fn child_of_another_session_does_not_keep_this_pane_working() {
        let mut app = crate::tui::app::tests::create_test_app();
        app.is_processing = false;
        app.remote_swarm_members = vec![member("stranger", Some("other-session"), "running")];
        assert_eq!(app.herdr_agent_state(), AgentState::Idle);
    }

    #[test]
    fn finished_children_and_nothing_else_reports_idle() {
        let mut app = crate::tui::app::tests::create_test_app();
        app.is_processing = false;
        let parent = app.session.id.clone();
        app.remote_swarm_members = vec![
            member("child-a", Some(&parent), "completed"),
            member("child-b", Some(&parent), "stopped"),
            member("child-c", Some(&parent), "failed"),
        ];
        assert_eq!(app.herdr_agent_state(), AgentState::Idle);
    }

    #[test]
    fn swarm_status_classifier_is_conservative() {
        for terminal in ["completed", "Failed", " stopped ", "cancelled", "DONE"] {
            assert!(!swarm_member_is_live(terminal), "{terminal} should be terminal");
        }
        for live in ["running", "ready", "spawned", "waiting", ""] {
            assert!(swarm_member_is_live(live), "{live} should count as live");
        }
    }
}
