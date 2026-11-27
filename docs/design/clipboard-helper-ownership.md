# Clipboard helper ownership: evidence, design, and blockers (issue #231)

Status: **design only. No production change in this branch.** The one open
question is a product/UX contract decision; every candidate answer changes
user-visible behaviour on the default Linux copy path, so it is recorded here
rather than assumed.

- Issue: KooshaPari/KCode#231
- Related: PR #228 (open, test-determinism refactor), PR #227 (closed,
  bounded-wait approach)
- Base of this document: `ac349069e7c397575b87c3fdfc36eadb18efb09a`
  (`origin/master` as observed at 2026-11-03T08:48Z)

## 1. Verified starting state

Observed, not assumed:

| Fact | Value |
| --- | --- |
| Working branch | `fix/clipboard-helper-fail-fast-window` |
| HEAD | `63d49fae22be09bbe6c44d8da453ebdd473d9e21` |
| HEAD parent | `ac349069e7c397575b87c3fdfc36eadb18efb09a` |
| Live `refs/heads/master` on origin | `ac349069e7c397575b87c3fdfc36eadb18efb09a` |
| Local `master` | `b9886143c13e683111aaf73d7f3ceab655860685` (stale, behind origin) |
| Working tree | clean |
| PR #228 | OPEN, head `63d49fae2`, base `master`, 1 file, +86/-9 |

`origin` is `https://github.com/KooshaPari/KCode.git`. PR #228 therefore sits
exactly one commit ahead of live master, and nothing in this branch disturbs it:
this document was produced in a separate worktree
(`git worktree add … ac349069e`) on branch `design/clipboard-helper-ownership-231`.

## 2. What the code does today

`crates/jcode-tui/src/tui/app/helpers/clipboard_helper.rs`

- `HELPER_FAIL_FAST_WINDOW` = 150 ms (`clipboard_helper.rs:24-25`).
- `copy_via_clipboard_helper_within` (`:41-91`) spawns the helper with piped
  stdin, writes the text, closes stdin, then polls `try_wait()` every 5 ms:
  - `Ok(Some(status))` with `success()` → `true` (observed exit 0)
  - `Ok(Some(_))` → `false` (observed nonzero exit)
  - `Ok(None)` past the deadline → moves `child` into a detached thread that
    only calls `wait()` (reaping, discards the status) and returns `true`
  - `Err(_)` → `false`

