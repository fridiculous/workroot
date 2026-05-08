# Workroot Source Tree Analysis

This file is a compact map of the repository layout.

## Top-level structure

```text
workroot/
|-- .github/
|-- docs/
|-- scripts/
|-- src/
|-- tests/
|-- Cargo.toml
|-- Cargo.lock
|-- README.md
`-- install.sh
```

## Important directories

### `src/`
Main Rust implementation.

Key files:
- `main.rs` - binary entry point
- `lib.rs` - library exports
- `cli.rs` - command model and dispatch
- `storage.rs` - path resolution and persistence
- `discovery.rs` - repo/worktree discovery and creation
- `resolver.rs` - repo/target resolution
- `status.rs` - status output
- `session.rs` - tmux-backed session behavior
- `prune.rs` - safe cleanup
- `push.rs` - push behavior
- `shell.rs` - shell integration snippets

### `tests/`
Integration-heavy contract tests using temporary repositories and shell/tool boundaries.

### `.github/workflows/`
CI and release workflows.

### `scripts/`
Operational helpers, including local install support.

### `docs/`
Project docs, workflow notes, and supporting reference material.

## Entry points

- Binary: `src/main.rs`
- Library: `src/lib.rs`
- Installer: `install.sh`
- Local shell installer: `scripts/install-workroot-shell.sh`

## Notes

- The repository is primarily code, tests, scripts, and docs.
- No Python footprint was found in the repo during the OSS-readiness audit.
- Public-facing docs should stay smaller and more intentional than internal planning material.
