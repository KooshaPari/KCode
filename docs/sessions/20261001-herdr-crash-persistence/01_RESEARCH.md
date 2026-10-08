# 01_RESEARCH — Sep-28 `kcode --resume` batch origin (F1)

Forensic trace of the 13–14 bare `kcode --resume` processes fired by a live herdr
session on **2026-09-28 13:12:00–13:12:15 local (11:12:00–11:12:15Z)**, ~53 s after
the server restore spawned pane shells at 13:11:09 local.

## Verdict

**No software path on this machine can author that batch. The evidence points to
human keystrokes at the herdr client TUI (click-to-focus + Up/Enter) into freshly
restored, bare shells.**

Two separable claims, with separate confidence:

| Claim | Confidence |
|---|---|
| No on-disk build/script/config can emit a `kcode` resume argv | **validated** (exhaustive elimination) |
| The actual sender was the human operator | **plausible** (strong indirect corroboration) |

## What was observed

- Batch processes: `kcode.real --resume session_hamster_…` (pid 2419) with
  **ppid = pane shell** (`-zsh`, ttys003, started 13:11:09), plus a child
  `/bin/zsh ~/.local/bin/kcode --resume` (pid 2420).
- The child is the wrapper's **process-substitution filter**
  (`exec "$REAL" "$@" 2> >( … )`), forked before `exec`, so it retains the
  pre-exec argv. That argv carries **no session id**.
- `kcode.real` proctitle carries the **cwd-resolved** sid, i.e. kcode itself
  resolved the session from the pane's cwd after being invoked bare.
- Result: the text fed to each shell was effectively **`kcode --resume` with no sid**.

## Eliminated suspects (evidence)

### herdr itself
- `agent_resume::plan()` (`src/agent_resume.rs:133`) starts with
  `if !is_official_agent_source(source, agent) { return None; }`;
  `is_official_agent_source` (line 265) lists 19 official agents — **no kcode, no forge**.
- `detect::Agent` enum has **24** variants — no `Kcode`. `lookup_agent()` /
  `agent_label()` / `interactive_agent_executable()` have **no `kcode` arm**
  (and `interactive_agent_executable` only ever returns *bare* names, never a path).
- `detect::parse_agent_label("kcode")` → `None`, so `agent.start` fails with
  `UnsupportedKind`. `start_agent` therefore cannot type a kcode command.
- Restore pipeline: `persist/restore.rs:539` sets `pending_agent_resume_plan` only
  from `pane_restore_startup` → `restore_plan_for_snapshot` → `plan()`. Same gate.
  The only thing that *writes bytes* into a pane shell is
  `app/agent_resume.rs:266-268` (`resume_command + '\r'`), and it requires that plan.
- `session.json` pane entries carry only `cwd/label/agent_session` — zero `kcode`,
  zero `--resume`, zero `launch_argv`.
- `tab.focus` (164 events) is **focus, never input** — `TabFocus` senders in the
  client are `mouse.rs` (tab-strip press), `actions.rs`/`endpoint_navigation.rs`
  (`focus_or_activate`), `context_menu.rs`, `worktrees.rs`. None writes bytes.
- No `re-run` / `rerun` / `repeat last command` / `restart agent` keybind exists.
- `pane.input.set` from the client is **only** the right-click passthrough toggle
  (`context_menu.rs:449`), not text.
- Custom commands (`command.invoke`): `~/.config/herdr/config.toml` (524 B) defines
  **none**, so `resolve_client_shell_command` always errors before focusing.
- Plugins: no `pane.input`/`send_text` writer; `machine-symbol-stamp.py` only sets
  `$mach` metadata; no plugin action call near the batch.
- `herdr-integration-set-hotkey` / `-set-state` strings exist in **no** source
  (herdr repo, kcode repo), rc file, `~/bin`, `~/.config/herdr`, `~/.local/share`.

### resume-all tooling
- `resume-all.py` builds `exec forge --conversation-id <sid> -C <cwd>`-style argv —
  sid-carrying, not sid-less; and its tmux/Ghostty backends don't touch herdr panes.
