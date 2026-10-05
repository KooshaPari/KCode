#!/usr/bin/env python3
"""Reconcile herdr session.json so every live jcode pane has agent_resume.

WHY THIS REPLACES THE HARDCODED cutover_session.py
--------------------------------------------------
The earlier script hardcoded (ws_index, tab, slot, pane_id, sid) tuples. Live
evidence showed that map going stale within hours: on 2026-10-04 the same
logical pane moved slot 8 -> 7 (Omni) and 9 -> 8 (ForgeCode) between two
reads ten minutes apart, one pane was closed, another was created, and w8:17
kept an `agent_resume` for a sid whose process no longer existed. A hardcoded
map would have written stale sids into the wrong panes -- i.e. it would have
created the very resume storm this project exists to prevent.

So nothing here is hardcoded except the two things that are genuinely stable:
  * the on-disk shape herdr itself writes (keys + JSON formatting), and
  * the rule for what counts as a phantom (an agent_session whose value is one
      of the injected probe_* markers).

Everything else -- which workspaces/tabs/slots exist, which pane owns which
sid, which entries are stale -- is DISCOVERED at run time from
  1. the session file itself,
  2. `herdr pane list`  (authoritative pane_id <-> label mapping), and
  3. `ps eww`          (authoritative pane_id -> live sid mapping).

DESIGN RULES
------------
1. Never invent a sid. A pane gets agent_resume ONLY if a live process proves
   it. Unprovable panes are reported, never fabricated.
2. Never touch agent_session on a pane whose agent is real (codex etc.).
3. Only jcode panes are considered; a jcode pane carrying a probe_* session is
   a phantom (those probes were injected into jcode panes and never used).
4. All writes are atomic (tmp + os.replace) and byte-shape preserving.
5. Refuses to run while a herdr server is live (it would overwrite us).
   --force exists ONLY for --dry-run against a copy.

Exit codes: 0 ok, 1 error/abort, 2 refused (live server).
"""

import argparse
import copy
import json
import os
import re
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from typing import Any

DEFAULT_PATH = os.path.expanduser("~/.config/herdr/session.json")
SOCK_PATH = os.path.expanduser("~/.config/herdr/herdr.sock")
HERDR_BIN = os.path.expanduser("~/.local/bin/herdr")

# Only hardcoded knowledge: the marker values the phantom probes used, and the
# shape of an agent_resume record. Slots, labels, pane_ids and sids are NEVER
# hardcoded -- they are discovered.
PROBE_VALUES = frozenset({"probe_devin", "probe_letta", "probe_hermes"})
RESUME_SOURCE = "herdr:jcode"
RESUME_AGENT = "jcode"
RESUME_ARGV0 = "jcode"
SID_RE = re.compile(r"session_[a-z0-9_]+_\d+_[0-9a-f]{16}")
PANE_ID_RE = re.compile(r"HERDR_PANE_ID=(\S+)")
LABEL_MAX = 64


def log(msg=""):
    print(msg)


def fail(msg):
    print(f"ERROR: {msg}", file=sys.stderr)
    raise SystemExit(1)


# --------------------------------------------------------------------------
# discovery: what is actually running right now
# --------------------------------------------------------------------------
def detect_live_server():
    """Return reasons a herdr server appears live (empty list == safe)."""
    reasons = []
    if os.path.exists(SOCK_PATH):
        try:
            out = subprocess.run(
                ["lsof", "--", SOCK_PATH], capture_output=True, text=True,
                timeout=10,
            ).stdout.strip()
            if out:
                reasons.append(f"lsof {SOCK_PATH}:\n{out}")
        except subprocess.TimeoutExpired:
            reasons.append("lsof timed out - cannot prove no server")
    try:
        out = subprocess.run(
            ["ps", "-axo", "pid=,command="], capture_output=True, text=True,
            timeout=10,
        ).stdout
        for line in out.splitlines():
            if "herdr server" in line and "grep" not in line:
                reasons.append(f"ps: {line.strip()}")
    except subprocess.TimeoutExpired:
        reasons.append("ps timed out - cannot verify no server pid")
    return reasons


