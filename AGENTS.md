# Repository Guidelines

## Development Workflow

- **Welcome pull requests from everyone** - Review contributions on their merits,
  regardless of whether the author is a maintainer, an existing contributor, a
  first-time contributor, or an agent. Good PRs can be merged directly after review
  and validation. Do not require a maintainer-authored rewrite merely because of
  who submitted the change. See `CONTRIBUTING.md` for the contribution policy.
- **Keep work scoped** - Work on your own branch and preserve unrelated work. When
  the user asks you to review or integrate a PR or branch, you may inspect, test,
  and integrate that contribution regardless of author status. Do not pull in
  unrelated branches or merge a PR without user authorization.

## Install Notes
- `~/.local/bin/kcode` is the launcher symlink used from `PATH`.
- `~/.kcode/builds/current/kcode` is the active local/source-build channel; self-dev builds and `scripts/install_release.sh` point the launcher here.
- `~/.kcode/builds/stable/kcode` is the stable release channel; `scripts/install.sh` installs this and points the launcher here.
- `~/.kcode/builds/versions/<version>/kcode` stores immutable binaries.
- `~/.kcode/builds/canary/kcode` still exists for canary/testing flows, but it is not the primary self-dev install path.
- On Windows, the equivalents are `%LOCALAPPDATA%\\kcode\\bin\\kcode.exe` for the launcher, `%LOCALAPPDATA%\\kcode\\builds\\stable\\kcode.exe` for stable, and `%LOCALAPPDATA%\\kcode\\builds\\versions\\<version>\\kcode.exe` for immutable installs; `scripts/install.ps1` currently installs the stable channel.
- Ensure `~/.local/bin` is **before** `~/.cargo/bin` in `PATH`.

## Verifying a change at runtime

`cargo build` alone proves nothing about behavior. `kcode run` and interactive
sessions are served by the long-lived daemon at
`~/.kcode/builds/shared-server/kcode`, which is a symlink into
`~/.kcode/builds/versions/<version>/`. Until that symlink is repointed and the
daemon restarted (`kcode self-dev --build`), a freshly built binary is inert and
every runtime check silently measures the old code.

To test a change without disturbing the shared daemon or the caller's session,
run your build against its own socket:

```bash
cargo build --profile selfdev
./target/selfdev/kcode run --no-update --socket /run/user/1000/kcode-mytest.sock '<prompt>'
```

Two things that waste time otherwise:

- `crate::logging::info` writes to a log file, not stderr, so instrumenting a
  code path with it produces no visible output under `--trace`. Use `eprintln!`
  for throwaway diagnostics and delete it before committing.
- Confirm which binary you are actually inspecting. `strings` on
  `builds/shared-server/kcode` reads a 70-byte symlink, not a program; resolve it
  with `readlink -f` first.
