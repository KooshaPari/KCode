# Nerd Font Research - 2026-09-13

## Decision: Stick with Plain Unicode for Live TUI

### Current State
- Terminal TUI uses plain Unicode symbols (no Nerd Font glyphs)
- Video export (SVG) already uses "Symbols Nerd Font" as CSS font-family fallback
- No Nerd Font runtime detection exists in codebase

### Why Not Nerd Fonts in TUI
1. **PUA width instability** — Nerd Font glyphs (U+E000-U+F8FF) have unpredictable terminal widths (0/1/2 cells), breaking grid alignment and truncate_line_for_narrow() budget math
2. **No reliable runtime detection** — Cannot query terminal font capabilities; env var adds UX burden
3. **Breaks non-Nerd-Font terminals** — Renders as replacement characters/blank boxes
4. **Video export already uses them** — Correct place for Nerd Fonts is rendered images, not live terminal
5. **Current symbols are strong** — Braille spinner is best-in-class; check/cross/circle are universal

### Current Symbol Inventory

| Usage | Symbol | Unicode | Width-stable |
|-------|--------|---------|-------------|
| Spinner | ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ | Braille | Yes |
| Completed | ✓ | U+2713 | Yes |
| Ready | • | U+2022 | Yes |
| Blocked | ⏸ | U+23F8 | Yes |
| Failed | ✗ | U+2717 | Yes |
| Stopped | ◼ | U+25FC | Yes |
| Warning | ▲ | U+25B2 | Yes |
| Memory | ◉ | U+25C9 | Yes |
| Compaction | ◧ | U+25E7 | Yes |
| Clipboard | ◫ | U+25EB | Yes |
| Search | ◎ | U+25CE | Yes |
| Target | ◇ | U+25C7 | Yes |
| Reload | ⟳ | U+27F3 | Yes |
| Rate limit | ⧖ | U+29D6 | Yes |
| Todo filled | ● | U+25CF | Yes |
| Todo hollow | ○ | U+25CB | Yes |

### If Ever Needed: Safe Opt-In Pattern
- Add JCODE_NERD_FONT=on env var (default off)
- Fallback table in code: Nerd Font glyph → plain Unicode glyph
- Width validation at startup (render test characters, measure)
- Only applied to status glyphs in swarm_gallery.rs
- Never applied to content/body text

### Files Referenced
- `crates/jcode-tui/src/video_export.rs:654-655` — SVG Nerd Font CSS
- `crates/jcode-tui-render/src/swarm_gallery.rs` — status_glyph function
- `crates/jcode-tui/src/tui/ui_messages.rs` — prefer_width_stable_system_glyphs()
- `crates/jcode-core/src/output_style.rs` — emoji suppression system
