#!/usr/bin/env python3
"""Cutover transaction for herdr 0.9.3 session.json — purge phantoms + gap-fill
agent_resume for all 13 known jcode panes, as ONE atomic file-level operation.

Designed to run with the herdr server STOPPED (guard d). The live server must
never be running when this touches a file: report-agent injection was abandoned
because the running server asynchronously batches/prunes entries (see
~/.jcode/scratch/gaps_filled.md and diag_report_agent.md, state.rs:568).

Usage:
  python3 cutover_session.py [--dry-run] [SESSION_JSON_PATH]   # default: dry-run
  python3 cutover_session.py --apply  [SESSION_JSON_PATH]      # atomic tmp + os.replace
  --force   # override the live-server refusal; ONLY for --dry-run against a COPY

Operations (allowlisted; anything else aborts without writing):
  1. PURGE:    workspaces[0](w7).tabs[0] slots "5","6","9" — remove phantom
               agent_session (labels PhenoShared/Agile/ForgeCode, values
               probe_devin/probe_letta/probe_hermes), label+value guarded.
  2. GAP-FILL: UPSERT agent_resume on the 13 target slots (ws0 "1".."12",
               ws1 "14") with the exact byte shape herdr itself writes:
                 {"source": "herdr:jcode", "agent": "jcode",
                  "argv": ["jcode", "--resume", "<sid>"]}

Guards:
  (a) input JSON must round-trip byte-identically (indent=2, ensure_ascii=False)
  (b) allowlist: agent_resume add/replace ONLY on the 13 target slots
      (label-checked against the plan inventory); agent_session removal ONLY on
      slots "5","6","9" with exact label + probe_* value checks
  (c) ANY byte difference outside those operations -> abort, no write
      (also: no probe_* value may survive anywhere, no agent_resume on any
      non-target slot may be added/removed/changed)
  (d) refuses to run while a live herdr server is detected (lsof on
      ~/.config/herdr/herdr.sock + 'herdr server' pid probe); --force overrides
  (e) prints before/after totals (agent_resume, agent_session) and per-slot changes
"""
from __future__ import annotations

import argparse
import copy
import difflib
import json
import os
import stat
import subprocess
import sys
import tempfile

DEFAULT_PATH = os.path.expanduser("~/.config/herdr/session.json")
SOCK_PATH = os.path.expanduser("~/.config/herdr/herdr.sock")

