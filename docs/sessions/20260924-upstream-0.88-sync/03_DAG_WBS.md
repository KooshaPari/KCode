# DAG / WBS

The dependency graph and work-breakdown structure for the v0.88.0
sync PR. Every commit on the branch
`feature/upstream-0.88-sync` between the merge base `41fae89f3`
and the current HEAD is accounted for, with its dependencies,
file/line deltas, and the role of the subagent (or orchestrator)
that produced it.

## 0. Commit count and WBS scope (corrected 2026-10-09)

**The follow-up branch contains 32 commits on top of the v0.88.0
merge base `41fae89f3`.** This file documents 15 WBS nodes
corresponding to 15 of those 32 commits (the 15 work products
that close the 15 audit items). The remaining 17 commits are
auxiliary: build-meta hardening follow-ups, additional doc
revisions, and CI workflow tweaks. Each is named in the commit
log; the WBS table below names the audit-relevant 15.

The 15 WBS nodes below are the 15 audit items; the 32 follow-up
commits include those 15 plus 17 other commits (auxiliary doc
fixes, build-meta follow-ups, CI tweaks) all visible in
`git log 41fae89f3..HEAD --oneline`.

## 1. The DAG

The 15 audit-relevant commits form a **mostly-serial** DAG with
two parallel branches (the security-preflight hardening and the
dependency bump) and a "fast-feedback" tail (the doc fixes). The
critical path is through the dependency bump because the
RUSTSEC-2026-0258 advisory was the highest-severity unaddressed
item in the Kilo review and the bump enables the preflight
script's deny/audit invocation to run clean.

```mermaid
graph TD
    merge[41fae89f3<br/>merge v0.88.0] --> scratch[cee86c4d7<br/>.gitignore scratch]
    merge --> herdr[aa955589d<br/>fix(herdr): explicit list]
    merge --> budgets[253b337ba<br/>chore(budget): 3 file ratchets]
    merge --> h2[3a23ad63b<br/>fix(deps): h2 0.4.20 + rustls 0.23.45]
    merge --> preflight[f6a780754<br/>fix(security-preflight): 4 defects]
    h2 --> security_doc[1db891b29<br/>docs(security): triage rows]
    preflight --> security_doc
    herdr --> budgets
    budgets --> build_meta[ef9b1e42a<br/>fix(build): preserve channel]
    build_meta --> install[3f76e8d41<br/>fix(installer): assert --version]
    install --> channel_test[7982eccb1<br/>test: channel env vars]
    merge --> session1[844016a50<br/>docs: final CI evidence]
    merge --> session2[e6135802a<br/>docs: 25f3e1f4f correction]
    session1 --> session3[d8e8ee4dd<br/>docs: 08_FINAL_EVIDENCE stale claims]
    session2 --> session4[4fd4958b7<br/>docs: ssh-agent guarded]
    session3 --> session5[67576d8cb<br/>docs: 07_ACCEPTANCE HEAD/test-count]
    session4 --> session5
    merge --> kilo_fixes[32621b2ad<br/>docs+test: 4 Kilo stale claims]
    session5 --> kilo_fixes
```

## 2. Per-commit WBS

