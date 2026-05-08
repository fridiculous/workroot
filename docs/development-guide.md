# Workroot Development Guide

## Prerequisites

- Rust stable toolchain with Cargo
- `git`
- `tmux` for managed-session commands and related tests
- optional `gh` for GitHub PR lineage proof during prune flows
- optional shells for integration tests and manual verification: zsh, bash, fish

## Setup

```bash
git clone https://github.com/fridiculous/workroot.git
cd workroot
cargo test
```

For local dogfooding:

```bash
./scripts/install-workroot-shell.sh zsh
./scripts/install-workroot-shell.sh bash
./scripts/install-workroot-shell.sh fish
```

## Common commands

| Task | Command |
| --- | --- |
| Build debug binary | `cargo build` |
| Run CLI from checkout | `cargo run -- status` |
| Run all tests | `cargo test` |
| Run shell-related tests | `cargo test shell` |
| Check formatting | `cargo fmt --check` |
| Run Clippy like CI | `cargo clippy --all-targets -- -D warnings` |
| List package contents | `cargo package --locked --allow-dirty --list` |
| Dry-run publish | `cargo publish --dry-run --locked --allow-dirty` |
| Install current checkout | `cargo install --path . --force` |

## Environment variables

Workroot supports storage-root overrides useful for tests and isolated dogfooding:

- `WORKROOT_CONFIG_HOME`
- `WORKROOT_STATE_HOME`
- `WORKROOT_CACHE_HOME`
- `XDG_CONFIG_HOME`
- `XDG_STATE_HOME`
- `XDG_CACHE_HOME`

Legacy pre-rename env vars are still supported as compatibility fallbacks during the rename transition.

## Local development flow

1. Start in `src/cli.rs` to understand command routing.
2. Follow the dispatched function into the feature module.
3. Use `src/domain.rs` for persisted schema details.
4. Use `src/storage.rs` for config/state/cache persistence changes.
5. Add or update tests in the closest integration file under `tests/`.
6. Run formatting, clippy, and tests before finalizing changes.

## Adding a command

1. Add the clap variant in `Commands` or `WorktreeCommand`.
2. Decide whether it should be public or hidden.
3. Add dispatch in `run` or `run_worktree`.
4. Keep behavior in a focused module rather than bloating the router.
5. Add parser coverage in `tests/core_contracts.rs`.
6. Add behavior coverage in a domain-specific test file.
7. Update README/help/docs if the public contract changed.

## Manual smoke checks

After changes that affect core CLI behavior:

```bash
cargo run -- --help
cargo run -- status
cargo run -- shell-init zsh
cargo run -- worktree scan
cargo run -- path workroot base
```

For tmux-backed flows, use an indexed worktree and run:

```bash
cargo run -- run workroot base -- make test
```

For prune changes, always test against a disposable repository first. The prune flow should continue to show proof evidence before any removal.
