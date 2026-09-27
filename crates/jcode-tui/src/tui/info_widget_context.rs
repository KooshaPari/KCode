//! Context mix widget: the detail layer behind the status line's context gauge.
//!
//! The overscroll status line already shows how full the window is
//! (`74k/256k ▰▰▰▱▱ 29%`). This widget never repeats that total or percentage.
//! It explains it instead: which kinds of content are filling the window, so
//! the user can see what compaction or a fresh session would actually free.

use super::InfoWidgetData;
use crate::prompt::ContextInfo;
use crate::tui::color_support::rgb;
use ratatui::prelude::*;

/// Most categories shown. The smallest buckets are dropped first since they
/// are the least actionable.
const CONTEXT_MIX_MAX_ROWS: usize = 4;
/// Width of the label column (`tool out`, `chat`, ...).
const LABEL_WIDTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MixRow {
    pub label: &'static str,
    pub tokens: usize,
    color: (u8, u8, u8),
}

/// Split the context into user-meaningful buckets, largest first.
///
/// Char counts are the only per-category signal available, so each bucket is
/// estimated from chars and then scaled so the buckets sum to the provider's
/// observed token count when one exists. That keeps the rows consistent with
/// the `74k` shown on the status line instead of a second, different total.
pub(super) fn context_mix_rows(data: &InfoWidgetData) -> Vec<MixRow> {
    let Some(info) = &data.context_info else {
        return Vec::new();
    };
    if info.total_chars == 0 {
        return Vec::new();
    }

    let system_chars = info
        .prompt_prefix_chars()
        .saturating_sub(info.tool_defs_chars);
    let buckets: [(&'static str, usize, (u8, u8, u8)); 5] = [
        ("tool out", info.tool_results_chars, (240, 170, 90)),
        (
            "chat",
            info.user_messages_chars + info.assistant_messages_chars,
            (140, 180, 255),
        ),
        ("tool in", info.tool_calls_chars, (200, 150, 230)),
        ("tool def", info.tool_defs_chars, (120, 200, 190)),
        ("system", system_chars, (150, 150, 165)),
    ];

    let bucket_chars: usize = buckets.iter().map(|(_, chars, _)| chars).sum();
    if bucket_chars == 0 {
        return Vec::new();
    }
    let total_tokens = data
        .observed_context_tokens
        .map(|t| t as usize)
        .unwrap_or_else(|| ContextInfo::estimated_tokens(info));

    let mut rows: Vec<MixRow> = buckets
        .into_iter()
        .filter(|(_, chars, _)| *chars > 0)
        .map(|(label, chars, color)| MixRow {
            label,
            tokens: ((chars as f64 / bucket_chars as f64) * total_tokens as f64).round() as usize,
            color,
        })
        .filter(|row| row.tokens > 0)
        .collect();
    rows.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    rows.truncate(CONTEXT_MIX_MAX_ROWS);
    rows
}

/// Content rows the widget renders, mirroring [`render_context_mix_widget`].
pub(super) fn context_mix_height(data: &InfoWidgetData) -> u16 {
    let rows = context_mix_rows(data).len() as u16;
    if rows == 0 {
        return 0;
    }
    rows + u16::from(compaction_hint(data).is_some())
}

pub(super) fn render_context_mix_widget(
    data: &InfoWidgetData,
    inner: Rect,
) -> Vec<Line<'static>> {
    let rows = context_mix_rows(data);
    if rows.is_empty() {
        return Vec::new();
    }

    let width = inner.width as usize;
    let max_tokens = rows.iter().map(|r| r.tokens).max().unwrap_or(1).max(1);
    let amount_width = rows
        .iter()
        .map(|r| format_tokens(r.tokens).chars().count())
        .max()
        .unwrap_or(3);
    // label + space + bar + space + amount
    let bar_width = width.saturating_sub(LABEL_WIDTH + 1 + 1 + amount_width);
    let label_style = Style::default().fg(rgb(150, 150, 160));
    let amount_style = Style::default().fg(rgb(130, 130, 145));
    let track_style = Style::default().fg(rgb(60, 60, 70));

    let mut lines: Vec<Line<'static>> = rows
        .iter()
        .map(|row| {
            let mut spans = vec![Span::styled(
                format!("{:<LABEL_WIDTH$} ", row.label),
                label_style,
            )];
            if bar_width >= 2 {
                let (filled, partial) = bar_cells(row.tokens, max_tokens, bar_width);
                let (r, g, b) = row.color;
                let mut bar = "█".repeat(filled);
                if let Some(ch) = partial {
                    bar.push(ch);
                }
                let used = filled + usize::from(partial.is_some());
                spans.push(Span::styled(bar, Style::default().fg(rgb(r, g, b))));
                spans.push(Span::styled(
                    " ".repeat(bar_width.saturating_sub(used)),
                    track_style,
                ));
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("{:>amount_width$}", format_tokens(row.tokens)),
                amount_style,
            ));
            Line::from(spans)
        })
        .collect();

    if let Some(hint) = compaction_hint(data) {
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(rgb(120, 210, 230)),
        )));
    }
    lines
}

