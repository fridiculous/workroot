# Workroot Architecture

Workroot is a small Rust CLI organized around a narrow command surface, typed persisted state, and explicit adapters to external tools like Git and tmux.

## Runtime shape

```text
user
  -> workroot binary (src/main.rs)
  -> CLI router (src/cli.rs)
     -> storage (src/storage.rs)
     -> discovery (src/discovery.rs)
     -> resolver (src/resolver.rs)
     -> status (src/status.rs)
     -> session (src/session.rs)
     -> prune (src/prune.rs)
     -> shell (src/shell.rs)
```

## Core ideas

- keep the public CLI small and teachable
- preserve path-only stdout for navigation-oriented commands
- expose JSON only as an explicit opt-in output format
- store machine-wide config, state, and cache outside individual repos
- use Git as the source of truth for worktree reality
- treat destructive cleanup conservatively

## Main modules

- `src/cli.rs` - clap definitions, help text, and command dispatch
- `src/domain.rs` - persisted config/state/cache types
- `src/storage.rs` - path resolution, locking, schema checks, atomic writes
- `src/discovery.rs` - adopt, scan, list/path, and new-worktree behavior
- `src/resolver.rs` - repo/target lookup and ambiguity handling
- `src/status.rs` - human-readable status and refresh behavior
- `src/session.rs` - tmux-backed managed sessions and command quoting
- `src/prune.rs` - prune reporting and interactive cleanup
- `src/lineage.rs` - merge-proof logic for safe prune behavior
- `src/shell.rs` - shell integration snippets for zsh, bash, and fish

## Persisted state

Workroot keeps three storage roots:
- config
- state
- cache

Resolution order is:
1. Workroot-specific env vars
2. XDG env vars
3. home-directory defaults

Primary env vars:
- `WORKROOT_CONFIG_HOME`
- `WORKROOT_STATE_HOME`
- `WORKROOT_CACHE_HOME`

Legacy pre-rename env vars are still supported as migration fallbacks.

## External integrations

- `git` is required for worktree discovery, branch checks, and cleanup
- `tmux` is used for managed-session workflows where enabled
- optional `gh` can improve prune lineage proof
- shell startup files are updated only by install scripts, not by the Rust runtime itself

## Command flow examples

`workroot new <repo> <target>`:
- resolve the repo
- choose/create the branch
- create the worktree under the configured root
- refresh cached discovery
- return the new path for shell integration, or structured worktree JSON with `-o json`

`workroot push <repo> <target>`:
- resolve the worktree
- verify the branch is safe to push
- push with `-u origin <branch>` on first push
- use normal `git push` once upstream exists
- return a human message by default, or structured push JSON with `-o json`

`workroot prune [repo] [target]`:
- collect candidate worktrees
- exclude base worktrees and unsafe cases
- require merge proof before removal
- show evidence before interactive deletion

## Testing strategy

The repo relies heavily on integration-style tests with real temporary Git repositories.

Important suites:
- `tests/core_contracts.rs`
- `tests/git_discovery.rs`
- `tests/resolver_shell_integration.rs`
- `tests/session_contracts.rs`
- `tests/status_release.rs`
- `tests/prune.rs`
- `tests/push.rs`
- `tests/stdio_contracts.rs`

## Constraints

- shell and tool behavior depend on the installed versions of Git, tmux, and shells
- status and session behavior must degrade cleanly when optional tools are absent
- prune behavior should remain conservative even as features grow
- public help, README copy, and tests should stay aligned
