use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::domain::{Cache, RepoRecord, WorktreeRecord};
use crate::error::AppResult;
use crate::git::Git;
use crate::lineage::detect_lineage;
use crate::resolver::Resolver;
use crate::storage::FileStorage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneRow {
    pub repo: String,
    pub target: String,
    pub base: String,
    pub head: String,
    pub state: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedWorktreeCandidate {
    pub repo: String,
    pub target: String,
    pub base_branch: String,
    pub worktree_branch: String,
    pub base_summary: String,
    pub head_summary: String,
    pub proof: String,
    pub evidence: Vec<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorktreeFilter {
    repo_alias: String,
    target: Option<String>,
}

pub fn prune_report(storage: &FileStorage, git: &Git) -> AppResult<String> {
    let cache = storage.load_cache()?;
    Ok(render_rows(prune_rows(&cache, git)))
}

pub fn prune_merged_interactive(
    storage: &FileStorage,
    git: &Git,
    input: &mut impl BufRead,
    output: &mut impl Write,
    repo: Option<&str>,
    target: Option<&str>,
) -> AppResult<()> {
    let _transaction = storage.transaction()?;
    let mut cache = storage.load_cache()?;
    let filter = worktree_filter(&cache, repo, target)?;
    let candidates = merged_worktree_candidates_filtered(&cache, git, filter.as_ref());

    if candidates.is_empty() {
        writeln!(output, "No merged worktrees found.")?;
        return Ok(());
    }

    let repos = cache
        .repos
        .iter()
        .map(|repo| (repo.alias.as_str(), repo))
        .collect::<BTreeMap<_, _>>();
    let mut removed = Vec::new();
    let mut skipped = 0usize;

    for candidate in candidates {
        writeln!(output, "{} {}", candidate.repo, candidate.target)?;
        writeln!(
            output,
            "  trunk  {}: {}",
            candidate.base_branch, candidate.base_summary
        )?;
        writeln!(
            output,
            "  branch {}: {}",
            candidate.worktree_branch, candidate.head_summary
        )?;
        writeln!(output, "  proof: {}", candidate.proof)?;
        for evidence in &candidate.evidence {
            writeln!(output, "    - {evidence}")?;
        }
        writeln!(output, "  path: {}", candidate.path.display())?;
        write!(output, "Remove this worktree? [y/N] ")?;
        output.flush()?;

        let mut answer = String::new();
        input.read_line(&mut answer)?;
        let answer = answer.trim();
        if !matches!(answer, "y" | "Y" | "yes" | "YES" | "Yes") {
            skipped += 1;
            writeln!(output, "skipped")?;
            continue;
        }

        let Some(repo) = repos.get(candidate.repo.as_str()) else {
            writeln!(output, "skipped: repo missing from cache")?;
            skipped += 1;
            continue;
        };

        git.remove_worktree(&repo.canonical_path, &candidate.path)?;
        cache.worktrees.retain(|worktree| {
            !(worktree.repo_alias == candidate.repo
                && worktree.target == candidate.target
                && worktree.path == candidate.path)
        });
        removed.push(candidate);
        writeln!(output, "removed")?;
    }

    storage.save_cache(&cache)?;
    writeln!(
        output,
        "Removed {} merged worktree(s); skipped {}.",
        removed.len(),
        skipped
    )?;
    Ok(())
}

pub fn prune_rows(cache: &Cache, git: &Git) -> Vec<PruneRow> {
    let repos = cache
        .repos
        .iter()
        .map(|repo| (repo.alias.as_str(), repo))
        .collect::<BTreeMap<_, _>>();

    let mut rows = cache
        .worktrees
        .iter()
        .map(|worktree| {
            let repo = repos.get(worktree.repo_alias.as_str()).copied();
            prune_row(repo, worktree, git)
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        left.repo
            .cmp(&right.repo)
            .then(left.target.cmp(&right.target))
            .then(left.path.cmp(&right.path))
    });
    rows
}

pub fn merged_worktree_candidates(cache: &Cache, git: &Git) -> Vec<MergedWorktreeCandidate> {
    merged_worktree_candidates_filtered(cache, git, None)
}

fn merged_worktree_candidates_filtered(
    cache: &Cache,
    git: &Git,
    filter: Option<&WorktreeFilter>,
) -> Vec<MergedWorktreeCandidate> {
    let repos = cache
        .repos
        .iter()
        .map(|repo| (repo.alias.as_str(), repo))
        .collect::<BTreeMap<_, _>>();

    let mut candidates = cache
        .worktrees
        .iter()
        .filter(|worktree| matches_filter(worktree, filter))
        .filter_map(|worktree| {
            let repo = repos.get(worktree.repo_alias.as_str()).copied()?;
            merged_worktree_candidate(repo, worktree, git)
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        left.repo
            .cmp(&right.repo)
            .then(left.target.cmp(&right.target))
            .then(left.path.cmp(&right.path))
    });
    candidates
}

fn worktree_filter(
    cache: &Cache,
    repo: Option<&str>,
    target: Option<&str>,
) -> AppResult<Option<WorktreeFilter>> {
    let Some(repo_input) = repo else {
        return Ok(None);
    };
    let resolver = Resolver::new(cache.clone());
    if let Some(target_input) = target {
        let resolved = resolver.resolve_worktree(repo_input, Some(target_input))?;
        return Ok(Some(WorktreeFilter {
            repo_alias: resolved.repo.alias,
            target: Some(resolved.worktree.target),
        }));
    }
    let repo = resolver.resolve_repo(repo_input)?;
    Ok(Some(WorktreeFilter {
        repo_alias: repo.alias,
        target: None,
    }))
}

fn matches_filter(worktree: &WorktreeRecord, filter: Option<&WorktreeFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    worktree.repo_alias == filter.repo_alias
        && filter
            .target
            .as_deref()
            .map(|target| worktree.target == target)
            .unwrap_or(true)
}

fn merged_worktree_candidate(
    repo: &RepoRecord,
    worktree: &WorktreeRecord,
    git: &Git,
) -> Option<MergedWorktreeCandidate> {
    let base_branch = repo.base_branch.as_deref().unwrap_or("HEAD");
    if repo.stale
        || !repo.canonical_path.exists()
        || worktree.stale
        || !worktree.path.exists()
        || same_path(&repo.canonical_path, &worktree.path)
        || worktree.branch.as_deref() == Some(base_branch)
    {
        return None;
    }

    let base_commit = git
        .rev_parse(&repo.canonical_path, base_branch)
        .ok()
        .flatten()?;
    let head_commit = git.rev_parse(&worktree.path, "HEAD").ok().flatten()?;
    let branch_name = worktree.branch.as_deref();
    let lineage = detect_lineage(
        git,
        &repo.canonical_path,
        base_branch,
        branch_name,
        &head_commit,
    )
    .ok()?;
    if !lineage.is_prune_safe() {
        return None;
    }

    Some(MergedWorktreeCandidate {
        repo: worktree.repo_alias.clone(),
        target: worktree.target.clone(),
        base_branch: base_branch.to_string(),
        worktree_branch: worktree
            .branch
            .clone()
            .unwrap_or_else(|| "detached".to_string()),
        base_summary: git
            .commit_summary(&repo.canonical_path, &base_commit)
            .ok()
            .flatten()
            .unwrap_or_else(|| short(&base_commit)),
        head_summary: git
            .commit_summary(&worktree.path, &head_commit)
            .ok()
            .flatten()
            .unwrap_or_else(|| short(&head_commit)),
        proof: lineage.proof_label().to_string(),
        evidence: lineage.evidence,
        path: worktree.path.clone(),
    })
}

fn prune_row(repo: Option<&RepoRecord>, worktree: &WorktreeRecord, git: &Git) -> PruneRow {
    let Some(repo) = repo else {
        return row(worktree, "-", "-", "missing-repo");
    };
    let base_branch = repo.base_branch.as_deref().unwrap_or("HEAD");
    let base_label = base_branch.to_string();

    if repo.stale || !repo.canonical_path.exists() || worktree.stale || !worktree.path.exists() {
        return row(worktree, &base_label, "-", "stale");
    }

    let Some(base_commit) = git
        .rev_parse(&repo.canonical_path, base_branch)
        .ok()
        .flatten()
    else {
        return row(worktree, &base_label, "-", "base-missing");
    };
    let Some(head_commit) = git.rev_parse(&worktree.path, "HEAD").ok().flatten() else {
        return row(worktree, &base_label, "-", "head-missing");
    };
    let base_in_head = match git.is_ancestor(&repo.canonical_path, &base_commit, &head_commit) {
        Ok(value) => value,
        Err(_) => {
            return row(
                worktree,
                &base_label,
                &short(&head_commit),
                "compare-failed",
            );
        }
    };
    let head_in_base = match git.is_ancestor(&repo.canonical_path, &head_commit, &base_commit) {
        Ok(value) => value,
        Err(_) => {
            return row(
                worktree,
                &base_label,
                &short(&head_commit),
                "compare-failed",
            );
        }
    };
    let state = if base_in_head {
        "fresh"
    } else if head_in_base {
        "behind-base"
    } else {
        "diverged-base"
    };
    row(
        worktree,
        &format!("{}@{}", base_label, short(&base_commit)),
        &short(&head_commit),
        state,
    )
}

fn row(worktree: &WorktreeRecord, base: &str, head: &str, state: &str) -> PruneRow {
    PruneRow {
        repo: worktree.repo_alias.clone(),
        target: worktree.target.clone(),
        base: base.to_string(),
        head: head.to_string(),
        state: state.to_string(),
        path: worktree.path.display().to_string(),
    }
}

fn short(commit: &str) -> String {
    commit.chars().take(8).collect()
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}

fn render_rows(rows: Vec<PruneRow>) -> String {
    let mut table = vec![vec![
        "REPO".to_string(),
        "TARGET".to_string(),
        "BASE".to_string(),
        "HEAD".to_string(),
        "STATE".to_string(),
        "PATH".to_string(),
    ]];

    for row in rows {
        table.push(vec![
            row.repo, row.target, row.base, row.head, row.state, row.path,
        ]);
    }

    render_table(table)
}

fn render_table(rows: Vec<Vec<String>>) -> String {
    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0; column_count];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    let mut output = String::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                output.push_str("  ");
            }
            output.push_str(cell);
            if index + 1 < row.len() {
                output.push_str(&" ".repeat(widths[index] - cell.len()));
            }
        }
        output.push('\n');
    }
    output
}
