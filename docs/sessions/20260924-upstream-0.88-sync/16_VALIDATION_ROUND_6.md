# Validation Round 6 — Launcher chain + daemon version drift (2026-10-11 01:13 PDT)

## Scope

Auto-prompted "Continue working, or update the todo tool" after the
round 5 corrections (`79e09cd8b`). Round 6 re-verifies the launcher
chain (`~/.local/bin/jcode` → ...) and checks whether the long-running
daemons are actually running the version the launcher implies.

## Critical finding: 2 jcode daemons are running stale 0.91.0

`ps -o command` shows the launcher's path (`/Users/kooshapari/.local/bin/jcode`),
but the **actual exe** (verified via `lsof -p PID | grep txt`) is different.
The 2 long-running jcode daemons are running the OLD 0.91.0 binary,
while the launcher points to 0.93.0. The auto-update replaced the
launcher but didn't restart the daemons, so the daemons kept their
0.91.0 binary in memory.

| PID | Uptime | ps shows | lsof actual exe | Verdict |
|---|---|---|---|---|
| 10085 | 1d 21h 36m | `~/.local/bin/jcode setup-hotkey` | `versions/0.91.0/jcode` | ❌ STALE 0.91.0 |
| 10347 | 1d 21h 36m | `~/.local/bin/jcode menubar` | `versions/0.91.0/jcode` | ❌ STALE 0.91.0 |
| 10348 | 1d 21h 36m | `~/.jcode/builds/stable/jcode serve` | (also 0.91.0; spawned by menubar) | ❌ STALE 0.91.0 |
| 2341 | 1d 21h 45m | `~/.local/bin/kcode setup-hotkey` | `versions/bb6174b21a/kcode` | ✓ current kcode |
| 4490 | 1d 21h 44m | `~/.local/bin/kcode menubar` | `versions/bb6174b21a/kcode` | ✓ current kcode |
| 36316 | 1d 5h 0m | `~/.kcode/builds/stable/kcode serve` | `versions/bb6174b21a/kcode` | ✓ current kcode |

**Net:** 6 long-running daemons. 3 are stale (jcode), 3 are current
(kcode). The 15 --resume sessions (14 jcode + 1 kcode) are all
current (0.93.0 / bb6174b21a).

## Why this matters

The jcode menubar is what handles the "Open in jcode" menu item and
the global hotkey. If the operator hits the hotkey or clicks the
menubar's "Open in jcode" entry, **the spawned process would be
0.91.0**, not 0.93.0. Only direct shell invocations (`jcode` in a
fresh shell) would use 0.93.0.

This is a **silent version-drift bug** that would not be caught by
`jcode --version` from the operator's interactive shell.

## The launcher chain (3 levels of indirection)

```
~/.local/bin/jcode                            (symlink, Oct 10 00:40)
  → ~/.jcode/builds/current/jcode             (symlink, Oct 10 00:40)
    → ~/.jcode/builds/versions/0.93.0/jcode   (129 MB Mach-O, Oct 10 00:40)
```

Both `current/` and `stable/` point to `versions/0.93.0/jcode`.
The version-tracking files (`current-version`, `stable-version`)
both contain the literal string `0.93.0`.

## What's leftover from the pre-auto-update state

### 1. `~/.local/bin/jcode.real` orphan

```
symlink → /Users/kooshapari/.jcode/builds/versions/0.91.0/jcode
created: Oct  9 23:53 (before the auto-update at 00:40)
```

The original zsh wrapper script (now in `jcode.legacy-pre-kcode.zsh.bak`)
used this file: it would `ln -sf` the first `~/.jcode/builds/versions/*/jcode`
match into `jcode.real`, then `exec "$REAL" "$@"`. When the daemons
were launched (Oct 9 23:53), `jcode.real` pointed to 0.91.0. The
daemons loaded 0.91.0 into memory. The auto-update replaced the
launcher at Oct 10 00:40, but the daemons were not restarted.

### 2. `~/.local/bin/jcode.legacy-pre-kcode.zsh.bak` (1.3 KB)

A backup of the original zsh wrapper. Safe to delete, but contains
useful debugging context (the wrapper handled macOS
`com.apple.quarantine` and `replacing existing signature` stderr
filtering — a sign the operator is running freshly-built binaries
without code-signing).

### 3. `~/.zshrc` has ~90 lines of dead code (lines 447-530)

```
447:  # ~/.local/bin/jcode.real points at the real Mach-O. Without this, `jcode update`
452:  _jcode_realpath_resolve() {
461:    local real="$HOME/.local/bin/jcode.real"
486:    target="$(_jcode_realpath_resolve 2>/dev/null || true)"
495:  REAL="$HOME/.local/bin/jcode.real"
530:  unset -f _jcode_ensure_wrapper _jcode_realpath_resolve
```

