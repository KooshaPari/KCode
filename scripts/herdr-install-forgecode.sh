#!/bin/sh
# Install the legacy HERDR screen manifest for ForgeCode.
#
# HERDR screen manifests are legacy-only: herdr-core (HC) infers agent state
# itself and does not read ~/.config/herdr/agent-detection, so under HC these
# files have no effect.
#
# This script no longer writes TOML itself. It used to heredoc a hand-maintained
# copy of the manifest which had drifted from the generator in
# crates/jcode-herdr/src/manifest.rs: it omitted the `agent` / `version` keys
# and wrote one multi-pattern rule per state, which cannot match under the
# documented AND semantics. The binary's generator is now the single source of
# truth; this file is kept only as an install entry point referenced by docs.
#
# POSIX-compatible -- no bashisms.

set -e

NOTICE='HERDR screen manifests are legacy-only: herdr-core (HC) infers agent state itself and does not read ~/.config/herdr/agent-detection, so under HC these files have no effect.'

echo "note: $NOTICE" >&2

installer=
for candidate in kcode jcode; do
    if command -v "$candidate" >/dev/null 2>&1; then
        installer=$candidate
        break
    fi
done

if [ -z "$installer" ]; then
    echo "error: neither 'kcode' nor 'jcode' is on PATH, so the manifest generator cannot run." >&2
    echo "       Install KCode/JCode first, then run: kcode herdr-install" >&2
    exit 1
fi

echo "running: $installer herdr-install" >&2
exec "$installer" herdr-install
