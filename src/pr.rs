use std::process::Command;

use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::resolver::Resolver;
use crate::storage::FileStorage;

pub fn create_pr(storage: &FileStorage, git: &Git, repo: &str, target: &str) -> AppResult<String> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(repo, Some(target))?;

    if resolved.worktree.detached {
        return Err(AppError::InvalidCommand(format!(
            "target `{}` is detached; create a branch first\nfix: run `workroot switch {} {} -c <branch>`\nthen: run `workroot push {} {}`\nthen: run `workroot pr {} {}`",
            resolved.worktree.target,
            resolved.repo.alias,
            resolved.worktree.target,
            resolved.repo.alias,
            resolved.worktree.target,
            resolved.repo.alias,
            resolved.worktree.target,
        )));
    }

    let branch = git.current_branch(&resolved.path)?.ok_or_else(|| {
        AppError::InvalidCommand(format!(
            "target `{}` is detached; create a branch first\nfix: run `workroot switch {} {} -c <branch>`",
            resolved.worktree.target, resolved.repo.alias, resolved.worktree.target,
        ))
    })?;
    let upstream = git.branch_upstream(&resolved.path, &branch)?;
    if upstream.is_none() {
        return Err(AppError::InvalidCommand(format!(
            "branch `{branch}` has no upstream\nfix: run `workroot push {} {}` first",
            resolved.repo.alias, resolved.worktree.target,
        )));
    }
    let base = resolved
        .repo
        .base_branch
        .as_deref()
        .unwrap_or("main")
        .to_string();

    let output = Command::new("gh")
        .current_dir(&resolved.path)
        .args(["pr", "create", "--base", &base, "--head", &branch])
        .output()
        .map_err(|_| AppError::MissingDependency { name: "gh" })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::CommandFailed(if stderr.is_empty() {
            "gh pr create".to_string()
        } else {
            stderr
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok(format!("created PR for `{branch}`\n"))
    } else {
        Ok(format!("{stdout}\n"))
    }
}
