# Workroot Component Inventory

This inventory maps the main runtime modules, scripts, and test suites in the repository.

## Runtime modules

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Binary entry point and exit handling |
| `src/lib.rs` | Library exports and public re-exports |
| `src/cli.rs` | Clap model, help text, and command dispatch |
| `src/domain.rs` | Persisted config/state/cache schema |
| `src/storage.rs` | XDG path resolution, locking, atomic writes |
| `src/error.rs` | Typed app errors and exit-code mapping |
| `src/git.rs` | Git CLI adapter and worktree parsing |
| `src/discovery.rs` | Adopt, scan, list/path, and new-worktree behavior |
| `src/resolver.rs` | Repo/target resolution and ambiguity handling |
| `src/session.rs` | tmux-backed session lifecycle and command quoting |
| `src/status.rs` | Status/radar rendering and refresh behavior |
| `src/prune.rs` | Prune reporting and interactive cleanup |
| `src/lineage.rs` | Merge-proof logic for safe prune behavior |
| `src/shell.rs` | Shell integration for zsh, bash, and fish |
| `src/push.rs` | Upstream-aware push behavior for worktrees |

## Operational scripts

| Script | Responsibility |
| --- | --- |
| `install.sh` | Public installer with release/source fallback |
| `scripts/install-workroot-shell.sh` | Local install and shell-integration helper |

## Test suites

| Test file | Coverage area |
| --- | --- |
| `tests/core_contracts.rs` | CLI parsing, help, shell snippets, storage and resolver contracts |
| `tests/git_discovery.rs` | Adopt/scan/new behavior and worktree discovery rules |
| `tests/resolver_shell_integration.rs` | Path output, ambiguity, completions, shell integration |
| `tests/session_contracts.rs` | Foreground execution and tmux-backed session behavior |
| `tests/status_release.rs` | Status/radar output, refresh, and session visibility |
| `tests/prune.rs` | Prune proof, prompting, and safe removal |
| `tests/push.rs` | Push behavior and upstream handling |
| `tests/stdio_contracts.rs` | Broken-pipe and stdout robustness |
| `tests/support/mod.rs` | Shared test helpers |

## Notes

- The codebase is organized by responsibility rather than by command.
- External boundaries are mostly Git, tmux, shell startup files, and optional `gh`.
- Public command behavior should stay aligned with tests, help text, and README examples.
