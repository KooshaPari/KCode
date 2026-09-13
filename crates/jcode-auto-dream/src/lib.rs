//! # jcode-auto-dream
//!
//! Background memory consolidation with three-gate triggering.
//!
//! autoDream runs a background consolidation process that merges, prunes, and
//! strengthens the memory graph when three independent gates all pass:
//!
//! 1. **Time gate** - at least 24 hours have elapsed since the last consolidation.
//! 2. **Session gate** - at least 5 sessions have occurred since the last
//!    consolidation.
//! 3. **Lock gate** - a file-based lock is acquired to prevent concurrent
//!    consolidation runs.
//!
//! Additional subsystems:
//! - **Scan throttle** - limits how often memory scans fire (10-minute cooldown).
//! - **Consolidation prompt** - builds the LLM prompt that performs the actual
//!   memory merge/deduplicate/strengthen pass.
//!
//! # Module overview
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`time_gate`] | Tracks elapsed wall-clock time since last consolidation |
//! | [`session_gate`] | Counts sessions since last consolidation |
//! | [`consolidation_lock`] | Acquires/releases a file lock for exclusive access |
//! | [`scan_throttle`] | 10-minute cooldown between background scans |
//! | [`consolidation_prompt`] | Builds the prompt payload for the consolidation LLM call |

pub mod consolidation_lock;
pub mod consolidation_prompt;
pub mod scan_throttle;
pub mod session_gate;
pub mod time_gate;

// Re-export key public types for downstream consumers.
pub use consolidation_lock::ConsolidationLock;
pub use consolidation_prompt::ConsolidationPrompt;
pub use scan_throttle::ScanThrottle;
pub use session_gate::SessionGate;

/// Configuration constants for auto-dream thresholds.
pub const DEFAULT_TIME_GATE_HOURS: u64 = 24;
pub const DEFAULT_SESSION_GATE_COUNT: u32 = 5;
pub const DEFAULT_SCAN_THROTTLE_MINUTES: u64 = 10;

/// Outcome of evaluating all three gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DreamTrigger {
    /// All three gates passed -- consolidation should run.
    Ready,
    /// One or more gates have not yet been satisfied.
    NotReady { reason: &'static str },
}

/// Top-level evaluator that checks all three gates and decides whether
/// consolidation should run.
#[derive(Debug)]
pub struct DreamCoordinator {
    pub time_gate_open: bool,
    pub session_gate: SessionGate,
    pub lock: ConsolidationLock,
}

impl DreamCoordinator {
    /// Create a new coordinator with the default thresholds.
    pub fn new(data_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            time_gate_open: false,
            session_gate: SessionGate::new(DEFAULT_SESSION_GATE_COUNT),
            lock: ConsolidationLock::new(data_dir.into()),
        }
    }

    /// Evaluate all three gates. Returns `Ready` only if every gate passes.
    pub fn evaluate(&self) -> DreamTrigger {
        if !self.time_gate_open {
            return DreamTrigger::NotReady {
                reason: "time gate not yet satisfied",
            };
        }
        if !self.session_gate.is_satisfied() {
            return DreamTrigger::NotReady {
                reason: "session gate not yet satisfied",
            };
        }
        if !self.lock.is_available() {
            return DreamTrigger::NotReady {
                reason: "consolidation lock is held",
            };
        }
        DreamTrigger::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_not_ready_when_gates_unsatisfied() {
        let coord = DreamCoordinator::new("/tmp/auto-dream-test");
        let result = coord.evaluate();
        assert_ne!(result, DreamTrigger::Ready);
    }
}
