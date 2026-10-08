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
import json
import os
import sys
import tempfile

from reconcile_sessions_discovery import (
    DEFAULT_PATH,
    detect_live_server,
    log,
)
from reconcile_sessions_plan import (
    apply_plan,
    build_plan,
    count,
    decode_plan,
    encode_plan,
    serialize,
    strip_allowed,
)

def fail(msg):
    print(f"ERROR: {msg}", file=sys.stderr)
    raise SystemExit(1)

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

if __name__ == "__main__":
    raise SystemExit(main())
