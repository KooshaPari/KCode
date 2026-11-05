//! Truthful clipboard status notices (issue #231).
//!
//! A clipboard helper that is still running past the fail-fast window has not
//! proven that it owns the selection. The UI therefore shows neutral
//! provisional wording, never a confirmed "Copied", and the idle tick applies
//! a late correction if the helper eventually exits. Every copy bumps a token
//! so a slow outcome can never overwrite a newer copy, and a correction is only
//! applied while the provisional notice is still the visible one.

use super::App;
use super::helpers::{ClipboardCopy, CopyOutcome, HelperExit, PendingCopy};

impl App {
    /// Report the result of a clipboard write and pick the notice wording.
    ///
    /// Returns whether the copy should be treated as accepted for control flow
    /// (spinner/selection teardown). `Unconfirmed` counts as accepted so call
    /// sites do not retry into a competing selection owner, but it is never
    /// rendered as a confirmed copy.
    pub(super) fn report_clipboard_copy(
        &mut self,
        copy: ClipboardCopy,
        success_text: impl Into<String>,
        provisional_text: impl Into<String>,
        failure_text: impl Into<String>,
    ) -> bool {
        self.copy_notice_token = self.copy_notice_token.wrapping_add(1);
        let token = self.copy_notice_token;
        let success_text = success_text.into();
        let failure_text = failure_text.into();

        match copy.outcome {
            CopyOutcome::Copied => {
                self.pending_copy = None;
                self.set_status_notice(success_text);
                true
            }
            CopyOutcome::Failed => {
                self.pending_copy = None;
                self.set_status_notice(failure_text);
                false
            }
            CopyOutcome::Unconfirmed => {
                let provisional_text = provisional_text.into();
                self.set_status_notice(provisional_text.clone());
                self.pending_copy = copy.watch.map(|watch| PendingCopy {
                    token,
                    watch,
                    success_text,
                    provisional_text,
                    failure_text,
                });
                true
            }
        }
    }

    /// Resolve at most one pending copy outcome. O(1) and non-blocking: it
    /// only does a channel `try_recv`, so it is safe on the idle tick.
    ///
    /// Returns whether the visible notice changed (and a redraw is needed).
    pub(super) fn poll_clipboard_outcome(&mut self) -> bool {
        let Some(pending) = self.pending_copy.as_ref() else {
            return false;
        };
        let Some(exit) = pending.watch.try_settle() else {
            return false;
        };
        let pending = self
            .pending_copy
            .take()
            .expect("pending copy checked above");
        // A newer copy superseded this one; drop the stale outcome.
        if pending.token != self.copy_notice_token {
            return false;
        }
        // Only correct while our provisional notice is still on screen. If the
        // user has moved on (a different notice), resurrecting this outcome
        // would be stale feedback.
        let still_provisional = self
            .status_notice
            .as_ref()
            .is_some_and(|(text, _)| *text == pending.provisional_text);
        if !still_provisional {
            return false;
        }
        match exit {
            HelperExit::ExitedZero => self.set_status_notice(pending.success_text),
            HelperExit::ExitedNonZero => self.set_status_notice(pending.failure_text),
        }
        true
    }
}

#[cfg(test)]
impl App {
    /// Test-only: the raw notice text, ignoring TTL (unit tests run instantly).
    pub(super) fn status_notice_text(&self) -> Option<&str> {
        self.status_notice.as_ref().map(|(text, _)| text.as_str())
    }
}
