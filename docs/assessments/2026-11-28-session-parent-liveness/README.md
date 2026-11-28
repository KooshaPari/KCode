# Session parent liveness: store gap assessment (issue #395)

Scope: documentation only. No Rust source or tests were changed. All line citations are to
worktree HEAD `55897fdba` (`git rev-parse --short HEAD`, run 2026-11-28). Paths in brackets are
relative to this file and were checked with `test -f`.

## 1. What the store records today

The persisted record is `Session`, defined at
[crates/jcode-base/src/session.rs:106](../../../crates/jcode-base/src/session.rs#L106) through line 220. The struct
is `#[derive(Debug, Clone, Serialize, Deserialize)]` (line 105). Its fields, in declaration order:

| Lines | Field | Rust type | Serde notes |
|---|---|---|---|
| 107 | `id` | `String` | always serialized |
| 108 | `parent_id` | `Option<String>` | always serialized; `None` when no parent |
| 109 | `title` | `Option<String>` | always serialized |
| 110-111 | `custom_title` | `Option<String>` | `default`, skipped when `None` |
| 112 | `created_at` | `DateTime<Utc>` | always serialized |
| 113 | `updated_at` | `DateTime<Utc>` | always serialized |
| 114 | `messages` | `Vec<StoredMessage>` | always serialized |
| 115-117 | `system_prompt` | `Option<String>` | `default`, skipped when `None` |
| 118-120 | `model_usage_turn_id` | `Option<String>` | `default`, skipped when `None` |
| 121-124 | `compaction` | `Option<StoredCompactionState>` | `default`, skipped when `None` |
| 125-127 | `provider_session_id` | `Option<String>` | `default`, skipped when `None` |
| 128-131 | `provider_key` | `Option<String>` | `default`, skipped when `None` |
| 132-134 | `model` | `Option<String>` | `default`, skipped when `None` |
| 135-138 | `route_api_method` | `Option<String>` | `default`, skipped when `None` |
| 139-141 | `reasoning_effort` | `Option<String>` | `default`, skipped when `None` |
| 142-144 | `subagent_model` | `Option<String>` | `default`, skipped when `None` |
| 145-147 | `improve_mode` | `Option<SessionImproveMode>` | `default`, skipped when `None` |
| 148-150 | `autoreview_enabled` | `Option<bool>` | `default`, skipped when `None` |
| 151-153 | `autojudge_enabled` | `Option<bool>` | `default`, skipped when `None` |
| 154-156 | `is_canary` | `bool` | `default` |
| 157-159 | `testing_build` | `Option<String>` | `default`, skipped when `None` |
| 160-162 | `working_dir` | `Option<String>` | `default`, skipped when `None` |
| 163-165 | `short_name` | `Option<String>` | `default`, skipped when `None` |
| 166-168 | `status` | `SessionStatus` | `default` |
| 169-171 | `last_pid` | `Option<u32>` | `default`, skipped when `None` |
| 172-174 | `last_active_at` | `Option<DateTime<Utc>>` | `default`, skipped when `None` |
| 175-177 | `is_debug` | `bool` | `default` |
| 178-180 | `saved` | `bool` | `default` |
| 181-183 | `save_label` | `Option<String>` | `default`, skipped when `None` |
| 184-186 | `env_snapshots` | `Vec<EnvSnapshot>` | `default`, skipped when empty |
| 187-189 | `memory_injections` | `Vec<StoredMemoryInjection>` | `default`, skipped when empty |
| 190-192 | `replay_events` | `Vec<StoredReplayEvent>` | `default`, skipped when empty |
| 193-196 | `migration_epoch` | `u64` | `default`, skipped when zero |
| 197-199 | `fork_depth` | `u8` | `default` |
| 200-202 | `fork_max_depth` | `u8` | `default = "default_fork_max_depth"` |
| 203-205 | `fork_chain` | `Vec<String>` | `default`, skipped when empty |
| 206-219 | `persist_state`, provider/memory caches | private fields | `#[serde(skip)]`, not on disk |

`SessionStatus` has variants `Active`, `Closed`, `Crashed { message }`, `Reloaded`, `Compacted`,
`RateLimited`, `Error { message }` at
[crates/jcode-session-types/src/lib.rs:160](../../../crates/jcode-session-types/src/lib.rs#L160)-173. A child's
parent link is a bare `Option<String>`; there is no parent-liveness field and no orphan index in
this struct.

The journal metadata mirrors the same gap: `SessionJournalMeta` carries `parent_id`, `status`,
`last_pid`, and `last_active_at` but no liveness-at-last-write field
([crates/jcode-base/src/session/journal.rs:10](../../../crates/jcode-base/src/session/journal.rs#L10)-40).

On-disk shape: `session_path()` resolves `<jcode_dir>/sessions/<session_id>.json`
([crates/jcode-base/src/session/storage_paths.rs:25](../../../crates/jcode-base/src/session/storage_paths.rs#L25)-28,
with `session_path_in_dir` at lines 7-9). Full snapshots are written from `Session` itself via
`storage::write_json_fast(snapshot_path, self)` at
[crates/jcode-base/src/session/persistence.rs:327](../../../crates/jcode-base/src/session/persistence.rs#L327) and read back
as `Session` via `storage::read_json(path)` at line 363. The thin read wrapper lives at
[crates/jcode-base/src/storage.rs:9](../../../crates/jcode-base/src/storage.rs#L9)-22.

Local observation (measured by me, 2026-11-28, not by issue #395): I sampled 500 JSON files under
`~/.jcode/sessions/` with a read-only Python `json.load` key/type census. All 500 had `id`,
`parent_id`, `title`, `created_at`, `updated_at`, `messages`, `model`, `is_canary`, `working_dir`,
`short_name`, `status`, `last_pid`, `last_active_at`, `is_debug`, and `saved`. `parent_id` was a
string in 364 files and null in 136; `status` was a string in 462 and a JSON object in 38; a
follow-up type census showed the object form is `{"Crashed": ...}` in all 38 cases; `last_pid` was
an integer in all 500. The census command was a Python heredoc using
`glob.glob(os.path.expanduser('~/.jcode/sessions/*.json'))`, `json.load`, and
`collections.Counter` over keys and Python value types; it wrote no files. Optional fields absent
from this sample are not evidence they cannot occur; they are declared in the struct above.

Liveness is tracked out of band: `active_pids/<session_id>` files are written by
`register_active_pid()` and deleted by `unregister_active_pid()` at
[crates/jcode-app-core/src/session_active_pids.rs:7](../../../crates/jcode-app-core/src/session_active_pids.rs#L7)-18, and
crash detection probes those pids separately
([crates/jcode-base/src/session/crash.rs:345](../../../crates/jcode-base/src/session/crash.rs#L345)-361). The parent link
is written when a child is created, for example `Session::create(Some(parent_session_id.to_string()), None)`
at [crates/jcode-app-core/src/server/client_actions.rs:905](../../../crates/jcode-app-core/src/server/client_actions.rs#L905) and
line 940. Resume handling is at
[crates/jcode-app-core/src/server/client_session.rs:1196](../../../crates/jcode-app-core/src/server/client_session.rs#L1196);
this assessment does not claim to have audited that handler's full safety logic.

## 2. The three unanswerable operator questions

Given only the store, with a parent whose process has exited and children still writing:

1. **Is the parent live?** `last_pid` names a process that may be dead, `status` may remain
   `Active`, and the live-pid registry is a separate file tree
   ([crates/jcode-app-core/src/session_active_pids.rs:3](../../../crates/jcode-app-core/src/session_active_pids.rs#L3)-18). No single
   field states parent liveness at the time the record was last written.
2. **Did children outlive the parent?** A child's `parent_id` only names the parent; it says
   nothing about parent liveness at the child's last write
   ([crates/jcode-base/src/session.rs:108](../../../crates/jcode-base/src/session.rs#L108),
   [crates/jcode-base/src/session/journal.rs:11](../../../crates/jcode-base/src/session/journal.rs#L11)). There is no
   `orphaned_children` index in the struct. An answer requires joining against an out-of-band pid
   probe.
3. **Is resuming the parent safe?** Even if a parent row is present and `status` says `Active`,
   the store cannot say whether live children are still writing under that parent. The issue
   (#395, "Measured evidence") states this consequence directly. I did not run a resume test, so
   no claim is made here about what the current resume handler does beyond the file citation above.

## 3. Evidence

All figures in this section are **quoted from issue #395**, written by the issue author (KooshaPari).
Reading date: 2026-11-28. Measurement date reported in the issue: 2026-11-28T02:52Z, host nixos.
I did **not** re-run the issue's audit commands and did not find those TSV artifacts in this repo;
they are external audit outputs, not repository files.

Issue #395 reports:

- `open-nixos.tsv`: 35 lines, `alive=0` 0, `alive=1` 35.
- `open-mac.tsv`: 630 lines, `alive=0` 537, `alive=1` 93.
- `open-desk.tsv`: 1152 lines, `alive=0` 1074, `alive=1` 78.
- `ledger.tsv` (7648 data rows, 16 columns, `parent` at column 13):
  - 5028 rows have a non-empty `parent`.
  - 46 of those point at a parent id absent from the store entirely; the issue states all 46
    children are themselves `open=FALSE`.
  - 3988 of those name a parent present in the store and still marked `open=TRUE`.
  - 542 rows have `agent_spawned=TRUE`.

The issue's reading: mixed `alive=0`/`alive=1` rows at one instant show dead recorded pids with
siblings still live; the 46 dangling references and 3988 stale-but-present references both defeat
a bare `parent` id as an answer to "is this parent alive". I am restating the author's measurement
and interpretation, not independently verifying it.

My only independent measurement is the 500-file JSON key/type census in section 1. It did not
count parents, orphan links, or pid liveness, and must not be read as corroboration of the issue's
5028/46/3988 figures.

## 4. Proposed minimal additive schema change (for a future PR, not implemented here)

Goal: make "parent live at last write" and "which children were orphaned" derivable from the
store itself, without breaking older readers.

- Add one optional field to `Session` (and to `SessionJournalMeta`) such as
  `parent_alive_at_last_write: Option<bool>`, annotated `#[serde(default,
  skip_serializing_if = "Option::is_none")]` following the existing optional-field convention
  ([crates/jcode-base/src/session.rs:110](../../../crates/jcode-base/src/session.rs#L110)-174,
  [crates/jcode-base/src/session/journal.rs:13](../../../crates/jcode-base/src/session/journal.rs#L13)-39). For a child,
  this records the parent's observed liveness when the child writes. For a parent, it records the
  parent's own liveness when it writes. `None` means "unknown / written before this field
  existed", never "alive".
- Treat `last_pid` as a pointer, not identity, as the field comment already says it is for crash
  detection ([crates/jcode-base/src/session.rs:169](../../../crates/jcode-base/src/session.rs#L169)-171). Do not reuse
  it as the liveness flag; it is absent for some records and reused across processes.
- Derive orphaned children by a read-time rule rather than a second persisted index, to keep the
  change to one PR: a child is *orphan-candidate* when `parent_id` is `Some`, and either
  `parent_alive_at_last_write == Some(false)`, or the referenced parent id is absent from the
  store, or the parent's own liveness observation is older than its last write. `None` liveness
  stays UNKNOWN, not safe and not orphaned. This is derived from fields already present plus the
  new field; a later PR could add a maintained index if a scan proves too slow.
- Migration/compat rule: additive only. No renaming, no removal, no `deny_unknown_fields`. The
  struct already uses per-field `serde(default)` and `skip_serializing_if` for optional fields,
  and `SessionStartupStub` uses defaults for every optional field it reads
  ([crates/jcode-base/src/session.rs:223](../../../crates/jcode-base/src/session.rs#L223)-277), so older JSON records
  should continue to deserialize. An older reader encountering the new key should likewise keep
  working because serde ignores unknown fields by default; the crash-header comment at
  [crates/jcode-base/src/session/crash.rs:313](../../../crates/jcode-base/src/session/crash.rs#L313) records that
  default explicitly. No migration command or data rewrite is proposed.
- Resume-safety rule an operator or agent would follow: before resuming a parent, read its own
  liveness observation and enumerate children whose `parent_id` equals it. If any child has
  `parent_alive_at_last_write == Some(false)` while the child itself is active, treat resume as
  unsafe without explicit operator acknowledgement; if the field is `None` or the child's state is
  unknown, report UNKNOWN rather than safe. This is a policy proposal only; the current resume
  handler was not modified and was not fully audited.

## 5. NON-goals and open questions

Non-goals for this assessment and for the proposed single PR:

- No Rust source, Rust test, workflow, or runtime behavior change.
- No implementation of the field, writer changes, derived index, or resume gate.
- No re-measurement of the issue's audit TSVs; no new telemetry or pid probe design.
- No claim that `status: Active` is itself proof of process liveness, and no full audit of the
  resume handler at `client_session.rs:1196`.
- No cleanup or mutation of existing session files.

Open questions:

- **UNKNOWN:** exact semantics intended for `parent_alive_at_last_write` on a parent record versus
  a child record; issue #395 proposes the field but does not specify the writer algorithm.
- **UNKNOWN:** which writer should refresh the cached liveness on children, and whether a parent
  write can safely update child records. Issue #395 proposes a refresh; this repo's write paths
  were inspected only at `persistence.rs:327` and the child-creation sites cited above.
- **UNKNOWN:** whether an `orphaned_children` index should be persisted rather than derived. Issue
  #395 asks for a maintained index; this document proposes a derived candidate relation to keep
  the change to one PR. That is an assessment choice, not a verified implementation constraint.
- **UNKNOWN:** whether the 46 dangling `parent` ids are typos, deleted records, or records from a
  different store; issue #395 lists examples but does not establish the cause.
- **UNKNOWN:** full behavior of `handle_resume_session` with respect to active children. I read its
  location only and did not audit its body; no conclusion is claimed.
- **UNKNOWN:** whether the new field must be mirrored into any snapshot/journal compatibility path
  beyond the two structs cited. I found `SessionStartupStub` and `SessionHeader` as additional
  partial readers but did not trace every compatibility path.
