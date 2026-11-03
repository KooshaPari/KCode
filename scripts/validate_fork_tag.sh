#!/usr/bin/env bash
# Validate a fork release tag against the KCode fork-version grammar.
#
# The release workflow gates fork publishing on `contains(ref_name, '-k')`,
# which admits malformed tags such as `v1.2.3-kbad`. The authoritative
# grammar lives in crates/jcode-build-meta/src/fork_version.rs (`fork_version::parse`),
# which is what the runtime uses to accept a version string as a fork
# identity. This script mirrors that grammar so a tag that this accepts is a
# tag the build will also accept.
#
# Grammar (mirroring fork_version::parse):
#   <base>-k<fork>[-<channel>]
#   base := three dot-separated non-negative integers, no leading zeros in a
#           multi-digit component, and not all zeros
#   fork := three dot-separated non-negative integers, no leading zeros in a
#           multi-digit component (all zeros is allowed)
#   channel (optional) := dot- or dash-separated alphanumeric components,
#           none empty
#
# Usage:
#   scripts/validate_fork_tag.sh v0.93.398-k0.1.0
#   scripts/validate_fork_tag.sh --quiet v0.93.398-k0.1.0
#
# Exits 0 when the tag is valid, 1 when it is not, 2 on usage error.
# With --quiet, prints nothing on success; otherwise prints the normalized
# version on success and a diagnostic on failure.

set -euo pipefail

QUIET=0
if [[ "${1:-}" == "--quiet" ]]; then
  QUIET=1
  shift
fi

TAG="${1:-}"
if [[ -z "$TAG" ]]; then
  echo "usage: $0 [--quiet] <tag>" >&2
  exit 2
fi

# Strip surrounding whitespace and an optional leading "v", matching
# fork_version::parse.
normalized="${TAG#"${TAG%%[![:space:]]*}"}"
normalized="${normalized%"${normalized##*[![:space:]]}"}"
normalized="${normalized#v}"

fail() {
  if [[ "$QUIET" -eq 0 ]]; then
    echo "invalid fork release tag: $1" >&2
    echo "expected <base>-k<fork>[-<channel>], e.g. v0.93.398-k0.1.0" >&2
  fi
  exit 1
}

# numeric <value> <nonzero>
# Three dot-separated non-negative integers. Rejects empty components,
# non-digits, multi-digit components with a leading zero, and (when nonzero
# is 1) a value where every component is zero.
numeric() {
  local value="$1"
  local nonzero="$2"
  local parts
  IFS='.' read -r -a parts <<<"$value"
  if [[ "${#parts[@]}" -ne 3 ]]; then
    return 1
  fi
  local part
  for part in "${parts[@]}"; do
    if [[ -z "$part" ]]; then
      return 1
    fi
    if [[ ! "$part" =~ ^[0-9]+$ ]]; then
      return 1
    fi
    if [[ "${#part}" -gt 1 && "$part" == 0* ]]; then
      return 1
    fi
  done
  if [[ "$nonzero" -eq 1 ]]; then
    if [[ "${parts[0]}" == "0" && "${parts[1]}" == "0" && "${parts[2]}" == "0" ]]; then
      return 1
    fi
  fi
  return 0
}

if [[ "$normalized" != *"-k"* ]]; then
  fail "missing the -k fork marker"
fi

# Split at the FIRST "-k", matching fork_version::parse's `split_once("-k")`.
# Both halves must use the same occurrence: `${normalized%%-k*}` strips the
# longest matching SUFFIX, so the surviving prefix is everything before the
# FIRST "-k", and `${normalized#*-k}` strips the shortest matching PREFIX, so
# the remainder is everything after the FIRST "-k". Both therefore split at
# the first occurrence, exactly like split_once.
#
# A version with two "-k" markers, e.g. "1.2.3-k0.1.0-k9.9.9", splits the same
# way here as it does in fork_version::parse: base "1.2.3", fork
# "0.1.0-k9.9.9", which the parser reads as fork version 0.1.0 with the
# alphanumeric channel "k9.9.9" and accepts. Verified against a verbatim copy
# of the Rust parser, so the two implementations agree.
base="${normalized%%-k*}"
fork="${normalized#*-k}"

if [[ -z "$base" ]]; then
  fail "missing base version before -k"
fi
numeric "$base" 1 || fail "base version '$base' is not three non-zero-allowing integers"

# A channel is separated from the fork version by a single dash.
channel=""
if [[ "$fork" == *-* ]]; then
  channel="${fork#*-}"
  fork="${fork%%-*}"
  if [[ -z "$channel" ]]; then
    fail "empty channel after the fork version"
  fi
  IFS='.' read -r -a channel_parts <<<"$channel"
  for part in "${channel_parts[@]}"; do
    # Split on dots and dashes: each piece must be non-empty alphanumeric.
    IFS='-' read -r -a dash_parts <<<"$part"
    for dash_part in "${dash_parts[@]}"; do
      if [[ -z "$dash_part" ]]; then
        fail "channel '$channel' has an empty component"
      fi
      if [[ ! "$dash_part" =~ ^[A-Za-z0-9]+$ ]]; then
        fail "channel '$channel' has a non-alphanumeric component"
      fi
    done
  done
fi

if [[ -z "$fork" ]]; then
  fail "missing fork version after -k"
fi
numeric "$fork" 0 || fail "fork version '$fork' is not three integers"

if [[ "$QUIET" -eq 0 ]]; then
  echo "$normalized"
fi
