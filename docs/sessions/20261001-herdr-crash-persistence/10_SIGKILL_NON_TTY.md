# SIGKILL on `kcode --resume` from non-TTY — Root cause and fix

**Date:** 2026-10-08
**Severity:** P1 (blocks crash-restore verification; the C1 fix `herdr:jcode`
source namespace is not exercised by any live pane)
**Status:** **Fixed and installed** (`27bd2299c`). New binary at
`~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode` runs cleanly from non-TTY.
Launchers (`~/.local/bin/kcode`) now point to the linker-signed build.

## TL;DR

`kcode --resume` (and `jcode --resume`) from a non-TTY context exits with code
137 (SIGKILL) because the **dirty build at `~/.jcode/builds/current/jcode`**
(`b87cd9955-dirty`) has only `flags=0x2(adhoc)` — no linker-signed attribute —
and macOS `amfid` rejects it with `AppleMobileFileIntegrityError Code=-423`,
triggering Gatekeeper SIGKILL.

The **stable build at `~/.jcode/builds/stable/jcode`** (`0.91.0`) has
`flags=0x20002(adhoc,linker-signed)` and runs cleanly. It also contains the
**C1 fix** (`herdr:jcode` source namespace).

The live jcode --resume processes were spawned from `b87cd9955-dirty`, so
`session.json` still shows `agent_resume.source = "jcode"` (pre-fix). All 11
jcode panes are affected.

## Reproduction

```sh
$ kcode --resume session_blossom_1789125438468_65a0cdb3f092c774 </dev/null
bash: line 1: 28658 Killed: 9               kcode --resume session_blossom_1789125438468_65a0cdb3f092c774 < /dev/null
$ echo $?
137
```

Direct invocation of the linker-signed binary works (errors cleanly with a
meaningful message instead of being killed):

```sh
$ /Users/kooshapari/.jcode/builds/stable/jcode --resume session_blossom_... </dev/null
Connecting to server...
Error: jcode TUI requires an interactive terminal (stdin/stdout must be a TTY)
$ echo $?
1
```

## Console log evidence

```
amfid: /Users/kooshapari/.jcode/builds/versions/b87cd9955-dirty/jcode not valid:
  Error Domain=AppleMobileFileIntegrityError Code=-423
  "The file is adhoc signed or signed by an unknown certificate chain"
```

## Signature inventory of all builds

| Build                        | mtime           | Size (B)      | flags          | Valid for amfid? |
| ---------------------------- | --------------- | ------------- | -------------- | ---------------- |
| `0.84.0`                     | Sep 11 04:12    | 136,167,536   | 0x20002        | YES              |
| `0.91.0` (→ `stable`)        | Oct 5  21:11    | 127,908,544   | 0x20002        | YES (live serve) |
| `06afe943a`                  | Sep 17 22:54    | 136,916,144   | 0x2            | NO               |
| `22970ba7d`                  | Sep 18 21:48    | 152,494,960   | 0x2            | NO               |
| `jcode-herdr-pr8`            | Sep 20 02:15    | 137,044,464   | 0x2            | NO               |
| `herdr-fix-fc9993936`        | Sep 24 01:36    | 137,116,624   | 0x2            | NO               |
| `81a223cfe-dirty`            | Sep 24 01:36    | 137,096,576   | 0x2            | NO               |
| `unfold-fix-642c4fe76`       | Sep 24 02:21    | 137,119,360   | 0x2            | NO               |
| `b87cd9955-dirty` (→ current)| Oct 4  17:53    | 141,962,592   | 0x2            | **NO (active)**  |
| `591df7ad5-dirty`            | Oct 4  18:04    | 137,119,360   | 0x2            | NO               |

**Conclusion:** Only the two `0.x.y` (semver) builds were linker-signed. Every
short-hash build produced by the install_release.sh flow in our local history
lacks the linker-signed attribute. The only currently-signing linkers are
those that emit `flags=0x20002` — and those builds correspond to the upstream
release pipeline (not our local builds).

## Why the wrapper picks the wrong binary

The kcode wrapper at `~/.local/bin/kcode` is a zsh script that resolves the
binary in this order:

1. `$KCODE_HOME/builds/current/kcode` — not set
2. `$JCODE_HOME/builds/current/jcode` — not set
3. `~/.kcode/builds/{current,stable}/kcode` — `~/.kcode` does not exist
4. `~/.jcode/builds/current/jcode` — **this exists** → picked first

