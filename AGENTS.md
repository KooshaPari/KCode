# Repository Guidelines

## Last updated
2026-10-05 (decoded from kcode to kcode; backwards-compat shim added)

## Working notes
- The product is now branded **kcode**. The cargo package is `kcode`; the binary on PATH is `kcode`; the home dir is `~/.kcode/`.
- The fork is the KooshaPari fork of `1jehuang/kcode`. v0.27.x+ builds of kcode retain the ability to read kcode's data for one release cycle (see backwards-compat shim in `crates/kcode-storage/src/lib.rs:home_dir()`).

## Build & install channels

The launcher is **`~/.local/bin/kcode`** (formerly `~/.local/bin/kcode`). It reads `KCODE_HOME` first, falls back to `KCODE_HOME`, then `~/.kcode/builds/current/kcode`, then `~/.kcode/builds/current/kcode`.

Build channel layout (all paths now use `kcode`):

| Channel     | Path                                          | Symlink inside                                       |
|-------------|-----------------------------------------------|------------------------------------------------------|
| Self-dev    | `~/.kcode/builds/current/kcode`                | `versions/<commit-sha>/kcode` (renamed from `kcode`) |
| Stable      | `~/.kcode/builds/stable/kcode`                  | same as above                                         |
| Immutable   | `~/.kcode/builds/versions/<commit-sha>/kcode` | the actual binary                                    |
| Shared-srv  | `~/.kcode/builds/shared-server/kcode`           | the long-lived daemon binary                          |

**Backwards compat:** for one release cycle, the launcher also accepts `~/.kcode/builds/.../kcode` paths so legacy installs continue to work. To migrate: run `scripts/migrate-from-kcode.sh`.

### Windows

The Windows equivalents are:
- `%LOCALAPPDATA%\kcode\bin\kcode.exe` (launcher)
- `%LOCALAPPDATA%\kcode\builds\stable\kcode.exe`
- `%LOCALAPPDATA%\kcode\builds\versions\<version>\kcode.exe`

### Secrets dir

~/.kcode/ is the data home. ~/.config/kcode/ is the secrets home (env-file creds). Both have backwards-compat fallback to ~/.kcode/ and ~/.config/kcode/.

### Environment variables

- `KCODE_HOME` — override the data home (formerly `KCODE_HOME`; still supported with deprecation warning)
- `KCODE_NO_EMOJI` — disable emoji globally
- `KCODE_BINARY` — SDK override
- `KCODE_API_SOCKET` — SDK socket override

### Verifying a change at runtime

After building, smoke-test by running `kcode --version` from a shell. The build channels (`current`, `stable`) are real directories containing one symlink to a version directory; renaming the inner symlink from `kcode` to `kcode` is a manual step that the build script does.

Long-lived daemon: `~/.kcode/builds/shared-server/kcode` is a daemon binary that does NOT update from a launcher rebuild. Restart the daemon after renaming.