def _sids_from_ps():
    """Fallback pane_id -> sid, scraped from the process table.

    Less authoritative than `herdr pane process-info` because it depends on
    HERDR_PANE_ID being exported into each pane's environment, but it works
    when the CLI is unavailable and covers panes the CLI does not list.
    """
    out = subprocess.run(
        ["ps", "eww", "-axo", "pid=,command="], capture_output=True, text=True,
    ).stdout
    candidates = {}
    for line in out.splitlines():
        pane = PANE_ID_RE.search(line)
        sid = SID_RE.search(line)
        if not (pane and sid):
            continue
        if "--resume" not in line and "jcode.real" not in line:
            continue
        candidates.setdefault(pane.group(1), set()).add(sid.group(0))
    return {p: next(iter(s)) for p, s in candidates.items() if len(s) == 1}


def _sids_from_process_info(pane_ids):
    """pane_id -> sid, read from herdr's own per-pane process table.

    This is the authoritative source: herdr reports the foreground argv of each
    pane directly, so it works for unlabeled panes and does not depend on
    environment inheritance. Panes with 0 or >1 distinct sid are omitted --
    the caller must never guess which session a pane should resume.

    The calls are issued concurrently: each one costs a full CLI round-trip
    (~1-8s), so a serial loop over a dozen panes made the whole reconciler
    unusably slow (observed: >120s, killed by timeout before it ever planned).
    """
    def query(pid):
        try:
            out = subprocess.run(
                [HERDR_BIN, "pane", "process-info", "--pane", pid],
                capture_output=True, text=True, timeout=20,
            ).stdout
            procs = json.loads(out)["result"]["process_info"]["foreground_processes"]
        except Exception:  # noqa: BLE001 - a pane we cannot read proves nothing
            return pid, set()
        sids = set()
        for proc in procs:
            argv = proc.get("argv") or []
            if "--resume" not in argv:
                continue
            for token in argv:
                if SID_RE.fullmatch(token):
                    sids.add(token)
        return pid, sids

    found = {}
    if not pane_ids:
        return found
    with ThreadPoolExecutor(max_workers=min(8, len(pane_ids))) as pool:
        for pid, sids in pool.map(query, pane_ids):
            if len(sids) == 1:
                found[pid] = next(iter(sids))
    return found


def live_sids_by_pane():
    """pane_id -> the single jcode sid currently running in that pane.

    Panes with zero or >1 candidate sids are intentionally omitted: the caller
    must not guess which session a pane should resume.
    """
    pane_ids = [p["pane_id"] for p in pane_table()]
    sids = _sids_from_process_info(pane_ids) if pane_ids else {}
    for pid, sid in _sids_from_ps().items():
        sids.setdefault(pid, sid)
    return sids


def pane_table():
    """Return the running server's pane records (authoritative, no filtering).

    Labels are NOT required: many panes (the w8 codex/jcode ones) have no
    label at all, and dropping them would make those panes uncorrelatable --
    which is exactly how an orphaned resume entry goes unnoticed. Every field
    herdr exposes is kept so callers can correlate by cwd when label is absent.
    """
    if not (os.path.isfile(HERDR_BIN) or shutil_which("herdr")):
        return []
    try:
        out = subprocess.run(
            [HERDR_BIN, "pane", "list"], capture_output=True, text=True,
            timeout=15,
        ).stdout
        return json.loads(out)["result"]["panes"]
    except Exception as exc:  # noqa: BLE001 - any failure means "unknown"
        log(f"WARNING: herdr pane list unavailable ({type(exc).__name__}: {exc})")
        return []


