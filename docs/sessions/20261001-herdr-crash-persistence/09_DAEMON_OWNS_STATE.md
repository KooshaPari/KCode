# Herdr session persistence: the daemon owns the file

## Proven (2026-10-04)

1. `session.json` is **not** a plain config file. The herdr daemon holds
   session state in memory and rewrites the **entire** document on a timer
   (~60s observed).
2. Therefore **any external edit is reverted within about a minute.**
   Measured: applied a reconciled file (11675 B, resume=13, probes=0) at
   17:42:20; by 17:43:17 the daemon had restored 11385 B / resume=10 /
   probes=3. The 3 reverted rows were exactly the ones just fixed
   (slots 5 PhenoShared, 6 Agile, 8 ForgeCode).
3. A benign `herdr pane list` did **not** trigger a rewrite -> the save is a
   timer, not per-request.
4. **Consequence:** writing the file is necessary but NOT sufficient. The
   write must be followed by a **daemon restart** so the daemon re-reads
   the reconciled document as its startup state. After that restart the
   reconciled state persisted (probes=0 stable over many minutes).

## Correct ordering

    discover (server MUST be live)      # CLI is the source of truth
      -> stop daemon                     # verify with ps, never the CLI
      -> write file atomically           # os.replace
      -> let the daemon restart / start it
      -> verify + stability watch

Discovery cannot run with the daemon stopped: herdr's CLI auto-starts the
daemon, which would clobber the write. That is exactly why
`reconcile_sessions.py` is two-phase (`--emit-plan` / `--plan-file`).

## Auto-restart caveat

`herdr server stop` does not leave the server stopped. An **interactive
`herdr` client** (observed pid 41240/94469 on ttys000, inside Ghostty)
restarts it. So the stop must be followed immediately by the write; do not
assume a stopped server persists. Detection uses `ps`/`lsof` only.

## Wrong turn worth recording

I briefly repointed `~/.kcode/builds/current/kcode` after concluding the
build was broken from `spctl: invalid signature`. **That was a wrong
diagnosis**: the same directory backs `stable/kcode`, which is the binary
of the *running, healthy* server (pid 4603, up 3 days), and the binary
passes `--version` (rc=0). The signature complaint is expected for an
adhoc-signed local build and is not the cause of anything.

The real SIGKILL is narrower: it happens when the binary tries to start a
**server in a non-TTY context** (`kcode --resume <sid>` with stdin from
/dev/null -> "Server exited before signalling ready (signal: 9)"). That is
pre-existing and unrelated to session.json reconciliation. The pointer was
restored to its original target.

**Update 2026-10-08:** the underlying cause of that SIGKILL was diagnosed
after this write-up. The real culprit is `self_heal_macos_code_signature()`
in `src/main.rs` running unconditionally on every launch and stripping
the linker-placed `CS_LINKER_SIGNED` attribute via
`codesign --force --deep --sign -`. The durable fix landed in
`27bd2299c` (opt-in via `KCODE_MACOS_STARTUP_REPAIR=1`) and the post-install
signature check in `bfbcf898f` (see `10_SIGKILL_NON_TTY.md` for the full
root-cause write-up, build/install verification, and lesson notes).

Lesson: verify a diagnosis against a *running* instance before changing
shared launcher state.