It does NOT fall through to `~/.jcode/builds/stable/jcode` because step 4
succeeds. So all non-TTY invocations land on `b87cd9955-dirty/jcode` (adhoc
only, SIGKILL'd by Gatekeeper).

The legacy warning the wrapper emits is also revealing:

```
warning: found legacy KCode build at /Users/kooshapari/.jcode/builds/current/jcode; rename to kcode
```

The wrapper is *literally telling us* the current symlink is on a legacy
filename. The wrapper will continue working with `jcode` as the binary name
(it's an explicit compat path), but the underlying build is the wrong one.

## Live session.json state (proves the C1 fix isn't exercised)

All 11 jcode panes in `~/.config/herdr/session.json` have:

```json
"agent_resume": {
  "source": "jcode",
  "agent":  "jcode",
  "argv":   ["jcode", "--resume", "session_..."]
}
```

The C1 fix changed this to `"source": "herdr:jcode"`. None of the 11 live jcode
panes show the new source — confirming they were all spawned from a binary
that pre-dates the C1 fix (`b87cd9955-dirty`, installed Oct 4 17:53; C1 landed
in commit `4f12e67dd` and was rebuilt into `0.91.0` on Oct 5 21:11).

The 4 codex panes correctly show `agent_session.source = "herdr:codex"` — that
fix landed earlier and was already in the build at the time codex was last
restarted.

## Fix paths

### PATH A — Repoint `current` symlink to `0.91.0` (5 seconds, recommended)

```sh
ln -sfn /Users/kooshapari/.jcode/builds/versions/0.91.0/jcode \
        /Users/kooshapari/.jcode/builds/current/jcode
printf '%s\n' 0.91.0 > /Users/kooshapari/.jcode/builds/current-version
```

Pros:
- Zero rebuild
- 0.91.0 has the C1 fix (`herdr:jcode`) plus all subsequent herdr stability
  fixes through 0.91.0
- Once live --resume processes are restarted (or new ones are spawned), they
  pick up the new binary and start emitting `source: "herdr:jcode"`
- Crash-restore end-to-end test becomes possible

Cons:
- Advances the live build to `0.91.0` (commit 439a243bb), which is NOT in this
  local jcode repo's history. The local repo's HEAD is `5067e1a6f` and doesn't
  contain 0.91.0's source. Future builds from this repo will not match the
  running binary until the upstream is re-merged.
- The 0.91.0 binary is older than some fixes that landed in `b87cd9955-dirty`
  (specifically: the ratatui buffer guard `bf8ac73b5` and the ratatui-buffer
  guard refactor `9d70bd1b6`). However, these are TUI-side and do not affect
  the headless/non-TTY use case.

### PATH B — Rebuild current HEAD with release-lto profile (~100 min)

```sh
cd /Users/kooshapari/CodeProjects/Phenotype/repos/jcode
KCODE_RELEASE_PROFILE=release-lto scripts/install_release.sh
```

Pros:
- Local source matches running binary
- Picks up the ratatui guard + any commits after 0.91.0

Cons:
- 100+ minute build
- During the build, the wrapper resolution still picks `current` (still
  b87cd9955-dirty), so the live state remains broken until the build finishes
  AND install_release.sh rewrites `current` to the new build
- A subsequent `server reload` (line 138 of install_release.sh) will
  gracefully hand live headless/swarm sessions to the new process

### PATH C — Add post-build `codesign --force --sign -` to install_release.sh

**Tested and ruled out.** `codesign --force --sign -` replaces the existing
signature but does NOT add the `CS_LINKER_SIGNED` flag — it only sets
`CS_ADHOC`. The flag is added by Apple's linker at link time (when release
profile is used), not by the codesign tool. So this approach does not fix the
problem; the binary would still be rejected by amfid.

### PATH D — Update the wrapper to skip adhoc-only binaries (defensive)

Modify the kcode wrapper to fall through to the next candidate if the resolved
binary has `flags=0x2(adhoc)` (no linker-signed). This is a workaround, not
a fix — it doesn't address the build pipeline's failure to produce
linker-signed binaries. Defer until we understand why release-lto builds
sometimes produce linker-signed and sometimes not.

## Resolution (chose and shipped PATH B)

The operator picked the **long-term durable path**: rebuild HEAD with
`cargo build --profile release` (no LTO, proven linker-signed) and add a
post-install verifier to `install_release.sh` so a future LTO regression
can't ship a SIGKILL-prone binary silently.

### What the rebuild actually surfaced

After the new build produced a clean `flags=0x20002(adhoc,linker-signed)`
binary at `target/release/kcode`, the very first launch
(`./target/release/kcode --version`) printed:

```
xattr: ... No such xattr: com.apple.quarantine
... replacing existing signature
kcode v0.0.0-dev (6e0f0fc9c, dirty)
```

After that single launch, `codesign -dvv` showed `flags=0x2(adhoc)` with
**two rejection hash slots** — the linker-signed attribute was gone.
This means the linker-placed CS_LINKER_SIGNED attribute is destroyed by
**every launch** of the binary, not just by the build pipeline.

The culprit is `self_heal_macos_code_signature()` in `src/main.rs`,
which runs unconditionally at the top of `run_main` and calls
`codesign --force --deep --sign -`. `--sign -` is an adhoc identity that
does not carry the `CS_LINKER_SIGNED` attribute the linker placed during
the build; `--force` overwrites the original signature, deleting the
linker-signed attribute.

This is why every locally built jcode binary since the self-heal landed
exhibits the same SIGKILL pattern: the first interactive launch tears
the signature, and every subsequent non-TTY launch SIGKILLs.

A correct opt-in commit (`51f4e27e8` — "make startup executable re-sign
repair explicit opt-in") exists on
`origin/impl/macos-trust-policy-20260930` but was never merged onto
main. `27bd2299c` re-applies the gate to `feature/herdr-plugin-manifest`:

- New env var `KCODE_MACOS_STARTUP_REPAIR=1` (also accepts
  `true`/`yes`/`on`) explicitly opts into the historical xattr-strip +
  re-adhoc-sign path.
- Default behavior is OFF for release installs; the install pipeline owns
  quarantine handling.
- `self_heal_macos_code_signature` → `maybe_repair_macos_code_signature`
  (rename reflects the new semantics).

### Verification

```
$ cargo build --profile release --bin kcode
    Finished `release` profile [optimized] target(s) in 48.77s

$ codesign -dvv target/release/kcode | grep -E 'flags=|hashes='
CodeDirectory v=20400 size=1070264 flags=0x20002(adhoc,linker-signed) hashes=33442+0

$ ./target/release/kcode --version
kcode v0.0.0-dev (6e0f0fc9c, dirty)
   -- (no xattr/codesign lines; opt-in env var unset)

$ codesign -dvv target/release/kcode | grep -E 'flags=|hashes='
CodeDirectory v=20400 size=1070264 flags=0x20002(adhoc,linker-signed) hashes=33442+0
   -- (preserved across the launch)

$ ~/.local/bin/kcode --version | cat      # non-TTY pipe
kcode v0.0.0-dev (6e0f0fc9c, dirty)
$ echo $?
0

$ ~/.local/bin/kcode --resume nonexistent_session_id </dev/null  # non-TTY
Error: No session found matching 'nonexistent_session_id'
Use `kcode --resume` to list available sessions.
$ echo $?
0
```

### Install

```
~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode     <- linker-signed, amfid-accept
~/.kcode/builds/stable/kcode      -> versions/6e0f0fc9c-dirty/kcode
~/.kcode/builds/current/kcode     -> versions/6e0f0fc9c-dirty/kcode
~/.local/bin/kcode                -> ~/.kcode/builds/current/kcode
```

### install_release.sh durability hook

`scripts/install_release.sh` now post-verifies the linker-signed attribute
on macOS after `install -m 755`:

- `flags=0x20002` (or `CS_LINKER_SIGNED`) → silent (binary is good).
- `flags=0x2` (or `CS_ADHOC`) → warn loudly, name the file, point at this
  doc. If `KCODE_REQUIRE_LINKER_SIGNED=1`, abort with `exit 1` and remove
  the bad install.
- Default behavior is **warn-and-continue** so CI/dev workflows that
  intentionally ship LTO-only binaries still install (with a one-line
  grep-friendly warning); strict-mode is one env-var away.

This is the durable fix: future builds can't silently ship an
amfid-rejectable binary without an operator seeing the warning or
explicitly disabling it.

### Open follow-ups

- **Verify C1 `herdr:jcode` source namespace end-to-end.** The 6e0f0fc9c-dirty
  build contains the C1 fix; any pane launched from it should write
  `agent_resume.source = "herdr:jcode"` to session.json. SIGKILL one live
  pane after this commit and confirm herdr daemon relaunches it with the
  new source.
- **Push upstream.** Consider opening a PR against `1jehuang/jcode` to
  merge `51f4e27e8` and the install_release.sh verifier, since the same
  SIGKILL pattern will hit any downstream KCode consumer on macOS.
- **Investigate `install_release.sh` rerun behavior.** Two consecutive
  installs have produced different signature outcomes (Oct 5 stable is
  linker-signed; Oct 7 dirty is not). Now that the self-heal is gated,
  this should disappear — but keep the verifier in place.

## Open questions

- **Why do release-lto builds sometimes produce `flags=0x2` and sometimes
  `flags=0x20002`?** The 0.91.0 build (Oct 5) is linker-signed. The
  b87cd9955-dirty build (Oct 4) is not. Same `install_release.sh` invocation.
  Same profile (`release-lto`). Difference may be in incremental linking state
  in the `target/` directory at the time of the build. Not blocking the fix,
  but worth investigating before the next release.
- **Was `stable` repointed to 0.91.0 manually, or did `install_release.sh`
  run twice in quick succession?** The mtimes suggest the latter, but the
  `current` symlink was not updated, which is a behavior we don't expect from
  install_release.sh. Worth a code review of the script.