def pane_correlation(doc):
    """Build slot -> pane_id maps that survive renames, closes and creations.

    Three keys are tried, strongest evidence first. The session file stores only
    `cwd`/`label` for unlabeled panes -- there is NO slot->pane_id field at all --
    so correlation must be inferred from the live server, and each rule below
    only fires when the evidence is unambiguous:

      1. label       -> unique live pane carrying that label.
      2. cwd         -> unique live-sid pane in that cwd.
      3. unique-sid  -> a pane is the ONLY live holder of exactly the sid this
                        slot already stores. This is what recovers an orphaned
                        entry (e.g. w8 slot 17 storing session_seedling while
                        that session actually runs in pane w8:pP). It is safe
                        because it requires a 1:1 match on the sid itself; if
                        two panes held the same sid, or the pane already has a
                        resume entry claiming it, the slot is left alone.

    Anything that fails all three rules is reported, never guessed.
    """
    panes = pane_table()
    by_label, by_cwd = {}, {}
    for p in panes:
        if p.get("label"):
            by_label.setdefault(p["label"], []).append(p["pane_id"])
        cwd = p.get("foreground_cwd") or p.get("cwd")
        if cwd:
            by_cwd.setdefault(cwd, []).append(p["pane_id"])

    live = live_sids_by_pane()
    sid_to_panes = {}
    for pid, sid in live.items():
        sid_to_panes.setdefault(sid, []).append(pid)

    label_map, cwd_map, sid_map, ambiguous = {}, {}, {}, []
    for wi, _ti, slot, _wsid, pane in iter_panes(doc):
        label = pane.get("label")
        cwd = pane.get("cwd")
        if label and len(by_label.get(label, [])) == 1:
            label_map[(wi, slot)] = by_label[label][0]
            continue
        cands = [pid for pid in by_cwd.get(cwd, []) if pid in live]
        if len(cands) == 1:
            cwd_map[(wi, slot)] = cands[0]
            continue
        # rule 3: the sid this slot stores is live in exactly one pane
        resume = pane.get("agent_resume")
        stored = None
        if isinstance(resume, dict):
            argv = resume.get("argv") or []
            stored = argv[-1] if argv else None
        holders = sid_to_panes.get(stored, []) if stored else []
        if stored and len(holders) == 1:
            sid_map[(wi, slot)] = holders[0]
            continue
        ambiguous.append((wi, slot, label, cwd, len(cands), stored))
    return label_map, cwd_map, sid_map, ambiguous, live


def label_to_pane_id():
    """label -> pane_id, from the running server (authoritative)."""
    panes = pane_table()
    return {p["label"]: p["pane_id"] for p in panes if p.get("label")}


def shutil_which(name: str):
    for d in os.environ.get("PATH", "").split(os.pathsep):
        cand = os.path.join(d, name)
        if os.path.isfile(cand) and os.access(cand, os.X_OK):
            return cand
    return None


# --------------------------------------------------------------------------
# planning
# --------------------------------------------------------------------------
def iter_panes(doc):
    """Yield (ws_index, tab_index, slot, workspace_id, pane_dict)."""
    for wi, ws in enumerate(doc.get("workspaces") or []):
        for ti, tab in enumerate(ws.get("tabs") or []):
            for slot, pane in (tab.get("panes") or {}).items():
                yield wi, ti, slot, ws.get("id"), pane


