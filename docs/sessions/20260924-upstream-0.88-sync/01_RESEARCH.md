# Research

External API and library research, precedents from upstream, and third-party
patterns discovered while syncing `KooshaPari/KCode` to upstream `1jehuang/jcode`
v0.88.0 (`ee4cd3db3`).

## RUSTSEC-2026-0258 (`h2` 0.4.x)

- **Published:** 2026-08-17 (rustsec.org advisory database).
- **Severity:** resource exhaustion, triggered by a remote peer sending
  unbounded empty DATA frames on an HTTP/2 connection.
- **Reachable in jcode:** any HTTP/2 client path — `reqwest` for provider
  calls, `aws-smithy-http-client` for AWS provider telemetry. The
  vulnerability is server-reachable; a malicious origin (or a
  man-in-the-middle that successfully completes the TLS handshake) can
  send the trigger frames to jcode's HTTP/2 clients.
- **Patched:** in `h2` 0.4.x at the `>= 0.4.16` line. The fork's pre-merge
  pin was `h2 0.4.13` (one patch behind), inherited from upstream
  v0.88.0's `Cargo.lock`.
- **Resolution in this PR:** `cargo update -p h2` -> `h2 0.4.20`. The
  `--ignore RUSTSEC-2026-0258` line in `scripts/security_preflight.sh`
  was removed at the same time so the next pre-release CI run exercises
  the patched path by default. Tracked in
  `docs/SECURITY_DEPENDENCIES.md` (row "Resolved 2026-10-08").
- **Upstream reference:** <https://rustsec.org/advisories/RUSTSEC-2026-0258.html>

## RUSTSEC-2026-0285 (`rustls` 0.23.x)

- **Published:** 2026-09-14 (rustsec.org advisory database).
- **Severity:** 5.3 ("high"). TLS 1.3 handshake messages can be accepted
  across encryption-level boundaries, which breaks the handshake state
  machine.
- **Reachable in jcode:** every rustls consumer — `tungstenite` /
  `tokio-tungstenite` (websocket), `hyper-rustls` (HTTPS via
  hyper-util), the IMAP/email stack. The vulnerability is
  remote-triggerable from any TLS endpoint jcode talks to; a malicious
  server can craft a handshake that crosses the boundary and then
  inject or replay records as if they were in a different encryption
  epoch.
- **Patched:** in `rustls` 0.23.x at the `>= 0.23.45` line. The fork's
  pre-merge pin was `rustls 0.23.37` (eight patches behind), inherited
  from upstream v0.88.0's `Cargo.lock`.
- **Resolution in this PR:** `cargo update -p rustls` -> `rustls 0.23.45`.
  The `--ignore RUSTSEC-2026-0285` line in `scripts/security_preflight.sh`
  was removed. The transitive companion `rustls-webpki` was pulled from
  `0.103.13` to `0.103.15` automatically by cargo's lockfile resolver.
  Tracked in `docs/SECURITY_DEPENDENCIES.md`.
- **Upstream reference:** <https://rustsec.org/advisories/RUSTSEC-2026-0285.html>

## webfactory/ssh-agent step on empty secrets

- **Behaviour:** `webfactory/ssh-agent@v0.9.0` runs
  `ssh-add - <<< "${{ secrets.DEPLOY_KEY }}"` and fails with
  `##[error]The ssh-private-key argument is empty` when the secret is
  unset (an empty string is forwarded as `<empty>` and the heredoc closes
  before the key body). On `master` and any branch with no populated
  `DEPLOY_KEY` secret, this step therefore fails every job that uses it.
- **Pattern in the fork's CI:** the four `Build & Test` jobs (macos /
  ubuntu / windows), `Quality Guardrails`, and `Windows Cross-Target
  Check (Linux)` all use `webfactory/ssh-agent` to provide a key for any
  `ssh://` git dependency. The fork has no `ssh://` git dependencies
  (verified by `git config --get-regexp '^url\.' | grep ssh://` =
  empty), so the checkout succeeds even when the step fails — but the
  step's failure propagates to the job as a hard error.
- **Resolution in this PR:** commit `3c96175f6` added a job-level guard
  `if: env.DEPLOY_KEY_PRESENT == 'true'` at the four sites
  (`.github/workflows/ci.yml:38/172/441/703`), with the env var
  declared at `:24/148/427/689` as
  `DEPLOY_KEY_PRESENT: ${{ secrets.DEPLOY_KEY != '' }}`. On branches
  with an empty `DEPLOY_KEY`, the step is now SKIPPED and the job
  proceeds to compile. The same guard can be applied to `master` in a
  one-commit follow-up.

## GitHub Actions step numbering and the `Set up job` log prepending quirk

- **Quirk:** the GitHub Actions log viewer numbers steps starting at
  `#1`, but the raw log stream from `gh api .../jobs/{id}/logs` prepends
  a `Set up job` line (and other housekeeping lines) that the UI hides
  but the API exposes. Any REST cross-check that lists the steps as a
  JSON array is off by one for indices after that header.
- **Consequence:** in `07_ACCEPTANCE_EVIDENCE.md` and elsewhere, step
  indices refer to the **log UI** numbering (the canonical one for
  human readers), and the off-by-one is called out where the doc needs
  to cross-reference REST data. Verified by hand: in the `Build & Test
  macos` job `110582407896`, the UI shows `ssh-agent` as step #3 and
  the first 2 lines in the raw stream are the `Set up job` boilerplate
  and the `Initialize containers` header.