The third arm is the defect. Its justification ("still running means it owns
the selection") is an inference from elapsed time, not an observation. A helper
that is merely unscheduled has not exited and has not taken the selection; it is
reported as a copy.

Call chain on Linux (`helpers.rs:468-496`): `wl-copy` → `xclip -selection
clipboard` → `xsel --clipboard --input` → `arboard` → `copy_to_clipboard_osc52`.
`copy_to_clipboard` (`helpers.rs:386`) returns `bool`; every caller turns that
`bool` into a status notice immediately.

Call sites that depend on the boolean (8):

| Site | Notice on `true` | Notice on `false` |
| --- | --- | --- |
| `copy_selection.rs:341-368` viewport | `Copied viewport context` | `Failed to copy viewport context` |
| `copy_selection.rs:372-390` selection | `Copied selection` | `Failed to copy selection` |
| `copy_selection.rs:~400-420` drag-copy | `Copied selection · highlight remains visible` | `Failed to copy selection` |
| `input.rs:2543-2552` copy badge | `target.copied_notice` | `Failed to copy {kind}` |
| `navigation.rs:194-211` image label cycle | `Image size: {size} · Mermaid code copied` / `· Image copied` | `… · Could not copy …` |
| `navigation.rs:1782-1790` badge click | `target.copied_notice` | `Failed to copy {kind}` |
| `misc_ui.rs:551-558` coverage report | `Copied provider test coverage report` | `Failed to copy …` |
| `onboarding_flow_control.rs:1684-1691` | `onboarding_repair::repair_brief_status(agent, true)` | `… (clipboard unavailable)` |
| `auth.rs:1692-1696` device code | `" (copied to clipboard - just paste it!)"` | `""` |

Injection seams used by tests: `copy_chat_viewport_context_to_clipboard_with`,
`copy_current_selection_to_clipboard_with`, `handle_copy_selection_mouse_with`,
`cut_input_line_to_clipboard_with`, all typed `F: FnOnce(&str) -> bool`.

Status plumbing already exists and is the natural home for a late correction:

- `App::status_notice: Option<(String, Instant)>` (`app.rs:1585`)
- `App::set_status_notice` (`conversation_state.rs:421`)
- `TuiState::status_notice` hides notices older than `STATUS_NOTICE_TTL` = 3 s
  (`app.rs:442`, `tui_state.rs:974-988`)
- `App::retire_expired_status_notice` (`conversation_state.rs:432-441`)
- Tick hook: `handle_tick` (`local.rs:60-110`) already polls
  `poll_usage_reset`, `poll_model_picker_load`, `poll_session_picker_load`,
  `poll_session_picker_presence`. There is a single, established place to add a
  `poll_clipboard_outcome`.

There are two further copies that share the same class of defect and are **out
of scope unless explicitly included**:

- `helpers.rs:992+ copy_image_to_clipboard` — decodes base64, uses `wl-copy -t
  image/png`, `xclip`, `arboard`.
- `productivity.rs:101 copy_image_to_clipboard(path, png)` — `wl-copy`,
  `xclip`, `arboard`.

## 3. The epistemic core

The only fact a spawned helper gives us is its exit status, and on a healthy
Linux desktop the success case never produces one: `wl-copy`, `xclip`, and
`xsel` fork a resident process that lives as long as it owns the selection.
Therefore:

1. `exit 0` ⇒ the write was accepted. Sound.
2. `exit nonzero` ⇒ the write failed. Sound.
3. Still running at time T ⇒ **nothing is known**. It may become the owner, or
   it may never be scheduled.

Any mapping from state 3 to `true` without further evidence is a false-success
generator; any mapping to `false` is a false-negative generator that also
changes fallback behaviour (see below). There is no third option that stays
inside a `bool`.

Two ways to obtain real evidence in state 3:

**(a) Bounded wait for exit** (PR #227). Still guesses for helpers that never
exit, and costs up to ~850 ms on the UI thread. Rejected by the user as a
UI-blocking regression; the issue also rejects it.

**(b) Read-back verification.** Ask the selection what it now holds, e.g.
`wl-paste --no-newline` / `xclip -selection clipboard -o` / `xsel --clipboard
--output`, and compare with the bytes written. This is a genuine observation of
the user-visible contract ("what does paste give me"), not an inference from
time.

Read-back is the only mechanism that can honestly upgrade state 3 to
"confirmed". It carries its own unverified assumptions:

- The read-back binary ships with the copy binary in practice
  (`wl-clipboard` provides both `wl-copy` and `wl-paste`; `xclip`/`xsel` are
  self-contained), but this is a packaging assumption, not something this
  repository controls.
- A read-back can observe a *previous* selection if it runs while the new owner
  is still starting, and can round-trip text with newline differences. Both
  need normalisation plus a deadline, which is more code that must itself be
  tested.
- A read-back blocks until the owner answers. Bounded by our own deadline, but
  that deadline is wall-clock time spent on whichever thread runs it.

**None of this has been measured.** It cannot be: the module is
`#[cfg(not(any(windows, target_os = "macos")))]`, this workstation is macOS, and
the user-owned build host `desk` is `MSYS_NT-10.0-28120 … x86_64 Msys` (Windows
under MSYS). There is no Linux desktop in reach to test `wl-copy`/`xclip`
behaviour against a real compositor. Any claim about read-back latency or
fidelity would be speculation, so this document does not make one.

## 4. Candidate contracts

All three are honest about state 3 in different ways; they differ in what the
user sees.

**Contract A — bounded wait (PR #227).** Wait a fixed grace period for exit;
treat survival across the whole period as ownership. Rejected: still a
time-based claim, and blocks the UI.

**Contract B — optimistic display, asynchronous correction (issue option 2).**
Claim the copy at the deadline as today, but keep the child's handle, and when
it exits nonzero later, replace the notice with a failure through the tick poll.
Fixes the persistent lie at zero added latency. Does not fix the claim itself:
"Copied" is still displayed on time alone, just briefly. Requires stale-token
handling so a slow outcome for copy N cannot overwrite the notice for copy N+1.

**Contract C — deferred notice (recommended).** Do not display any success text
until the outcome is known. The helper chain runs off the UI thread; the notice
is set from the tick poll when a helper either exits 0, is confirmed by
read-back, or every fallback has failed. No new wording is introduced: the
existing `success_text` / `failure_text` pairs are used unchanged, they simply
appear a tick or two later (tens of ms) instead of synchronously.
- Casts no claim on elapsed time.
- Never blocks the UI (`handle_tick` stays O(1)).
- Keeps the fallback chain and the platform ordering intact.
- Cost: the notice is not instant, and `copy_to_clipboard` can no longer return
  `bool`, so all 8 call sites plus the four `FnOnce(&str) -> bool` test seams
  change shape.

Contract C still needs an answer for state 3 *when read-back is unavailable or
unconclusive*: the honest answer is "unconfirmed", which is a third UI state
(e.g. keep the neutral prior notice, or show `Copy sent (unconfirmed)`). That is
the blocking decision below.

## 5. Recommended implementation (Contract C, read-back optional)

Not written yet; this is the plan a follow-up PR can execute.

1. **`clipboard_helper.rs`**: replace `bool` with
   ```rust
   pub(crate) enum HelperOutcome {
       Copied,        // observed exit 0
       Failed,        // spawn/pipe error, or observed nonzero exit
       Unconfirmed,   // still running at the deadline; and/or read-back did not confirm
   }
   ```
   `Unconfirmed` keeps the background reaper but also records the exit status
   into a shared cell so it can be reported later. Keep the parameterised window
   from PR #228 (`copy_via_clipboard_helper_within`) — this design depends on it,
   so #228 should land first or this work must carry it.
2. **Optional read-back probe**: `verify_selection(program, args, expected,
   deadline) -> bool`, spawned from the same worker, only for `Unconfirmed`.
   Compare after newline normalisation. Runs with its own deadline and is killed
   if it overruns.
3. **`helpers.rs`**: `copy_to_clipboard` becomes
   `start_clipboard_copy(text) -> ClipboardRequest`, returning either an
   immediately-known outcome or a `oneshot::Receiver<CopyOutcome>` plus the
   reaper handle. `arboard` stays on the worker (Linux only); `OSC 52` must stay
   on the UI thread because it writes to the TUI's stdout and would otherwise
   interleave with ratatui frames. So the worker resolves
   `Copied | Failed | Unconfirmed`, and `Unconfirmed`/`Failed` after all helpers
   and `arboard` hands back to the UI thread for `copy_to_clipboard_osc52`, as
   today.
4. **`App`**: add `pending_copy: Option<PendingCopy>` where
   ```rust
   struct PendingCopy {
       token: u64,                      // monotonic, bumped by every new copy
       receiver: oneshot::Receiver<CopyOutcome>,
       success_text: String,
       failure_text: String,
   }
   ```
   `App::set_copy_notice_token()` bumps `token` on every copy request.
5. **`local.rs` `handle_tick`**: `needs_redraw |= app.poll_clipboard_outcome();`
   which drops a stale `token` and otherwise sets the pending notice and returns
   `true`.
6. **Call sites**: each replaces its `if success { set_status_notice(..) } else
   { .. }` with a single `self.request_copy(text, success_text, failure_text)`.
   `auth.rs` needs a small variant because it embeds the result in a message
   body rather than a notice.
7. **Test seams**: change `FnOnce(&str) -> bool` to
   `FnOnce(&str) -> ClipboardRequest`; the existing fake closures can return a
   resolved request, which keeps current tests asserting synchronously.
8. **Image tests**: `crates/jcode-tui/src/tui/app/tests/scroll_copy_02/part_02.rs:943`
   and `:1211` assert `status_notice() == "Image size: large"` exactly. Those
   assertions only hold because the image copy path currently reports failure on
   non-Wayland CI (see §2's second copy path) and the notice therefore never
   gains the `· … copied` suffix. They encode the bug and must be updated with
   whichever contract is chosen, not left as-is.

## 6. Deterministic test plan

Runnable only on Linux (`cargo test -p jcode-tui --target x86_64-unknown-linux-gnu`
locally cannot execute; the target *is* installed so `cargo check --target
x86_64-unknown-linux-gnu -p jcode-tui` can at least typecheck). CI `Build & Test
(desk-linux)` is the acceptance path, exactly as PR #227 noted.

Using the existing `StubPath` stub-PATH harness and PR #228's explicit window:

1. `late_nonzero_exit_is_not_a_copy` — stub `sh -c 'sleep 0.25; exit 1'`;
   assert `HelperOutcome::Unconfirmed` (not `Copied`), and after the reaper
   reports, `CopyOutcome::Failed`.
2. `long_lived_helper_does_not_block` — stub `sleep 30`; assert the call returns
   in well under 1 s and that the UI-thread portion never exceeds the fail-fast
   window. In Contract C this asserts the worker is detached, not that the
   result is "Copied".
3. `no_working_helper_reports_failure_and_falls_through` — all stubs `exit 1`;
   the chain reaches `arboard`/OSC52.
4. `stale_outcome_does_not_overwrite_newer_notice` — start copy A (late
   failure), start copy B (fast success), resolve A after B; assert the notice
   is still B's and B's token is current.
5. `tick_poll_is_non_blocking` — `poll_clipboard_outcome` with an unresolved
   receiver returns `false` immediately and leaves the notice untouched.
6. `exit_zero_still_counts` — stub `cat > /dev/null; exit 0`.
7. If read-back is implemented: `read_back_mismatch_is_not_a_copy`,
   `read_back_match_confirms_after_window`.

## 7. Blockers

**B1 (blocking, product).** What should the UI say while a copy is unconfirmed?
Options: (i) deferred notice, show nothing until known (recommended, no new
wording); (ii) a neutral `Copy sent (unconfirmed)` state; (iii) optimistic
`Copied` plus asynchronous correction (issue option 2, keeps today's instant
feedback but retains a brief time-based claim).
Every option changes the feel of the single most-used keybinding in the TUI, so
this needs an explicit decision rather than a maintainer guess.

**B2 (blocking, scope).** Is read-back verification in scope? Without it,
Contract C can only ever report "unconfirmed" for the normal resident-helper
case, which is why B1 has to be answered first. With it, the ambiguous state has
an honest resolution but adds a second subprocess per copy and depends on
`wl-paste` being installed alongside `wl-copy`.

**B3 (blocking, verification).** No Linux desktop is reachable from this
environment (macOS workstation; `desk` is Windows/MSYS). Real
`wl-copy`/`xclip` behaviour, read-back fidelity, and the resident-helper path
can only be validated in CI `desk-linux`. Until then, any statement that the
resident path "works" is unverified.

**B4 (sequencing).** This design uses the parameterised window from PR #228
(open). Either #228 merges first, or the implementing PR must carry that commit,
otherwise the two changes conflict in the same file.

**B5 (scope).** `copy_image_to_clipboard` exists twice
(`helpers.rs:992`, `productivity.rs:101`) with the same class of defect and a
different fallback order. Decide whether the fix covers images too; the two
image tests that currently encode the bug will move under either answer.

## 8. Why no code change in this branch

Each of the three contracts requires the B1 decision, and the only mechanism
that can honestly resolve the ambiguous state (read-back) cannot be measured in
this environment (B3). Shipping any of them now would either re-introduce a
time-based success claim (A, B) or silently pick new user-facing wording (C),
and would be validated only by an unrun test suite. Per the task constraints,
the evidence-backed design and these blockers are the deliverable.

## Appendix: commands used

```
git rev-parse --abbrev-ref HEAD                     # fix/clipboard-helper-fail-fast-window
git log --oneline -8                                # HEAD 63d49fae2
git rev-parse HEAD HEAD^ origin/master               # 63d49fae2 ac349069e ac349069e
git ls-remote origin refs/heads/master              # ac349069e
git status --porcelain                              # (empty)
gh pr view 228 --repo KooshaPari/KCode --json …     # OPEN, 1 file, +86/-9
gh pr view 227 --repo KooshaPari/KCode --json …     # CLOSED
gh issue view 231 --repo KooshaPari/KCode --json …
ssh desk uname -a                                   # MSYS_NT-10.0-28120 … x86_64 Msys
rustup target list --installed                      # includes x86_64-unknown-linux-gnu
```
