//! Swarm strip rendering: horizontal and vertical agent strip views.

use ratatui::prelude::*;

use jcode_tui_style::color::rgb;

use super::hover::{hovered_detail_body, render_hovered_detail};
use super::types::GalleryMember;
use super::util::{
    card_status_label, clamp_line_to_width, count_digits, disp_w, format_elapsed, format_model,
    is_active_status, role_color, sort_members_for_display, status_accent, status_glyph,
    truncate_label,
};


/// Bounds for per-chip task labels on the strip: never wider than MAX (keeps
/// one agent from dominating the line) and dropped entirely below MIN (a two-
/// column "·…" label is noise, not information).
const CHIP_TASK_MAX_W: usize = 24;
const CHIP_TASK_MIN_W: usize = 6;

/// A key/label pair for the swarm strip hint line.
pub struct SwarmStripHint {
    /// The key chord to show, e.g. "alt+n" or "j/k".
    pub key: String,
    /// What it does, e.g. "select".
    pub label: String,
}


/// Render the compact swarm strip shown directly above the status line.
///
/// - Unfocused: a single line of agent "chips" (status glyph + name + optional
///   `done/total` todo count), colored by status, plus a right-aligned
///   `M/N active` readout. A trailing hint shows how to enter the controls.
/// - Focused: the chips line (selected agent highlighted) + an expanded detail
///   viewport for the hovered agent (transcript tail + todo list, bounded by
///   `max_height`) + a keybinding hint line.
///
/// `max_height` is the total line budget for the strip when focused (chips +
/// detail + hints). Small budgets degrade to a single inline detail line.
/// `spinner_frame` animates the glyph for active agents. Returns empty when
/// there are no members or no width.
///
/// ```text
/// 🐝 swarm  · ⠙ researcher 8/16  ✓ reviewer         2/3 active · ctrl+t controls
/// ```
#[allow(clippy::too_many_arguments)]
pub fn render_swarm_strip(
    members: &[GalleryMember],
    selected: usize,
    focused: bool,
    hints: &[SwarmStripHint],
    enter_hint: Option<&str>,
    spinner_frame: usize,
    width: usize,
    max_height: usize,
    batch_selected: Option<&std::collections::HashSet<usize>>,
) -> Vec<Line<'static>> {
    if members.is_empty() || width < 8 {
        return Vec::new();
    }
    let ordered = sort_members_for_display(members);
    let selected = selected.min(ordered.len().saturating_sub(1));
    let active = members
        .iter()
        .filter(|m| is_active_status(&m.status))
        .count();

    // ---- Leading "🐝 swarm" label ----
    let lead: Vec<Span<'static>> = vec![
        Span::styled("🐝 ", Style::default().fg(rgb(255, 200, 100))),
        Span::styled("swarm", Style::default().fg(rgb(160, 160, 170))),
        Span::styled("  · ", Style::default().fg(rgb(80, 80, 90))),
    ];
    let lead_w: usize = lead.iter().map(|s| disp_w(&s.content)).sum();

    // ---- Right tail: "M/N active" plus, when unfocused, the controls hint.
    // Degrade gracefully on narrow widths: drop the hint first, then the tally,
    // so the chips never get pushed past the line width.
    let tally = format!("{active}/{} active", members.len());
    let tally_w = disp_w(&tally);
    let hint_text = if focused { None } else { enter_hint };
    let hint_sep = " · ";
    let gap = 2usize; // minimum gap between chips and the right tail
    let hint_w = hint_text.map(|h| disp_w(h) + disp_w(hint_sep)).unwrap_or(0);

    // ---- Chips: "<glyph> <name>[·task][ done/total]" ----
    // Task labels are additive: chips are fitted by their base width (glyph +
    // name + todo) so a long task can never hide other agents; leftover line
    // width is then shared out to task labels (see below).
    struct Chip {
        glyph: String,
        name: String,
        task: Option<String>,
        todo: Option<String>,
        color: Color,
        active: bool,
        is_sel: bool,
        is_batch: bool,
    }
    let chips: Vec<Chip> = ordered
        .iter()
        .enumerate()
        .map(|(idx, m)| Chip {
            glyph: status_glyph(&m.status, spinner_frame).to_string(),
            name: m.label.clone(),
            task: m
                .task
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .map(|t| truncate_label(t, CHIP_TASK_MAX_W)),
            todo: m.todo.map(|(done, total)| format!("{done}/{total}")),
            color: role_color(m.role.as_deref()).unwrap_or_else(|| status_accent(&m.status)),
            active: is_active_status(&m.status),
            is_sel: idx == selected,
            is_batch: batch_selected.is_some_and(|set| set.contains(&idx)),
        })
        .collect();
    let chip_w = |c: &Chip| -> usize {
        let prefix = if c.is_sel && focused { 2 } else { 0 }; // '▸ ' width
        let batch_pfx = if c.is_batch { 2 } else { 0 }; // '✓ ' width
        prefix + batch_pfx + disp_w(&c.glyph) + 1 + disp_w(&c.name)
            + c.todo.as_ref().map(|t| disp_w(t) + 1).unwrap_or(0)
    };

    // Fit as many chips as possible into `budget`, collapsing overflow into a
    // "+N" marker that is itself budgeted so the line can never exceed `width`.
    // Returns (chips shown, columns used including any "+N" marker).
    const CHIP_SEP: &str = "  ";
    let sep_w = disp_w(CHIP_SEP);
    let fit_chips = |budget: usize| -> (usize, usize) {
        let mut shown = 0usize;
        let mut acc = 0usize;
        for (i, chip) in chips.iter().enumerate() {
            let s = if i == 0 { 0 } else { sep_w };
            let w = chip_w(chip);
            // Reserve room for a "+N" marker when chips would remain hidden.
            let remaining_after = chips.len() - i - 1;
            let reserve = if remaining_after > 0 {
                1 + 1 + count_digits(remaining_after)
            } else {
                0
            };
            if acc + s + w + reserve > budget {
                break;
            }
            acc += s + w;
            shown += 1;
        }
        let hidden = chips.len() - shown;
        if hidden > 0 {
            acc += 1 + 1 + count_digits(hidden);
        }
        (shown, acc)
    };

    // Pick the richest right tail that still leaves room for the chips:
    // tally + hint, then tally only, then no tail at all.
    let mut tail_configs: Vec<usize> = Vec::new();
    if hint_w > 0 {
        tail_configs.push(tally_w + hint_w);
    }
    tail_configs.push(tally_w);
    tail_configs.push(0);
    let mut shown = 0usize;
    let mut chips_used = 0usize;
    let mut tail_w = 0usize;
    for tw in tail_configs {
        let reserved = if tw > 0 { tw + gap } else { 0 };
        let budget = match width.checked_sub(lead_w + reserved) {
            Some(b) => b,
            None => continue,
        };
        let (s, u) = fit_chips(budget);
        if s > 0 || tw == 0 {
            shown = s;
            chips_used = u;
            tail_w = tw;
            break;
        }
    }
    let show_hint = tail_w > tally_w;
    let show_tally = tail_w > 0;

    // ---- Task label allocation: share leftover width across shown chips ----
    // Only when every chip already fits does the strip spend columns on task
    // labels, splitting the slack evenly (each capped at CHIP_TASK_MAX_W,
    // dropped entirely below CHIP_TASK_MIN_W so we never show "·…").
    let per_task_w: usize = {
        let budget = width.saturating_sub(lead_w + if tail_w > 0 { tail_w + gap } else { 0 });
        let leftover = budget.saturating_sub(chips_used);
        let task_count = chips
            .iter()
            .take(shown)
            .filter(|c| c.task.is_some())
            .count();
        if shown == chips.len() && task_count > 0 {
            // +1 per label for the '·' separator.
            let per = leftover / task_count;
            if per > CHIP_TASK_MIN_W {
                (per - 1).min(CHIP_TASK_MAX_W)
            } else {
                0
            }
        } else {
            0
        }
    };

    let mut spans: Vec<Span<'static>> = lead;
    let mut task_used = 0usize;
    let used: usize;
    if shown == 0 && !chips.is_empty() {
        // Degenerate width: show the first chip truncated.
        let c = &chips[0];
        let prefix_w = if c.is_sel && focused { 2 } else { 0 }; // '▸ ' width
        let batch_pfx_w = if c.is_batch { 2 } else { 0 }; // '✓ ' width
        let budget = width.saturating_sub(lead_w + if show_tally { tail_w + gap } else { 0 });
        let avail = budget.saturating_sub(disp_w(&c.glyph) + 1 + prefix_w + batch_pfx_w);
        let name = truncate_label(&c.name, avail.max(1));
        let mut style = if c.active {
            Style::default().fg(c.color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(rgb(110, 110, 125))
        };
        if c.is_batch {
            style = style.bg(rgb(20, 30, 40));
        }
        let mut prefix = String::new();
        if c.is_sel && focused {
            prefix.push_str("▸ ");
        }
        if c.is_batch {
            prefix.push_str("\u{2713} ");
        }
        spans.push(Span::styled(
            format!("{prefix}{} {}", c.glyph, name),
            style,
        ));
        used = prefix_w + batch_pfx_w + disp_w(&c.glyph) + 1 + disp_w(&name);
    } else {
        for (i, chip) in chips.iter().take(shown).enumerate() {
            if i > 0 {
                spans.push(Span::raw(CHIP_SEP));
            }
            let mut style = if chip.active {
                Style::default().fg(chip.color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(rgb(110, 110, 125))
            };
            if chip.is_batch {
                style = style.bg(rgb(20, 30, 40));
            }
            if chip.is_sel && focused {
                style = style
                    .add_modifier(Modifier::REVERSED | Modifier::UNDERLINED);
            } else if chip.is_sel {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            let mut prefix = String::new();
            if chip.is_sel && focused {
                prefix.push_str("▸ ");
            }
            if chip.is_batch {
                prefix.push_str("\u{2713} ");
            }
            spans.push(Span::styled(
                format!("{prefix}{} {}", chip.glyph, chip.name),
                style,
            ));
            if per_task_w > 0
                && let Some(task) = &chip.task
            {
                let label = truncate_label(task, per_task_w);
                task_used += 1 + disp_w(&label);
                spans.push(Span::styled(
                    format!("·{label}"),
                    Style::default().fg(rgb(150, 150, 160)),
                ));
            }
            if let Some(todo) = &chip.todo {
                spans.push(Span::styled(
                    format!(" {todo}"),
                    Style::default().fg(rgb(130, 130, 140)),
                ));
            }
        }
        let hidden = chips.len().saturating_sub(shown);
        if hidden > 0 {
            spans.push(Span::styled(
                format!(" +{hidden}"),
                Style::default().fg(rgb(140, 140, 150)),
            ));
        }
        used = chips_used + task_used;
    }

    // ---- Right-align the tail (tally [+ hint]) ----
    if show_tally {
        let consumed = lead_w + used;
        if consumed + gap + tail_w <= width {
            let pad = width - consumed - tail_w;
            spans.push(Span::raw(" ".repeat(pad)));
            spans.push(Span::styled(
                tally,
                Style::default().fg(if active > 0 {
                    rgb(255, 200, 100)
                } else {
                    rgb(120, 120, 130)
                }),
            ));
            if show_hint && let Some(hint) = hint_text {
                spans.push(Span::styled(
                    hint_sep.to_string(),
                    Style::default().fg(rgb(80, 80, 90)),
                ));
                spans.push(Span::styled(
                    hint.to_string(),
                    Style::default().fg(rgb(110, 130, 170)),
                ));
            }
        }
    }

    let mut out = vec![Line::from(spans)];

    // ---- Focused extras: expanded detail viewport + hint line ----
    if focused {
        // Budget: chips line (already emitted) + hint line are fixed; the
        // detail viewport gets the rest. Degrade to one inline line when the
        // budget is too small for the expanded view.
        let hint_rows = usize::from(!hints.is_empty());
        let detail_budget = max_height.saturating_sub(1 + hint_rows);
        if let Some(m) = ordered.get(selected) {
            if detail_budget >= 4 {
                out.extend(render_hovered_detail(
                    m,
                    spinner_frame,
                    width,
                    detail_budget,
                ));
            } else {
                // Compact fallback: a single inline line of the latest output.
                let detail = m
                    .body
                    .iter()
                    .rev()
                    .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('·'))
                    .cloned()
                    .unwrap_or_else(|| format!("[{}]", m.status));
                let prefix = format!("   {} ", status_glyph(&m.status, spinner_frame));
                let prefix_w = prefix.chars().count();
                let body = truncate_label(&detail, width.saturating_sub(prefix_w));
                out.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(role_color(m.role.as_deref()).unwrap_or_else(|| status_accent(&m.status)))),
                    Span::styled(body, Style::default().fg(rgb(180, 180, 190))),
                ]));
            }
        }

        if !hints.is_empty() {
            let mut hint_spans: Vec<Span<'static>> = vec![Span::raw("   ")];
            for (i, h) in hints.iter().enumerate() {
                if i > 0 {
                    hint_spans.push(Span::styled(" · ", Style::default().fg(rgb(80, 80, 90))));
                }
                hint_spans.push(Span::styled(
                    h.key.clone(),
                    Style::default().fg(rgb(150, 170, 210)),
                ));
                hint_spans.push(Span::raw(" "));
                hint_spans.push(Span::styled(
                    h.label.clone(),
                    Style::default().fg(rgb(120, 120, 130)),
                ));
            }
            // Trim to width.
            let mut total = 0usize;
            let mut trimmed: Vec<Span<'static>> = Vec::new();
            for s in hint_spans {
                let w = s.content.chars().count();
                if total + w > width {
                    break;
                }
                total += w;
                trimmed.push(s);
            }
            out.push(Line::from(trimmed));
        }
    }

    // Hard bound: no matter how the budgeting above worked out, never emit a
    // line wider than the strip (degenerate widths, wide glyphs, etc.).
    for line in &mut out {
        clamp_line_to_width(line, width);
    }

    out
}


