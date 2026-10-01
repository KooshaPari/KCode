//! HERDR terminal runtime integration.
//!
//! Provides a global [`jcode_herdr::HerdrReporter`] that the TUI and
//! provider layers use to report lifecycle state to HERDR. All
//! operations are no-ops when jcode is not running inside a HERDR pane.

use jcode_herdr::{AgentState, HerdrReporter};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

static SESSION_REVISION: AtomicU64 = AtomicU64::new(0);
static STATE_REVISION: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static LAST_REQUESTED_STATE: std::cell::Cell<Option<AgentState>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn take_requested_state() -> Option<AgentState> {
    LAST_REQUESTED_STATE.with(|state| state.take())
}

static REPORTER: OnceLock<Mutex<Option<HerdrReporter>>> = OnceLock::new();

/// Spawn a [`report_state`] report onto the tokio runtime.
///
/// Fire-and-forget lifecycle reports run from sync contexts (the TUI event
/// loop, `finish_turn`) panic on `tokio::spawn` when no reactor exists, as in
/// unit tests. Reports are already no-ops without an initialized reporter, so
/// skipping the spawn entirely outside a runtime is behavior-preserving.
pub fn spawn_report(state: AgentState) {
    #[cfg(test)]
    LAST_REQUESTED_STATE.with(|last| last.set(Some(state)));
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    let revision = STATE_REVISION.fetch_add(1, Ordering::SeqCst) + 1;
    tokio::spawn(async move {
        if let Some(m) = REPORTER.get() {
            let guard = m.lock().await;
            if revision == STATE_REVISION.load(Ordering::SeqCst) {
                if let Some(reporter) = guard.as_ref() {
                    reporter.set_state(state).await;
                }
            }
        }
    });
}

/// Initialize the global HERDR reporter. Safe to call multiple times;
/// only the first call takes effect.
pub fn init(agent_label: &str) {
    let _ = REPORTER.set(Mutex::new(Some(HerdrReporter::new(agent_label))));
}

/// Force-initialize with `HERDR_ENV=1` for testing outside a real pane.
pub fn init_forced(agent_label: &str) {
    unsafe {
        std::env::set_var("HERDR_ENV", "1");
    }
    let _ = REPORTER.set(Mutex::new(Some(HerdrReporter::new(agent_label))));
}

/// Report a state transition to HERDR. No-op if not initialized or
/// not running inside HERDR.
pub async fn report_state(state: AgentState) {
    if let Some(m) = REPORTER.get() {
        let guard = m.lock().await;
        if let Some(reporter) = guard.as_ref() {
            reporter.set_state(state).await;
        }
    }
}

/// Report session identity for restore.
pub async fn report_session_id(session_id: String) {
    if let Some(m) = REPORTER.get() {
        let guard = m.lock().await;
        if let Some(reporter) = guard.as_ref() {
            reporter.set_session_id(session_id).await;
        }
    }
}

/// Spawn a [`report_session_id`] report onto the tokio runtime.
///
/// The remote-client event loop is synchronous, so it cannot await the
/// report directly. Mirrors [`spawn_report`]: a no-op when no reactor
/// exists (unit tests) or when the reporter is not initialized.
pub fn spawn_report_session_id(session_id: String) {
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    let revision = SESSION_REVISION.fetch_add(1, Ordering::SeqCst) + 1;
    tokio::spawn(async move {
        if let Some(m) = REPORTER.get() {
            let guard = m.lock().await;
            if revision == SESSION_REVISION.load(Ordering::SeqCst) {
                if let Some(reporter) = guard.as_ref() {
                    reporter.set_session_id(session_id).await;
                }
            }
        }
    });
}

/// Send the initial idle report on session start.
pub async fn on_session_start() {
    if let Some(m) = REPORTER.get() {
        let guard = m.lock().await;
        if let Some(reporter) = guard.as_ref() {
            reporter.on_session_start().await;
        }
    }
}

/// Release the agent and shut down the reporter.
pub async fn shutdown() {
    if let Some(m) = REPORTER.get() {
        let mut guard = m.lock().await;
        if let Some(reporter) = guard.take() {
            reporter.release().await;
        }
    }
}

/// Returns `true` if HERDR is active and the reporter is initialized.
pub fn is_active() -> bool {
    REPORTER
        .get()
        .and_then(|m| m.try_lock().ok())
        .and_then(|guard| guard.as_ref().map(|r| r.is_active()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_without_runtime_are_noops() {
        super::spawn_report_session_id("test-session".into());
        super::spawn_report(jcode_herdr::AgentState::Idle);
    }
}
