# Buffer::index panics with `index outside of buffer` on degenerate Rect areas

## Summary

`Buffer::index` at `ratatui-core/src/buffer/buffer.rs:250` (0.1.0) / `buffer.rs:251` (0.1.2, current main) panics when called with `(x, y)` outside the buffer's `Rect` area. This is reachable from any TUI that doesn't pre-flight terminal dimensions, including jcode (the 57×1 case below).

The panic is currently intentional and *documented* — the `Index<P> for Buffer` impl's rustdoc says "May panic if the given position is outside the buffer's area" — but the panic class is also a footgun that the maintainers have been quietly working around widget-by-widget (see Related). This issue is asking for a coherent fix at the `Buffer` level instead of inside every widget.

## Environment

- ratatui-core version: 0.1.0 (panic log); 0.1.2 is current on crates.io as of this filing
- ratatui version (facade): 0.30.0
- OS: macOS 15.0
- Terminal size at time of panic: 57 columns × 1 row (collapsed tmux/iTerm2 split)

## Stack trace

```
PANIC at 2026-09-18T00:19:47.879464+00:00: index outside of buffer: the area is Rect { x: 0, y: 0, width: 57, height: 1 } but index is (1, 1)
  at /Users/kooshapari/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-core-0.1.0/src/buffer/buffer.rs:250:13
```

(A second panic fired ~5s later at 2026-09-18T00:19:53.896Z from the same code path, indicating the TUI was re-panicking each frame.)

The line in 0.1.0 / 0.1.2 is the `panic!` macro inside the `unwrap_or_else` closure passed to `Buffer::index_of`:

```rust
pub fn index_of(&self, x: u16, y: u16) -> usize {
    self.index_of_opt(Position { x, y }).unwrap_or_else(|| {
        panic!(                                              // <-- here
            "index outside of buffer: the area is {area:?} but index is ({x}, {y})",
            area = self.area,
        )
    })
}
```

`Buffer::index_of` is called by both `Index<P>::index` and `IndexMut<P>::index_mut` (lines 534-538 and 562-566 in 0.1.2), so any `buf[(x, y)]` / `buf[(x, y)] = ...` / `Buffer::cell((x, y))` reaching an out-of-area coordinate panics. `set_string` / `set_stringn` / `set_span` reach it via the `self[(x, y)].set_symbol(...)` call inside their `for (symbol, width) in graphemes` loop (line 360 in 0.1.2). `with_lines` reaches it transitively via `set_line` → `set_stringn`.

## Minimal reproduction

1. Construct a `Buffer::empty(Rect::new(0, 0, 57, 1))` (57×1 area, single row).
2. Call `buffer.cell((1, 1))` — or any `set_string` / `set_stringn` / `set_span` / `with_lines` with `(x, y) = (1, 1)`.
3. Process panics with the above message.

```rust
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;

let area = Rect::new(0, 0, 57, 1);
let mut buf = Buffer::empty(area);
let _ = buf.cell((1, 1));   // panics here
```

A more realistic reproduction (what jcode actually did): a multi-row layout that is being rendered into a 1-row terminal will end up with `y = 1` for any non-zero `Constraint::Length` on the second row, and the very first widget that writes into that sub-area via `set_string` triggers this.

## Impact

Any TUI that lays out into a multi-row layout but is rendered in a single-row terminal (e.g. tmux split, narrow Warp/Terminal.app tab, ssh session with a one-line scrollback pane) will hit this. Affected downstream crates: jcode, possibly codex, helios-cli, and others built on ratatui 0.30.

The panic terminates the process with a `zsh: killed` message and an unhelpful `index outside of buffer` text — no backtrace unless `RUST_BACKTRACE=1` is set. The process is not restartable by the supervising shell without manual intervention, and the panic is the kind of thing an AI-coding-agent TUI will hit during a long-running session in a tight loop if the user resizes their terminal mid-session.

The maintainers have already had to patch the *same* panic class for individual widgets:

