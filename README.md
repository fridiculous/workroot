<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./workroot_logo_dark_1022x382.png">
    <source media="(prefers-color-scheme: light)" srcset="./workroot_logo_light_1044x390.png">
    <img src="./workroot_logo_light_1044x390.png" alt="Workroot logo" width="240" />
  </picture>
</p>

# Workroot

Workroot is the machine-wide switchboard for git worktrees.

Use it when you work across multiple repos or many `git worktree` checkouts and want one small CLI for finding, creating, entering, running, pushing, and pruning them.

## Quick start

Install Workroot:

```bash
curl -fsSL https://raw.githubusercontent.com/fridiculous/workroot/main/install.sh | bash
```

Discover a repo, inspect status, and create a detached target:

```bash
workroot discover /path/to/repo
workroot status
cd "$(workroot new my-repo my-target)"
```

Optional shorthand:
- `wr` is supported as a short alias for `workroot`

All examples use `workroot` for clarity; use `wr` anywhere you want the shorter command.

## 20-second demo

```bash
workroot discover ~/projects/workroot
workroot status
cd "$(workroot new workroot public-launch)"
workroot run workroot public-launch -- cargo test
workroot switch workroot public-launch -c feat/public-launch
workroot push workroot public-launch
# After the branch is merged:
workroot prune workroot public-launch
```

## Core workflow

1. Discover a repo once.
2. Check your machine-wide worktree status.
3. Create or enter a named target worktree. New targets are detached by default.
4. Run commands in that target.
5. Create a branch when the work is worth reviewing, or merge it into an existing branch worktree.
6. Push the target branch when using the review path.
7. Prune it after Workroot proves it was merged.

A target is one unit of work: one worktree path, one status row, and one optional managed session. It may be detached or attached to a branch.

## Command map

| Need | Command |
| --- | --- |
| Index repos | `workroot discover [path]` |
| See known worktrees | `workroot status [repo] [target]` |
| Create a detached target worktree | `workroot new <repo> <target>` |
| Create an attached branch worktree | `workroot new <repo> <target> --branch [branch]` |
| Switch a detached target to a branch | `workroot switch <repo> <target> -c <branch>` |
| Merge a target into a branch worktree | `workroot merge <repo> <target> --into <branch>` |
| Detach a branch-backed target | `workroot detach <repo> <target>` |
| Print a target path | `workroot path <repo> [target]` |
| Change directory through shell integration | `workroot cd <repo> [target]` |
| Run a command in a target | `workroot run <repo> <target> -- <cmd...>` |
| Push a target branch | `workroot push <repo> <target>` |
| Create a GitHub PR | `workroot pr <repo> <target>` |
| Remove merged targets safely | `workroot prune [repo] [target]` |
| Install shell integration | `workroot shell-init <shell>` |

## Install

Today:

```bash
curl -fsSL https://raw.githubusercontent.com/fridiculous/workroot/main/install.sh | bash
```

Cargo:

```bash
cargo install workroot-cli
```

Homebrew tap support is planned but not live yet.

## Shell integration

```bash
eval "$(workroot shell-init zsh)"
eval "$(workroot shell-init bash)"
workroot shell-init fish | source
```

Shell integration also defines `wr` as a shorthand for `workroot`.

`workroot cd` needs shell integration because a child process cannot change the parent shell directory.

## Output contract

```bash
cd "$(workroot path workroot public-launch)"
cd "$(workroot new workroot docs)"
```

`workroot path` and direct `workroot new` print path-only stdout.

## Scope

Workroot is not:
- a Git hosting tool
- a project management tool
- a replacement for Git
- a general tmux or session manager

It wraps local Git workflows. tmux support exists for managed command sessions, but terminal management is not the main product.

## Docs

- [Agent guide](./docs/agent-guide.md)
- [Target-first workflow](./docs/target-first-workflow.md)
- [Development guide](./docs/development-guide.md)
- [Documentation index](./docs/index.md)

## License

MIT