/// Native compaction trigger, the one context fact the status line lacks.
fn compaction_hint(data: &InfoWidgetData) -> Option<String> {
    let threshold = data.native_compaction_threshold_tokens?;
    data.native_compaction_mode.as_ref()?;
    Some(format!("compacts at {}", format_tokens(threshold)))
}

/// Filled cells plus an optional eighth-block remainder, so small categories
/// still show a sliver instead of vanishing.
fn bar_cells(value: usize, max: usize, width: usize) -> (usize, Option<char>) {
    const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let eighths = ((value as f64 / max as f64) * (width * 8) as f64).round() as usize;
    let eighths = eighths.clamp(1, width * 8);
    let full = eighths / 8;
    let rem = eighths % 8;
    let partial = (rem > 0).then(|| EIGHTHS[rem - 1]);
    (full, partial)
}

fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{}k", (tokens as f64 / 1_000.0).round() as usize)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> InfoWidgetData {
        InfoWidgetData {
            context_info: Some(ContextInfo {
                system_prompt_chars: 16_000,
                tool_defs_chars: 36_000,
                user_messages_chars: 40_000,
                assistant_messages_chars: 48_000,
                tool_calls_chars: 4_000,
                tool_results_chars: 164_000,
                total_chars: 308_000,
                ..Default::default()
            }),
            context_limit: Some(256_000),
            ..Default::default()
        }
    }

    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn rows_are_largest_first_and_capped() {
        let rows = context_mix_rows(&data());
        let labels: Vec<_> = rows.iter().map(|r| r.label).collect();
        assert_eq!(labels, ["tool out", "chat", "tool def", "system"]);
    }

    #[test]
    fn rows_scale_to_observed_tokens_so_they_match_the_status_line() {
        let mut d = data();
        d.observed_context_tokens = Some(74_000);
        let sum: usize = context_mix_rows(&d).iter().map(|r| r.tokens).sum();
        // The dropped `tool in` bucket is ~1% of the total.
        assert!((72_000..=74_000).contains(&sum), "sum {sum}");
    }

    #[test]
    fn widget_never_repeats_the_status_line_total_or_percentage() {
        let mut d = data();
        d.observed_context_tokens = Some(74_000);
        let out = text(&render_context_mix_widget(&d, Rect::new(0, 0, 24, 6)));
        assert!(!out.contains('%'), "{out}");
        assert!(!out.contains("256k"), "{out}");
        assert!(!out.contains("74k"), "{out}");
        assert!(out.contains("tool out"), "{out}");
    }

    #[test]
    fn height_matches_rendered_rows_including_compaction_hint() {
        let mut d = data();
        d.native_compaction_mode = Some("auto".to_string());
        d.native_compaction_threshold_tokens = Some(200_000);
        let lines = render_context_mix_widget(&d, Rect::new(0, 0, 24, 8));
        assert_eq!(lines.len() as u16, context_mix_height(&d));
        assert!(text(&lines).contains("compacts at 200k"));
    }

    #[test]
    fn empty_context_renders_nothing() {
        let d = InfoWidgetData::default();
        assert!(render_context_mix_widget(&d, Rect::new(0, 0, 24, 6)).is_empty());
        assert_eq!(context_mix_height(&d), 0);
    }
}
