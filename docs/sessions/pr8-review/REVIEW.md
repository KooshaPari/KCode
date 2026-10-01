# PR8 review correction

Base: 42cfcfde4. Normal worktree is dirty and ahead at b87cd9955; preserved.
All 59 review threads were inspected; repeated comments map to the same fixes.

Changes: Cargo dual filters and duplicate workflow env mapping; HERDR status
argv, empty parent path, host platform assertion, Windows early return, proper
plugin directories plus plugin link; stale docs; recovered output assertion;
unique private diagnostic captures, failed-write disablement, Responses API
capture; environment-free capture tests and TempDir cleanup; accepted-send
Working and completed/error Idle states; revision-ordered session/state reports;
launcher test private install directory with RAII restoration.

Adjudication: raw-content redaction is not applied because these explicitly
opt-in forensic tools require the exact request/response. Private permissions,
create-new semantics and dedicated-directory documentation address exposure.
Triple-clone suggestion is cosmetic and does not reduce required owned copies.
The old one-file concurrent-test weakening is replaced by deterministic injected
capture paths, so no global environment mutation remains in capture tests.

Installed `herdr plugin link --help` confirms `herdr plugin link <PATH>`.
No local build or tests. Rustfmt and diff whitespace inspection only; free hosted
review-correction workflow carries the changed-crate, TUI and CLI gates.


Independent follow-up: History now reports state after adopting activity instead
of publishing Idle during reset. The active-resume regression checks the actual
requested HERDR state using thread-local test-only observation. The focused TUI
cohort includes it. ACP docs now distinguish handler invocation from compilation.
Duplicate env mappings in Windows/iOS workflows are consolidated. CI SSH setup
was removed after verifying no .gitmodules and only public HTTPS git dependencies.
