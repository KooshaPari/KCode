#!/usr/bin/env bash
# tests/install_release_signature_test.sh
#
# Unit test for the install_release.sh signature classifier.
# Runs synthetic flag strings through install_signature_classify and asserts
# the expected verdict. No real binary or codesign invocation needed.
#
# Run with:  bash tests/install_release_signature_test.sh
#
# Exit 0 on success, exit 1 on any failed assertion.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/lib_install_signature.sh
. "$REPO_ROOT/scripts/lib_install_signature.sh"

# --- harness ---
failed=0
total=0

assert_classify() {
  local label="$1"
  local input="$2"
  local expected="$3"
  total=$((total + 1))

  local actual
  actual="$(install_signature_classify "$input")"
  if [ "$actual" = "$expected" ]; then
    printf "  PASS  %-50s  %-8s  <- %q\n" "$label" "$actual" "$input"
  else
    printf "  FAIL  %-50s  expected %-8s got %-8s  <- %q\n" "$label" "$expected" "$actual" "$input"
    failed=$((failed + 1))
  fi
}

echo "== install_signature_classify: good =="
assert_classify "0x20002 suffix"          "CodeDirectory v=20400 size=1070264 flags=0x20002(adhoc,linker-signed) hashes=33442+0 location=embedded"  "good"
assert_classify "0x20002 with spaces"      "flags=0x20002 (adhoc,linker-signed)"                                                                "good"
assert_classify "CS_LINKER_SIGNED text"   "flags=0x12(cs_linker_signed)"                                                                        "good"
assert_classify "CS_LINKER_SIGNED upper"   "CodeDirectory v=20400 flags=0x20002(CS_LINKER_SIGNED)"                                              "good"

echo
echo "== install_signature_classify: bad =="
assert_classify "0x2 suffix"              "CodeDirectory v=20400 size=1070264 flags=0x2(adhoc) hashes=33442+0"                                  "bad"
assert_classify "0x2 with paren"          "flags=0x2(adhoc)"                                                                                     "bad"
assert_classify "CS_ADHOC text"           "flags=0x2(CS_ADHOC)"                                                                                  "bad"
assert_classify "0x2 alone"               "0x2"                                                                                                  "bad"

echo
echo "== install_signature_classify: unknown =="
assert_classify "empty string"            ""                                                                                                      "unknown"
assert_classify "missing flags="          "CodeDirectory v=20400 size=1070264 hashes=33442+0 location=embedded"                                  "unknown"
assert_classify "future flag 0x80000"     "CodeDirectory v=99999 size=0 flags=0x80000(secure)"                                                   "unknown"
assert_classify "garbage"                 "not a codesign output"                                                                                "unknown"

echo
echo "== install_signature_classify: precedence (good matches before bad) =="
# 0x20002 contains 0x2 as a substring, so the case statement must check
# 0x20002 first. Verify by passing a flag string with BOTH present.
assert_classify "0x20002 must beat 0x2"    "flags=0x20002(adhoc,linker-signed) flags=0x2(adhoc)"                                                "good"

echo
echo "== total $total, failed $failed =="
if [ "$failed" -ne 0 ]; then
  echo "FAIL"
  exit 1
fi
echo "OK"