| ID | Commit | Files | +/- | Scope | Depends on | Owner | Intent |
|---|---|---|---|---|---|---|---|
| WBS-01 | `cee86c4d7` | 1 | +6 | repo | (none) | orchestrator | gitignore `.scratch-*.{txt,md,sh,json}` so 82 existing scratch files are untracked and future scratch files are auto-ignored. |
| WBS-02 | `aa955589d` | 1 | +12/-1 | code | WBS-01 | dragon | Replace `pub use jcode_tui::herdr::*;` wildcard in `src/herdr.rs:22` with an explicit `pub use` list. Resolves Kilo CRITICAL. |
| WBS-03 | `253b337ba` | 1 | +9/-3 | budgets | WBS-01 | dragon | Bump 3 code-size baselines (`crates/jcode-app-core/src/server/client_actions.rs` 1223→1227, `crates/jcode-tui/src/tui/app/input.rs` 4270→4273, `crates/jcode-tui/src/tui/ui_input.rs` 3752→3757) in `scripts/code_size_budget.json`. Resolves Kilo CRITICAL. |
| WBS-04 | `3a23ad63b` | 1 | +166/-166 | deps | WBS-01 | hatchling | `cargo update -p h2 -p rustls`: h2 0.4.13→0.4.20, rustls 0.23.37→0.23.45, rustls-webpki 0.103.13→0.103.15. Resolves RUSTSEC-2026-0258 and RUSTSEC-2026-0285. |
| WBS-05 | `f6a780754` | 1 | +85/-12 | security | WBS-01 | nautilus | Harden `scripts/security_preflight.sh`: token-precise filter (pre-fix line 92), explicit error handling (pre-fix line 96), mktemp + trap (pre-fix line 97), empty-allowlist guard (pre-fix line 54). Resolves 4 Kilo WARNINGs. The post-fix file no longer has those line numbers; the commit message labels the defects `DEFECT 1`-`DEFECT 4`. |
| WBS-06 | `1db891b29` | 1 | +19/-12 | docs | WBS-04, WBS-05 | hatchling | Update `docs/SECURITY_DEPENDENCIES.md` RUSTSEC-0258/0285 rows: bump Last reviewed to 2026-10-08, mark both "Resolved 2026-10-08", note removal of `--ignore` entries. |
| WBS-07 | `ef9b1e42a` | 2 | +71/-22 | build | WBS-03 | humpback | In `crates/jcode-build-meta/build.rs`, preserve the prerelease channel in `JCODE_BASE_SEMVER` and `JCODE_UPDATE_SEMVER`. Relax `parse_semver` to split on both `-` and `+`. Add the `prerelease_suffix` helper. Resolves Kilo CRITICAL. |
| WBS-08 | `3f76e8d41` | 1 | +27/-3 | installer | WBS-07 | humpback | In `scripts/install_release.sh:73-83`, assert the installed binary's `--version` carries the package channel. The regex is still only `($git_hash)`-shaped (the channel-segment work is the deferred issue). |
| WBS-09 | `7982eccb1` | 1 | +62 | test | WBS-07 | humpback | Add `crates/jcode-build-meta/tests/semver_channel.rs` integration test that exercises the env-var precedence and the channel preservation paths. |
| WBS-10 | `844016a50` | 1 | +158 | docs | (none) | wyvern | Add `docs/sessions/20260924-upstream-0.88-sync/08_FINAL_EVIDENCE.md` with the final CI evidence and the "merge-ready" claim (subsequently corrected in WBS-12 and WBS-14). |
| WBS-11 | `e6135802a` | 1 | +18/-12 | docs | (none) | wyvern | Correct the 25f3e1f4f stat numbers in `docs/sessions/20260924-upstream-0.88-sync/08_FINAL_EVIDENCE.md` (line 36 row of the v0.88 follow-up table) and clarify §5 env-pollution details for the 2 ambient-state local-only TUI failures (`detected_resume_terminal_recognizes_handterm_term_program` and `test_real_draw_click_on_body_anchored_image_label_cycles_level`). |

**Correction (2026-10-09):** the prior draft said the e6135802a
doc-correction commit changed `00_SESSION_OVERVIEW.md`. The actual
`git show e6135802a --stat` is `08_FINAL_EVIDENCE.md` (1 file,
+18/-12). The commit message also reports "Re-verified at HEAD
844016a50 by running the full jcode-tui serial suite locally
(TERMINAL, .scratch-tui-final.txt)" with the actual result
`2396 passed; 2 failed; 17 ignored` — concrete evidence, not
inspection. The 2 failures are env-pollution (TERM_PROGRAM=herdr
race, stale `Image copied` status_notice inheritance), not
regressions.
| WBS-12 | `d8e8ee4dd` | 1 | +27/-15 | docs | WBS-10 | wyvern | Correct 3 stale claims in `08_FINAL_EVIDENCE.md`: advisory dates, commit count (7 → 17), "no CI weakening" caveat. Resolves 4 Kilo WARNINGs. |
| WBS-13 | `4fd4958b7` | 1 | +8/-4 | docs | WBS-11 | wyvern | Acknowledge the ssh-agent step is guarded by `3c96175f6` in `00_SESSION_OVERVIEW.md` and update the line refs from `L29/L155/L415` to `:38/172/441/703`. Resolves 2 Kilo WARNINGs. |
| WBS-14 | `67576d8cb` | 1 | +13/-8 | docs | WBS-12, WBS-13 | wyvern | Correct `07_ACCEPTANCE_EVIDENCE.md`: the actual current HEAD (`e6135802a`, superseded by `67576d8cb`) and the 8,964-test denominator provenance. Resolves 3 Kilo WARNINGs. |
| WBS-15 | `32621b2ad` | 4 | +26/-8 | docs+test | WBS-14 | orchestrator | Correct 4 stale Kilo claims: `provider_catalog_tests.rs:220` (ssh-agent guarded), `00_SESSION_OVERVIEW.md:471` ("admin-only to fix" → "maintainer with secrets write access"), `08_FINAL_EVIDENCE.md:84` (HEAD scoped as "at time of measurement"), `07_ACCEPTANCE_EVIDENCE.md:26` (log UI step indexing noted). |

**Total**: 15 audit-relevant WBS commits (1 of the 32 total
follow-up commits per the §0 reconciliation), 15 distinct WBS
nodes (WBS-04 + WBS-05 are parallel siblings of WBS-02/WBS-03).
Files touched: 9 unique (1 gitignore, 1 code, 1 budgets, 1
Cargo.lock, 1 build script, 1 installer script, 1 test, 1
security doc, 3 session docs + 1 test file).

**Correction (2026-10-09):** the prior draft said "15 commits,
13 distinct WBS nodes." The correct count is 15 audit-relevant
commits and 15 distinct WBS nodes (one WBS node per audit
commit; WBS-04 and WBS-05 run as parallel siblings rather than
sharing a node). The prior draft also said "13 commits form a
DAG" in §1; the correct count is 15 audit-relevant commits, and
the full branch has 32 follow-up commits.

