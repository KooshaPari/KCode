"""Discover Herdr panes and their live JCode session ids."""

import json
import os
import re
import subprocess
from concurrent.futures import ThreadPoolExecutor

DEFAULT_PATH = os.path.expanduser("~/.config/herdr/session.json")
SOCK_PATH = os.path.expanduser("~/.config/herdr/herdr.sock")
HERDR_BIN = os.path.expanduser("~/.local/bin/herdr")
PROBE_VALUES = frozenset({"probe_devin", "probe_letta", "probe_hermes"})
SID_RE = re.compile(r"session_[a-z0-9_]+_\d+_[0-9a-f]{16}")
PANE_ID_RE = re.compile(r"HERDR_PANE_ID=(\S+)")

def log(msg=""):
    print(msg)

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
