# Session: TUI Swarm Panel Visual Differentiation

## Goals
Improve swarm panel readability in crowded multi-pane layouts (4x2 to 6x6+).

## Completed Features

### v0.85.0 (shipped)
1. **Grid selection highlighting** (`bb575ef`) — selected cell gets accent `╭─` border + `▸` prefix, inactive dimmed
2. **Strip/dock active-vs-idle** (`4d3232c`) — running agents get accent+bold, idle get dim `rgb(110,110,125)`
3. **Background tinting** (`531e215`) — selected tile `rgb(25,25,35)`, active not-selected `rgb(20,20,30)`
4. **Per-role border colors** (`9861b1e`) — coordinator=gold, worker=blue, reviewer=green, researcher=purple
5. **Selected param threading** (`cc69dea`) — wired `selected: Option<usize>` through gallery callers

### Post-v0.85.0 (unreleased)
6. **Integration tests** (`74b975d`) — 10 fuzz audit tests
7. **Swarm plan DAG visualization** (`5efe66c`) — render_swarm_plan_dag() with status indicators
8. **Agent search/filter** (`3a76ea7`) — `/` key enters filter mode
9. **Gallery decomposition** (`310a2d8ac`) — 3232-line swarm_gallery.rs → 10 domain modules
10. **Detail card enhancements** (`651a56395`) — elapsed time, effort, auth shown for selected agent
11. **Agent rename UI** (`6acbce03f`) — `r` key opens inline rename
12. **Summary bar** (`af07b0ee7`) — per-status agent counts (running/queued/done/failed/idle)
13. **Multi-select batch** (`f8f7725b5`) — Space/Ctrl+a for batch selection with ✓ indicators

## Keyboard bindings
- `Alt+j/k` or `Alt+Up/Down` — select agent
- `Space` — toggle batch selection
- `Ctrl+a` — select all agents
- `r` — rename agent
- `/` — filter agents
- `Alt+o` or `Alt+Enter` — pop-out selected agent
- `Alt+Shift+p` — swarm prompt
- `Alt+n` — full-page swarm panel
- `Esc` — clear selection / exit focus

## Architecture
- `jcode-tui-render/src/swarm_gallery/` — pure rendering (ratatui, no app state)
  - `util.rs` — status_accent, role_glyph, role_color, summary_line helpers
  - `types.rs` — GalleryMember, GalleryTodo, GalleryToolIntent structs
  - `strip.rs` — horizontal/vertical agent strip rendering
  - `dock.rs` — bottom ticker dock
  - `panel.rs` — full-page swarm panel
  - `card.rs` — agent detail cards
  - `hover.rs` — hover/focus detail popups
  - `render.rs` — gallery grid, header, tile conversion
  - `tests.rs` — 64 passing fuzz audit tests
- `jcode-tui/src/tui/info_widget_swarm_gallery.rs` — bridges protocol types to render layer
- `jcode-tui/src/tui/app/tui_state.rs` — keyboard handling, selection state

## Blocked items (need protocol changes)
- Agent performance metrics (needs input_tokens/output_tokens on SwarmMemberRuntime)
- Auto-scaling hints (needs queue depth data)
- Cost budget alerts (needs token cost calculation)
