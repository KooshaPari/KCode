# KCode product isolation

KCode and jcode are independent installations. Internal Rust library crate
names remain compatible with this source base; the executable is `kcode`
(`kcode.exe` on Windows), and the root package version carries a `-k` suffix.

| Resource | KCode | jcode (never a KCode destination) |
| --- | --- | --- |
| User state/config/sessions | `~/.kcode` | `~/.jcode` |
| Platform provider config | platform config directory + `kcode` | platform config directory + `jcode` |
| Windows binaries/builds | `%LOCALAPPDATA%\kcode` | `%LOCALAPPDATA%\jcode` |
| Unix launcher | `~/.local/bin/kcode` | `~/.local/bin/jcode` |
| Runtime sockets | `kcode.sock`, `kcode-api.sock` | `jcode.sock`, `jcode-api.sock` |
| Process overrides | `KCODE_*` | `JCODE_*` (not inherited as fallbacks) |

The executable, SDK, SSH launcher, self-development builds, installer, and
hotkey listener use KCode identity. Storage rejects explicit jcode destinations
and existing symlink/junction aliases into jcode storage.

## Updates and installation

Self-update is disabled, including background checks, explicit `/update`, and
direct download/install entry points. This does not depend on a version suffix
or an environment setting. The old updater used upstream's release repository;
the Windows installer independently used upstream metadata and install paths.
Neither is an acceptable KCode update source.

The fork's older `v0.85.0-k1.0.0` release is not a maintained compatible update
channel for this `0.93.0` source base. Re-enabling updates requires a deliberate
product-specific channel and artifact verification change. There is no fallback
to jcode. Source discovery uses only the KCode root package and fork URL.

Build and install a trusted local artifact on Windows:

```powershell
cargo build --release --bin kcode
$artifactVersion = ((& .\target\release\kcode.exe --version) -split '\s+')[1]
.\scripts\install.ps1 -ArtifactExePath .\target\release\kcode.exe -Version $artifactVersion
```

On Unix, use `scripts/install_release.sh --fast` to build and install from this
checkout. The download installer intentionally refuses to run. No shared PATH
shim is installed.

## One-way session import

```text
kcode session import-jcode <session-id>
```

This explicit operation copies the selected snapshot, backup, and journal from
`~/.jcode/sessions` to KCode storage, before normal startup side effects. It
never loads or repairs a source session, imports credentials/config/indexes,
or writes a reverse-sync record. Existing KCode session files are not replaced.
These are independent snapshots, rather than a continuous bidirectional mirror.

Regression tests import two sessions, edit the KCode copies, attempt repeated
imports, and require the entire jcode source tree to retain identical file
bytes, modification times, and permissions. Negative tests cover overlap,
traversal, and existing destinations. Installer tests execute Windows rejection
paths and verify that no install directory is created.

The earlier decollision and development-isolation branches supplied the separate
namespace conventions. Their older wholesale crate rename and Unix-only dev
importer were not suitable for merging over this newer source tree.

The TypeScript SDK uses KCode binaries, homes, config and sockets. Upstream npm
runtime dependencies are removed and its fork package is private until a
maintained channel exists. Existing `JcodeClient` API names remain compatible;
credential inheritance rejects jcode sources and aliases before creating files
or links. Build locally with `npm ci --ignore-scripts --omit=optional` and
`npm run build` from `sdk/typescript`.

## Verification

Windows executable check: `cargo check -p kcode --bin kcode
--no-default-features -j 1` passed. The six configured-model regressions and six
SDK boundary regressions passed. Storage's no-write-back and path-boundary
tests passed; the observed non-Unix PID-liveness test failure was explicitly
filtered on the focused storage rerun. Storage clippy passed with `-D warnings`.

The complete Windows build-support suite and the repository code-size budget
are not green. Full workspace/default-feature release validation and live
ConPTY behavior are not claimed by these checks. The anchor and kitty fixes
remain separate stacked changes.

## Forward delivery work breakdown

Audit anchor: `master` at `2aea04dd871f21da5c5b21800d156a2c9b0830b1`,
2026-10-21. Hosted source/release inspection is evidence about repository state;
it is not a local build, device install, CI pass, or remote-session receipt.
The initial Windows execution helper failed before command execution with
`orchestrator_helper_incomplete: setup helper exited successfully before setup completed`.
The execution transport was subsequently restored. Standalone native metadata
checks passed: nine build-script tests, three runtime tests, and actual generated
identity/override/rejection assertions. A persistent isolated Windows source build is now running; completion is not
claimed. Installation, live-server and terminal acceptance remain pending.
The current acceptance scope is Windows Terminal and Windows HERDR only.
Preserve shared agents and the separate stock jcode installation.