# --- Allowlist ---------------------------------------------------------------
# REVALIDATED against the live server 2026-10-04 10:2x (+02) immediately before
# apply. Two inventory drifts vs the 10-02/10-03 plan were found and absorbed:
#   (1) w7 slot 7 ("Cons", w7:p1B) NO LONGER EXISTS -- the pane was closed, so
#       its target is dropped (writing to a non-existent slot is impossible and
#       the allowlist must not name it).
#   (2) w8 slot 19 ("Cockpit", w8:pN) is a LIVE jcode pane holding sid
#       session_pawprint_... that the old plan did not know about; it is now a
#       target so it does not stay uncovered.
# Result: still 13 targets (11 in w7 + Ag Stat + Cockpit), same as EXPECTED.
# workspace index -> allowed slots for agent_resume add/replace
RESUME_SLOTS = {
    0: {"1", "2", "3", "4", "5", "6", "8", "9", "10", "11", "12"},
    1: {"14", "19"},
}
WS_IDS = {0: "w7", 1: "w8"}
# slot -> expected label (drift => refuse)
LABELS = {
    0: {"1": "Port", "2": "Fabric", "3": "Khostty", "4": "Mux",
        "5": "PhenoShared", "6": "Agile", "8": "Omni",
        "9": "ForgeCode", "10": "KCode", "11": "ShareCLI", "12": "Byte"},
    1: {"14": "Ag Stat", "19": "Cockpit"},
}
# (ws index, tab index, slot, pane_id, sid) — pane->slot->sid fully known.
# All 13 sids below were re-verified live via `ps eww` at 2026-10-04 10:21
# (+02) against the running processes, i.e. each sid here is the argv of a
# LIVE jcode process in that pane. w7:p1C -> slot 9 resolved from the plan
# inventory table (slot 9 = ForgeCode = w7:p1C, phantom agent_session
# probe_hermes). Panes 5/6/9 run live jcode while carrying that phantom ref.
# Slot 7 (Cons/w7:p1B) dropped: pane closed before cutover. w8:pN added as
# slot 19 "Cockpit" (sid session_pawprint_..., previously uncovered).
TARGETS = [
    (0, 0, "1",  "w7:p1A", "session_blossom_1789125438468_65a0cdb3f092c774"),
    (0, 0, "9",  "w7:p1C", "session_panda_1789273883949_ace2cf2f18024501"),
    (0, 0, "8",  "w7:p1D", "session_hamster_1789261418797_fd89d299577f2977"),
    (0, 0, "10", "w7:p1E", "session_stallion_1789257559166_3014fe0794334b42"),
    (0, 0, "12", "w7:p1F", "session_seedling_1789292027946_1d7ff1eda343917a"),
    (0, 0, "5",  "w7:p1G", "session_gorilla_1789551285195_f387609a70778409"),
    (0, 0, "6",  "w7:p1H", "session_palmtree_1789521601786_cdfc3359d9c75442"),
    (0, 0, "11", "w7:p1J", "session_cricket_1789211664742_b5a6db707b4cafb8"),
    (0, 0, "2",  "w7:p17", "session_daisy_1789125897339_6acaa21cf4346b86"),
    (0, 0, "4",  "w7:p18", "session_bat_1789625142652_30eeb82e18ad06cf"),
    (0, 0, "3",  "w7:p19", "session_calf_1789553284753_4beadc3f37d2b6dc"),
    (1, 0, "14", "w8:pD",  "session_snake_1789889071967_eadc10bc01a51627"),
    (1, 0, "19", "w8:pN",  "session_pawprint_1789589862380_3f143f0ff67ff234"),
]
EXPECTED_RESUME_TOTAL = 13

# ws index -> tab index -> slot -> (expected label, expected phantom value)
PHANTOM = {
    0: {0: {
        "5": ("PhenoShared", "probe_devin"),
        "6": ("Agile",       "probe_letta"),
        "9": ("ForgeCode",   "probe_hermes"),
    }}
}
PROBE_VALUES = ("probe_devin", "probe_letta", "probe_hermes")

KEEP_FIELDS = {"cwd", "label"}
# Reported sibling hooks that would belong to the phantom ref if ever present.
OPTIONAL_SIBLING_FIELDS = {
    "agent_state", "reported_source", "reported_seq",
    "authority_seq", "agent_status",
}
ALLOWED_PANE_FIELDS = KEEP_FIELDS | {"agent_session", "agent_resume"} | OPTIONAL_SIBLING_FIELDS

QA, QS = '"agent_resume"', '"agent_session"'


def fail(msg: str) -> None:
    print(f"REFUSED: {msg}", file=sys.stderr)
    sys.exit(2)


def serialize(data) -> str:
    # Verified byte-identical to herdr 0.9.3's session.json format
    # (indent=2, ensure_ascii=False, no trailing newline) on 2026-10-02;
    # re-verified by guard (a) at runtime on the actual input file.
    return json.dumps(data, indent=2, ensure_ascii=False)


def desired_resume(sid: str) -> dict:
    return {"source": "herdr:jcode", "agent": "jcode",
            "argv": ["jcode", "--resume", sid]}


