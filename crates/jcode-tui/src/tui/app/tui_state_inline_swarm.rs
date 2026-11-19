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
        let descendants = super::swarm_subtree_session_ids(&self.remote_swarm_members, self_id);
        self.remote_swarm_members.iter().any(|member| {
            descendants.contains(member.session_id.as_str())
                && member_status_is_working(&member.status)
        })
    }

    pub(super) fn inline_swarm_members_unticked(&self) -> Vec<crate::protocol::SwarmMemberStatus> {
        if self.debug_force_inline_gallery {
            return self.remote_swarm_members.clone();
        }
        if !self.swarm_enabled {
            return Vec::new();
        }
        // Scope the inline gallery to the subtree this session actually spawned.
        // Other sessions can share the same swarm (e.g. same repo) without this
        // session having spawned them; showing those would be noise. The spawn
        // tree is reconstructed from each member's `report_back_to_session_id`
        // parent edge.
        let self_id = if self.is_remote {
            self.remote_session_id.as_deref()
        } else {
            Some(self.session.id.as_str())
        };
        match self_id {
            Some(self_id) => filter_inline_swarm_subtree(&self.remote_swarm_members, self_id),
            // Session identity is not known yet (e.g. right after connect,
            // before the History event sets `remote_session_id`). Showing all
            // swarm members here caused the inline strip to flash on startup
            // and then disappear once the subtree filter kicked in. Hide the
            // strip until we know who we are.
            None => Vec::new(),
        }
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