### Findings and existing work

- Root Cargo package and binary are `kcode`, version `0.93.0-k0.1.0`.
  The build metadata script parsed the fork-bearing patch as an integer,
  defaulted the numeric base to zero, and dropped the fork suffix while deriving
  a development version. This change uses its existing channel-aware parser,
  preserves the suffix and requires nonzero `BASE-kFORK_SEMVER` identity for
  root metadata and build overrides. Invalid runtime overrides preserve compiled
  KCode identity. A shared parser rejects bare stock, empty and malformed suffixes. This is source reasoning; the old installed
  Jaguar client's origin has not been demonstrated.
- `docs/sessions/20260917-herdr-pwsh-triage/FINDINGS.md` is historical
  source-based triage with estimated reproduction steps. Its OSC/PowerShell
  explanation remains a hypothesis until isolated reproduction proves it.
  Do not drain arbitrary queued input or suppress escapes based on that note.
- Fork PR [#124](https://github.com/KooshaPari/KCode/pull/124) already landed
  native Windows keyboard negotiation; inspect its behavior before another fix.
  The isolation document's earlier checks exclude full default-feature release
  validation and live ConPTY acceptance.
- Existing open convergence work includes [#152](https://github.com/KooshaPari/KCode/pull/152)
  (test binary identity), [#156](https://github.com/KooshaPari/KCode/pull/156)
  (format/lint), [#157](https://github.com/KooshaPari/KCode/pull/157)
  (turn notification identity), [#160](https://github.com/KooshaPari/KCode/pull/160)
  (SDK assertions), [#163](https://github.com/KooshaPari/KCode/pull/163)
  (Windows pipe fallback), and [#166](https://github.com/KooshaPari/KCode/pull/166),
  [#168](https://github.com/KooshaPari/KCode/pull/168),
  [#169](https://github.com/KooshaPari/KCode/pull/169) (workspace lint/build
  cache convergence). Reuse reviewed fixes; do not assume open means integrated.
- The published [v0.85.0-k1.0.0](https://github.com/KooshaPari/KCode/releases/tag/v0.85.0-k1.0.0)
  contains only a jcode-named FreeBSD asset. There is no verified Windows
  artifact in that release. The v0.90.0-k1.0.0 tag is not a published release;
  [Release run 37421410488](https://github.com/KooshaPari/KCode/actions/runs/37421410488)
  skipped.
- [Windows Smoke run 39111372009](https://github.com/KooshaPari/KCode/actions/runs/39111372009)
  failed runtime smoke. x64 build/test compilation passed, but 13 targeted filters discovered zero
  tests and only the final filter ran one test. Runtime smoke failed at
  provider_behavior::test_model_switch_resets_provider_session with
  'Did not receive Done for first message' (30.30s, exit 101).
  Lifecycle, version, and installer steps did not execute; ARM64 skipped and
  there were no downloadable artifacts. These partial passes do not clear the
  installation gate.
- The historical `docs/drafts/pr-fix-build-meta-channel.md` acceptance of
  zero versions and its test-pass assertions are superseded by this plan;
  they are not current validation receipts.
- The release workflow is guarded to upstream `1jehuang/jcode`; its packaging
  and Windows smoke paths still use `jcode.exe`, `jcode-*`, and
  `JCODE_BUILD_SEMVER`. Removing that guard alone cannot produce a safe fork
  channel. Dependabot alert state is UNKNOWN because the connector rejected
  that endpoint; no security pass is claimed.


### Windows console and input findings

- [Console isolation commit 3bf72746](https://github.com/KooshaPari/KCode/commit/3bf72746fce8185e15921d93ce8f6f56c36d7ab2)
  gives a newly spawned Windows KCode server null stdin and its own hidden
  console through the existing platform helper. Null/piped standard handles
  alone do not isolate console modes. The native regression passed one test:
  an inherited-child negative control changed input/output modes from
  `(503, 3)` to `(499, 7)`; the isolated child preserved `(503, 3)`.
  The Unix spawn path is unchanged. This proves the ownership mechanism,
  not the cause or resolution of every reported symptom.
- Existing live servers are adopted before the spawn path. Installing the
  candidate does not retrofit console isolation onto an already running
  stock server; preserve that server and verify KCode's separate runtime.
- `scripts/test_windows_conpty_input.ps1 -CompareVT` passed 48 isolated
  backend/case assertions at
  [b1672882](https://github.com/KooshaPari/KCode/commit/b16728829e9479c44e2bcf17248ae4f22b8c3bbd).
  Native console input with VT-input bit 512 off strips complete paste
  framing; splitting the initial escape can instead expose literal marker
  characters. With bit 512 on, both the record and byte readers preserve
  framing, including short/leading-newline and delayed payloads. These are
  transport fixtures, not KCode application acceptance.
- Locked Crossterm 0.29.0 preserves an inherited VT-input bit during raw-mode
  changes but uses its Win32 key decoder on Windows. Enabling VT input alone,
  disabling paste mode, or applying a typing-speed heuristic is not a complete
  fix: multiline input can reach deliberate submit shortcuts.
- Borrow candidate [Crossterm PR #1030](https://github.com/crossterm-rs/crossterm/pull/1030),
  exact revision `94ed7154e35d24a2f643c61760e1eb05b8ca9e63`, remains
  unmerged and is not a production dependency change here. It combines scoped
  VT-input mode with the shared ANSI decoder. Qualification must cover actual
  `Event::Paste`, ordinary keys, mouse/focus/resize, mode restoration and the
  reachable Windows restart/reload handoff. Its process-local original-mode
  snapshot does not by itself preserve ownership across a replacement process.
  Split opening escapes and unbounded unterminated paste remain explicit
  review gates; do not treat this candidate as a complete fix.
  The subsequent actual-Crossterm-event ConPTY probe passed 13 of 15 cases
  and failed both cases that split the opening escape by 30 ms: it emitted
  an Escape press and literal marker/payload key presses instead of one paste.
  Complete CR/LF/CRLF, leading-newline, delayed payload, trailing-escape split,
  ordinary controls, focus and resize cases passed. The exact candidate is
  rejected for a production dependency pin in this pass. Its successful
  transport cases do not clear the failing application-input boundary.
  Qualify the narrower console-isolation artifact before expanding backend work.

The observed failure domains are separate; failures are intermittent within
their reported ranges, not guaranteed message-count triggers:

| Route/domain | Reported behavior | Required distinction |
| --- | --- | --- |
| Direct Windows Terminal, before a message | Cursor can move above the composer into PowerShell; `exit` returns to Jcode, which may then continue working | Distinguish a child shell/console reader from TUI exit and from a visual cursor error |
| Direct and HERDR, reported around chats 2-5 | Literal `[200~` / `[43d` fragments and consumed Esc/Backspace make input unusable | Paste framing and cursor-output fragments are different evidence; correlate input/output mode ownership |
| Windows HERDR, including at startup | Cursor can fall below the composer into PowerShell; `exit` closes the pane | Preserve the difference from the direct route; identify which process and pane actually exited |

Do not extrapolate these symptoms outside their reported domains or use the
historical OpenSSH control as proof of native desktop PowerShell behavior.

### Dependency order and acceptance

```text
W1 transport + exact identity
  -> W2 version + existing convergence -> W3 native terminal matrix
  -> W4 Windows HERDR matrix
  -> W5 fork artifacts -> W6 coexistence install -> W7 update channel
```

| Work | Owner lane | Current state | Exit evidence |
| --- | --- | --- | --- |
| W1 | Windows runtime | Transport restored; complete runtime receipt pending | Resolve all PATH candidates. Record installed client and long-lived server executable, SHA-256, fork version, runtime directory, socket/pipe/port, lock and launch owner on this device. Distinguish a source archive from a checkout and a file version from a running service. Preserve stock processes/data. |
| W2 | KCode metadata/convergence | Focused native metadata gate passed; broader gates pending | Run metadata build-script tests, crate tests, product-isolation tests, formatting and required local gates; require nonzero discovered/passed test counts at the exact candidate SHA. Built client and daemon must report a nonzero BASEVERSION-kSEMVER; reject zero, malformed or mismatched release identity. Reuse open fixes after review. |
| W3 | Windows Terminal | Native mechanism fixtures passed; live application pending | Test direct Windows Terminal with PowerShell: ordinary typing, deliberate Enter/Esc/Backspace, Shift+Enter, short and leading-newline paste, delayed chunks, resize, reconnect, Ctrl+C and exit. Repeat within the observed failure ranges. Use process-owned input/output logs only; no screenshots/recordings. |
| W4 | Windows HERDR | UNKNOWN live application behavior | Repeat the Windows terminal matrix inside HERDR, including pane exit and session continuity; compare with the direct Windows Terminal control. Resolve the actual host/pane process and launch owner. Mac client, build and remote-view acceptance are outside this pass. |
| W5 | Build/release | Windows x64 and ARM64 artifacts DELIVERED and natively verified; both Windows targets now link the CRT statically; workflow patch staged, not merged | Use suitable existing self-hosted/free compute; record placement. Produce a KCode-only Windows x64 artifact for this device from the exact tested SHA (qualify ARM64 separately before publishing an ARM64 asset), fork versions, hashes, signing/provenance and nonzero client/server output. Add fork-specific workflow/installer-smoke paths; preserve upstream guard until replacement is validated. |
| W6 | Device install/coexistence | BLOCKED: no verified artifact/transport | Install trusted local artifact with scripts/install.ps1; verify user/process PATH in a fresh shell, executable hash/version, and correct server attachment. Stock jcode must remain separately launchable. Compare stock state bytes/mtimes/permissions before and after imports and KCode operations. |
| W7 | Maintained channel | Disabled intentionally | Publish only verified fork artifacts and correct release notes. Then enable fork-only authenticated/checksummed update discovery/install and test upgrade, failed download, wrong product/version/hash and rollback without touching jcode. |

W2 focused commands (standalone metadata gate executed on Windows in the
2026-10-21 follow-up; remaining workspace/isolation gates still unexecuted):

```text
pwsh -NoProfile -File scripts/test_build_meta.ps1
cargo test -p jcode-build-meta
python3.11 -m unittest discover -s tests -p test_kcode_product_isolation.py
cargo fmt --all -- --check
```

The isolation gate needs a **Python 3.11+** interpreter:
`tests/test_kcode_product_isolation.py` imports `tomllib`, which joined the
stdlib in 3.11. A bare `python` on a host that still defaults to 3.9 fails at
import with `ModuleNotFoundError: No module named 'tomllib'`, which is a host
artifact, not a product failure.

Before declaring W2 complete, verify that valid explicit fork overrides retain
precedence, zero overrides cannot produce zero output, missing/invalid root
metadata fails instead of manufacturing a version, and the selected Windows
release build preserves the full fork suffix. A test build alone does not
replace the W1 client/server runtime receipt.

### W5 delivery receipt (2026-10-25)

Windows x64 artifact for this device, built from the exact PR #173 head
`9476f47824f648cd6f61055a7793e04ac33858e6` (398 commits past `v0.93.0`, so
the fork version resolves to `0.93.398-k0.1.0`):

| Field | Value |
| --- | --- |
| macOS arm64 artifact | `kcode` (Mach-O 64-bit arm64), SHA-256 `bcc936b3296af6fc3a4e9d72c4f9350b9e78fe17d50cb776632a1e8f15eca5b7` |
| Windows x64 artifact | `kcode.exe` (PE32+ x86-64 console), SHA-256 `ea9eac909a83a3436978f478b56bd2c22d0ffe82b1db980e005d4bf7bf2c8182` |
| Windows ARM64 artifact | `kcode-arm64.exe` (PE32+ Aarch64 console), SHA-256 `e703df2500461bb4ced5bb774b01e4a1650120aedb937d19ef81bad319a483ba` |
| Reported version | `kcode v0.93.398-k0.1.0-dev (9476f478)` on all three artifacts |
| Reported semver | `0.93.398-k0.1.0` (fork channel preserved; no `0.0.0` collapse) |
| Native Windows run | `kcode.exe --version` and `kcode.exe version --json` both exit 0 on Windows (x64) |
| ARM64 native run | **PASS** — static-CRT artifact ran on a genuine ARM64 Windows guest (build 26100, CPU Architecture 12) with `C:\Windows\SysArm64` holding **0 files** and no `vcruntime*.dll` anywhere; `--version` and `version --json` both exit 0 |
| Windows ARM64 static-CRT artifact | `kcode-arm64-staticcrt.exe`, 79,425,536 bytes, SHA-256 `f1c5e7befbf72ec1c2117c83b4be8d7a37e8206773e6c36a7078ab60637a2f3c` |
| Windows x64 static-CRT artifact | `kcode-x64-staticcrt.exe`, 96,145,408 bytes, SHA-256 `e3769d51a96f9488e6accaef9b0591b1face43a224ade7f9f23ab7b15772e204` |
| Placement | macOS host: native arm64 build plus `cargo xwin` MSVC cross-builds; desk: native execution validation only |
| Preserved copies | `~/.kcode/builds/versions/9476f478/{kcode,kcode.exe,kcode-arm64.exe}` |

Placement note: the desk's Fedora WSL vhdx lives on `D:` (physical disk 0,
931 GB total, ~22 GB free). The first cross-compile attempt exhausted `D:`
and killed WSL mid-build, which also cost the `desk-linux` self-hosted CI
runner its connection and cancelled the remaining CI jobs. The Windows build
was therefore moved to the macOS host, which has the
`x86_64-pc-windows-msvc` target and `cargo-xwin` installed, leaving the
desk free for native execution validation only.

Workflow patch (branch `feature/fork-workflow-kcode-binaries`, PR #187,
issue #192, not merged): the workflows referenced `jcode.exe` and built with
`-p jcode --bin jcode`, while the root package is `kcode` and its only bin
target of that name is `kcode` (`jcode` is the library target). Those steps
therefore failed on a missing path instead of validating the binary that was
actually produced. Corrected in `ci.yml`, `release.yml`, `windows-smoke.yml`
(x64 and ARM64) and `freebsd-smoke.yml`. The release gate now admits this fork
only when the tag carries a fork-version channel, leaving the upstream guard
and third-party forks unchanged.

Still outstanding for W5: signing/provenance for the published Windows binary.

The ARM64 cross-build needed three fixes to cargo-xwin's ARM64 path, since
`ring` forces plain clang for windows+aarch64 while the clang-cl backend
emits MSVC-style flags, and cc-rs passes those through untouched:
`/imsvc <dir>` becomes `-I <dir>`, `/D<x>` becomes `-D<x>`, `/std:c11`
becomes `-std=c11`, and `/experimental:*` is dropped. Switching to
`cargo xwin --cross-compiler clang` is not a substitute: it substitutes
windows-msvc-sysroot headers whose ARM64 NEON intrinsics clang cannot
compile for MSVC. The real SDK headers compile cleanly with plain clang.

The dynamic-CRT binaries needed a VC++ runtime that a bare Windows image does
not have. The ARM64 image had an empty `C:\Windows\SysArm64`, so the first
native run failed with `STATUS_DLL_NOT_FOUND` on `VCRUNTIME140.dll` until an
ARM64 `vcruntime140.dll` was placed alongside it. This was a **target-wide**
defect, not an ARM64 one: `llvm-objdump -p` shows the x64 asset also imported
`VCRUNTIME140.dll` plus 11 `api-ms-win-crt-*` UCRT forwarders, and it only ran
because the desk happened to have the redistributable installed.

Resolved by linking the MSVC CRT statically rather than shipping a DLL or
requiring the redistributable:

```text
RUSTFLAGS="-C target-feature=+crt-static"
```

Applying it to `aarch64-pc-windows-msvc` removes `VCRUNTIME140.dll` and all 11
UCRT forwarders from the import table, leaving only the 17 Win32 system DLLs;
`x86_64-pc-windows-msvc` behaves identically. The static ARM64 artifact was
then run natively on an ARM64 guest after deleting a stray `vcruntime140.dll`
left by the earlier session, with `SysArm64` confirmed at zero files, and both
`--version` and `version --json` returned exit 0. The x64 static artifact is
6.9 MB smaller than its dynamic equivalent.

A second build defect surfaced while doing this for x64: repointing the
Homebrew `clang` symlink at the translating wrapper also intercepts
`clang-cl`, which then loses cl driver mode and rejects MSVC-only flags such
as `/FI` in `aws-lc-sys`. `clang-cl` therefore gets its own
`--driver-mode=cl` wrapper, and the build trap restores both symlinks on
`EXIT`/`INT`/`TERM`.

The operator confirmed the one-way boundary: Jcode data may be imported into
KCode only. Imports remain explicit and opt-in; KCode must never write back
into Jcode or share a writable home with it. The existing importer implements:

```text
stock jcode sessions --explicit selected snapshot import--> KCode sessions
stock jcode state   <------------- no write-back ---------- KCode
```

Configuration, credentials, caches, logs, sockets/ports, locks, launchers,
application/package identifiers and update channels require independent KCode
ownership. Internal Rust/API compatibility names may remain when they cannot
route runtime writes or stock launches. Continuous cross-device sync is not
implemented by the snapshot importer and is not claimed here.
