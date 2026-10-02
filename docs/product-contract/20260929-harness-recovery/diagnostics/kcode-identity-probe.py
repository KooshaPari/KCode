"""Synthetic oracle-design experiment, not KCode execution or a production verifier.
The daemon/client distinction is grounded in KCode AGENTS.md at the source revision.
Policy values and receipts below are fixtures, not observations of installed binaries.
"""
from copy import deepcopy
from datetime import datetime, timezone
from hashlib import sha256
from pathlib import Path
import json
import sys

SOURCE = "046ea2af5e01e84449f65d086510b51152360215"
POLICY = {
    "product": "KooshaPari/KCode", "contract": "fixture-contract-v1",
    "criterion": "fixture-resume", "configuration": "fixture-config-a",
    "environment": "fixture-linux", "verifier": "fixture-grader-v1",
    "candidate": {"source": SOURCE, "client_digest": "client-new", "daemon_digest": "daemon-new", "socket": "isolated-socket-a"},
    "required_cases": {"positive", "wrong-daemon", "restart"},
}

def accepts(receipt: dict, policy: dict) -> bool:
    """Pure acceptance predicate for a fixed synthetic policy; no cryptographic trust."""
    for key in ("product", "contract", "criterion", "configuration", "environment", "verifier", "candidate"):
        if key not in receipt or receipt[key] != policy[key]:
            return False
    if receipt.get("status") != "passed" or receipt.get("collector") != "complete":
        return False
    if receipt.get("conflict") is not False or receipt.get("fresh") is not True:
        return False
    cases = receipt.get("cases")
    if not isinstance(cases, dict) or not policy["required_cases"]:
        return False
    if any(cases.get(case) != "passed" for case in policy["required_cases"]):
        return False
    # Raw artifact digest validation and trusted attestation are intentionally NOT implemented.
    if not isinstance(receipt.get("artifact"), str) or not receipt["artifact"]:
        return False
    return True

base = {key: deepcopy(value) for key, value in POLICY.items() if key != "required_cases"}
base.update(status="passed", collector="complete", conflict=False, fresh=True,
            cases={key: "passed" for key in POLICY["required_cases"]}, artifact="fixture-only")
fixtures = [("matching-fixture", base, True)]
for key, value in (("product", "another-product"), ("contract", "old-contract"),
                   ("configuration", "other-config"), ("environment", "other-env"),
                   ("verifier", "unapproved-grader"), ("status", "skipped"),
                   ("collector", "failed"), ("conflict", True), ("fresh", False),
                   ("cases", {}), ("artifact", "")):
    r = deepcopy(base); r[key] = value
    fixtures.append(("reject-" + key, r, False))
r = deepcopy(base); r["candidate"]["daemon_digest"] = "daemon-old"
fixtures.append(("reject-new-client-old-daemon", r, False))
r = deepcopy(base); del r["candidate"]
fixtures.append(("reject-missing-candidate", r, False))
r = deepcopy(base); r["cases"]["restart"] = "skipped"
fixtures.append(("reject-skipped-required-case", r, False))
results = []
for name, receipt, expected in fixtures:
    observed = accepts(receipt, POLICY)
    assert observed is expected, name
    results.append({"id": name, "observed": observed, "expected": expected})
print(json.dumps({
    "schema": "harness-recovery-diagnostic/v1", "product": "KooshaPari/KCode",
    "source_revision": SOURCE, "timestamp": datetime.now(timezone.utc).isoformat(),
    "script_sha256": sha256(Path(__file__).read_bytes()).hexdigest(),
    "runtime": sys.version.split()[0], "scope": "synthetic identity-policy model only",
    "diagnostic_assertions": "passed", "native_product_tests": "NOT_RUN",
    "product_acceptance": "UNKNOWN", "production_verifier": False,
    "limitations": ["No daemon launched", "No artifact or attestation verified", "Freshness is a fixture flag, not a validated clock", "Policy trust and storage immutability not implemented"],
    "cases": results,
}, indent=2))
