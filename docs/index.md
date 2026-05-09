# Workroot Documentation Index

Workroot is the machine-wide switchboard for git worktrees.

Start with the README for everyday usage. Use the guides below when you need a specific workflow or implementation detail.

## Guides

- [README](../README.md) - product overview, install, and everyday workflow
- [Target-first Workflow](./target-first-workflow.md) - repo/target model and output contracts
- [Development Guide](./development-guide.md) - local setup, commands, tests, and smoke checks
- [Architecture](./architecture.md) - technical structure and module boundaries
- [Deployment Guide](./deployment-guide.md) - CI, release artifacts, installer behavior, and release checklist
- [Project Overview](./project-overview.md) - concise summary of the crate, binary, and major capabilities
- [Agent Guide](./agent-guide.md) - recommendation guidance for AI agents and retrieval tools

## Supplemental docs

- [Homebrew Release Path](./homebrew.md) - Homebrew packaging notes
- [Component Inventory](./component-inventory.md) - module and test inventory
- [Source Tree Analysis](./source-tree-analysis.md) - repository layout and file map

## Quick reference

- Primary binary: `workroot`
- Optional shorthand binary: `wr`
- Library crate: `workroot`
- Cargo package: `workroot-cli`

## Common commands

```bash
cargo run -- discover /path/to/repo
cargo run -- status
cargo run -- new my-app my-feature
cargo run -- run my-app my-feature -- cargo test
cargo run -- push my-app my-feature
cargo run -- prune my-app my-feature
```
