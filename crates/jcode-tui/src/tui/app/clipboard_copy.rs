//! Clipboard copy outcome types (issue #231).
//!
//! A clipboard helper that is still running when the fail-fast window closes has
//! not proven that it owns the selection. These types carry that distinction so
//! a call site can render "sent but unconfirmed" instead of claiming a copy, and
//! can still apply the helper's real exit later, from the idle tick.

/// What a clipboard write established about selection ownership.
///
/// `Copied` means the write was accepted (the helper exited zero, or a native
/// API / OSC 52 accepted it). `Failed` means the write was rejected. Both are
/// observations. `Unconfirmed` means a helper is still running and may or may
/// not own the selection: it is explicitly *not* a copy (issue #231).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyOutcome {
    Copied,
    Failed,
    Unconfirmed,
}

/// The eventual exit of a helper that outlived the fail-fast window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HelperExit {
    // Constructed by the Linux-only helper module; on Windows/macOS that module
    // is not compiled, so the variant is legitimately unused there.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    ExitedZero,
    ExitedNonZero,
}

/// Receives the late exit of an unconfirmed clipboard helper.
///
/// The detached reaper owns the child and sends its exit here. `try_settle`
/// never blocks, so the UI tick can poll it and apply an honest correction
/// without stalling input handling.
#[derive(Debug)]
pub(super) struct ClipboardWatch {
    rx: std::sync::mpsc::Receiver<HelperExit>,
}

impl ClipboardWatch {
    // Only the Linux-only helper module opens a watch channel.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    pub(super) fn channel() -> (Self, std::sync::mpsc::Sender<HelperExit>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Self { rx }, tx)
    }

    /// Non-blocking: `None` means the helper has not exited yet. A disconnected
    /// reaper means the exit will never be reported, so it settles as a
    /// non-zero exit: an unconfirmed helper must never be promoted to a copy
    /// on missing evidence.
    pub(super) fn try_settle(&self) -> Option<HelperExit> {
        match self.rx.try_recv() {
            Ok(exit) => Some(exit),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(HelperExit::ExitedNonZero),
        }
    }
}

/// A clipboard write plus the optional watch for an unconfirmed helper.
///
/// `From<bool>` keeps the many existing test seams (`|text| true`) working:
/// they express an already-resolved copy or failure.
#[derive(Debug)]
pub(super) struct ClipboardCopy {
    pub(super) outcome: CopyOutcome,
    pub(super) watch: Option<ClipboardWatch>,
}

impl ClipboardCopy {
    pub(super) fn copied() -> Self {
        Self { outcome: CopyOutcome::Copied, watch: None }
    }

    pub(super) fn failed() -> Self {
        Self { outcome: CopyOutcome::Failed, watch: None }
    }

    // Only the Linux-only helper module produces an unconfirmed copy.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    pub(super) fn unconfirmed(watch: ClipboardWatch) -> Self {
        Self { outcome: CopyOutcome::Unconfirmed, watch: Some(watch) }
    }
}

impl From<bool> for ClipboardCopy {
    fn from(success: bool) -> Self {
        if success { Self::copied() } else { Self::failed() }
    }
}

/// A copy whose helper is still running. The tick poll watches for its late
/// exit and corrects the provisional notice, unless a newer copy (token) or a
/// different notice superseded it.
#[derive(Debug)]
pub(super) struct PendingCopy {
    pub(super) token: u64,
    pub(super) watch: ClipboardWatch,
    pub(super) success_text: String,
    pub(super) provisional_text: String,
    pub(super) failure_text: String,
}
