# Workroot Project Overview

Workroot is a Rust CLI and library for managing git worktrees across repos from one machine-wide place.

## Summary

Workroot keeps the public workflow intentionally small:
- `status`
- `path`
- `cd`
- `new`
- `run`
- `push`
- `prune`

It stores config, state, and cache outside the repo using XDG-style paths, and it uses Git plus optional tmux-based session helpers to support the workflow.

## Naming and packaging

- Cargo package: `workroot-cli`
- Primary binary: `workroot`
- Optional shorthand binary: `wr`
- Library crate: `workroot`

## Core capabilities

- discover and index git worktree families
- create new worktrees under a configured root
- resolve repo and target names for navigation and execution
- provide shell integration for parent-shell `cd` behavior
- run commands inside worktrees
- show a machine-wide status view across tracked worktrees
- push branch work with upstream-aware defaults
- prune merged worktrees conservatively with proof before removal

## Architecture at a glance

The codebase is organized by responsibility:
- `src/cli.rs` for command parsing and routing
- `src/storage.rs` for config/state/cache paths, locking, and persistence
- `src/discovery.rs` for adopt/scan/new/path behavior
- `src/resolver.rs` for repo/target resolution
- `src/status.rs` for status rendering and refresh
- `src/session.rs` for tmux-backed managed-session behavior
- `src/prune.rs` and `src/lineage.rs` for safe prune logic

## Development basics

Prerequisites:
- Rust stable toolchain with Cargo
- `git`
- `tmux` for managed-session flows and related tests
- optional `gh` for GitHub PR lineage proof

Common commands:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --help
cargo run -- status
```

## Related docs

- [README](../README.md)
- [Target-first Workflow](./target-first-workflow.md)
- [Development Guide](./development-guide.md)
- [Architecture](./architecture.md)
- [Deployment Guide](./deployment-guide.md)
