//! Resolve a mouse position to the in-view target under it.
//!
//! Each `try_*_at` helper answers one question: given the column and row the
//! pointer is on, which already-rendered target did the user hit, and what
//! should happen to it? They return `false` when the point missed, so the
//! caller can keep looking and the click still falls through to selection and
//! scrolling.
//!
//! Kept separate from `navigation.rs` because the click-to-target resolution
//! is a distinct concern from scrolling and view state, and because that file
//! is already over the size budget.

use super::*;
use crate::tui::elicitation_types::ElicitHit;

impl App {
    /// If a left-click landed on an inline image's `expand` badge or on the
    /// rendered image itself, cycle that image's size and return `true`.
    /// Returns `false` (so the click can fall through to link/selection
    /// handling) when neither was hit.
    pub(super) fn try_cycle_image_expand_at(&mut self, column: u16, row: u16) -> bool {
        let centered = self.centered;
        let Some(image_id) = super::super::ui::inline_image_expand_target_from_screen(column, row)
            .or_else(|| {
                super::super::ui::inline_image_body_target_from_screen(column, row, centered)
            })
        else {
            return false;
        };
        let level = self.cycle_image_expand(image_id);
        let size = match level {
            crate::tui::ui::inline_image_ui::ImageExpandLevel::Fit => "fit",
            crate::tui::ui::inline_image_ui::ImageExpandLevel::Large => "large",
            crate::tui::ui::inline_image_ui::ImageExpandLevel::Full => "full",
        };
        crate::tui::mermaid::set_mermaid_inline_expand_level(image_id, level as u8);
        if let Some(source) = crate::tui::mermaid::mermaid_source_for_hash(image_id) {
            self.report_mermaid_copy(&source, size);
        } else if let Some((media_type, data)) =
            super::super::ui::inline_image_ui::payload_for_copy(image_id)
        {
            let copied = super::helpers::copy_image_to_clipboard(&media_type, &data);
            self.set_status_notice(if copied {
                format!("Image size: {size} · Image copied")
            } else {
                format!("Image size: {size} · Could not copy image")
            });
        }
        true
    }

    /// Ctrl+wheel (also how terminals report a trackpad pinch) over an inline
    /// image steps its size one level without wrapping. Returns `false` when
    /// the pointer is not over an image so the wheel scrolls the chat instead.
    pub(super) fn try_zoom_inline_image_at(
        &mut self,
        column: u16,
        row: u16,
        direction: i8,
    ) -> bool {
        use crate::tui::ui::inline_image_ui::ImageExpandLevel;
        let centered = self.centered;
        let Some(image_id) =
            super::super::ui::inline_image_body_target_from_screen(column, row, centered)
                .or_else(|| super::super::ui::inline_image_expand_target_from_screen(column, row))
        else {
            return false;
        };
        let current = self
            .expanded_images
            .get(&image_id)
            .copied()
            .unwrap_or(ImageExpandLevel::Fit);
        let next =
            ImageExpandLevel::from_index(crate::tui::mermaid::step_distinct_mermaid_inline_level(
                image_id,
                current as u8,
                direction,
            ));
        if next == current {
            return true;
        }
        if matches!(next, ImageExpandLevel::Fit) {
            self.expanded_images.remove(&image_id);
        } else {
            self.expanded_images.insert(image_id, next);
        }
        self.expanded_images_version = self.expanded_images_version.wrapping_add(1);
        crate::tui::mermaid::set_mermaid_inline_expand_level(image_id, next as u8);
        self.set_status_notice(match next {
            ImageExpandLevel::Fit => "Image size: fit",
            ImageExpandLevel::Large => "Image size: large",
            ImageExpandLevel::Full => "Image size: full",
        });
        true
    }

    /// If a left-click landed on a swarm notification's `▸ expand` /
    /// `▾ collapse` badge, toggle that notification between its tldr line and
    /// its full body. Returns `false` when the click was elsewhere.
    pub(super) fn try_toggle_swarm_expand_at(&mut self, column: u16, row: u16) -> bool {
        let Some(msg_idx) = super::super::ui::swarm_expand_target_from_screen(column, row) else {
            return false;
        };
        self.toggle_swarm_message_expand(msg_idx)
    }

    /// Toggle the collapsed/expanded state of the swarm notification at
    /// transcript index `msg_idx`. Returns `true` when the message was a
    /// collapsible swarm card and its state changed.
    pub(super) fn toggle_swarm_message_expand(&mut self, msg_idx: usize) -> bool {
        let Some(message) = self.display_messages.get(msg_idx) else {
            return false;
        };
        if message.role != "swarm" {
            return false;
        }
        let Some(toggled) = jcode_tui_messages::toggle_collapsible_swarm_content(&message.content)
        else {
            return false;
        };
        let expanded = jcode_tui_messages::parse_collapsible_swarm_content(&toggled)
            .map(|parsed| parsed.expanded)
            .unwrap_or(false);
        if !self.replace_display_message_content(msg_idx, toggled) {
            return false;
        }
        self.set_status_notice(if expanded {
            "Swarm message expanded"
        } else {
            "Swarm message collapsed"
        });
        true
    }

    /// If the elicitation overlay is showing and a left-press landed on one of
    /// its option rows, select that row and return `true` so the click stops
    /// here. The overlay is modal, so a hit must never reach the surface
    /// underneath; a miss returns `false` and the click falls through
    /// untouched.
    pub(super) fn consume_elicit_click(&mut self, mouse: MouseEvent) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }
        let Some(state) = self.elicit_overlay.as_mut() else {
            return false;
        };
        // The regions come from the last render, so this can only ever match a
        // row the user is looking at.
        let hit = crate::tui::ui::elicit_overlay::hit_test(
            &state.hit_regions.borrow(),
            mouse.column,
            mouse.row,
        );
        let Some(ElicitHit::Option(option_idx)) = hit else {
            return false;
        };
        state.selected = option_idx;
        true
    }
}
