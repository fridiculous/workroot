use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::resolver::Resolver;
use crate::storage::FileStorage;

pub fn push_worktree(
    storage: &FileStorage,
    git: &Git,
    repo: &str,
    target: &str,
) -> AppResult<String> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(repo, Some(target))?;

    if resolved.worktree.detached {
        return Err(AppError::InvalidCommand(format!(
            "target `{}` is detached; choose a branch to push first\nfix: run `git -C {} checkout <branch>`\nthen: run `workroot push {} {}`",
            resolved.worktree.target,
            resolved.path.display(),
            resolved.repo.alias,
            resolved.worktree.target,
        )));
    }

    let branch = git.current_branch(&resolved.path)?.ok_or_else(|| {
        AppError::InvalidCommand(format!(
            "target `{}` is detached; choose a branch to push first\nfix: run `git -C {} checkout <branch>`\nthen: run `workroot push {} {}`",
            resolved.worktree.target,
            resolved.path.display(),
            resolved.repo.alias,
            resolved.worktree.target,
        ))
    })?;

    if resolved.worktree.target == "base"
        || resolved.repo.base_branch.as_deref() == Some(branch.as_str())
    {
        let base = resolved
            .repo
            .base_branch
            .as_deref()
            .unwrap_or(branch.as_str());
        return Err(AppError::InvalidCommand(format!(
            "target `{}` is the base branch `{base}`; push feature worktrees instead\nfix: create one with `workroot new {} <worktree>`\nthen: run `workroot push {} <worktree>`",
            resolved.worktree.target, resolved.repo.alias, resolved.repo.alias,
        )));
    }

    let upstream = git.branch_upstream(&resolved.path, &branch)?;
    match upstream {
        Some(upstream) => {
            git.push(&resolved.path)?;
            Ok(format!("pushed `{branch}` to `{upstream}`\n"))
        }
        None => {
            git.push_with_upstream(&resolved.path, "origin", &branch)?;
            Ok(format!(
                "pushed `{branch}` to `origin/{branch}` and set upstream\n"
            ))
        }
    }
}
