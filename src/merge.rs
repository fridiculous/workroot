use std::process::Command;

use crate::domain::{Cache, RepoRecord, WorktreeRecord};
use crate::error::{AppError, AppResult};
use crate::git::{Git, canonical_or_self};
use crate::resolver::Resolver;
use crate::storage::FileStorage;

pub fn merge_worktree(
    storage: &FileStorage,
    git: &Git,
    repo: &str,
    source_target: &str,
    into_branch: &str,
) -> AppResult<String> {
    let cache = storage.load_cache()?;
    let resolved = Resolver::new(cache.clone()).resolve_worktree(repo, Some(source_target))?;
    if git.is_dirty(&resolved.path)? {
        return Err(AppError::InvalidCommand(format!(
            "source target `{}` has uncommitted changes; commit or stash before merging",
            resolved.worktree.target
        )));
    }

    let destination = destination_worktree(&cache, &resolved.repo, into_branch)?;
    if same_path(&resolved.path, &destination.path) {
        return Err(AppError::InvalidCommand(format!(
            "source target `{}` is already the destination branch `{into_branch}`",
            resolved.worktree.target
        )));
    }
    if git.is_dirty(&destination.path)? {
        return Err(AppError::InvalidCommand(format!(
            "destination branch `{into_branch}` has uncommitted changes at {}; commit or stash before merging",
            destination.path.display()
        )));
    }

    let source_head = git
        .rev_parse(&resolved.path, "HEAD")?
        .ok_or_else(|| AppError::Git(format!("could not resolve HEAD for `{source_target}`")))?;
    let output = Command::new(git.executable())
        .arg("-C")
        .arg(&destination.path)
        .args(["merge", &source_head])
        .output()
        .map_err(|_| AppError::MissingDependency { name: "git" })?;

    if !output.status.success() {
        let details = merge_failure_details(&output);
        return Err(AppError::InvalidCommand(format!(
            "merge of `{}` into `{into_branch}` has conflicts at {}\nfix: resolve conflicts there, then commit or run `git -C {} merge --abort`{}",
            resolved.worktree.target,
            destination.path.display(),
            destination.path.display(),
            details
        )));
    }

    crate::discovery::refresh_family(storage, git, &resolved.repo.canonical_path)?;
    Ok(format!(
        "merged `{}` into `{into_branch}` at {}\n",
        resolved.worktree.target,
        destination.path.display()
    ))
}

fn destination_worktree<'a>(
    cache: &'a Cache,
    repo: &RepoRecord,
    branch: &str,
) -> AppResult<&'a WorktreeRecord> {
    cache
        .worktrees
        .iter()
        .find(|worktree| {
            worktree.repo_alias == repo.alias
                && !worktree.stale
                && worktree.branch.as_deref() == Some(branch)
        })
        .ok_or_else(|| {
            AppError::InvalidCommand(format!(
                "destination branch `{branch}` is not checked out in a known Workroot worktree\nfix: run `workroot new {} <target> --branch {branch}`",
                repo.alias
            ))
        })
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    canonical_or_self(left) == canonical_or_self(right)
}

fn merge_failure_details(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("\ngit output: {stdout}"),
        (true, false) => format!("\ngit output: {stderr}"),
        (false, false) => format!("\ngit output: {stdout}\n{stderr}"),
    }
}