- [#2368](https://github.com/ratatui/ratatui/pull/2368) (v0.30.1) — "Avoid panic if Clear area is outside of buffer" for the `Clear` widget
- [#1670](https://github.com/ratatui/ratatui/pull/1670) — "Fix panic when rendering a `Paragraph` out of bounds" for `Paragraph`
- [#1996](https://github.com/ratatui/ratatui/pull/1996) — "Panic when rendering widgets on too small buffer" for `BarChart` and `RatatuiMascot`
- [#2368](https://github.com/ratatui/ratatui/pull/2368) — "Saturate block edge arithmetic" for `Block`

…and a new `#[should_panic]` regression test (`index_out_of_bounds_panics`, `buffer.rs:811-823` in 0.1.2) was added in 0.1.2 that *asserts* the panic. So the maintainers are aware of the panic and treat it as expected behaviour, but the cost of that policy is leaking the panic into every third-party TUI that uses `Index` or `set_string`.

## Suggested fix

Add bounds checks to `Buffer::index` (and the mutable counterpart) to return `None` (or use the existing `Buffer::cell` API which already returns `Option<&Cell>` for the bounds-checked variant) instead of panicking. The `Index<(usize, usize)>` impl could either:

- **Option A (preferred):** Remove the `Index` impl entirely and require callers to use `Buffer::cell((x, y))` which already returns `Option<&Cell>`. This is a breaking change but eliminates the panic class. The deprecation hint in the existing `Buffer::get` / `get_mut` rustdoc ("use `Buffer[(x, y)]` instead. To avoid panicking, use `Buffer::cell((x, y))`.") already nudges callers this way, so flipping the recommendation is mostly a doc + codemod change.
- **Option B:** Replace the panic with a `debug_assert!` and return `&Cell::EMPTY` (or similar default) in release builds. Backwards compatible but masks bugs.
- **Option C:** Keep `Index` but make it panic-safe via a wrapper that returns a default `Cell` on out-of-bounds. Same trade-off as B but explicit.

I'd suggest **Option A** since `Buffer::cell` is already the documented safe API and the `Index` impl is a footgun. As a follow-up, `set_string` / `set_stringn` / `set_span` should be changed to early-return when `y` is outside the buffer's `Rect` (rather than calling `self[(x, y)]`), and `with_lines` should be hardened to not panic when a single input line is taller than the area.

## Related

Note: GitHub's web search and unauthenticated issue search API were unreachable from the reporter's environment when drafting this, so the list below reflects what is visible in the source code and the project CHANGELOG only. Please re-search before treating this as comprehensive.

- [#1122](https://github.com/ratatui/ratatui/issues/1122): referenced inside `Buffer::index_of` and `Buffer::pos_of` (in 0.1.2: `buffer.rs:221` and `buffer.rs:262,281`) as the historical reason `index_of_opt` is `const fn`-private. Different concern (API exposure), but indicates the maintainers have already thought about which buffer methods are user-facing.
- [PR #2225](https://github.com/ratatui/ratatui/pull/2225): "Discourage use of `Buffer`'s `pos_of`, `index_of`" — added the "Usage discouraged" doc comment in the same file.
- [PR #2368](https://github.com/ratatui/ratatui/pull/2368) (v0.30.1): "Avoid panic if Clear area is outside of buffer" — fixes the *same* panic class but only for the `Clear` widget.
- [PR #1670](https://github.com/ratatui/ratatui/pull/1670): "Fix panic when rendering a `Paragraph` out of bounds" — same panic class for `Paragraph`.
- [PR #1996](https://github.com/ratatui/ratatui/pull/1996): "Panic when rendering widgets on too small buffer" — same class for `BarChart` and `RatatuiMascot`.

## Reporter

jcode (https://github.com/1jehuang/jcode), a Claude Code-style CLI TUI built on ratatui 0.30 / ratatui-core 0.1.0. Found via a real session crash on 2026-09-18 — the affected session data and panic log are preserved at `~/.jcode/logs/panics/session_cricket_*.panic.log`. jcode's terminal was a 57-column × 1-row pane inside tmux when the panic fired; the TUI had been running for several minutes and was being asked to render a layout with more rows than the terminal had.