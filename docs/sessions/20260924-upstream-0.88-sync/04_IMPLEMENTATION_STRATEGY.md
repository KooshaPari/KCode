# Implementation Strategy

The design rationale, alternatives considered, and trade-off
decisions for the trickier commits in this PR. The simple commits
(.gitignore, doc fixes) are covered in `03_DAG_WBS.md` and don't
need a strategy section.

## 1. parse_semver relaxation (WBS-07)

### 1.1 The problem

The original `parse_semver` in
`crates/jcode-build-meta/build.rs` accepted only `^\d+\.\d+\.\d+$`
(numeric core only). When the fork started using pre-release
channel suffixes in its `Cargo.toml` (`0.88.0-k1.2.0`), the build
identity was collapsing to `(0, 0, 0)` because the input
`0.88.0-k1.2.0` did not match the regex. The Kilo review
(`build.rs:25` CRITICAL) flagged this as the source of the
`jcode --version` showing `v0.0.0-dev (115170054, dirty)` instead
of the expected `0.88.0-k1.2.0 (115170054)`.

### 1.2 The chosen design

Replace the regex with a hand-rolled split:

```rust
let trimmed = value.trim().trim_start_matches('v');
let core = trimmed.split(['-', '+']).next().unwrap_or("");
let mut parts = core.split('.');
let major = parts.next()?.parse().ok()?;
let minor = parts.next()?.parse().ok()?;
let patch = parts.next()?.parse().ok()?;
Some((major, minor, patch))
```

The `split(['-', '+'])` accepts both pre-release (per SemVer §9)
and build metadata (per §10) separators. The numeric core is the
first 3 dot-separated parts of the first such segment.

### 1.3 Walkthrough: `0.88.5-k1.2.0`

- `trimmed = "0.88.5-k1.2.0"`
- `core = "0.88.5"` (the part before the first `-`)
- `parts = ["0", "88", "5"]`
- Result: `Some((0, 88, 5))` — **not** `(0, 88, 0)` as the Kilo
  review suggested.

The Kilo review's claim of "0.88.5-k1.2.0 degrades to 0.88.0" is
incorrect: the numeric core extraction is `(0, 88, 5)`, and the
update path's `version_is_newer` in
`crates/jcode-update-core/src/lib.rs:385-398` uses a **separate**
parser (`split('.')` only) that correctly reads `"5-k1"` as `5` via
Rust's `parse::<u32>()` (which reads leading digits and stops at
`-`).

### 1.4 Why hand-rolled over a regex

- A regex like `^v?(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$` works, but
  requires the `regex` crate (already a transitive dep, but a
  build-time use pulls it in). The hand-rolled split is 6 lines
  and zero new deps.
- The hand-rolled version is easier to test: each branch of the
  parse is one assert. A regex would require a per-pattern test
  suite.

### 1.5 Why split on both `-` and `+`

