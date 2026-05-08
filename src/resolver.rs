use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::PathBuf;

use crate::domain::{Cache, RepoRecord, WorktreeRecord};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorktree {
    pub repo: RepoRecord,
    pub worktree: WorktreeRecord,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Resolver {
    cache: Cache,
}

impl Resolver {
    pub fn new(cache: Cache) -> Self {
        Self { cache }
    }

    pub fn resolve_repo(&self, input: &str) -> AppResult<RepoRecord> {
        let alias_matches: Vec<_> = self
            .cache
            .repos
            .iter()
            .filter(|repo| repo.alias == input)
            .cloned()
            .collect();

        match alias_matches.as_slice() {
            [repo] => return Ok(repo.clone()),
            many if many.len() > 1 => {
                return Err(AppError::AmbiguousRepo {
                    input: input.to_string(),
                    candidates: format_repo_candidates(many),
                });
            }
            _ => {}
        }

        let matches: Vec<_> = self
            .cache
            .repos
            .iter()
            .filter(|repo| repo.display_name == input)
            .cloned()
            .collect();

        match matches.as_slice() {
            [repo] => Ok(repo.clone()),
            [] => Err(AppError::RepoNotFound(input.to_string())),
            many => Err(AppError::AmbiguousRepo {
                input: input.to_string(),
                candidates: format_repo_candidates(many),
            }),
        }
    }

    pub fn resolve_worktree(
        &self,
        repo_input: &str,
        target_input: Option<&str>,
    ) -> AppResult<ResolvedWorktree> {
        let repo = self.resolve_repo(repo_input)?;
        let candidates: Vec<_> = self
            .cache
            .worktrees
            .iter()
            .filter(|worktree| worktree.repo_alias == repo.alias)
            .filter(|worktree| {
                target_input
                    .map(|target| worktree.target == target || worktree.display_name == target)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        match candidates.as_slice() {
            [worktree] => {
                if worktree.stale {
                    return Err(AppError::StaleWorktree(worktree.path.clone()));
                }
                Ok(ResolvedWorktree {
                    repo,
                    path: worktree.path.clone(),
                    worktree: worktree.clone(),
                })
            }
            [] => Err(AppError::TargetNotFound {
                repo: repo.alias,
                target: target_input.unwrap_or("<default>").to_string(),
            }),
            many if target_input.is_none() => {
                let Some(worktree) = pick_target_from_tty(&repo.alias, many)? else {
                    return Err(AppError::AmbiguousTarget {
                        repo: repo.alias,
                        candidates: format_worktree_candidates(many),
                    });
                };
                if worktree.stale {
                    return Err(AppError::StaleWorktree(worktree.path.clone()));
                }
                Ok(ResolvedWorktree {
                    repo,
                    path: worktree.path.clone(),
                    worktree: worktree.clone(),
                })
            }
            many => Err(AppError::AmbiguousTarget {
                repo: repo.alias,
                candidates: format_worktree_candidates(many),
            }),
        }
    }

    pub fn complete_repos(&self, prefix: Option<&str>) -> Vec<String> {
        let prefix = prefix.unwrap_or_default();
        self.cache
            .repos
            .iter()
            .filter(|repo| !repo.stale)
            .filter_map(|repo| {
                first_matching_name([repo.alias.as_str(), repo.display_name.as_str()], prefix)
            })
            .collect()
    }

    pub fn complete_targets(
        &self,
        repo_input: &str,
        prefix: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let repo = self.resolve_repo(repo_input)?;
        let prefix = prefix.unwrap_or_default();
        Ok(self
            .cache
            .worktrees
            .iter()
            .filter(|worktree| worktree.repo_alias == repo.alias && !worktree.stale)
            .filter_map(|worktree| {
                first_matching_name(
                    [worktree.target.as_str(), worktree.display_name.as_str()],
                    prefix,
                )
            })
            .collect())
    }
}

fn format_repo_candidates(repos: &[RepoRecord]) -> String {
    repos
        .iter()
        .map(|repo| format!("{} ({})", repo.alias, repo.canonical_path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn first_matching_name<const N: usize>(names: [&str; N], prefix: &str) -> Option<String> {
    names
        .into_iter()
        .find(|name| name.starts_with(prefix))
        .map(str::to_string)
}

fn format_worktree_candidates(worktrees: &[WorktreeRecord]) -> String {
    worktrees
        .iter()
        .map(|worktree| format!("{} ({})", worktree.target, worktree.path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn pick_target_from_tty(
    repo_alias: &str,
    worktrees: &[WorktreeRecord],
) -> AppResult<Option<WorktreeRecord>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Ok(None);
    }

    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(tty) => tty,
        Err(_) => return Ok(None),
    };
    let mut reader = BufReader::new(tty.try_clone().map_err(|source| {
        AppError::Picker(format!("could not clone /dev/tty handle: {source}"))
    })?);
    let mut writer = tty;

    writeln!(writer, "Select target for repo `{repo_alias}`:")
        .map_err(|source| AppError::Picker(source.to_string()))?;
    for (index, worktree) in worktrees.iter().enumerate() {
        writeln!(
            writer,
            "  {}) {}  {}",
            index + 1,
            worktree.target,
            worktree.path.display()
        )
        .map_err(|source| AppError::Picker(source.to_string()))?;
    }
    write!(writer, "Target number: ").map_err(|source| AppError::Picker(source.to_string()))?;
    writer
        .flush()
        .map_err(|source| AppError::Picker(source.to_string()))?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .map_err(|source| AppError::Picker(source.to_string()))?;
    let selection = input
        .trim()
        .parse::<usize>()
        .map_err(|_| AppError::Picker("expected a target number".to_string()))?;
    worktrees
        .get(selection.saturating_sub(1))
        .cloned()
        .ok_or_else(|| AppError::Picker(format!("target number {selection} is out of range")))
        .map(Some)
}