- **Implication for automation:** tooling that parses the raw log must
  subtract 1 from each step index after the `Set up job` line, or use
  the structured `steps` array from `gh api .../jobs/{id}` (which is
  the authoritative source) and ignore the raw stream.

## Cargo semver (pre-release + build metadata)

- **Spec reference:** [SemVer 2.0.0](https://semver.org/), specifically
  §9 ("A pre-release version MAY be denoted by appending a hyphen and a
  series of dot separated identifiers immediately following the patch
  version. Identifiers MUST comprise only ASCII alphanumerics and
  hyphens [0-9A-Za-z-]. Identifiers MUST NOT be empty.") and §10
  ("Build metadata MAY be denoted by appending a plus sign and a series
  of dot separated identifiers immediately following the patch or
  pre-release version.").
- **Cargo's behaviour:** `cargo update` respects the `^` and `~`
  operators in `Cargo.toml` and the `=` operator in dependency
  specifications. The `cargo update -p <crate>` form **without** a
  version specifier takes the latest version that is still
  semver-compatible with the constraint (i.e. the latest in the same
  major). For `h2 = "^0.4"` (upstream's pin), this means the latest
  `0.4.x` is selected — `0.4.20` at the time of the bump.
- **Build identity interaction:** the fork uses a pre-release channel
  suffix in its `Cargo.toml` (`version = "0.88.0-k1.2.0"`) so the
  build identity is distinguishable from upstream `0.88.0`. The
  `parse_semver` helper in `crates/jcode-build-meta/build.rs:235-244`
  extracts the numeric core by splitting on both `-` and `+` and
  taking the first 3 dot-separated parts — see
  `04_IMPLEMENTATION_STRATEGY.md` §"parse_semver relaxation" for the
  design rationale.

## The 5 fork-pinned dependency advisories

These are the five RUSTSEC ignores that remain in
`scripts/security_preflight.sh` after this PR. Each is **intentionally**
kept on the ignore list (the triage is recorded in
`docs/SECURITY_DEPENDENCIES.md`); removing the ignore requires either a
patched release or a transitive dependency upgrade that brings the fix.

| Crate | Advisory | Triage owner | Wait condition |
|---|---|---|---|
| `lettre` | RUSTSEC-2026-0141 | not isolated; the boring-tls feature is not enabled | `lettre` ships a patched release **or** jcode changes its TLS backend |
| `rustls-webpki` (rustls 0.21) | RUSTSEC-2026-0049 | in `aws-smithy` rustls 0.21 / `imap`/`rustls-connector` rustls 0.22 stack | major bumps of `aws-sdk` and `imap` dependency stacks |
| `rustls-webpki` (rustls 0.23) | RUSTSEC-2026-0098, RUSTSEC-2026-0099, RUSTSEC-2026-0104 | in `rustls` 0.23 stack | upstream `rustls-webpki` releases a fix in the line that the `rustls 0.23` consumers require |
| `lopdf` (via `pdf-extract 0.8.2`) | RUSTSEC-2026-0187 | in `jcode-pdf -> pdf-extract 0.8.2 -> lopdf 0.34`; only reachable when jcode extracts text from a user-opened PDF | `pdf-extract` ships a release depending on `lopdf >= 0.42` |
| `quick-xml` (via `wayland-scanner`) | RUSTSEC-2026-0194, RUSTSEC-2026-0195 | in `wayland-scanner` build-time proc-macro; parses trusted, vendored Wayland protocol XML | `wayland-scanner` moves to `quick-xml >= 0.41` |

The transitive `bincode` (RUSTSEC-2025-0141), `paste` (RUSTSEC-2024-0436),
`lru` (RUSTSEC-2026-0002), `rand` (RUSTSEC-2026-0097), and `lexical-core`
(RUSTSEC-2023-0086) advisories are not in the ignore list — they appear
in `cargo audit` output as warnings and the preflight script exits 0
when the only output is warnings (the script distinguishes
`RUSTSEC-*-0*` from warnings, only the former trips the gate).

## Upstream PRs/issues that motivated the v0.88.0 merge

The 00_SESSION_OVERVIEW.md cites these as the pre-merge fork baseline:

- The fork's own commits up to `115170054` (last commit before the
  merge) on the `feature/herdr-plugin-manifest` branch — these are
  preserved by the non-squash merge and appear on this PR's branch as
  the 21 commits immediately after the merge commit `41fae89f3`.
- Upstream `1jehuang/jcode` v0.88.0 release commit `ee4cd3db3`. The
  release notes (in upstream's CHANGELOG) call out the breaking changes
  in the build identity, the provider catalog, and the TUI test
  contract — all of which surface in this PR's follow-up fixes.
- The fork's divergence on the `JCODE_BUILD_SEMVER` env var
  (introduced in `2767fafff` on the fork side, with the upstream
  `parse_semver` collapsing fork versions to `0.0.0-dev` until
  `25f3e1f4f`-style fixes landed).

## What the deferred issues track

Three follow-up issues are pre-staged in `/tmp` and the bodies are
referenced from `05_KNOWN_ISSUES.md`. They capture the work that this PR
deliberately did not do because it would have expanded the diff beyond
the v0.88.0 sync scope:

- `parse_semver` env-var channelization (the install_release.sh regex
  still only matches `($git_hash)`, not the channel segment).
- `cargo fmt` cleanup (102 pre-existing dirty files blocking the CI
  `Quality Guardrails` job from reaching the budget gates).
- Budget growth (4 absorbed ratchets since v0.88.0; no decomposition
  path, and the panic baseline moved 77 -> 181 with no offsetting
  reduction).

See `05_KNOWN_ISSUES.md` for the precise deferral record.