/// Render the vertical swarm strip shown directly above the status line: one
/// agent per row, capped to `max_rows` agent lines with a `+N more` overflow
/// marker on the last row.
///
/// Each row is `<status glyph> <icon> · <task>` with a right-aligned todo
/// counter when present. The first row carries the leading "🐝" swarm marker;
/// the header tally (`M/N active`) and the enter hint live on the first row's
/// right side, mirroring the horizontal strip.
///
/// - Unfocused: up to `max_rows` agent rows.
/// - Focused: an accordion. The selected agent's row gains a `▸` marker and
///   its full icon+name, and its live transcript tail + todos expand in place
///   directly beneath its row; a keybinding hint line closes the strip. All
///   bounded by `max_height` total lines.
///
/// ```text
/// 🐝 ⠙ 🦊 · wire the auth flow 3/9            2/3 active · alt+n controls
///  ▸ ⠹ 🐝 bee · audit the webhook path
///    │ checking the signing secret path
///    │ ▸ verify replay protection
///    ✓ 🐅 · support/contact page
///    alt+↑/↓ select · alt+o open · alt+shift+p prompt · esc exit
/// ```
#[allow(clippy::too_many_arguments)]
pub fn render_swarm_strip_vertical(
    members: &[GalleryMember],
    selected: usize,
    focused: bool,
    hints: &[SwarmStripHint],
    enter_hint: Option<&str>,
    spinner_frame: usize,
    width: usize,
    max_rows: usize,
    max_height: usize,
    batch_selected: Option<&std::collections::HashSet<usize>>,
) -> Vec<Line<'static>> {
    if members.is_empty() || width < 8 || max_rows == 0 {
        return Vec::new();
    }
    let ordered = sort_members_for_display(members);
    let selected = selected.min(ordered.len().saturating_sub(1));
    let active = members
        .iter()
        .filter(|m| is_active_status(&m.status))
        .count();

    // Row budget: reserve one row for "+N more" when not everything fits.
    let shown = if ordered.len() <= max_rows {
        ordered.len()
    } else {
        max_rows.saturating_sub(1).max(1)
    };
    let hidden = ordered.len() - shown;

    // Keep the selected agent visible: window the list around the selection.
    let start = if selected >= shown {
        selected + 1 - shown
    } else {
        0
    };

    // ---- First-row right tail: "M/N active" plus the controls hint. ----
    let tally = format!("{active}/{} active", members.len());
    let tally_w = disp_w(&tally);
    let hint_text = if focused { None } else { enter_hint };
    let hint_sep = " · ";
    let gap = 2usize;
    let hint_w = hint_text.map(|h| disp_w(h) + disp_w(hint_sep)).unwrap_or(0);

    const LEAD: &str = "🐝 ";
    const INDENT: &str = "   ";
    let lead_w = disp_w(LEAD);

    let mut out: Vec<Line<'static>> = Vec::new();
    // Where the selected agent's row landed in `out` (focused accordion).
    let mut selected_row_at: Option<usize> = None;
    for (row, m) in ordered.iter().enumerate().skip(start).take(shown) {
        let first = out.is_empty();
        let is_sel = row == selected;
        let color = role_color(m.role.as_deref()).unwrap_or_else(|| status_accent(&m.status));
        let glyph = status_glyph(&m.status, spinner_frame);
        let todo = m.todo.map(|(done, total)| format!("{done}/{total}"));

        let mut spans: Vec<Span<'static>> = Vec::new();
        if first {
            spans.push(Span::styled(
                LEAD.to_string(),
                Style::default().fg(rgb(255, 200, 100)),
            ));
        } else {
            spans.push(Span::raw(INDENT));
        }

        // In batch mode, prepend a checkbox for each agent.
        if let Some(selected_set) = batch_selected {
            let checked = selected_set.contains(&(start + row));
            let check = if checked { "[\u{2713}] " } else { "[ ] " };
            let check_color = if checked { rgb(100, 200, 100) } else { rgb(100, 100, 110) };
            spans.push(Span::styled(
                check.to_string(),
                Style::default().fg(check_color),
            ));
        }

        // Right tail only on the first row; degrade by dropping hint first.
        let (row_tail_w, show_hint) = if first {
            if lead_w + gap + tally_w + hint_w + 16 <= width && hint_w > 0 {
                (tally_w + hint_w, true)
            } else if lead_w + gap + tally_w + 12 <= width {
                (tally_w, false)
            } else {
                (0, false)
            }
        } else {
            (0, false)
        };

        let body_budget = width
            .saturating_sub(lead_w)
            .saturating_sub(if row_tail_w > 0 { row_tail_w + gap } else { 0 });

        // Expanded workers use the full-width information card header approved
        // for the inline swarm view. Compact rows retain the older chip layout.
        if focused && is_sel && !m.todo_items.is_empty() {
            let left = format!("{} ", m.label);
            spans.push(Span::styled(
                left.clone(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));

            let mut metadata = vec![format!(
                "{} {}",
                status_glyph(&m.status, spinner_frame),
                card_status_label(&m.status)
            )];
            if let Some((done, total)) = m.todo {
                metadata.push(format!("Todo {done}/{total}"));
            }
            if let Some(elapsed) = m.elapsed_secs {
                metadata.push(format_elapsed(elapsed));
            }
            if let Some(model) = m.model.as_deref().filter(|model| !model.trim().is_empty()) {
                metadata.push(format_model(model));
            }

            let mut tail = metadata.join(" · ");
            while metadata.len() > 1 && lead_w + disp_w(&left) + gap + disp_w(&tail) > width {
                metadata.pop();
                tail = metadata.join(" · ");
            }
            let consumed = lead_w + disp_w(&left);
            if consumed + gap + disp_w(&tail) <= width {
                spans.push(Span::raw(" ".repeat(width - consumed - disp_w(&tail))));
                spans.push(Span::styled(tail, Style::default().fg(rgb(150, 150, 160))));
            }
            if is_sel {
                selected_row_at = Some(out.len());
            }
            out.push(Line::from(spans));
            continue;
        }

        // <glyph> [icon ]<name>[ · task][ done/total]
        let row_active = is_active_status(&m.status);
        let mut style = if row_active {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(rgb(110, 110, 125))
        };
        let is_batch_row = batch_selected.is_some_and(|set| set.contains(&(start + row)));
        if is_batch_row {
            style = style.bg(rgb(20, 30, 40));
        }
        if is_sel {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        // The assigned animal icon already identifies the worker. Keep compact
        // strip rows emoji-only so names such as "sauropod" do not consume the
        // task-label space; the expanded chat card carries the full identity.
        let ident = match m.icon.as_deref().filter(|i| !i.is_empty()) {
            Some(icon) => icon.to_string(),
            None => m.label.clone(),
        };
        let marker = if focused {
            if is_sel { "▸ " } else { "  " }
        } else {
            ""
        };
        let head = format!("{marker}{glyph} {ident}");
        let head_w = disp_w(&head);
        let todo_w = todo.as_ref().map(|t| disp_w(t) + 1).unwrap_or(0);
        let mut used = head_w;
        spans.push(Span::styled(head, style));

        if let Some(task) = m.task.as_deref().filter(|t| !t.trim().is_empty()) {
            let avail = body_budget.saturating_sub(used + todo_w + disp_w(" · "));
            if avail >= CHIP_TASK_MIN_W {
                let label = truncate_label(task, avail);
                used += disp_w(" · ") + disp_w(&label);
                spans.push(Span::styled(
                    format!(" · {label}"),
                    Style::default().fg(rgb(150, 150, 160)),
                ));
            }
        }
        if let Some(todo) = &todo
            && used + todo_w <= body_budget
        {
            used += todo_w;
            spans.push(Span::styled(
                format!(" {todo}"),
                Style::default().fg(rgb(130, 130, 140)),
            ));
        }

        // ---- Right-align the first-row tail ----
        if row_tail_w > 0 {
            let consumed = lead_w + used;
            if consumed + gap + row_tail_w <= width {
                let pad = width - consumed - row_tail_w;
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(Span::styled(
                    tally.clone(),
                    Style::default().fg(if active > 0 {
                        rgb(255, 200, 100)
                    } else {
                        rgb(120, 120, 130)
                    }),
                ));
                if show_hint && let Some(hint) = hint_text {
                    spans.push(Span::styled(
                        hint_sep.to_string(),
                        Style::default().fg(rgb(80, 80, 90)),
                    ));
                    spans.push(Span::styled(
                        hint.to_string(),
                        Style::default().fg(rgb(110, 130, 170)),
                    ));
                }
            }
        }
        if is_sel {
            selected_row_at = Some(out.len());
        }
        out.push(Line::from(spans));
    }

    if hidden > 0 {
        out.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(
                format!("+{hidden} more"),
                Style::default().fg(rgb(140, 140, 150)),
            ),
        ]));
    }

    // ---- Accordion detail under the selected row ----
    // Expanded details belong to the explicitly focused control surface. The
    // always-visible live card is rendered in chat beneath the spawning tool
    // call, so the persistent status strip stays compact.
    if focused {
        let hint_rows = usize::from(focused && !hints.is_empty());
        let detail_budget = max_height.saturating_sub(out.len() + hint_rows);
        if let (Some(m), Some(at)) = (ordered.get(selected), selected_row_at)
            && detail_budget >= 1
        {
            // Insert directly beneath the selected row so the list expands in
            // place (accordion) instead of jumping to a detached pane below.
            let detail = hovered_detail_body(m, Some(spinner_frame), width, detail_budget, true);
            for (i, line) in detail.into_iter().enumerate() {
                out.insert(at + 1 + i, line);
            }
        }
        if focused && !hints.is_empty() {
            let mut hint_spans: Vec<Span<'static>> = vec![Span::raw(INDENT)];
            for (i, h) in hints.iter().enumerate() {
                if i > 0 {
                    hint_spans.push(Span::styled(" · ", Style::default().fg(rgb(80, 80, 90))));
                }
                hint_spans.push(Span::styled(
                    h.key.clone(),
                    Style::default().fg(rgb(150, 170, 210)),
                ));
                hint_spans.push(Span::raw(" "));
                hint_spans.push(Span::styled(
                    h.label.clone(),
                    Style::default().fg(rgb(120, 120, 130)),
                ));
            }
            out.push(Line::from(hint_spans));
        }
    }

    for line in &mut out {
        clamp_line_to_width(line, width);
    }
    out
}

