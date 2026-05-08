# Contributing to Workroot

Thanks for your interest in contributing to Workroot.

Workroot aims to stay small, composable, and target-first:
- small public command surface
- strong stdout/stderr contracts
- global visibility across repos
- conservative behavior around destructive actions

## Development setup

Prerequisites:
- Rust stable toolchain with Cargo
- `git`
- `tmux` for managed-session flows and related tests
- optional: `gh` for GitHub PR lineage proof work

Clone and test:

```bash
git clone https://github.com/fridiculous/workroot.git
cd workroot
cargo test
```

Useful commands:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- status
cargo run -- --help
```

## Contribution guidelines

Please keep changes aligned with the product direction:
- prefer a smaller, clearer CLI over a larger one
- preserve target-first naming and workflow
- preserve path-only stdout for composable commands like `workroot path` and `workroot new`
- avoid turning Workroot into a large tmux or agent control plane unless there is clear demand

For behavior changes:
- add or update tests first when practical
- keep help text and README examples in sync with the actual command behavior
- prefer narrow, reviewable pull requests

## Pull requests

Before opening a PR:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

A good PR includes:
- a clear problem statement
- the intended user-facing behavior
- tests for changed behavior
- docs/help updates if the public contract changed

## Design notes

If you are adding or changing commands, please read:
- `README.md`
- `docs/target-first-workflow.md`
- `docs/project-overview.md`

## Security

Please do not file public issues for sensitive security problems. See `SECURITY.md` for reporting guidance.
