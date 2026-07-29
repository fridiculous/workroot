# Workroot Roadmap

This roadmap covers the path from v0.0.1 to v0.4.0. It is ordered by dependency:
trust fixes land before the agent lifecycle, visibility lands before repair, and
irreversible deletion lands last, behind recovery infrastructure that is shipped
and verified first.

Each release is independently shippable. Stopping after any release leaves a
coherent tool.

## Principles

- **Never destroy without proof, confirmation, or recovery guidance.** Later
  releases extend the reach of this invariant; none loosen it.
- **Threat model.** Generation markers, ownership records, and quarantine
  journals guard against concurrency and mistakes, not adversaries. Any process
  that can write the repository can forge them. That is acceptable and
  intentional.
- **Authority model.** Git and the filesystem establish worktree existence.
  Provenance records establish Workroot ownership. tmux establishes managed
  session liveness. The cache is always derived state and never proof.
- **Epistemic honesty.** Missing evidence produces `unknown`, never a
  destructive default. Backfilled records say `first_observed`, not `created`.
  Capped counts display as capped (`1000+`), not exact.

## v0.0.2 — Trust and release integrity

Fixes for confirmed defects, plus the identity and release groundwork that
every later release depends on.

### Correctness fixes

- Fix the GitHub merge proof: `gh pr list --head` takes a bare branch name, not
  `owner:branch`. Handle unfetched merge-commit SHAs per PR instead of aborting
  detection.
- Prune: handle per-candidate removal failures (report, count as skipped,
  continue) and save the cache incrementally so a mid-run failure never strands
  already-removed worktrees as phantom records.
- Fix `ignore` ordering: capture the ignored repo's alias and `git_common_dir`
  before filtering `cache.repos`, then purge worktree rows and sessions from
  that exact set.
- Refuse prune when the repo base branch is unknown instead of silently
  comparing against `HEAD`.

### Two-phase locking

- Read-only paths (`status`, `list`) stop taking the exclusive transaction
  lock.
- Interactive and blocking flows follow: snapshot → unlock → prompt or attach →
  relock → reload and revalidate → mutate and save → unlock.
- Revalidation before any destructive step rechecks path, HEAD, dirty state,
  merge proof, live session, and generation marker. Any mismatch refuses.

### Generation markers

- `workroot new` writes a UUID marker file into the worktree's Git
  administrative directory (`.git/worktrees/<id>/`). The marker identifies the
  worktree generation — it does not claim branch ownership.
- Existing worktrees are backfilled with `first_observed_unix` and
  `ownership: unknown`. Backfill never invents `created_unix` or ownership.
- Marker records live in a separately versioned `provenance.json` under the
  state directory, never in the evictable cache.
- Inode and ctime are not identity: a changed value is at most a reason to
  refuse, never authority to act.

### Legacy healing

- One-time repair for caches corrupted by the `ignore` bug: remove worktree
  rows and sessions whose repo is confirmed absent from both cache and state.
- A missing path may be an unmounted volume. Rows are purged only when the
  repository is reachable and Git metadata corroborates absence; otherwise they
  stay stale.

### Release gates

- Tagged release SHAs must be reachable from `origin/main`.
- Formatting, clippy, and the full test suite run natively on Linux and both
  macOS targets against the exact release SHA. Cross-compiling a target is not
  testing it.
- Packaged binaries are executed on a compatible native runner after packaging.
- The installer verifies published SHA256 checksums before extraction.
- crates.io instructions are removed from docs *and* the installer fallback, or
  the crate is published and verified. Docs and installer change together.
- `config.toml` gets a serde default for `schema_version` and a documentation
  page covering `scan_roots`, `default_worktree_root`, and
  `repos.<alias>.base_branch`.

### Documented limitations (accepted until later releases)

- A commit made in a worktree after final revalidation but before removal can
  still be lost; the window is narrow but real. No Workroot recovery mechanism
  exists until v0.3.0 quarantine.
- Worktrees created before v0.0.2 have best-effort identity only.

## v0.1.0 — Agent worktree lifecycle (PR #4)

Amend and merge the detached-by-default worktree lifecycle, capturing at birth
the facts that can never be reconstructed later.

### PR #4 amendments

- Restore idempotency: re-running `workroot new` for an existing detached
  worktree returns its path instead of erroring.
- `merge`, `switch`, and `detach` take the storage transaction like every other
  mutating command.
- `merge` reports conflicts only for conflicts; other failures (unrelated
  histories, lock contention, hooks, mid-rebase destination) get accurate
  diagnostics. Suggested commands quote paths.
