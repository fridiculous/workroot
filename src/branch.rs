use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::resolver::Resolver;
use crate::storage::FileStorage;

pub fn branch_worktree(
    storage: &FileStorage,
    git: &Git,
    repo: &str,
    target: &str,
    branch: &str,
) -> AppResult<String> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(repo, Some(target))?;

    if let Some(current) = git.current_branch(&resolved.path)? {
        return Err(AppError::InvalidCommand(format!(
            "target `{}` is already attached to branch `{}`",
            resolved.worktree.target, current
        )));
    }

    if let Some(path) = git.branch_checked_out_path(&resolved.repo.canonical_path, branch)? {
        return Err(AppError::InvalidCommand(format!(
            "branch `{branch}` is already checked out at {}; refusing to attach it here",
            path.display()
        )));
    }

    if git.branch_exists(&resolved.repo.canonical_path, branch)? {
        let head = git.rev_parse(&resolved.path, "HEAD")?;
        let branch_head = git.rev_parse(&resolved.repo.canonical_path, branch)?;
        if head.is_some() && head == branch_head {
            git.switch_branch(&resolved.path, branch)?;
        } else {
            return Err(AppError::InvalidCommand(format!(
                "branch `{branch}` already exists at a different commit; choose a new branch name"
            )));
        }
    } else {
        git.switch_create_branch(&resolved.path, branch)?;
    }
    refresh(storage, git, &resolved.repo.canonical_path)?;
    Ok(format!(
        "branched `{}` as `{branch}`\n",
        resolved.worktree.target
    ))
}

pub fn detach_worktree(
    storage: &FileStorage,
    git: &Git,
    repo: &str,
    target: &str,
) -> AppResult<String> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(repo, Some(target))?;

    if git.current_branch(&resolved.path)?.is_none() {
        return Ok(format!(
            "target `{}` is already detached\n",
            resolved.worktree.target
        ));
    }
    if resolved.worktree.target == "base" {
        return Err(AppError::InvalidCommand(
            "refusing to detach the base worktree".to_string(),
        ));
    }
    if git.is_dirty(&resolved.path)? {
        return Err(AppError::InvalidCommand(format!(
            "target `{}` has uncommitted changes; commit or stash before detaching",
            resolved.worktree.target
        )));
    }

    git.switch_detach(&resolved.path)?;
    refresh(storage, git, &resolved.repo.canonical_path)?;
    Ok(format!("detached `{}`\n", resolved.worktree.target))
}

fn refresh(storage: &FileStorage, git: &Git, repo_path: &std::path::Path) -> AppResult<()> {
    crate::discovery::refresh_family(storage, git, repo_path)
}
