//! Native TUI elicitation overlay renderer.
//!
//! Renders a centered, bordered panel for structured agent-to-user input
//! (multi-choice, text, boolean, integer, date/time). Follows the same
//! overlay pattern as the session picker and changelog overlays.

use super::{clear_area, dim_color, rgb};
use crate::tui::elicitation_types::{ChoiceOption, ElicitHit, ElicitOverlayState, FieldSpec};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Render the elicitation overlay as a centered modal panel.
pub(super) fn draw_elicit_overlay(frame: &mut Frame, area: Rect, state: &ElicitOverlayState) {
    // Regions from the previous frame describe rows this frame may not draw.
    let mut hit_regions = state.hit_regions.borrow_mut();
    hit_regions.clear();
    // Dim the background.
    clear_area(frame, area);

    let request = &state.request;
    let border_color = request.urgency.border_color();
    let icon = request.urgency.icon();

    // Compute panel dimensions: 60% width, auto-height capped at 70%.
    let panel_w = (area.width * 60 / 100).max(40).min(area.width);
    let panel_h = compute_panel_height(state, panel_w).min(area.height * 70 / 100);

    let panel_x = area.x + (area.width.saturating_sub(panel_w)) / 2;
    let panel_y = area.y + (area.height.saturating_sub(panel_h)) / 2;
    let panel_area = Rect::new(panel_x, panel_y, panel_w, panel_h);

    // Clear the panel area so dimmed background doesn't bleed through.
    frame.render_widget(Clear, panel_area);

    // Build the bordered block.
    let title = format!(" {} {} ", icon, request.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(
            button_footer(request),
            Style::default().fg(dim_color()),
        )));

    let inner = block.inner(panel_area);
    frame.render_widget(block, panel_area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    // (option index, first drawn row, last drawn row) for clickable choice
    // blocks, resolved to screen rects once the paragraph's inner area is known.
    let mut option_rows: Vec<(usize, usize, usize)> = Vec::new();

    // Question / description.
    if let Some(ref question) = request.question {
        for wrapped in text_wrap(question, inner.width as usize) {
            lines.push(Line::from(Span::styled(
                wrapped,
                Style::default().fg(rgb(200, 200, 220)),
            )));
        }
        lines.push(Line::from(""));
    }

    // Field-specific rendering.
    match &request.field {
        FieldSpec::Choice {
            label,
            options,
            default_index,
        } => {
            if !label.is_empty() {
                lines.push(Line::from(Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
            }
            let selected = state.selected;
            let inner_w = inner.width as usize;
            for (i, option) in options.iter().enumerate() {
                let marker = if i == selected { "●" } else { "○" };
                let style = if i == selected {
                    Style::default()
                        .fg(border_color)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(rgb(170, 170, 185))
                };
                let first_row = lines.len();
                lines.extend(option_lines(option, marker, style, inner_w));
                // A label or description long enough to wrap keeps every row it
                // occupies clickable, and no row count is assumed.
                option_rows.push((i, first_row, lines.len().saturating_sub(1)));
            }
            // Show default hint.
            if let Some(default_idx) = default_index
                && let Some(default_opt) = options.get(*default_idx)
            {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  Default: {}", default_opt.label),
                    Style::default().fg(dim_color()),
                )));
            }
        }
        FieldSpec::Boolean { label, default } => {
            if !label.is_empty() {
                lines.push(Line::from(Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
            }
            // Boolean is rendered as two buttons in the footer.
            let _ = default;
        }
        FieldSpec::Text {
            label,
            default,
            placeholder,
            secret,
            ..
        } => {
            if !label.is_empty() {
                lines.push(Line::from(Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let display_value = if state.text_buffer.is_empty() {
                default
                    .clone()
                    .or_else(|| placeholder.clone())
                    .unwrap_or_default()
            } else if *secret {
                "*".repeat(state.text_buffer.len())
            } else {
                state.text_buffer.clone()
            };
            let input_style = if state.text_buffer.is_empty() {
                Style::default().fg(dim_color())
            } else {
                Style::default().fg(rgb(220, 220, 240))
            };
            lines.push(Line::from(Span::styled(
                format!("  > {}", display_value),
                input_style,
            )));
        }
        FieldSpec::LongText { label, default, .. } => {
            if !label.is_empty() {
                lines.push(Line::from(Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let display_value = if state.text_buffer.is_empty() {
                default.clone().unwrap_or_default()
            } else {
                state.text_buffer.clone()
            };
            let input_style = if state.text_buffer.is_empty() {
                Style::default().fg(dim_color())
            } else {
                Style::default().fg(rgb(220, 220, 240))
            };
            for line_text in display_value.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", line_text),
                    input_style,
                )));
            }
            if display_value.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (Ctrl+Enter to submit)",
                    Style::default().fg(dim_color()),
                )));
            }
        }
        FieldSpec::Integer {
            label,
            default,
            min,
            max,
        } => {
            if !label.is_empty() {
                lines.push(Line::from(Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let value = if state.text_buffer.is_empty() {
                default.map(|v| v.to_string()).unwrap_or_default()
            } else {
                state.text_buffer.clone()
            };
            lines.push(Line::from(Span::styled(
                format!("  > {}", value),
                Style::default().fg(rgb(220, 220, 240)),
            )));
            let range_hint = match (min, max) {
                (Some(lo), Some(hi)) => format!("  Range: {}..{}", lo, hi),
                (Some(lo), None) => format!("  Min: {}", lo),
                (None, Some(hi)) => format!("  Max: {}", hi),
                (None, None) => String::new(),
            };
            if !range_hint.is_empty() {
                lines.push(Line::from(Span::styled(
                    range_hint,
                    Style::default().fg(dim_color()),
                )));
            }
        }
        FieldSpec::DateTime { label, default, .. } => {
            if !label.is_empty() {
                lines.push(Line::from(Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let value = if state.text_buffer.is_empty() {
                default.clone().unwrap_or_default()
            } else {
                state.text_buffer.clone()
            };
            lines.push(Line::from(Span::styled(
                format!("  > {}", value),
                Style::default().fg(rgb(220, 220, 240)),
            )));
        }
    }

    // Notes area.
    if let Some(ref notes_spec) = request.notes {
        lines.push(Line::from(""));
        let label = if notes_spec.required {
            format!("{} *", notes_spec.label)
        } else {
            notes_spec.label.clone()
        };
        lines.push(Line::from(Span::styled(
            label,
            Style::default()
                .fg(rgb(180, 180, 200))
                .add_modifier(Modifier::BOLD),
        )));
        let notes_value = notes_spec.default.clone().unwrap_or_default();
        if notes_value.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (optional)",
                Style::default().fg(dim_color()),
            )));
        } else {
            for note_line in notes_value.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", note_line),
                    Style::default().fg(rgb(200, 200, 220)),
                )));
            }
        }
    }

    let paragraph = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(paragraph, inner);

    // Each option now spans a known, contiguous run of rows because its text was
    // pre-wrapped: the paragraph has nothing left to re-wrap.
    let bottom = inner.y.saturating_add(inner.height);
    for (option_idx, first_row, last_row) in option_rows {
        let Some(top) = inner.y.checked_add(first_row as u16) else {
            continue;
        };
        if top >= bottom {
            continue;
        }
        let rows = (last_row.saturating_sub(first_row) as u16).saturating_add(1);
        let height = rows.min(bottom - top);
        hit_regions.push((
            Rect::new(inner.x, top, inner.width, height),
            ElicitHit::Option(option_idx),
        ));
    }
    // The panel is modal: a left click on its blank area must not reach the chat
    // or picker behind it. Pushed last, so option rows win the first match.
    hit_regions.push((inner, ElicitHit::Backdrop));
}