- `snapshot.jsonl` mtime **Aug 15** and contains **0 `kcode` rows** — structurally
  incapable of emitting a kcode line.
- No log row in the batch window in either local (`13:1x`) or UTC (`11:1x`) form
  across `~/.local/share/resume-all/*` and `~/Library/Logs/resume-all/*`.
  `resume-watch` starts 13:14:29 (after the batch). `dependencies.log` last real
  entries Aug 7 (dry-run only).
- `voice-activate` state: `last_recognized = 2026-08-08`, `command_count = 4`.

### shell / environment
- `~/.zshrc` role gate + `_kcode_ensure_wrapper` only repair the wrapper symlink
  (`ln -sf`); **nothing runs kcode at shell startup**.
- Ambient queue `~/.kcode/ambient/queue.json` is a scheduled-task queue (future
  `scheduled_for`, created Sep 29–Oct 1) with no pane-write capability.
- PTY writers on ttys003–012: only pane shells, `kcode.real`, its `/bin/zsh` child,
  and `caffeinate`.

## Positive evidence for human keystrokes

1. **Zero `api.request` on Sep 28** outside 2 `pane.release_agent` at 04:58.
   `api.request.*` lines are emitted for CLI/API traffic (`request_id="cli:…"`),
   and the whole month shows 34/28/163/57/374/307/255/18/**4**/35 per day —
   Sep 28 is 4 lines = 2 requests, i.e. **no CLI/automation drove the batch**.
2. The 164 `tab.focus` are **client-shell** (silent) events, all `w7:t1`, with
   **irregular 0.4–4 s gaps** from 11:11:13Z through 11:14+ — a human cadence,
   not a fixed-rate loop, and the storm *brackets* the batch window.
3. `~/.zsh_history` contains bursts of bare `kcode --resume`
   (lines 834–848 ≈ 15; lines 993–1019 ≈ 14) interleaved with
   - **SGR mouse-report garbage** (`35;38;14M`, `0;16;7m`, …) — the signature of a
     shell sitting at a prompt while herdr forwards mouse motion into the pane, and
   - random key mashing (`sdf`, `vdsvsdfs`, `fsdf`, `ds`) and `clear`.
   That is exactly what a human produces while clicking panes and typing.
4. The shells were spawned at 11:11:09Z, so a fresh zsh loads `~/.zsh_history` and
   **Up+Enter re-runs the last command** = `kcode --resume`. This explains:
   - an argv with **no session id** (history stores the bare habit), and
   - the ps shape `/bin/zsh /Users/kooshapari/.local/bin/kcode --resume` — the
     full path is the **shebang interpreter argument** (PATH-resolved `kcode`),
     not a full path typed by the caller.

## Residual uncertainty

- `~/.zsh_history` is **not** `EXTENDED_HISTORY`, so lines carry no epoch; the
  bursts cannot be pinned to 13:12 by timestamps alone. Corroboration is that
  Sep 28 has exactly one restore + focus-storm event in the server log.
- The `herdr-integration-set-hotkey` / `-set-state` process names seen at 13:11:46
  local remain **unexplained** (present in no source/config/bin). They are
  `integration`-install-shaped, not input writers, so they do not change the verdict.
- The running server is a **patched 0.9.1 binary no longer on disk**
  (inode 911080800, 21500816 B). Its `plan()` cannot be inspected directly — but
  even a patched `plan()` could not fire here, because a plan requires a persisted
  `agent_session` and kcode panes had none (sid-less argv).

## Why this matters for the fix

The storm was **not** herdr double-firing. herdr *could not* resume kcode at all,
so the operator had to hand-resume 13 sessions under load 226 (the ENOSPC crash
window). Making `kcode`/`forge` first-class detected agents with a persisted
`agent_resume` `resume_argv` removes the manual Up+Enter loop and gives herdr's
dedup (`resumed_sessions` set in `pane_restore_startup`) a single-fire guarantee.

---

## 2026-10-02 — Verified orchestrator findings (live herdr 0.9.3 + reporter fix)

