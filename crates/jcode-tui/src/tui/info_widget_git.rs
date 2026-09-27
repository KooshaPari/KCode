//! Changes widget: the detail layer behind the status line's git segment.
//!
//! The overscroll status line already owns the branch and the dirty/ahead
//! counts (`main ~3 ?2 ↑1`). This widget never repeats those. It answers the
//! follow-up question instead: *which* files are dirty.

use super::text::truncate_smart;
use super::{DirtyFile, GitInfo, InfoWidgetData};
use crate::tui::color_support::rgb;
use ratatui::prelude::*;

/// Maximum file rows before collapsing the rest into a `+N more` row.
pub(super) const CHANGES_MAX_FILES: usize = 5;

/// Whether the Changes widget has anything to show. A clean tree (or one that
/// is only ahead/behind) has no file detail, and the status line already
/// covers ahead/behind.
pub(super) fn changes_has_data(info: &GitInfo) -> bool {
    !info.dirty_files.is_empty()
}

/// Content rows the Changes widget renders, mirroring [`render_git_widget`].
pub(super) fn changes_height(info: &GitInfo) -> u16 {
    if !changes_has_data(info) {
        return 0;
    }
    // Up to CHANGES_MAX_FILES rows. When files overflow, the last of those
    // rows becomes `+N more`, so the height never exceeds the cap.
    let total = info.dirty_total.max(info.dirty_files.len());
    let shown = info.dirty_files.len().min(CHANGES_MAX_FILES);
    if total > shown {
        CHANGES_MAX_FILES.min(shown + 1) as u16
    } else {
        shown as u16
    }
}

pub(super) fn render_git_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    let Some(info) = &data.git_info else {
        return Vec::new();
    };
    if !changes_has_data(info) {
        return Vec::new();
    }

    let w = inner.width as usize;
    let total = info.dirty_total.max(info.dirty_files.len());
    let mut max_files = (inner.height as usize).min(CHANGES_MAX_FILES);
    if total > max_files && max_files > 0 {
        // Reserve the last row for the overflow count.
        max_files -= 1;
    }

    let mut lines: Vec<Line<'static>> = info
        .dirty_files
        .iter()
        .take(max_files)
        .map(|file| changes_file_line(file, w))
        .collect();

    let hidden = total.saturating_sub(lines.len());
    if hidden > 0 {
        lines.push(Line::from(Span::styled(
            format!("  +{hidden} more"),
            Style::default().fg(rgb(100, 100, 115)),
        )));
    }
    lines
}

fn changes_file_line(file: &DirtyFile, width: usize) -> Line<'static> {
    let (letter_color, path_color) = match file.status {
        'A' => (rgb(100, 200, 100), rgb(170, 190, 170)),
        'D' => (rgb(255, 120, 110), rgb(150, 130, 130)),
        'R' => (rgb(140, 180, 255), rgb(160, 170, 190)),
        'U' => (rgb(255, 90, 90), rgb(230, 170, 160)),
        '?' => (rgb(120, 120, 135), rgb(130, 130, 145)),
        _ => (rgb(240, 200, 80), rgb(170, 170, 180)),
    };
    Line::from(vec![
        Span::styled(
            format!("{} ", file.status),
            Style::default().fg(letter_color).bold(),
        ),
        Span::styled(
            truncate_smart(&changes_display_path(&file.path), width.saturating_sub(2)),
            Style::default().fg(path_color),
        ),
    ])
}

/// Show the file name first: the tail of the path is what identifies it in a
/// narrow column. `crates/a/src/tool/mod.rs` becomes `tool/mod.rs` for
/// generic names and `turn_execution.rs` otherwise. Renames keep the target.
pub(super) fn changes_display_path(path: &str) -> String {
    let path = path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"');
    let trimmed = path.trim_end_matches('/');
    let mut segments = trimmed.rsplit('/');
    let name = segments.next().unwrap_or(trimmed);
    let generic = matches!(
        name,
        "mod.rs" | "lib.rs" | "main.rs" | "index.ts" | "index.js" | "__init__.py" | "Cargo.toml"
    );
    let base = match (generic, segments.next()) {
        (true, Some(parent)) => format!("{parent}/{name}"),
        _ => name.to_string(),
    };
    if path.ends_with('/') {
        format!("{base}/")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(files: &[(char, &str)], total: usize) -> GitInfo {
        GitInfo {
            branch: "main".to_string(),
            modified: files.len(),
            staged: 0,
            untracked: 0,
            ahead: 1,
            behind: 0,
            dirty_files: files.iter().map(|(s, p)| DirtyFile::new(*s, *p)).collect(),
            dirty_total: total,
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
    fn changes_lists_files_without_repeating_branch_or_counts() {
        let data = InfoWidgetData {
            git_info: Some(info(
                &[
                    ('M', "crates/x/src/agent/turn_execution.rs"),
                    ('?', "notes.md"),
                ],
                2,
            )),
            ..Default::default()
        };
        let out = text(&render_git_widget(&data, Rect::new(0, 0, 30, 5)));
        assert!(out.contains("M turn_execution.rs"), "{out}");
        assert!(out.contains("? notes.md"), "{out}");
        assert!(!out.contains("main"), "branch belongs to the status line: {out}");
        assert!(!out.contains("↑1"), "counts belong to the status line: {out}");
    }

    #[test]
    fn changes_overflow_counts_files_beyond_the_capture_cap() {
        let files: Vec<(char, &str)> = (0..10).map(|_| ('M', "a.rs")).collect();
        let git = info(&files, 23);
        let data = InfoWidgetData {
            git_info: Some(git.clone()),
            ..Default::default()
        };
        let lines = render_git_widget(&data, Rect::new(0, 0, 30, 10));
        assert_eq!(lines.len() as u16, changes_height(&git));
        assert!(text(&lines).contains("+19 more"), "{}", text(&lines));
    }

    #[test]
    fn clean_or_ahead_only_tree_has_no_changes_widget() {
        assert!(!changes_has_data(&info(&[], 0)));
        assert_eq!(changes_height(&info(&[], 0)), 0);
    }

    #[test]
    fn display_path_keeps_parent_for_generic_names_and_rename_targets() {
        assert_eq!(changes_display_path("crates/a/src/tool/mod.rs"), "tool/mod.rs");
        assert_eq!(changes_display_path("src/foo.rs"), "foo.rs");
        assert_eq!(changes_display_path("old.rs -> new/place.rs"), "place.rs");
        assert_eq!(changes_display_path("scratch/"), "scratch/");
    }

    #[test]
    fn porcelain_pairs_collapse_to_one_letter() {
        use crate::tui::app::helpers::porcelain_status_letter as l;
        assert_eq!(l(b'?', b'?'), '?');
        assert_eq!(l(b' ', b'M'), 'M');
        assert_eq!(l(b'M', b' '), 'M');
        assert_eq!(l(b'A', b' '), 'A');
        assert_eq!(l(b'A', b'M'), 'M');
        assert_eq!(l(b' ', b'D'), 'D');
        assert_eq!(l(b'R', b' '), 'R');
        assert_eq!(l(b'U', b'U'), 'U');
        assert_eq!(l(b'A', b'A'), 'U');
    }
}
