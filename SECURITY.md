# Security policy

## Reporting a vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Instead, report them privately by emailing `simon@simonfrid.com` with a subject
line that starts with `Workroot security:`. If GitHub private vulnerability
reporting is enabled for the repository, that channel is also appropriate.

When reporting a vulnerability, please include:
- affected version or commit
- operating system and shell
- exact steps to reproduce
- expected impact
- any proof-of-concept or logs that help reproduce the issue safely

We will try to:
- acknowledge receipt within a reasonable timeframe
- reproduce and assess the issue
- coordinate a fix and disclosure timeline when appropriate

## Scope

Workroot is a local developer CLI that interacts with Git repositories, tmux, and
files on the local machine. Security-sensitive areas include:
- shell integration
- command construction and quoting
- file deletion and prune flows
- path resolution and worktree discovery
- interactions with external tools such as `git`, `tmux`, and optional `gh`

## Supported versions

Because Workroot is pre-1.0, security fixes are expected to land on the latest
mainline release rather than being backported to a long support matrix.