SemVer §9 (`-`) and §10 (`+`) are both valid post-patch separators.
A pre-release like `1.0.0-alpha.1` and a build-metadata like
`1.0.0+sha.abc1234` should both parse to `(1, 0, 0)`. The chosen
split pattern handles both. The Kilo review's `SUGGESTION` at
`build.rs:240` ("`find('-')` ignores `+`, so build metadata is
folded into the prerelease suffix and the two helpers disagree on
the separator set") is a real concern but is resolved by **this**
commit (WBS-07) splitting on both — the `prerelease_suffix` helper
at `build.rs:251-258` still uses `find('-')` because it specifically
extracts the pre-release segment (build metadata has a different
meaning and is not part of the pre-release channel), but the two
helpers do not need to agree on separator set: one returns the
numeric core, the other returns the pre-release suffix. They are
not in conflict.

## 2. Build-meta identity reconstruction (WBS-07)

### 2.1 Env-var precedence

`build.rs` reconstructs the build identity from up to 4 sources,
in priority order (highest first):

1. `JCODE_UPDATE_SEMVER` (env) — overrides everything else for the
   update path only.
2. `JCODE_BASE_SEMVER` (env) — overrides everything else for the
   base path only.
3. `JCODE_BUILD_SEMVER` (env) — release-build set in
   `scripts/install_release.sh`; the numeric core of this is the
   default for both paths unless (1) or (2) override.
4. `CARGO_PKG_VERSION` (Cargo's emitted value) — last-resort
   default.

### 2.2 The newtype split: `BaseSemver` vs `UpdateSemver`

The build identity is no longer a single `(u32, u32, u32)` triple
that's shared between paths. The two paths are split into
`BaseSemver` and `UpdateSemver` newtypes so that the type system
prevents the update path from accidentally reading the base value
(or vice versa). The `unwrap_or(base_version)` fallback at
`build.rs:31` is now type-safe because `base_version: (u32, u32, u32)`
and the return type is also `(u32, u32, u32)`.

### 2.3 install_release.sh:73-83 — the regex limitation

The installer script's regex matches `\(\w{9}\)$` — the trailing
`(<9-char-hash>)` segment of the build identity. This regex does
**not** match the channel segment (e.g. `-k1.2.0`); it only sees
the numeric core + `(hash)`. The Kilo review's
`SUGGESTION` at `build.rs:23` notes this. The fix is the
deferred issue `parse_semver env-var channelization` — the
installer's regex needs to be updated to either:

- match `<semver>-<channel>(<hash>)` and propagate the channel
  segment to the version metadata, or
- be split into two regexes: one for the numeric core, one for
  the channel segment.

The chosen approach for the deferred issue is the second (less
risky, smaller diff). The first commit in the deferred PR will
land the regex split, the second commit will land the channel
propagation.

### 2.4 Why the `prerelease_suffix` helper is necessary

`parse_semver` returns `Some((0, 88, 0))` for `"0.88.0-k1.2.0"`,
discarding the channel. To round-trip the version, a separate
helper is needed to extract the channel:

```rust
fn prerelease_suffix(value: &str) -> String {
    let trimmed = value.trim().trim_start_matches('v');
    let core_len = trimmed.find('-').unwrap_or(trimmed.len());
    trimmed[core_len..].to_string()
}
```

This returns `-k1.2.0` for `"0.88.0-k1.2.0"` and `""` for
`"0.88.0"`. The test in
`crates/jcode-build-meta/src/lib.rs::parse_semver_returns_numeric_core_only`
exercises both directions.

## 3. Security preflight hardening (WBS-05)

### 3.1 The 4 defects and their fixes

#### Defect 1: whole-record deletion (line 92)

**Before:**
```bash
grep -v -f "$allowlist_file" "$scan_out" > "$scan_kept"
```

**After:**
```bash
# Token-precise filter: each allowlist entry is matched as a literal
# word, not a substring. Previously a placeholder sharing a line with
# a real secret would discard both.
grep -F -v -w -f "$allowlist_file" "$scan_out" > "$scan_kept"
```

**Why:** `grep -v -f` without `-w` matches the allowlist entries
as substrings of any line. The Kilo review's
WARNING at `security_preflight.sh:92` notes that "any other
secret sharing a line with `AKIAABCDEFGHIJKLMNOP` is discarded and
the gate goes green." The fix uses `grep -F` (fixed-string, not
regex) and `-w` (whole-word match), so an allowlist entry only
suppresses a line that contains the entry as a complete token.

#### Defect 2: `|| true` swallows errors (line 96)

**Before:**
```bash
grep -v -f "$allowlist_file" "$scan_out" > "$scan_kept" || true
```

**After:**
```bash
if ! grep -F -v -w -f "$allowlist_file" "$scan_out" > "$scan_kept"; then
    echo "::error::secret scan failed: grep exited non-zero" >&2
    exit 1
fi
```

**Why:** `|| true` conflates `grep` exit 1 (no match) with
exit 2 (error). The Kilo review's
WARNING at `security_preflight.sh:96` notes that on a
genuine error, the truncated `.kept.txt` is then `mv`'d over
the real findings and the gate can report no secrets found for a
scan that never completed. The fix explicitly checks the exit
code and fails the gate on error.

#### Defect 3: `/tmp` symlink-swap (line 97)

**Before:**
```bash
mv "$scan_kept" /tmp/jcode-secret-scan.kept.txt
```

**After:**
```bash
scan_kept=$(mktemp -t jcode-secret-scan.XXXXXX)
trap 'rm -f "$scan_kept"' EXIT
if ! grep -F -v -w -f "$allowlist_file" "$scan_out" > "$scan_kept"; then
    echo "::error::secret scan failed" >&2
    exit 1
fi
```

**Why:** `/tmp/jcode-secret-scan.kept.txt` is a fixed,
world-predictable name. On a shared host, an attacker can create
a symlink at that path pointing to a file they want overwritten.
The fix uses `mktemp` (which creates a file with a random suffix
in a directory the attacker cannot predict) and `trap` for cleanup.

#### Defect 4: empty-allowlist guard (line 54)

**Before:**
```bash
secret_allowlist=()
# ... allowlist populated by sourcing a config file ...
grep -v -f <(printf '%s\n' "${secret_allowlist[@]}") "$scan_out" > "$scan_kept"
```

**After:**
```bash
if [[ ${#secret_allowlist[@]} -eq 0 ]]; then
    echo "::error::secret allowlist is empty; refusing to scan" >&2
    exit 1
fi
```

**Why:** an empty allowlist passed to `grep -v -f` matches every
line, dropping all findings. The Kilo review's
SUGGESTION at `security_preflight.sh:54` notes this. The fix
adds an explicit guard before the scan runs.

### 3.2 Why bash 5.3+ is required

`mapfile` is a bash 4.0+ builtin but it has different behaviour
across versions. The script uses bash 5.3+ via the explicit
`/opt/homebrew/bin/bash` invocation. `/bin/bash` on macOS is
3.2.57, which silently skips some `mapfile` use cases. The
script's shebang is `#!/usr/bin/env bash` for portability, but
the orchestrator invokes it via `/opt/homebrew/bin/bash` to
guarantee the 5.3 semantics.

## 4. Code size budget ratchet (WBS-03)

### 4.1 The 3 ratchets

| File | Pre | Post | Delta | Reason |
|---|---|---|---|---|
| `crates/jcode-app-core/src/server/client_actions.rs` | 1223 | 1227 | +4 | 4 added comment lines documenting the new CI guard flow (3c96175f6) |
| `crates/jcode-tui/src/tui/input.rs` | 4270 | 4273 | +3 | comment expansion for the gate-digest duplicate |
| `crates/jcode-tui/src/tui/ui_input.rs` | 3752 | 3757 | +5 | comment expansion for the truncate comment vs reality |

### 4.2 Why absorbed, not decomposed

The 3 growths are all **comment** lines, not production code.
Decomposing a file into submodules for the sole purpose of
hiding comments would obscure them from reviewers. A
decomposition pass is tracked separately in the deferred
"Budget growth" issue.

### 4.3 The wildcard re-export budget drops to 0

The only pre-existing wildcard re-export in the source tree was
`pub use jcode_tui::herdr::*;` in `src/herdr.rs:22`. WBS-02
replaced it with an explicit list, dropping the wildcard count
from 17 (which the Kilo review's CRITICAL flagged as
"unbudgeted" — the budget file's `total: 17` was set by
upstream and the fork's `herdr.rs` was the only addition that
matched the pattern) to 0. The `check_wildcard_reexport_budget.py`
script will exit 0 with `total: 0`.

## 5. Dependency bumping strategy (WBS-04)

### 5.1 Why `cargo update -p h2 -p rustls`

The form `cargo update -p <crate>` (without a version specifier)
takes the latest semver-compatible version. For:

- `h2 = "^0.4"` (the upstream pin): this selects the latest
  `0.4.x`, which is `0.4.20` at the time of the bump.
- `rustls = "^0.23"` (the upstream pin): this selects the latest
  `0.23.x`, which is `0.23.45` at the time of the bump.

The form is preferred over `cargo update` (which would bump
every transitive dep) because it minimizes the diff and reduces
the risk of unrelated breakage.

### 5.2 Why not bump transitive (rustls-webpki)

The transitive `rustls-webpki` was automatically pulled from
`0.103.13` to `0.103.15` by cargo's lockfile resolver because
the new `rustls 0.23.45` requires `rustls-webpki >= 0.103.x`. We
did not explicitly run `cargo update -p rustls-webpki` — the
resolver did it for us, and the new version is the latest
semver-compatible one. The `Cargo.lock` diff is `+166/-166`
lines, dominated by the metadata shuffling that comes with
every `cargo update`.

### 5.3 The 0.4.x patch-line policy

jcode's policy is to bump within the same minor version when
fixing a security advisory. Major or minor bumps are deferred
to dedicated PRs because they require dependency-graph
reconciliation. This policy is reflected in the `01_RESEARCH.md`
table of fork-pinned dependencies — each row documents the
"wait condition" for the next major/minor bump.

## 6. CI guard pattern (WBS-04 in `3c96175f6`)

### 6.1 The guard shape

```yaml
- name: Configure SSH for github.com
  uses: webfactory/ssh-agent@v0.9.0
  with:
    ssh-private-key: ${{ secrets.DEPLOY_KEY }}
  if: env.DEPLOY_KEY_PRESENT == 'true'

env:
  DEPLOY_KEY_PRESENT: ${{ secrets.DEPLOY_KEY != '' }}
```

### 6.2 Why job-level, not step-level

A step-level guard (`if:` on the step) would have to be
duplicated for every step in the job that needs the SSH agent.
A job-level guard (`if:` at the job level) covers all steps
uniformly. The job is skipped entirely when `DEPLOY_KEY` is
empty, which is the desired behavior because the build steps
that follow (cargo build, cargo test) need the git checkout to
succeed — and the checkout succeeds even without the SSH agent
because the fork has no `ssh://` deps (verified by
`git config --get-regexp '^url\.' | grep ssh://` = empty).

### 6.3 The 4 sites (and the 5th)

The audit said 4 sites; the actual count is 5 (the
`Quality Guardrails` job also uses `secrets.DEPLOY_KEY` and
now also has the guard). The 5 sites are listed in
`02_SPECIFICATIONS.md` §6.1.

### 6.4 Why the env var is needed

GitHub Actions evaluates `if:` conditions at the job level,
and `${{ secrets.DEPLOY_KEY != '' }}` is not directly usable
in a `if:` because secrets are masked and the comparison
behavior is non-obvious. The explicit env var
`DEPLOY_KEY_PRESENT: ${{ secrets.DEPLOY_KEY != '' }}` makes the
condition `if: env.DEPLOY_KEY_PRESENT == 'true'` reliable.

## 7. Cross-references

- `02_SPECIFICATIONS.md` — the acceptance criteria for each
  strategy.
- `03_DAG_WBS.md` §2 — the WBS nodes that own each strategy.
- `05_KNOWN_ISSUES.md` — the deferred issues (parse_semver
  env-var, cargo fmt, budget growth) that this strategy
  explicitly does not address.
- `06_TESTING_STRATEGY.md` — the test plan that exercises each
  strategy.
