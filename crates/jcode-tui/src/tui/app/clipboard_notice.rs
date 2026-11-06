//! Truthful clipboard status notices (issue #231).
//!
//! A clipboard helper that is still running past the fail-fast window has not
//! proven that it owns the selection. The UI therefore shows neutral
//! provisional wording, never a confirmed "Copied", and the idle tick applies
//! a late correction if the helper eventually exits. Every copy bumps a token
//! so a slow outcome can never overwrite a newer copy, and a correction is only
//! applied while the provisional notice is still the visible one.
//!
//! Call-site-specific wording lives here too, so the already-oversized UI
//! modules that own those call sites do not grow.

use super::App;
use super::helpers::{ClipboardCopy, CopyOutcome, HelperExit, PendingCopy};

/// Device-code login suffix: only a confirmed write may say "copied".
pub(super) fn login_device_code_suffix(outcome: CopyOutcome) -> &'static str {
    match outcome {
        CopyOutcome::Copied => " (copied to clipboard - just paste it!)",
        CopyOutcome::Unconfirmed => " (copy sent - paste to check it arrives)",
        CopyOutcome::Failed => "",
    }
}

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
        self.clipboard.token = self.clipboard.token.wrapping_add(1);
        let token = self.clipboard.token;
        let success_text = success_text.into();
        let failure_text = failure_text.into();

        match copy.outcome {
            CopyOutcome::Copied => {
                self.clipboard.pending = None;
                self.set_status_notice(success_text);
                true
            }
            CopyOutcome::Failed => {
                self.clipboard.pending = None;
                self.set_status_notice(failure_text);
                false
            }
            CopyOutcome::Unconfirmed => {
                let provisional_text = provisional_text.into();
                self.set_status_notice(provisional_text.clone());
                self.clipboard.pending = copy.watch.map(|watch| PendingCopy {
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
        let Some(pending) = self.clipboard.pending.as_ref() else {
            return false;
        };
        let Some(exit) = pending.watch.try_settle() else {
            return false;
        };
        let pending = self
            .clipboard
            .pending
            .take()
            .expect("pending copy checked above");
        // A newer copy superseded this one; drop the stale outcome.
        if pending.token != self.clipboard.token {
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

    /// Copy the Mermaid source behind an image and report the result.
    pub(super) fn report_mermaid_copy(&mut self, source: &str, size: &str) {
        let copy = super::helpers::copy_to_clipboard(source);
        self.report_clipboard_copy(
            copy,
            format!("Image size: {size} · Mermaid code copied"),
            format!("Image size: {size} · Mermaid copy sent (unconfirmed)"),
            format!("Image size: {size} · Could not copy Mermaid code"),
        );
    }

    /// Copy a visible badge target and record feedback only for an observed
    /// outcome. Unconfirmed must not render as a success or failure checkmark.
    pub(super) fn copy_visible_badge_target(
        &mut self,
        target: &crate::tui::ui::VisibleCopyTarget,
        key: char,
    ) {
        let copy = super::helpers::copy_to_clipboard(&target.content);
        self.record_copy_badge_key_press(key);
        match copy.outcome {
            CopyOutcome::Copied => self.record_copy_badge_feedback(key, true),
            CopyOutcome::Failed => self.record_copy_badge_feedback(key, false),
            // Neither a checkmark nor a cross is honest here, and a feedback mark
            // left over from an earlier copy would claim this one succeeded.
            CopyOutcome::Unconfirmed => self.copy_badge_ui.copied_feedback = None,
        }
        self.report_clipboard_copy(
            copy,
            target.copied_notice.clone(),
            format!("Copy sent (unconfirmed) · {}", target.kind_label),
            format!("Failed to copy {}", target.kind_label),
        );
    }

    /// Report the agent repair brief copy, keeping one of three truthful
    /// status lines rather than collapsing unconfirmed into copied/failed.
    pub(super) fn report_repair_brief_copy(
        &mut self,
        copy: ClipboardCopy,
        agent: Option<super::onboarding_repair::RepairAgent>,
    ) {
        self.report_clipboard_copy(
            copy,
            super::onboarding_repair::repair_brief_status_for(agent, CopyOutcome::Copied),
            super::onboarding_repair::repair_brief_status_for(agent, CopyOutcome::Unconfirmed),
            super::onboarding_repair::repair_brief_status_for(agent, CopyOutcome::Failed),
        );
    }
}

pub(super) fn cut_input_line_to_clipboard(app: &mut App) -> bool {
    cut_input_line_to_clipboard_with(app, super::helpers::copy_to_clipboard)
}

pub(super) fn cut_input_line_to_clipboard_with<F, R>(app: &mut App, mut copy_text: F) -> bool
where
    F: FnMut(&str) -> R,
    R: Into<ClipboardCopy>,
{
    if app.input.is_empty() {
        return false;
    }

    let copy: ClipboardCopy = copy_text(&app.input).into();
    let outcome = copy.outcome;
    // A late confirmed exit may arrive for a cut we did not apply (the input
    // was kept because ownership was unconfirmed). Its correction must not
    // claim the input was cleared.
    let late_success = if outcome == CopyOutcome::Copied {
        "✂ Cut input line"
    } else {
        "Cut copy confirmed · input kept so you can clear it"
    };
    app.report_clipboard_copy(
        copy,
        late_success,
        "Cut copy sent (unconfirmed) · input kept so you can retry",
        "Failed to copy input line",
    );
    // Only a confirmed write justifies clearing the input. An unconfirmed
    // helper may never have owned the selection, so losing the text would be
    // unrecoverable.
    if outcome != CopyOutcome::Copied {
        return false;
    }

    app.remember_input_undo_state();
    app.input.clear();
    app.cursor_pos = 0;
    app.reset_tab_completion();
    app.sync_model_picker_preview_from_input();
    true
}

#[cfg(test)]
impl App {
    /// Test-only: the raw notice text, ignoring TTL (unit tests run instantly).
    pub(super) fn status_notice_text(&self) -> Option<&str> {
        self.status_notice.as_ref().map(|(text, _)| text.as_str())
    }
}