- `pr` refuses an unknown base branch instead of defaulting to `main`, verifies
  local HEAD equals the upstream before creating, detects an already-created PR
  on retry, and records the repository slug and PR number.

### Birth records and creation protocol

- Creation is journaled: `PendingCreate` → `git worktree add` → marker write →
  record complete. Markerless worktrees with no matching pending record are
  classified adopted/unknown, never owned.
- Birth record fields: generation UUID, birth HEAD, full branch ref (if any),
  repository identity (`git_common_dir`), whether the branch was newly created,
  and `created_unix`.
- Branch ownership is a separate record written only when Workroot actually
  creates a branch (`new --branch`, `switch -c`). Attaching an existing branch
  never confers ownership.
- `pr` writes a submitted record (`pr_number`, URL, head OID at submit).
  Landed records are written only when a landing is actually observed.

## v0.2.0 — Audit-first reconciler

See all disagreements before repairing any. No destructive remedies in this
release.

- A reconciliation pass enumerates disagreements across Git worktree metadata,
  the filesystem, provenance, the cache, and sessions, producing typed findings
  with evidence under the authority model above.
- Safe repairs only:
  - Stale Git administrative entries, per entry (never repo-wide
    `git worktree prune` when the user declined some entries; `locked` entries
    are never counted stale).
  - Dead cache rows, gated on age and corroboration.
  - Dead sessions, after a tmux liveness check.
- Directories containing a `.git` entry are never offered destructive remedies
  regardless of classification; broken worktrees get repair guidance
  (`git worktree repair`, remount, re-discover).
- Session identity migrates from alias/target to generation and path before any
  automatic cache repair can retarget sessions.
- Provenance runs in shadow mode: record-based merge verdicts are computed and
  compared against forensic proofs (ancestry, GitHub PR, patch-id), with
  divergence logged. Records gain no authority in this release. The record
  check requires both head identity (current HEAD equals recorded source head)
  and reachability of the landed commit from base.

## v0.3.0 — Journaled quarantine

Reversible removal only. Nothing in this release can destroy data.

- Quarantine relocates a worktree with `git worktree move` into a trash area
  and locks it with reason `workroot-trash:v1:<generation-uuid>`.
- The operation is journaled write-ahead: `pending` → `moved` → `locked` →
  `committed`, with the journal entry written before the move. Discovery
  recovers interrupted states deterministically instead of treating a relocated
  worktree as active.
- The lock reason is a signal, not authority: a matching reason without a
  journal entry is repair-only.
- Quarantined becomes a first-class lifecycle state. Discovery, status,
  resolver, scan, new, prune, and branch commands recognize it; trash entries
  never reappear as active targets.
- Eligibility is explicit: the primary worktree, submodule-containing
  worktrees, and externally locked worktrees are refused with reasons.
- `workroot trash list` and `workroot trash restore` ship here. Restore-path
  collisions have defined behavior. `empty` does not ship here.

## v0.4.0 — Permanent deletion

The destructive boundary, crossed only with re-verification and recovery.

- `workroot trash empty` re-verifies before destroying: generation, HEAD,
  dirty state, session state, and changes made after quarantine. Detached
  HEADs get rescue refs (`refs/workroot/trash/...`) before removal. Dirty or
  unknown-state worktrees are refused, or snapshotted behind an explicit
  second confirmation.
- Owned-branch deletion requires all of: Workroot created the branch, the
  exact full ref still exists, it is not checked out anywhere, its current tip
  equals the proven source head, and the merge proof remains valid. `-d` for
  ancestry-proven branches; rescue ref then `-D` for squash- or PR-proven
  branches. Unowned branches are report-only.
- Abandoned-worktree cleanup uses labeled signals: last HEAD movement from the
  reflog (which is not user activity and may be expired or disabled), commits
  ahead of base, and unpushed commits via `rev-list --count HEAD --not
  --remotes` with a zero-remote guard and capped display. Missing evidence is
  `unknown`, and unknown is never abandoned.
- Trash lifecycle: `trash empty --older-than <days>`, with rescue refs and
  trash entries expiring through the same confirmed, evidence-first flow.

## Parallel track (no dependencies)

- Fix the duplicate `[[bin]]` warning (both binaries point at `src/main.rs`).
- Update `docs/gemini-code-review.md` to reflect current review configuration.
- Delete merged branches; keep `main` current.
- Homebrew tap (already noted in README as planned; unscheduled).

## Release process

Releases are tag-triggered (`v*`) with a manual `workflow_dispatch` fallback.
Each release here corresponds to a Cargo.toml version bump, a tag on a commit
reachable from `origin/main`, and the gates listed under v0.0.2.