def build_plan(doc):
    """Discover every action needed. Pure function of the discovered state."""
    label_map, cwd_map, sid_map, ambiguous, live = pane_correlation(doc)
    by_label = dict(label_map)
    for key, pid in cwd_map.items():
        by_label.setdefault(key, pid)
    for key, pid in sid_map.items():
        by_label.setdefault(key, pid)
    log(f"discovered: {len(live)} panes with exactly one live jcode sid; "
        f"correlated {len(label_map)} slots by label, {len(cwd_map)} by cwd, "
        f"{len(sid_map)} by unique sid")
    for wi, slot, label, cwd, n, stored in ambiguous:
        log(f"  AMBIGUOUS ws{wi} slot {slot} ({label or 'unlabeled'}, "
            f"{n} candidate panes for cwd {cwd}"
            + (f", stored sid {stored} not uniquely live" if stored else "")
            + ") - not correlated")

    add = []      # panes needing agent_resume
    replace = []  # panes whose stored sid is wrong/dead
    keep = []     # panes already correct
    purge = []    # phantom probe_* agent_session to drop
    unverifiable = []  # jcode panes with no provable sid

    for wi, ti, slot, wsid, pane in iter_panes(doc):
        resume = pane.get("agent_resume")
        asess = pane.get("agent_session")
        sid_stored = None
        if isinstance(resume, dict):
            argv = resume.get("argv") or []
            sid_stored = argv[-1] if argv else None

        # 1. phantom purge: a probe_* agent_session on any pane
        if isinstance(asess, dict) and asess.get("value") in PROBE_VALUES:
            purge.append({
                "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                "label": pane.get("label"), "value": asess.get("value"),
            })
            # A pane can carry BOTH a phantom agent_session and a live jcode
            # process. Purging with `continue` alone would discard that pane's
            # resume guarantee in the same transaction -- the exact coverage
            # loss that caused the original cutover to regress. Resolve the
            # resume independently: keep a proven sid, replace a wrong one,
            # and only report unverifiable when nothing proves it.
            pid_p = by_label.get((wi, slot))
            live_p = live.get(pid_p) if pid_p else None
            if live_p and live_p == sid_stored:
                keep.append({
                    "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                    "label": pane.get("label"), "sid": sid_stored,
                })
            elif live_p and live_p != sid_stored:
                replace.append({
                    "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                    "label": pane.get("label"), "old": sid_stored, "new": live_p,
                })
            elif sid_stored and pid_p is None:
                unverifiable.append({
                    "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                    "label": pane.get("label"), "sid": sid_stored,
                    "why": "phantom agent_session purged; no live pane proves this sid",
                })
            # else: no resume row and no proof -> nothing to assert, stay silent
            continue

        # 2. already-correct resume rows
        if isinstance(resume, dict) and sid_stored:
            label = pane.get("label")
            pid = by_label.get((wi, slot))
            live_sid = live.get(pid) if pid else None
            if live_sid and live_sid == sid_stored:
                keep.append({
                    "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                    "label": label, "sid": sid_stored,
                })
                continue
            if live_sid and live_sid != sid_stored:
                replace.append({
                    "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                    "label": label, "old": sid_stored, "new": live_sid,
                })
                continue
            # no live process proves this pane -> stale/orphaned entry
            unverifiable.append({
                "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                "label": label, "sid": sid_stored,
                "why": "no live process in this pane proves this sid",
            })
            continue

        # 3. panes with no resume at all: fill only if a live process proves a sid
        label = pane.get("label")
        pid = by_label.get((wi, slot))
        live_sid = live.get(pid) if pid else None
        if live_sid:
            add.append({
                "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                "label": label, "sid": live_sid,
            })
        else:
            unverifiable.append({
                "wi": wi, "ti": ti, "slot": slot, "ws": wsid,
                "label": label, "sid": None,
                "why": ("live jcode process exists but its pane_id has no "
                        "label mapping")
                if (pid and pid in live) else "no live jcode process proves a sid",
            })

    return {
        "add": add, "replace": replace, "keep": keep,
        "purge": purge, "unverifiable": unverifiable,
        "live": live, "by_label": by_label,
    }


def desired_resume(sid):
    """Exactly the shape herdr writes (verified byte-for-byte in cutover_session.py)."""
    return {
        "source": RESUME_SOURCE,
        "agent": RESUME_AGENT,
        "argv": [RESUME_ARGV0, "--resume", sid],
    }


def apply_plan(doc, plan):
    """Return a new document with only the planned mutations applied."""
    out = copy.deepcopy(doc)
    for grp in ("purge", "add", "replace"):
        for item in plan[grp]:
            pane = out["workspaces"][item["wi"]]["tabs"][item["ti"]]["panes"][item["slot"]]
            if grp == "purge":
                pane.pop("agent_session", None)
            else:
                pane["agent_resume"] = desired_resume(item["new"] if grp == "replace" else item["sid"])
    return out


