# Inbox Notification Gap — source trace and false-alarm correction

**Session:** `feature/upstream-0.88-sync` at `8ee60a85766eb29ab34cff0e2119cb5428e114c9`
**Date:** 2026-10-09
**Outcome:** one real defect found (notification gap), one earlier concern disproven (`QUOTED_DATA` execution risk).

---

## 1. What triggered this investigation

While verifying the six deferred `gh` write actions staged for operator approval, I found
every queued inbox item carried `notified_via: []`. Two separate concerns surfaced:

1. **Concern A (later disproven):** the pre-tool hook appeared to substitute the literal
   string `QUOTED_DATA` for any whitespace-containing argument, so `gh issue create --title ...`
   would execute with `title=QUOTED_DATA` and fail.
2. **Concern B (confirmed as a real bug):** deferred approvals are never surfaced to the
   operator by any passive channel.

Both were traced through source. Results below.

---

## 2. Concern A — `QUOTED_DATA`: disproven

### The claim (from an earlier pass)

`jcode-shell-classifier.py:176` contains:

```python
args = [arg if not re.search(r"\s", arg) else "QUOTED_DATA" for arg in words[1:]]
```

The five deferred `gh issue create` commands showed `--title QUOTED_DATA` in the inbox
`spec.question`. I concluded that on approval the command would run with the literal
placeholder and fail.

### Why the claim was wrong

The substitution is real, but it is **display-only**. Tracing the full path:

**Step 1 — classification copy (`jcode-tool-safety:80`):**

```sh
INPUT="$(printf '%s' "$INPUT" | python3 "$(dirname "$0")/jcode-shell-classifier.py")"
```

`jcode-shell-classifier.py` emits a *sanitized re-rendering* of the command used only for
pattern matching. Note the classifier's own comment at lines 175-176:

```python
# Quoted prose must not become executable tokens or option substrings.
```

The intent is to prevent quoted prose from being misparsed as a new command or flag during
classification. It is a parse-safety measure, not a rewrite of the user's command.

**Step 2 — the sanitized string is used only for `grep` gating
(`jcode-tool-safety:80-115`):** every subsequent decision is `echo "$INPUT" | grep -qE '...'`.
No code path re-executes `$INPUT`.

**Step 3 — what is sent to the approver (`jcode-tool-safety:96`):**

```sh
local_cmd="$(printf '%s' "$INPUT" | head -c 400)"
```

`local_cmd` is passed as the third argument to `gate()`, which passes it as `$3` to
`request_approval()`. There it lands in `details.command` and the `Command:` line of
`spec.question` — **UI text for the operator to read.**

**Step 4 — `request_approval()` never executes (`jcode-tool-safety:24-72`):** it calls
`elicitate ask --from-json "$spec"`, inspects the response status, and then does exactly one
of:

| status | action |
|---|---|
| `answered` + approved | `exit 0` — **the original bash command runs normally through jcode** |
| `answered` + denied | `block "…was DENIED by the operator."` → `exit 2` |
| `deferred` | `block "…was DEFERRED to the inbox (request_id=$rid)…"` → `exit 2` |
| `cancelled` | `block`, `exit 2` |
| `timed_out` | `block`, `exit 2` |
| anything else | `block`, `exit 2` |

`exit 0` hands control back to jcode, which runs the **original, unmodified** `bash` tool
input. The `--title` argument arrives at `gh` fully intact.

### Independent confirmation from the phinbox binary

`phinbox` cannot execute commands even in principle. Of 8365 strings in
`~/.local/bin/phinbox`:

| execution primitive | occurrences |
|---|---|
| `Command::new` | 0 |
| `process::Command` | 0 |
| `execvp` | 0 |
| `/bin/sh` | 0 |
| `/bin/zsh` | 0 |
| `sh -c` | 0 |

The only `spawn` targets are **renderers**: `spawn osascript:`, `spawn notify-send:`,
`spawn powershell:` — the native notification and dialog primitives for macOS/Linux/Windows.

### Corroborating runtime evidence

Answered item `hook-18f81afe6a321191ca3443da28e87818` carries the note
`"Superseded; dispatch fired via hook-083d21b8 replay"`, and its sibling
`hook-083d21b8975d46770ea88c253a68cf7e` contains the **clean, un-substituted** command
`gh workflow run publish-pypi.yml --ref main -f version=khostty-v0.1.0 -R KooshaPari/Khostty`.
`elicitate:127-150` implements deterministic replay:

```python
rid = "hook-" + hashlib.sha256((title + "\x00" + cmd).encode()).hexdigest()[:32]
...
def replay():
    # 24h replay window from record expiry
    if time.time() * 1000 > rec.get("expires_at_ms", 0):
        return None
```

