# Target-first workflow

Workroot uses a target-first workflow for git worktree management.

## Core model

- repo = a stable project family
- target = one unit of work
- one target maps to one branch, one worktree path, one session identity, and one status row

This keeps naming, navigation, execution, and cleanup aligned.

## First-time setup

Index an existing repo or worktree family before using repo and target names:

```bash
workroot discover /path/to/repo
```

## Lifecycle

1. Inspect the machine-wide portfolio

```bash
workroot status
workroot status <repo>
workroot status <repo> <target>
```

2. Spawn a target from the repo base branch

```bash
workroot new <repo> <target>
cd "$(workroot new <repo> <target>)"
```

`workroot new` prints the created path to stdout so it composes with shell command substitution.

3. Navigate to an existing target

```bash
workroot path <repo> <target>
cd "$(workroot path <repo> <target>)"
```

If shell integration is installed, you can also use:

```bash
workroot cd <repo> <target>
```

4. Run inside the target

```bash
workroot run <repo> <target> -- <cmd...>
```

Use this when you want Workroot to manage the tmux session identity for a target.

5. Publish the target

```bash
workroot push <repo> <target>
```

Workroot pushes with `git push -u origin <branch>` on first push, then uses normal `git push` once upstream exists.

6. Prune merged targets

```bash
workroot prune
workroot prune <repo>
workroot prune <repo> <target>
```

Workroot only removes worktrees after merge proof and interactive confirmation.

## Base is special

The repo base worktree is not ordinary feature work.

- use it as the stable source of truth
- create new work from it with `workroot new`
- avoid treating `base` as just another target to push and prune casually

## Output contracts

Path-only stdout:

```bash
workroot path <repo> <target>
workroot new <repo> <target>
```

This enables shell-native composition:

```bash
cd "$(workroot path <repo> <target>)"
cd "$(workroot new <repo> <target>)"
```

Human-oriented commands:

```bash
workroot status
workroot run <repo> <target> -- <cmd...>
workroot push <repo> <target>
workroot prune [repo] [target]
```

## Why this model works

This target-first model gives Workroot a clean, opinionated workflow without becoming a giant repo-local control plane:

- small public command surface
- global visibility across repos
- predictable naming
- shell composability
- optional tmux integration
- conservative cleanup