The `_jcode_realpath_resolve` and `_jcode_ensure_wrapper` functions
were only invoked by the old zsh wrapper. Since the auto-update
replaced the wrapper with a Mach-O symlink, these functions are
dead code (they were never called, and the `unset -f` at the end
would have been a no-op since the wrapper was the only caller).

### 4. `versions/0.91.0/` directory (122 MB)

Contains the old 0.91.0 binary. Cannot be deleted while PIDs 10085
and 10347 are running — it would prevent their restart. To clean
up: kill the 2 daemons first, wait for them to restart from
`current/` → 0.93.0, then `rm -rf versions/0.91.0/`.

### 5. `manifest.json` and `manifest.bak` (344 bytes each, from Sep 27)

Per AGENTS.md: "manifest files are stale test fixtures (not a real
bug)" — they contain literal `test-reload-hash` and
`test-reload-fingerprint` placeholders. Real install state is in
the sibling `current-version` and `stable-version` text files.
No action needed.

## New lessons captured (round 6)

**23. `ps -o command` shows the launcher's path, not the actual
exe the process is running. Use `lsof -p PID | grep txt` to see
the real exe path.** Discovered 2026-10-11: 2 jcode daemons showed
`~/.local/bin/jcode menubar` in ps, but their actual exe was
`versions/0.91.0/jcode` because they were launched before the
auto-update replaced the launcher. The launcher is just a symlink
to a directory of symlinks, and the process's actual binary is
whatever was resolved at exec() time.

**24. Long-running daemons keep their old binary in memory across
launcher updates.** Auto-update patterns that replace the launcher
(`~/.local/bin/jcode`) do not retroactively change the daemons
that were launched via the old launcher. The 2 jcode daemons
(PIDs 10085 + 10347) loaded 0.91.0 into memory at Oct 9 23:53; the
auto-update at Oct 10 00:40 changed the launcher to 0.93.0; the
daemons are still 0.91.0. To force them to pick up the new
version, they must be killed and restarted (e.g., by launchd or
by the operator manually).

**25. The legacy `~/.local/bin/jcode.real` pattern requires a
zsh wrapper to update.** The old wrapper (in
`jcode.legacy-pre-kcode.zsh.bak`) would scan `versions/*/jcode`
and `ln -sf` the first match into `jcode.real`. The auto-update
replaced the wrapper, so `jcode.real` is now an orphan pointing
to 0.91.0 forever (or until manually updated). The 2 stale
daemons still read 0.91.0 because they were launched with the
old wrapper that pointed `jcode.real` to 0.91.0.

## Net corrections through 6 rounds

| # | Claim in earlier rounds | Round 6 actual | Status |
|---|---|---|---|
| 1 | "5 long-running daemons: 3 kcode + 2 jcode" | 6 daemons (3 kcode + 3 jcode, including PID 10348 jcode serve I missed) | ❌ missed PID 10348 |
| 2 | "All jcode/kcode on 0.93.0/bb6174b21a" | **3 jcode daemons are on 0.91.0** (10085, 10347, 10348); 14 jcode --resume + 1 kcode --resume on current | ❌ missed daemon version drift |
| 3 | "0.91.0 directory is unused" | ❌ WRONG — 3 daemons load 0.91.0 into memory; can't be deleted | ❌ can't delete 0.91.0 |
| 4 | "jcode.real is orphan, no other references" | ~90 lines of dead zshrc code reference it; functions are no-op | ⚠️ partial |
| 5 | "Auto-update was clean" | It replaced the launcher but left daemons on old binary; 0.91.0 still needed on disk | ❌ missed daemon-update gap |
| 6 | "kcode daemons bb6174b21a" | ✓ correct (3 kcode daemons all on bb6174b21a) | ✓ correct |
| 7 | "Stable and current both 0.93.0" | ✓ correct | ✓ correct |
| 8 | "Manifest files are stale" | ✓ correct (per AGENTS.md) | ✓ correct |

## Operator-actionable items (new this round)

1. **Decide whether to restart the 2 stale jcode daemons.** Killing
   PIDs 10085 + 10347 would let them restart on 0.93.0 (via the
   current launcher chain). Risk: brief menubar gap (1-2s).
2. **Clean up 0.91.0 directory** (122 MB) once daemons are
   restarted.
3. **Decide whether to remove `jcode.real` orphan** (still valid as
   a fallback if the launcher chain ever breaks).
4. **Decide whether to remove `jcode.legacy-pre-kcode.zsh.bak`**
   (1.3 KB backup of the old wrapper).
5. **Decide whether to remove the 90 lines of dead zshrc code**
   (the `_jcode_realpath_resolve` and `_jcode_ensure_wrapper`
   functions plus their callsite).

None of these is autonomous; all need operator decision.
