# Repository Guidelines

## Last updated
2026-10-05 (decoded from jcode to kcode; backwards-compat shim added)

## Working notes
- The product is now branded **kcode**. The cargo package is `kcode`; the binary on PATH is `kcode`; the home dir is `~/.kcode/`.
- The fork is the KooshaPari fork of `1jehuang/jcode`. v0.27.x+ builds of kcode retain the ability to read jcode's data for one release cycle (see backwards-compat shim in `crates/jcode-storage/src/lib.rs:home_dir()`).

## Build & install channels

The launcher is **`~/.local/bin/kcode`** (formerly `~/.local/bin/jcode`). It reads `KCODE_HOME` first, falls back to `JCODE_HOME`, then `~/.kcode/builds/current/kcode`, then `~/.jcode/builds/current/jcode`.

Build channel layout (all paths now use `kcode`):

| Channel     | Path                                          | Symlink inside                                       |
|-------------|-----------------------------------------------|------------------------------------------------------|
| Self-dev    | `~/.kcode/builds/current/kcode`                | `versions/<commit-sha>/kcode` (renamed from `jcode`) |
| Stable      | `~/.kcode/builds/stable/kcode`                  | same as above                                         |
| Immutable   | `~/.kcode/builds/versions/<commit-sha>/kcode` | the actual binary                                    |
| Shared-srv  | `~/.kcode/builds/shared-server/kcode`           | the long-lived daemon binary                          |

**Backwards compat:** for one release cycle, the launcher also accepts `~/.jcode/builds/.../jcode` paths so legacy installs continue to work. To migrate: run `scripts/migrate-from-jcode.sh`.

### Windows

The Windows equivalents are:
- `%LOCALAPPDATA%\kcode\bin\kcode.exe` (launcher)
- `%LOCALAPPDATA%\kcode\builds\stable\kcode.exe`
- `%LOCALAPPDATA%\kcode\builds\versions\<version>\kcode.exe`

### Secrets dir

~/.kcode/ is the data home. ~/.config/kcode/ is the secrets home (env-file creds). Both have backwards-compat fallback to ~/.jcode/ and ~/.config/jcode/.

### Environment variables

- `KCODE_HOME` — override the data home (formerly `JCODE_HOME`; still supported with deprecation warning)
- `KCODE_NO_EMOJI` — disable emoji globally
- `KCODE_BINARY` — SDK override
- `KCODE_API_SOCKET` — SDK socket override

### Verifying a change at runtime

After building, smoke-test by running `kcode --version` from a shell. The build channels (`current`, `stable`) are real directories containing one symlink to a version directory; renaming the inner symlink from `jcode` to `kcode` is a manual step that the build script does.

Long-lived daemon: `~/.kcode/builds/shared-server/kcode` is a daemon binary that does NOT update from a launcher rebuild. Restart the daemon after renaming.