# Configuring the System Prompt

kcode builds its system prompt from several layers. Two of them are user-editable
files, so you can tune agent behavior without rebuilding.

## Layers (in order)

1. **Base system prompt** — built-in `crates/kcode-base/src/prompt/system_prompt.md`,
   overridable by file (see below).
2. Capability modules (e.g. Mermaid guidance).
3. Product-specific self-dev guidance. Sessions rooted in a Kcode Desktop
   checkout automatically receive the Desktop prompt and `desktop_selfdev` tool,
   separate from CLI/TUI self-dev flags, `selfdev`, and `debug_socket`.
4. `AGENTS.md` — project `./AGENTS.md` and global `~/AGENTS.md`.
5. Prompt overlay — `./.kcode/prompt-overlay.md` and `~/.kcode/prompt-overlay.md`.
6. Preferred tools — `./.kcode/preferred-tools.md` and `~/.kcode/preferred-tools.md`.
7. Memory and the active skill prompt (dynamic, not cached).

## Adding guidance (most common)

Append instructions without touching the default prompt:

- `~/.kcode/prompt-overlay.md` — applies everywhere.
- `./.kcode/prompt-overlay.md` — applies to one project.

Both are included when present. For layers 4–6, if the project and global paths
resolve to the same canonical path (for example, when working in `$HOME` or using
symlink aliases), the file is included once under its project heading. Distinct
files are still both included, even when their contents match. The global
`.kcode` directory respects `KCODE_HOME` when set.

## Replacing the base prompt

To fully replace layer 1, create either file:

- `./.kcode/system-prompt.md` (project, highest precedence)
- `~/.kcode/system-prompt.md` (global)

The first non-empty file wins; otherwise the built-in default is used. An empty or
whitespace-only file falls back to the default, so you cannot accidentally ship an
empty prompt.

This replaces only the base prompt. AGENTS.md, overlays, skills, and memory still apply.

## Notes

- Changes to these files take effect for **new sessions**; a running session keeps the
  prompt captured at start.
- Editing the built-in `system_prompt.md` requires a rebuild (`selfdev build-reload`),
  since it is embedded with `include_str!`.
- Swarm model-routing guidance has its own analogous file: `.kcode/swarm-prompt.md`.
  Use `/swarm-prompt` to edit the active project or global file. New agents load
  the latest contents immediately; already-running agents keep the prompt they
  captured at session creation so their tool definition and context cache stay stable.

## Direct SDK overrides

SDK callers can replace the **complete assembled system prompt** when creating a
session, without writing files:

```typescript
const session = await client.createSession({
  workingDir: process.cwd(),
  systemPrompt: "You are a concise programming tutor.",
});
```

Rust callers use `create_session_with_options(CreateSessionOptions {
working_dir: None, system_prompt: Some("You are a concise programming tutor.".into())
})`. The existing `create_session(working_dir)` API remains available.

Unlike `system-prompt.md`, which replaces only the base layer, this option replaces
all assembled prompt layers. Omit the option to retain normal Kcode prompting.
An empty string explicitly selects an empty system prompt. The override belongs
to that session and is persisted for resume and inherited by forks. Attaching to
an existing session does not change its prompt. This requires a daemon version
that supports the `system_prompt` session-creation field.