A previously approved command is replayed from `answered/` rather than re-prompted. The
`--title QUOTED_DATA` text in the inbox is a **rendering artifact**, confirmed inert.

### Verdict on Concern A

**No bug. The five deferred `gh issue create` commands will file with their full titles
intact on approval.** No re-issue required. Retained here because the false alarm consumed
significant investigative effort and the reasoning is non-obvious from any single file.

---

## 3. Concern B — notification gap: confirmed

### The defect

Every queued item in `~/Library/Application Support/phinbox/inbox/` has:

```json
"notified_via": []
```

with `state: "pending"`, `urgency: "warning"`, and TTL ≥ 21h remaining. The write was
correctly blocked. The operator was never told anything was waiting.

### Root cause

The deferral path is:

```
bash tool call
  → jcode-tool-safety (pre-tool hook)
      → request_approval()                       # jcode-tool-safety:24-72
          → elicitate ask --from-json <spec>     # shim at ~/bin/elicitate
              → phinbox ask --from-file <tmp> --async
```

`elicitate`'s Python block (`~/bin/elicitate:155-190`):

1. Pops `details` and rebuilds a stripped `PromptSpec` with only
   `title`/`question`/`field`/`buttons`/`urgency`/`timeout_secs`.
2. Writes `/tmp/elicitate-sanitized-<rid>.json`.
3. Calls `phinbox ask --from-file <tmp> --async` — **async queue, no notification channel.**

`elicitate`'s own usage header (lines 19-25) documents that blocking popups require
`--block`, and the default is the defer flow. There is no equivalent flag to route a
deferred item to an OS notification.

**phinbox has the capability.** The binary contains `spawn osascript:`, `spawn notify-send:`,
and `spawn powershell:` — the native notification primitives. The caller never asks for them.

### Corroboration

The answered items the operator *did* act on all carry notes of the form
`"Operator approved via native OS popup in current Jcode session"` — the operator was already
in the session and saw the request incidentally. Nothing ever surfaced a passive item.

### Impact

The `AGENTS.md` deferral contract is only half-implemented. An agent can report "filed to
operator inbox" and the work looks complete, while in reality nothing happens until the
operator opens the inbox by chance or by hand.

### Proposed fix

Three options were drafted, with a shell-only change to `jcode-tool-safety` recommended as
the immediate fix (keeps approval and notification policy in one auditable file, no binary
rebuild), and a default-notify `--async` in phinbox recommended as the correct long-term home.

Full text is in `10a_ISSUE_inbox-notification-gap.md` in this directory.

---

## 4. Harness decollation verification (collateral)

Tracing the notification path required confirming which harness was actually running.
Findings, all verified:

| Check | Result |
|---|---|
| `jcode` launcher | zsh script at `~/.local/bin/jcode` |
| → `jcode.real` | symlink → `~/.jcode/builds/stable/jcode` |
| → resolved version | `0.91.0`, sha256 `45de5a60…` |
| `kcode` binary | separate Mach-O at `~/.kcode/builds/versions/bb6174b21a/kcode`, sha256 `b1973399…` |
| Same binary? | **No — different SHAs.** Decollation is clean. |
| `~/.jcode/sessions/*.json` | 3473 |
| `~/.kcode/sessions/*.json` | 57 |
| Live sessions present | `session_panda…`, `session_cricket…`, `session_stallion…` all present as `.json` |
| Providers (`~/.jcode`) | `default_provider = "opencode-go"`, `default_model = "mimo-v2.6-flash"`, `swarm_model` set |
| Providers (`~/.kcode`) | own `[provider]` block with `max_retries = 8` |
| `kcode menubar` process | PID 4490, PPID 1 — independent background process, benign |

**The "missing sessions" and "no providers" symptoms were both misperceptions:**
sessions exist with full hash suffixes (the short prefix shown in `ps` is a truncation), and
providers are configured in both homes.

---

## 5. Follow-ups

1. Fix the notification gap (shell-only change to `jcode-tool-safety`, or
   default-notify `--async` in phinbox). Acceptance criteria in the issue body.
2. Consider recording the actual channel in `notified_via` (e.g. `["macos_notification"]`)
   so future audits can distinguish "not notified" from "notified, ignored".
3. The lesson generalises: **when a harness substitutes or truncates a value for display,
   confirm whether the substitution is confined to the display path before treating it as an
   execution defect.** Two files (`jcode-shell-classifier.py` and `jcode-tool-safety`) had to
   be read together to see that the rewritten string never reaches the executor.