def strip_allowed(doc, plan):
    """Remove every field this tool is allowed to touch; the rest must be identical."""
    out = copy.deepcopy(doc)
    for item in plan["purge"]:
        pane = out["workspaces"][item["wi"]]["tabs"][item["ti"]]["panes"][item["slot"]]
        pane.pop("agent_session", None)
    for item in plan["add"]:
        pane = out["workspaces"][item["wi"]]["tabs"][item["ti"]]["panes"][item["slot"]]
        pane.pop("agent_resume", None)
    for item in plan["replace"]:
        pane = out["workspaces"][item["wi"]]["tabs"][item["ti"]]["panes"][item["slot"]]
        pane.pop("agent_resume", None)
    return out


def serialize(doc, trailing_newline=False):
    """Match herdr's own on-disk shape.

    herdr 0.9.3 writes json.dumps(indent=2, ensure_ascii=False) with NO
    trailing newline (verified byte-exact against session.json). Older files
    did carry one, so the convention is detected from the input rather than
    assumed -- an unfamiliar shape is refused by the byte-shape guard.
    """
    return json.dumps(doc, indent=2, ensure_ascii=False) + ("\n" if trailing_newline else "")


# --------------------------------------------------------------------------
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("path", nargs="?", default=DEFAULT_PATH)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", default=True,
                      help="print the plan and diff, write nothing (DEFAULT)")
    mode.add_argument("--apply", action="store_true",
                      help="atomically write the reconciled file")
    ap.add_argument("--plan-file", help=(
        "apply a plan JSON emitted by a prior --emit-plan run; performs NO "
        "herdr CLI calls, so it is safe while the server is stopped"))
    ap.add_argument("--emit-plan", help="write the discovered plan to this path and exit")
    ap.add_argument("--force", action="store_true",
                    help="override the live-server refusal (dry-run on a COPY only)")
    args = ap.parse_args()

    path = os.path.abspath(os.path.expanduser(args.path))
    if not os.path.isfile(path):
        fail(f"session file not found: {path}")

    # A plan apply issues NO herdr CLI call, so a running server cannot
    # clobber it -- it is the only mode safe to run without stopping the
    # daemon. Discovering (build_plan) DOES call the CLI, so it must never
    # race a write; that is what the two-phase split enforces.
    plan_only = bool(args.plan_file)

    if args.force and args.apply and not plan_only:
        fail("--force with --apply is refused: never override the live-server "
             "guard for a write that runs discovery against a live server")

    reasons = detect_live_server()
    discover_only = bool(args.emit_plan) or (not args.apply and not args.plan_file)
    # Discovery is read-only and REQUIRES the server (it is the source of
    # truth for pane->sid), so a live daemon is expected in that mode.
    # It is refused only when the run would also write.
    unsafe = bool(args.apply) and not plan_only
    if reasons and unsafe and not (args.force and not args.apply):
        log("REFUSED: a herdr server is running and this run would write "
            "after doing discovery against it.")
        for r in reasons:
            log(f"  - {r}")
        log("Either replay a plan with --plan-file (no CLI call), or stop "
            "the server (and its auto-restart) first.")
        return 2
    if reasons and discover_only:
        log("NOTE: server is live; this run is read-only discovery, which "
            "needs it as the source of truth.")

    with open(path, encoding="utf-8") as fh:
        original = fh.read()
    try:
        doc = json.loads(original)
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {path}: {exc}")

    # byte-shape guard: herdr's own formatting must round-trip byte-for-byte,
    # with the trailing-newline convention detected from the input file.
    trailing_nl = original.endswith("\n")
    if serialize(doc, trailing_nl) != original:
        fail("byte-shape guard: input JSON does not round-trip with "
             "indent=2/ensure_ascii=False (trailing_newline=%s); refusing to "
             "touch an unfamiliar format" % trailing_nl)

    if args.plan_file:
        with open(os.path.expanduser(args.plan_file), encoding="utf-8") as fh:
            plan = decode_plan(json.load(fh))
        log("two-phase apply: using the plan emitted by a prior discovery run;")
        log("no herdr CLI call is made, so this is safe with the server stopped.")
        before_sha = plan.get("_source_sha")
        if before_sha and before_sha != __import__("hashlib").sha256(
                original.encode()).hexdigest():
            fail("plan/source mismatch: the file changed since the plan was "
                 "discovered; refusing to apply a stale plan (re-run discovery)")
    else:
        plan = build_plan(doc)
        if args.emit_plan:
            plan["_source_sha"] = __import__("hashlib").sha256(
                original.encode()).hexdigest()
            with open(os.path.expanduser(args.emit_plan), "w", encoding="utf-8") as fh:
                json.dump(encode_plan(plan), fh, indent=2)
            log(f"wrote plan to {args.emit_plan}; nothing was modified")
            return 0

    log("\n=== herdr session reconcile — " + ("DRY-RUN" if not args.apply else "APPLY") + " ===")
    log(f"file: {path}")
    for item in plan["purge"]:
        log(f"  REMOVE   {item['ws']} slot {item['slot']} ({item['label']}).agent_session: {item['value']}")
    for item in plan["add"]:
        log(f"  ADD      {item['ws']} slot {item['slot']} {item['label']} <- {item['sid']}")
    for item in plan["replace"]:
        log(f"  REPLACE  {item['ws']} slot {item['slot']} {item['label']}: {item['old']} -> {item['new']}")
    for item in plan["keep"]:
        log(f"  KEEP     {item['ws']} slot {item['slot']} {item['label']} = {item['sid']}")
    for item in plan["unverifiable"]:
        log(f"  SKIP     {item['ws']} slot {item['slot']} {item['label']}: {item['why']}"
            + (f" (stored {item['sid']})" if item.get("sid") else ""))

    before = count(doc)
    result = apply_plan(doc, plan)
    after = count(result)

    log("")
    log(f"agent_resume:  {before[0]} -> {after[0]}   "
        f"(+{after[0]-before[0]} added, {len(plan['replace'])} replaced, {len(plan['keep'])} unchanged)")
    log(f"agent_session: {before[1]} -> {after[1]}   (-{len(plan['purge'])} removed)")
    out_text = serialize(result, trailing_nl)
    log(f"bytes:         {len(original.encode())} -> {len(out_text.encode())}")

    # guard: nothing outside the plan may change
    if strip_allowed(doc, plan) != strip_allowed(result, plan):
        fail("guard: mutations outside the discovered plan; aborting without writing")
    if json.loads(out_text) != result:
        fail("guard: serialization round-trip mismatch")

    if not args.apply:
        log("DRY-RUN: nothing written.")
        return 0

    d = os.path.dirname(path)
    fd, tmp = tempfile.mkstemp(dir=d, prefix=".session.json.", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(out_text)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, path)
        st = os.stat(path)
        log(f"WROTE {path} atomically (os.replace) — {st.st_size} bytes")
    except Exception:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise
    return 0