All items below are verified observations from the live system (trust; not
re-derived here).

1. **Running herdr is stock 0.9.3** (pid 1266); source tag read via
   `git show v0.9.3:<path>` in `~/.kcode/scratch/herdr` (a git mirror — never run
   cargo/build there).
2. **Proven live consumption path:** `herdr pane report-agent <PANE> --source
   herdr:kcode --agent kcode --state <s> --seq <n> -- kcode --resume <sid>`
   persists `.agent_resume` into `session.json`. Restore prefers
   `reported_resume` over `agent_session` (`restore.rs:459/537`).
   `resume_agents_on_restore` defaults true (`config/model.rs:278`).
3. **Native `agent_session` path is impossible for kcode:**
   `is_official_agent_source` (`agent_resume.rs:328`) has no kcode/forge arm.
   Not needed — `agent_resume` works without it.
4. **BUG FIXED:** kcode reporter built `source = format!("kcode:{agent_label}")`
   → silently no-oped (source must start with `herdr:`). Fixed in
   `crates/kcode-herdr/src/reporter.rs` to `format!("herdr:{agent_label}")` with
   explanatory comment; test asserts `"herdr:kcode"`.
   `cargo test -p kcode-herdr` = 24/24. Release build
   (`scripts/install_release.sh`, profile `release-lto`) in progress so the fix
   lands in the installed binary.
5. **Gate `can_record_reported_resume` (resume_argv):** needs hook authority OR
   (detected_agent matches label AND no recent exit). Phantom panes (agent=None)
   reject resume_argv — observed 3 failures.
6. **CRITICAL seq rule:** `hook_report_sequences` are set by reporters using
   micros-epoch timestamps (~1.78e15). Any seq below that is rejected as stale
   and the call **SILENTLY no-ops (returns ok)**. Use seq
   `1800000000000000000`.
7. **Phantom purge gap:** `clear_agent_authority` and `release-agent` both return
   `{"type":"ok"}` but only clear `hook_authority`;
   `persisted_agent_session` (probe_letta/hermes/devin refs) survives every API
   variant. NO API clears the persisted session ref → file-level purge required
   with server stopped.
8. `state.rs:568 set_reported_resume(None)` on a detection event wipes a
   previously reported resume (happened to calf/w7:p19) → must re-inject at
   cutover.
9. `session.json` pane keys are **layout slot numbers** (`ws0 tab0 key=N`), NOT
   pane_ids; map via `public_pane_numbers` or live `pane list` label/cwd match.
10. Config added to `~/.config/herdr/config.toml`:
    `[experimental] pane_history = true` (opt-in, default false),
    `[session] resume_agents_on_restore = true`,
    `startup_per_agent_delay_ms = 150`.
11. Helper: `~/.kcode/scratch/herdr_call.py` (raw socket API caller:
    `herdr_call.py <method> <json-params>`).
12. **K1 keybind:** kcode alternate-send/queue = cmd+enter (TUI sees
    `SUPER+Enter`); Ghostty ships `keybind = super+enter=toggle_fullscreen`
    consuming the chord before the app (confirmed via
    `ghostty +list-keybinds`; kcode keymap-snapshot showed
    `{action: toggle_fullscreen, source: terminal}`). Herdr has no super+enter
    binding. **FIXED:** appended `keybind = super+enter=unbind` to
    `~/Library/Application Support/com.mitchellh.ghostty/config`;
    `ghostty +list-keybinds` now shows only
    `super+shift+enter=toggle_split_zoom`. kcode already has conflict-detection
    infra: `crates/kcode-setup-hints/src/keymap/conflicts.rs`
    (`detect_conflicts`, `kcode_bindings`).
13. **Ops incident:** release build failed twice with "No space left on device"
    (Data volume hit 100%); purgeable space reclaimed to ~34 Gi, build restarted
    via `bash scripts/install_release.sh` (NOT repo-root `install_release.sh`)
    with `export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk
    MACOSX_DEPLOYMENT_TARGET=15.0 KCODE_SKIP_SERVER_RELOAD=1`.
