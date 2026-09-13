# Contributing to Jcode

## Branch Workflow

All changes must go through a feature branch and pull request. Direct pushes to `master` are blocked by pre-tool hooks.

```
git checkout -b feat/my-feature
# make changes
git commit -m "feat(scope): description"
git push origin feat/my-feature
gh pr create
```

## Commit Convention

Use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat(scope):` new feature
- `fix(scope):` bug fix
- `docs(scope):` documentation only
- `refactor(scope):` code change that neither fixes a bug nor adds a feature
- `test(scope):` adding or updating tests
- `chore(scope):` maintenance tasks

## Code Quality

- All Rust code must pass `cargo check` and `cargo clippy`
- Line length limit: 100 characters
- Files must stay under 500 lines (target 350)
- Tests required for new functionality

## TUI Changes

Status bar and layout changes should be verified against compressed pane sizes (30x7) and standard layouts (14" laptop, 27" desktop).