def detect_live_server() -> list[str]:
    """Return human-readable reasons a live herdr server is detected."""
    reasons: list[str] = []
    if os.path.exists(SOCK_PATH):
        try:
            out = subprocess.run(["lsof", "--", SOCK_PATH],
                                 capture_output=True, text=True, timeout=15)
            lines = [l for l in out.stdout.splitlines() if l.strip()]
            if out.returncode == 0 and len(lines) > 1:
                holders = len(lines) - 1  # minus header
                reasons.append(f"lsof: {holders} process(es) hold {SOCK_PATH}")
            elif out.returncode == 1 and not lines:
                reasons.append(f"{SOCK_PATH} exists with no lsof holder "
                               f"(stale socket?)")
            elif out.returncode not in (0, 1):
                reasons.append(f"lsof rc={out.returncode} while {SOCK_PATH} "
                               f"exists — holders unknown")
        except FileNotFoundError:
            reasons.append(f"lsof unavailable and {SOCK_PATH} exists — "
                           f"cannot verify server is stopped")
        except subprocess.TimeoutExpired:
            reasons.append(f"lsof timed out while {SOCK_PATH} exists")
    # pid probe: ps scan for the server process (pgrep -f does NOT reliably
    # match multi-word argv on this host — verified 2026-10-03: pgrep -f
    # 'herdr server' returned no match for the live pid 1266).
    try:
        out = subprocess.run(["ps", "-axo", "pid=,command="],
                             capture_output=True, text=True, timeout=15)
        hits = [l.strip() for l in out.stdout.splitlines()
                if "herdr server" in l]
        if hits:
            pids = [h.split(None, 1)[0] for h in hits]
            reasons.append(f"'herdr server' pid(s) detected: "
                           f"{', '.join(pids)}")
    except (FileNotFoundError, subprocess.TimeoutExpired) as e:
        reasons.append(f"ps unavailable ({type(e).__name__}) — "
                       f"cannot verify no server pid")
    return reasons


