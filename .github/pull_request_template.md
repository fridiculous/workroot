<!--
Use a Conventional Commit title for this PR and for the first commit on the branch.
Examples: feat: add worktree ignore command, fix: keep path output stdout-only, docs: clarify shell integration setup.
This keeps squash-merge commit titles predictable.
-->

## Summary

Describe what changed and why.

## User-facing impact

- [ ] No user-facing change
- [ ] CLI behavior changed
- [ ] Help text changed
- [ ] Docs changed

## Verification

Paste the commands you ran:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Checklist

- [ ] Tests added or updated when behavior changed
- [ ] README/help/docs updated if the public contract changed
- [ ] Output contracts were considered for stdout vs stderr
- [ ] PR title and first commit use Conventional Commits format