/// Resolve a screen point to the clickable region containing it.
///
/// `regions` comes from the most recent render, so a click can only ever match
/// a row the user is actually looking at. A point outside every region yields
/// `None`, letting the caller pass the click on.
pub(crate) fn hit_test(regions: &[(Rect, ElicitHit)], x: u16, y: u16) -> Option<ElicitHit> {
    regions.iter().find_map(|(rect, hit)| {
        let inside_x = x >= rect.x && x < rect.x.saturating_add(rect.width);
        let inside_y = y >= rect.y && y < rect.y.saturating_add(rect.height);
        (inside_x && inside_y).then_some(*hit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_rows() -> Vec<(Rect, ElicitHit)> {
        vec![
            (Rect::new(0, 0, 10, 1), ElicitHit::Option(0)),
            (Rect::new(0, 1, 10, 1), ElicitHit::Option(1)),
        ]
    }

    #[test]
    fn point_inside_a_region_resolves_to_that_region() {
        assert_eq!(hit_test(&two_rows(), 3, 0), Some(ElicitHit::Option(0)));
        assert_eq!(hit_test(&two_rows(), 3, 1), Some(ElicitHit::Option(1)));
    }

    #[test]
    fn point_outside_every_region_is_none() {
        assert_eq!(hit_test(&two_rows(), 50, 50), None);
        // One column past the right edge.
        assert_eq!(hit_test(&two_rows(), 10, 0), None);
        // One row past the last row.
        assert_eq!(hit_test(&two_rows(), 3, 2), None);
    }

    #[test]
    fn region_bounds_are_half_open() {
        let regions = vec![(Rect::new(5, 7, 3, 2), ElicitHit::Option(9))];
        // Top-left corner is inside; the far edges are not.
        assert_eq!(hit_test(&regions, 5, 7), Some(ElicitHit::Option(9)));
        assert_eq!(hit_test(&regions, 7, 8), Some(ElicitHit::Option(9)));
        assert_eq!(hit_test(&regions, 8, 7), None);
        assert_eq!(hit_test(&regions, 5, 9), None);
    }

    #[test]
    fn empty_region_list_is_none() {
        assert_eq!(hit_test(&[], 0, 0), None);
    }

    fn option(label: &str, description: Option<&str>) -> ChoiceOption {
        ChoiceOption {
            label: label.to_string(),
            value: label.to_string(),
            description: description.map(str::to_string),
        }
    }

    #[test]
    fn short_option_renders_one_row() {
        let lines = option_lines(&option("Yes", None), "●", Style::default(), 40);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "  ● Yes");
    }

    #[test]
    fn long_option_wraps_into_rows_that_fit_the_width() {
        let label = "a remarkably long option label that cannot possibly fit on one row";
        let lines = option_lines(&option(label, None), "●", Style::default(), 30);
        assert!(lines.len() > 1, "expected a wrapped label, got {lines:?}");
        for line in &lines {
            assert!(
                line.width() <= 30,
                "row wider than the panel: {:?}",
                line.to_string()
            );
        }
        // Continuation rows line up under the label, not under the marker.
        assert!(lines[1].to_string().starts_with("    "));
    }

    #[test]
    fn description_occupies_its_own_rows() {
        let with = option_lines(
            &option("Yes", Some("do the thing")),
            "○",
            Style::default(),
            40,
        );
        let without = option_lines(&option("Yes", None), "○", Style::default(), 40);
        assert_eq!(with.len(), without.len() + 1);
    }

    #[test]
    fn wrapped_option_keeps_every_row_clickable() {
        // Two options where the first occupies rows 0..=1 and the second row 2.
        let regions = vec![
            (Rect::new(0, 0, 10, 2), ElicitHit::Option(0)),
            (Rect::new(0, 2, 10, 1), ElicitHit::Option(1)),
            (Rect::new(0, 0, 10, 3), ElicitHit::Backdrop),
        ];
        assert_eq!(hit_test(&regions, 2, 0), Some(ElicitHit::Option(0)));
        // The wrapped continuation row still selects the same option.
        assert_eq!(hit_test(&regions, 2, 1), Some(ElicitHit::Option(0)));
        assert_eq!(hit_test(&regions, 2, 2), Some(ElicitHit::Option(1)));
    }

    #[test]
    fn panel_click_below_the_options_resolves_to_the_backdrop() {
        let regions = vec![
            (Rect::new(0, 0, 10, 1), ElicitHit::Option(0)),
            (Rect::new(0, 0, 10, 5), ElicitHit::Backdrop),
        ];
        assert_eq!(hit_test(&regions, 2, 0), Some(ElicitHit::Option(0)));
        assert_eq!(hit_test(&regions, 2, 4), Some(ElicitHit::Backdrop));
    }

    #[test]
    fn choice_rows_counts_wrapped_rows() {
        let short = vec![option("A", None), option("B", None)];
        assert_eq!(choice_rows(&short, 40), 2);
        let long = vec![option(
            "a remarkably long option label that cannot possibly fit on one row",
            None,
        )];
        assert!(choice_rows(&long, 30) > 1);
    }
}

/// Compute the panel height based on content.
fn compute_panel_height(state: &ElicitOverlayState, width: u16) -> u16 {
    let mut h: u16 = 4; // top border + bottom border + padding
    // Question.
    if state.request.question.is_some() {
        h += 2;
    }
    // Field content.
    match &state.request.field {
        FieldSpec::Choice { options, .. } => {
            // Wrapped options occupy more than one row each, so the height has to
            // count rendered rows or the last options fall off the panel.
            h += choice_rows(options, width.saturating_sub(2)) as u16 + 1;
        }
        FieldSpec::Boolean { .. } => {
            h += 2;
        }
        FieldSpec::Text { .. } | FieldSpec::Integer { .. } | FieldSpec::DateTime { .. } => {
            h += 2;
        }
        FieldSpec::LongText { .. } => {
            h += 4;
        }
    }
    // Notes.
    if state.request.notes.is_some() {
        h += 3;
    }
    h.max(8)
}

/// Rows needed to render every option at `width` columns.
fn choice_rows(options: &[ChoiceOption], width: u16) -> usize {
    options
        .iter()
        .map(|option| option_lines(option, "●", Style::default(), width as usize).len())
        .sum()
}

/// Build the button footer text.
fn button_footer(request: &crate::tui::elicitation_types::ElicitRequest) -> String {
    let buttons = request
        .buttons
        .as_ref()
        .map(|b| (b.confirm.as_str(), b.cancel.as_str()))
        .unwrap_or(("Confirm", "Cancel"));

    match &request.field {
        FieldSpec::Boolean { .. } => " Y = Yes · N = No · Tab to switch ".to_string(),
        FieldSpec::Choice { .. } => {
            format!(" ↑↓ Select · Enter {} · Esc {} ", buttons.0, buttons.1)
        }
        _ => {
            format!(" Enter {} · Esc {} ", buttons.0, buttons.1)
        }
    }
}

/// Build the rendered lines for one choice option.
///
/// The text is wrapped to `width` here rather than by `Paragraph`, so one
/// returned line is exactly one screen row. That is what lets the caller turn a
/// line index into a hit region: with render-time wrapping a long label would
/// occupy several rows while the recorded index counted one, shifting every
/// option below it. Continuation rows are indented under the marker, and a
/// description becomes its own dim, indented block.
fn option_lines(
    option: &ChoiceOption,
    marker: &str,
    style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let indent = format!("  {} ", marker);
    let pad_width = indent.chars().count();
    let text_width = width.saturating_sub(pad_width).max(1);
    let pad = " ".repeat(pad_width);
    let mut lines = Vec::new();
    for (n, segment) in text_wrap(&option.label, text_width).into_iter().enumerate() {
        let lead = if n == 0 { indent.clone() } else { pad.clone() };
        lines.push(Line::from(vec![
            Span::styled(lead, style),
            Span::styled(segment, style),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(indent, style)));
    }
    if let Some(ref desc) = option.description {
        for segment in text_wrap(desc, text_width) {
            lines.push(Line::from(Span::styled(
                format!("{}{}", pad, segment),
                Style::default().fg(dim_color()),
            )));
        }
    }
    lines
}

/// Simple text wrapping for question text.
fn text_wrap(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.len() <= max_width {
            result.push(paragraph.to_string());
        } else {
            let mut remaining = paragraph;
            while remaining.len() > max_width {
                // Find last space within max_width.
                let break_at = remaining[..max_width].rfind(' ').unwrap_or(max_width);
                result.push(remaining[..break_at].to_string());
                remaining = remaining[break_at..].trim_start_matches(' ');
            }
            if !remaining.is_empty() {
                result.push(remaining.to_string());
            }
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}