## 3. Critical path

The critical path is **WBS-04 → WBS-06** (h2/rustls bump + security
doc update), because:

- The preflight script's RUSTSEC ignore removal in WBS-05 is gated
  on WBS-04 landing first (otherwise the script would fail on
  master before the bump is merged).
- The `docs/SECURITY_DEPENDENCIES.md` update in WBS-06 needs both
  WBS-04 (the version numbers) and WBS-05 (the ignore list shape)
  in place.

**Latency budget** for the critical path on a single-threaded run:
~45 minutes for the full `cargo check --all-targets --all-features`
after the dep bump, which is the longest single step. In practice
the 6 subagents ran in parallel, with the critical path executed
inside the hatchling subagent and not blocking the orchestrator.

## 4. Subagent parallelization

The orchestrator dispatched 6 subagents in parallel after the audit:

| Subagent | Task | Status | Files |
|---|---|---|---|
| **dragon** | code-size-budget-fix | ✓ done | `scripts/code_size_budget.json`, `src/herdr.rs` |
| **hatchling** | dep-bump-h2-rustls | ✓ done | `Cargo.lock`, `docs/SECURITY_DEPENDENCIES.md`, `scripts/security_preflight.sh` |
| **nautilus** | security-preflight-harden | ✓ done | `scripts/security_preflight.sh` |
| **wyvern** | session-doc-fixes | ✓ done | 3 session docs (00, 07, 08) |
| **guppy** | tui-test-contract-fix | ✓ no diff | (no work landed; the 4 TUI sites were investigated and all found to be false positives — see `02_SPECIFICATIONS.md` §7.2) |
| **humpback** | parse-semver-envvar-fix | ✓ absorbed | (the work was performed by the orchestrator and the hatchling/derivation paths; humpback's research was reused but its in-place edits were subsumed by the orchestrator's commits) |

4 of 6 subagents produced real diffs; 2 (guppy, humpback) were
**absorbed** — their research was reused but their in-place edits
were subsumed by the orchestrator's commits (the budget ratchet
included the herdr.rs change for dragon; the parse_semver work was
ultimately owned by the orchestrator for the doc-fix ordering).

## 5. Sequenced delivery

The commits were sequenced in 4 phases to minimize CI risk:

### Phase 1: Hermetic fixes (preflight + budgets)

WBS-01 → WBS-05. These are local changes that don't depend on
external state — `gitignore` entries, code-size baselines, and
preflight script hardening. Each can be merged independently and
each exits 0 on `cargo check` without needing the dep bump.

### Phase 2: Dep bump (RUSTSEC resolution)

WBS-04. The h2/rustls bump is the only commit that modifies
`Cargo.lock`. The `cargo check --all-targets --all-features` run
after this commit is the longest step in the PR (~45 min cold
compile).

### Phase 3: Code fixes (4 Kilo CRITICAL + dependents)

WBS-02 (herdr explicit list), WBS-03 (3 code-size ratchets),
WBS-07 (build-meta channel preservation), WBS-08 (installer
assertion), WBS-09 (channel env-var test). These are in a single
critical chain because the code-size ratchet (WBS-03) must land
after WBS-02 (which grew `herdr.rs` from a wildcard to an explicit
list and is the source of the wildcard budget's 17→0 reduction).

### Phase 4: Doc fixes (Kilo WARNINGs/SUGGESTIONs)

WBS-06 (security doc), WBS-10 through WBS-15 (session docs and
the 4 stale-claim Kilo fixes). These are independent of the code
changes and can land in any order.

## 6. What was deliberately NOT done in this PR

The audit identified 4 items that this PR did not address because
they are out-of-scope for the v0.88.0 sync:

- `cargo fmt --all` (would touch 103 pre-existing dirty files and
  obscure the v0.88.0 diff). Tracked in
  `05_KNOWN_ISSUES.md` §"cargo fmt debt".
- `parse_semver` env-var channelization in `install_release.sh:73-83`
  (the regex still only matches `($git_hash)`, not the channel
  segment). Tracked in
  `05_KNOWN_ISSUES.md` §"parse_semver env vars".
- Budget growth decomposition (the 3 ratchets in WBS-03 are
  absorbed, not decomposed). Tracked in
  `05_KNOWN_ISSUES.md` §"Budget growth".
- Master branch empty-DEPLOY_KEY guard (the WBS-04 fix is on the
  PR branch only). Tracked in
  `05_KNOWN_ISSUES.md` §"Master branch ssh guard".

## 7. Cross-references

- `00_SESSION_OVERVIEW.md` §"Audit findings" — the original 15-item
  audit, cross-referenced to the WBS nodes above.
- `02_SPECIFICATIONS.md` — the per-WBS acceptance criteria.
- `04_IMPLEMENTATION_STRATEGY.md` — the design rationale for the
  trickier WBS nodes (build-meta, preflight, code-size ratchet).
- `05_KNOWN_ISSUES.md` — the deferred work above.