def encode_plan(plan):
    """JSON-safe copy of a plan: tuple keys become "wi|slot" strings."""
    def fix(obj):
        if isinstance(obj, dict):
            return {"|".join(str(x) for x in k) if isinstance(k, tuple) else k:
                    fix(v) for k, v in obj.items()}
        if isinstance(obj, list):
            return [fix(v) for v in obj]
        return obj
    return fix(plan)


def decode_plan(raw):
    """Inverse of encode_plan: restore "wi|slot" keys to (wi, slot) tuples."""
    def fix(obj):
        if isinstance(obj, dict):
            out = {}
            for k, v in obj.items():
                if isinstance(k, str) and k.count("|") == 1:
                    a, b = k.split("|")
                    try:
                        k2 = (int(a), int(b))
                    except ValueError:
                        k2 = k
                else:
                    k2 = k
                out[k2] = fix(v)
            return out
        if isinstance(obj, list):
            return [fix(v) for v in obj]
        return obj
    return fix(raw)


def count(doc):
    resume = session = 0
    for _wi, _ti, _slot, _ws, pane in iter_panes(doc):
        resume += 1 if isinstance(pane.get("agent_resume"), dict) else 0
        session += 1 if isinstance(pane.get("agent_session"), dict) else 0
    return resume, session


if __name__ == "__main__":
    raise SystemExit(main())