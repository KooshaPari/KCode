use super::*;

impl App {
    /// True when a child/subagent session this session spawned is still
    /// working. Reads only the in-memory server snapshot
    /// (`remote_swarm_members`) restricted to this session's spawn subtree, so
    /// it performs no filesystem access and never scans `~/.jcode/sessions`.
    pub(crate) fn has_working_child_agent(&self) -> bool {
        if self.remote_swarm_members.is_empty() {
            return false;
        }
        let self_id = if self.is_remote {
            self.remote_session_id.as_deref()
        } else {
            Some(self.session.id.as_str())
        };
        let Some(self_id) = self_id else {
            return false;
        };
        let descendants =
            super::tui_state::swarm_subtree_session_ids(&self.remote_swarm_members, self_id);
        self.remote_swarm_members.iter().any(|member| {
            descendants.contains(member.session_id.as_str())
                && member_status_is_working(&member.status)
        })
    }
}

/// Lifecycle statuses that mean a spawned agent is actively doing work. Kept
/// in sync with the server's own active-status set (`running`, `streaming`,
/// `thinking`, plus the stale/queued variants a long turn can hold).
fn member_status_is_working(status: &str) -> bool {
    matches!(
        status,
        "running" | "running_stale" | "streaming" | "thinking" | "queued"
    )
}

#[cfg(test)]
mod herdr_child_state_tests {
    use super::member_status_is_working;

    #[test]
    fn active_lifecycle_statuses_are_working() {
        for status in [
            "running",
            "running_stale",
            "streaming",
            "thinking",
            "queued",
        ] {
            assert!(member_status_is_working(status), "{status} must be working");
        }
    }

    #[test]
    fn settled_lifecycle_statuses_are_not_working() {
        for status in [
            "ready",
            "waiting",
            "completed",
            "done",
            "failed",
            "stopped",
            "crashed",
            "closed",
            "disconnected",
        ] {
            assert!(
                !member_status_is_working(status),
                "{status} must not be working"
            );
        }
    }
}
