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
identity/override/rejection assertions. Home checkout access and a complete
application build remain pending; no install, live-server or remote-session
acceptance is implied. Mac runtime state remains UNKNOWN. Preserve shared agents.

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
  contains only a jcode-named FreeBSD asset. There is no verified Windows or Mac
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

### Dependency order and acceptance

```text
W1 transport + exact identity
  -> W2 version + existing convergence -> W3 native terminal matrix
  -> W4 Mac client -> Windows HERDR host matrix
  -> W5 fork artifacts -> W6 coexistence install -> W7 update channel
```

| Work | Owner lane | Current state | Exit evidence |
| --- | --- | --- | --- |
| W1 | Windows/Mac runtime | Transport restored; complete runtime receipt pending | Resolve repo/branch/dirty state and all PATH candidates. Record client and long-lived server executable, SHA-256, fork version, runtime directory, socket/pipe/port, lock and launch owner independently on both hosts. Preserve stock processes/data. |
| W2 | KCode metadata/convergence | Focused native metadata gate passed; broader gates pending | Run metadata build-script tests, crate tests, product-isolation tests, formatting and required local gates; require nonzero discovered/passed test counts at the exact candidate SHA. Built client and daemon must report a nonzero BASEVERSION-kSEMVER; reject zero, malformed or mismatched release identity. Reuse open fixes after review. |
| W3 | Windows terminal | UNKNOWN live behavior | Standard Windows Terminal with PowerShell, then HERDR-local: test ordinary typing, Enter, Shift+Enter, paste, resize, reconnect, Ctrl+C and exit using process-owned input/output logs. Independent negative control outside HERDR. No screenshots/recordings. |
| W4 | Mac client / Windows HERDR host | UNKNOWN live behavior | The Mac is the remote client viewing/controlling the Windows HERDR host. Resolve the actual deployed transport from docs/config, then verify Windows host server transport and HERDR remote-view integration, including typing, paste, resize, reconnect and session continuity from the Mac. Record host server identity/IPC separation and client identity independently. Native Unix SSH-client support is not the primary blocker for this route. Do not substitute a Windows-to-Mac route. |
| W5 | Build/release | BLOCKED by validation | Use suitable existing self-hosted/free compute; record placement. Produce KCode-only Windows x64/ARM64 and supported Mac assets from exact tested SHA, fork versions, hashes, signing/provenance and nonzero client/server output. Add fork-specific workflow/installer-smoke paths; preserve upstream guard until replacement is validated. |
| W6 | Device install/coexistence | BLOCKED: no verified artifact/transport | Install trusted local artifact with scripts/install.ps1; verify user/process PATH in a fresh shell, executable hash/version, and correct server attachment. Stock jcode must remain separately launchable. Compare stock state bytes/mtimes/permissions before and after imports and KCode operations. |
| W7 | Maintained channel | Disabled intentionally | Publish only verified fork artifacts and correct release notes. Then enable fork-only authenticated/checksummed update discovery/install and test upgrade, failed download, wrong product/version/hash and rollback without touching jcode. |

W2 focused commands (standalone metadata gate executed on Windows in the
2026-10-21 follow-up; remaining workspace/isolation gates still unexecuted):

```text
pwsh -NoProfile -File scripts/test_build_meta.ps1
cargo test -p jcode-build-meta
python -m unittest discover -s tests -p test_kcode_product_isolation.py
cargo fmt --all -- --check
```

Before declaring W2 complete, verify that valid explicit fork overrides retain
precedence, zero overrides cannot produce zero output, missing/invalid root
metadata fails instead of manufacturing a version, and both Windows and Mac
release builds preserve the full fork suffix. A test build alone does not
replace the W1 client/server runtime receipt.

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
