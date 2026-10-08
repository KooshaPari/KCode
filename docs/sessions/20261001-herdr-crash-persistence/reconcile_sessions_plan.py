"""Build, validate, and serialize session reconciliation plans."""

import copy
import json

from reconcile_sessions_discovery import (
    PROBE_VALUES,
    iter_panes,
    log,
    pane_correlation,
)

RESUME_SOURCE = "herdr:jcode"
RESUME_AGENT = "jcode"
RESUME_ARGV0 = "jcode"

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