def strip_allowed(d: dict) -> dict:
    """Remove every byte this script is ALLOWED to change, for guard (c).

    Whatever remains must serialize identically between input and result,
    which proves no other byte changed.
    """
    d = copy.deepcopy(d)
    workspaces = d["workspaces"]
    for wi, ti, slot, _pane, _sid in TARGETS:
        pane = workspaces[wi]["tabs"][ti]["panes"].get(slot)
        if isinstance(pane, dict):
            pane.pop("agent_resume", None)
    for wi, tabs_by in PHANTOM.items():
        for ti, slots in tabs_by.items():
            panes = workspaces[wi]["tabs"][ti]["panes"]
            for slot in slots:
                pane = panes.get(slot)
                if isinstance(pane, dict):
                    pane.pop("agent_session", None)
                    for fld in list(pane):
                        if fld in OPTIONAL_SIBLING_FIELDS:
                            pane.pop(fld)
    return d


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("path", nargs="?", default=DEFAULT_PATH)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true",
                      help="print diff + counts, write nothing (DEFAULT)")
    mode.add_argument("--apply", action="store_true",
                      help="atomically write the result (tmp + os.replace)")
    ap.add_argument("--force", action="store_true",
                    help="override the live-server refusal "
                         "(only for --dry-run against a COPY)")
    args = ap.parse_args()
    do_apply = args.apply  # default / --dry-run both mean: do not write

    path = os.path.abspath(os.path.expanduser(args.path))
    if not os.path.isfile(path):
        fail(f"session file not found: {path}")

    # Guard (d): refuse to run while the herdr server is live. Checked again
    # immediately before any write to narrow the TOCTOU window.
    reasons = detect_live_server()
    if reasons:
        if not args.force:
            fail("guard (d): live herdr server detected — this transaction "
                 "must run with the server STOPPED:\n  - "
                 + "\n  - ".join(reasons)
                 + "\n(stop the server, or use --force ONLY for --dry-run "
                 "against a COPY)")
            return 2  # unreachable; fail() exits
        print("WARNING: --force overriding live-server detection "
              "(dry-run only is safe):", file=sys.stderr)
        for r in reasons:
            print(f"  - {r}", file=sys.stderr)
        if do_apply:
            fail("guard (d): --force does NOT permit --apply while a live "
                 "server is detected; stop the server first")

    raw = open(path, encoding="utf-8").read()
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as e:
        fail(f"invalid JSON: {e}")

    # Guard (a): byte-preservation precondition.
    if serialize(data) != raw:
        fail("guard (a): JSON round-trip is not byte-identical; refusing to "
             "rewrite (non-target bytes would change).")

    before_resume, before_session = raw.count(QA), raw.count(QS)

    workspaces = data.get("workspaces")
    if not isinstance(workspaces, list):
        fail("missing top-level 'workspaces' list")
    for wi, want in WS_IDS.items():
        if wi >= len(workspaces):
            fail(f"workspace index {wi} out of range")
        got = workspaces[wi].get("id")
        if got != want:
            fail(f"guard (b): workspaces[{wi}].id = {got!r} != {want!r} "
                 f"— nesting does not match the plan inventory")

    original = copy.deepcopy(data)
    changes: list[str] = []
    n_added = n_replaced = n_unchanged = n_removed = 0

    # --- Operation 1: PURGE phantom agent_session on slots 5/6/9 ------------
    for wi, tabs_by in PHANTOM.items():
        for ti, slots in tabs_by.items():
            tabs = workspaces[wi].get("tabs", [])
            if ti >= len(tabs):
                fail(f"tab index {ti} out of range in workspace {wi}")
            panes = tabs[ti].get("panes", {})
            for slot, (want_label, want_value) in slots.items():
                base = f"workspaces[{wi}].tabs[{ti}].panes[{slot}]"
                if slot not in panes:
                    changes.append(f"  purge-skip {base} ({want_label}): "
                                   f"slot absent (nothing to do)")
                    continue
                pane = panes[slot]
                unexpected = set(pane) - ALLOWED_PANE_FIELDS
                if unexpected:
                    fail(f"guard (b): {base} has fields outside the allowlist: "
                         f"{sorted(unexpected)}")
                if pane.get("label") != want_label:
                    fail(f"guard (b): {base} label mismatch: "
                         f"{pane.get('label')!r} != {want_label!r}")
                sess = pane.get("agent_session")
                if sess is None:
                    changes.append(f"  purge-skip {base} ({want_label}): "
                                   f"no agent_session (nothing to do)")
                    continue
                if (not isinstance(sess, dict)
                        or sess.get("value") != want_value):
                    fail(f"guard (b): {base} agent_session value mismatch: "
                         f"{json.dumps(sess)} != {want_value!r}")
                changes.append(f"  REMOVE   {base}.agent_session "
                               f"({want_label}): {want_value}")
                del pane["agent_session"]
                n_removed += 1
                for fld in list(pane):  # phantom sibling hooks, if ever present
                    if fld in OPTIONAL_SIBLING_FIELDS:
                        changes.append(f"  REMOVE   {base}.{fld} "
                                       f"(phantom sibling hook)")
                        del pane[fld]

    # --- Operation 2: GAP-FILL agent_resume for all 13 panes ----------------
    for wi, ti, slot, pane_id, sid in TARGETS:
        if slot not in RESUME_SLOTS.get(wi, set()):
            fail(f"guard (b): target slot {slot!r} not in the agent_resume "
                 f"allowlist for workspace {wi}")
        tabs = workspaces[wi].get("tabs", [])
        if ti >= len(tabs):
            fail(f"tab index {ti} out of range in workspace {wi}")
        panes = tabs[ti].get("panes", {})
        if slot not in panes:
            fail(f"target slot absent from the file: "
                 f"workspaces[{wi}].tabs[{ti}].panes[{slot}] ({pane_id}) — "
                 f"cannot gap-fill; layout drift?")
        pane = panes[slot]
        unexpected = set(pane) - ALLOWED_PANE_FIELDS
        if unexpected:
            fail(f"guard (b): workspaces[{wi}].tabs[{ti}].panes[{slot}] has "
                 f"fields outside the allowlist: {sorted(unexpected)}")
        want_label = LABELS[wi][slot]
        if pane.get("label") != want_label:
            fail(f"guard (b): label mismatch at "
                 f"workspaces[{wi}].tabs[{ti}].panes[{slot}]: "
                 f"{pane.get('label')!r} != {want_label!r} — layout drift, "
                 f"refusing to write {pane_id}")
        desired = desired_resume(sid)
        have = pane.get("agent_resume")
        where = f"workspaces[{wi}].tabs[{ti}].panes[{slot}]"
        if have is None:
            pane["agent_resume"] = desired
            n_added += 1
            changes.append(f"  ADD      {where} {want_label} ({pane_id}) "
                           f"<- {sid}")
        elif have == desired:
            n_unchanged += 1
            changes.append(f"  KEEP     {where} {want_label} ({pane_id}) "
                           f"= {sid}")
        else:
            pane["agent_resume"] = desired
            n_replaced += 1
            changes.append(f"  REPLACE  {where} {want_label} ({pane_id}): "
                           f"{json.dumps(have, sort_keys=True)} -> {sid}")

    new_raw = serialize(data)

    # Guard (c): strip everything this script may touch from BOTH documents;
    # the remainder must serialize identically => no other byte changed.
    if serialize(strip_allowed(original)) != serialize(strip_allowed(data)):
        fail("guard (c): result differs outside the allowlisted operations; "
             "aborting without writing")

    # Guard (e): totals.
    after_resume, after_session = new_raw.count(QA), new_raw.count(QS)
    if after_session != before_session - n_removed:
        fail(f"guard (e): agent_session count mismatch "
             f"{before_session} -> {after_session} "
             f"(removals recorded: {n_removed})")
    if after_resume != before_resume + n_added:
        fail(f"guard (e): agent_resume count mismatch "
             f"{before_resume} -> {after_resume} "
             f"(adds recorded: {n_added}, replaces: {n_replaced})")
    if after_resume != EXPECTED_RESUME_TOTAL:
        fail(f"guard (e): agent_resume total would be {after_resume}, "
             f"expected exactly {EXPECTED_RESUME_TOTAL} — refusing "
             f"(stray entry on a non-target slot, or a target missing?)")
    for probe in PROBE_VALUES:
        if probe in new_raw:
            fail(f"guard (c): phantom value {probe!r} still present somewhere "
                 f"outside the allowlisted slots")
    # Belt-and-braces: every target slot must now hold exactly the desired entry.
    for wi, ti, slot, pane_id, sid in TARGETS:
        got = workspaces[wi]["tabs"][ti]["panes"][slot].get("agent_resume")
        if got != desired_resume(sid):
            fail(f"post-condition failed for {pane_id} (slot {slot}): "
                 f"{json.dumps(got)}")

    # --- Report -------------------------------------------------------------
    print(f"=== cutover_session.py — {'APPLY' if do_apply else 'DRY-RUN'} ===")
    print(f"target: {path}")
    if reasons:
        print(f"server guard: OVERRIDDEN via --force ({len(reasons)} reason(s))")
    else:
        print("server guard: OK (no live herdr server detected)")
    print()
    print("--- per-slot changes ---")
    for line in changes:
        print(line)
    print()
    print("--- totals ---")
    print(f"agent_resume:  {before_resume} -> {after_resume}   "
          f"(+{n_added} added, {n_replaced} replaced, "
          f"{n_unchanged} unchanged)")
    print(f"agent_session: {before_session} -> {after_session}   "
          f"(-{n_removed} removed)")
    print(f"bytes:         {len(raw)} -> {len(new_raw)} "
          f"({len(new_raw) - len(raw):+d})")
    print()
    print("--- unified diff (before -> after) ---")
    diff = list(difflib.unified_diff(
        raw.splitlines(keepends=True), new_raw.splitlines(keepends=True),
        fromfile="session.json (before)", tofile="session.json (after)"))
    if diff:
        sys.stdout.writelines(diff)
        if not diff[-1].endswith("\n"):
            sys.stdout.write("\n")
    else:
        print("(no byte differences)")

    if not do_apply:
        print()
        print("DRY-RUN: nothing written.")
        return 0

    if new_raw == raw:
        print()
        print("No changes: file already in target state; nothing written.")
        return 0

    # Guard (d) re-check immediately before writing (TOCTOU narrowing).
    reasons_now = detect_live_server()
    if reasons_now:
        fail("guard (d): live herdr server appeared before the write — "
             "aborting without writing:\n  - " + "\n  - ".join(reasons_now))

    dname = os.path.dirname(path) or "."
    mode_bits = stat.S_IMODE(os.stat(path).st_mode)
    fd, tmp = tempfile.mkstemp(prefix=".session.json.", suffix=".tmp", dir=dname)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(new_raw)
            f.flush()
            os.fsync(f.fileno())
        os.chmod(tmp, mode_bits)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)
    print()
    print(f"WROTE {path} atomically (os.replace).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
