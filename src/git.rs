use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreeEntry {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    pub top_level: PathBuf,
    pub common_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Git {
    executable: OsString,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            executable: OsString::from("git"),
        }
    }
}

impl Git {
    pub fn new(executable: impl Into<OsString>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn executable(&self) -> &OsString {
        &self.executable
    }

    pub fn verify(&self, path: &Path) -> AppResult<GitRepository> {
        let output = self.git(path, ["rev-parse", "--git-common-dir", "--show-toplevel"])?;
        let mut lines = output.lines();
        let common = lines
            .next()
            .ok_or_else(|| AppError::Git("rev-parse did not return git common dir".to_string()))?;
        let top = lines
            .next()
            .ok_or_else(|| AppError::Git("rev-parse did not return top level".to_string()))?;
        let top_level = PathBuf::from(top);
        let common_dir = absolutize_git_path(path, common);

        Ok(GitRepository {
            top_level: canonical_or_self(&top_level),
            common_dir: canonical_or_self(&common_dir),
        })
    }

    pub fn worktrees(&self, path: &Path) -> AppResult<Vec<GitWorktreeEntry>> {
        parse_worktree_porcelain(&self.git(path, ["worktree", "list", "--porcelain"])?)
    }

    pub fn current_branch(&self, path: &Path) -> AppResult<Option<String>> {
        let output = self.git(path, ["branch", "--show-current"])?;
        let branch = output.trim();
        if branch.is_empty() {
            Ok(None)
        } else {
            Ok(Some(branch.to_string()))
        }
    }

    pub fn rev_parse(&self, path: &Path, rev: &str) -> AppResult<Option<String>> {
        let output = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "--verify", rev])
            .output()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        if output.status.success() {
            let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok((!commit.is_empty()).then_some(commit))
        } else {
            Ok(None)
        }
    }

    pub fn is_ancestor(&self, path: &Path, ancestor: &str, descendant: &str) -> AppResult<bool> {
        let status = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .status()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(AppError::Git(format!(
                "failed to compare commits `{ancestor}` and `{descendant}`"
            ))),
        }
    }

    pub fn cherry(&self, path: &Path, upstream: &str, head: &str) -> AppResult<Vec<CherryMark>> {
        let output = self.git(path, ["cherry", upstream, head])?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let mark = parts.next()?;
                let commit = parts.next()?;
                let applied = match mark {
                    "-" => true,
                    "+" => false,
                    _ => return None,
                };
                Some(CherryMark {
                    applied,
                    commit: commit.to_string(),
                })
            })
            .collect())
    }

    pub fn remote_url(&self, path: &Path, remote: &str) -> AppResult<Option<String>> {
        let output = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["remote", "get-url", remote])
            .output()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok((!url.is_empty()).then_some(url))
        } else {
            Ok(None)
        }
    }

    pub fn commit_summary(&self, path: &Path, rev: &str) -> AppResult<Option<String>> {
        let output = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["log", "-1", "--format=%h %s", rev])
            .output()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        if output.status.success() {
            let summary = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok((!summary.is_empty()).then_some(summary))
        } else {
            Ok(None)
        }
    }

    pub fn branch_exists(&self, path: &Path, branch: &str) -> AppResult<bool> {
        let status = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["show-ref", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .status()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(AppError::Git(format!(
                "failed to check whether branch `{branch}` exists"
            ))),
        }
    }

    pub fn remote_default_branch(&self, path: &Path) -> AppResult<Option<String>> {
        let output = self.git(
            path,
            [
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
        );
        match output {
            Ok(value) => Ok(value
                .trim()
                .strip_prefix("origin/")
                .map(str::to_string)
                .filter(|branch| !branch.is_empty())),
            Err(AppError::Git(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn branch_upstream(&self, path: &Path, branch: &str) -> AppResult<Option<String>> {
        let output = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "--abbrev-ref"])
            .arg(format!("{branch}@{{upstream}}"))
            .output()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        if output.status.success() {
            let upstream = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok((!upstream.is_empty()).then_some(upstream))
        } else {
            Ok(None)
        }
    }

    pub fn is_dirty(&self, path: &Path) -> AppResult<bool> {
        let output = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(["status", "--porcelain"])
            .output()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        if output.status.success() {
            Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(AppError::Git(if stderr.is_empty() {
                format!("failed to inspect dirty state in {}", path.display())
            } else {
                stderr
            }))
        }
    }

    pub fn pull_ff_only(&self, path: &Path) -> AppResult<()> {
        self.git(path, ["pull", "--ff-only"]).map(|_| ())
    }

    pub fn pull_ff_only_from(&self, path: &Path, remote: &str, branch: &str) -> AppResult<()> {
        self.git(path, ["pull", "--ff-only", remote, branch])
            .map(|_| ())
    }

    pub fn push(&self, path: &Path) -> AppResult<()> {
        self.git(path, ["push"]).map(|_| ())
    }

    pub fn push_with_upstream(&self, path: &Path, remote: &str, branch: &str) -> AppResult<()> {
        self.git(path, ["push", "-u", remote, branch]).map(|_| ())
    }

    pub fn create_branch(&self, path: &Path, branch: &str, base: &str) -> AppResult<()> {
        self.git(path, ["branch", branch, base]).map(|_| ())
    }

    pub fn delete_branch(&self, path: &Path, branch: &str) -> AppResult<()> {
        self.git(path, ["branch", "-D", branch]).map(|_| ())
    }

    pub fn remove_worktree(&self, repo_path: &Path, worktree_path: &Path) -> AppResult<()> {
        self.git_os(
            repo_path,
            [
                OsString::from("worktree"),
                OsString::from("remove"),
                worktree_path.as_os_str().to_os_string(),
            ],
        )
        .map(|_| ())
    }

    pub fn add_worktree(
        &self,
        repo_path: &Path,
        target_path: &Path,
        branch: &str,
    ) -> AppResult<()> {
        self.git_os(
            repo_path,
            [
                OsString::from("worktree"),
                OsString::from("add"),
                target_path.as_os_str().to_os_string(),
                OsString::from(branch),
            ],
        )
        .map(|_| ())
    }

    fn git<'a>(&self, path: &Path, args: impl IntoIterator<Item = &'a str>) -> AppResult<String> {
        self.git_os(path, args.into_iter().map(OsString::from))
    }

    fn git_os(&self, path: &Path, args: impl IntoIterator<Item = OsString>) -> AppResult<String> {
        let output = Command::new(&self.executable)
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .map_err(|_| AppError::MissingDependency { name: "git" })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(AppError::Git(if stderr.is_empty() {
                format!("git failed in {}", path.display())
            } else {
                stderr
            }));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CherryMark {
    pub applied: bool,
    pub commit: String,
}

fn absolutize_git_path(cwd: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

pub fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn parse_worktree_porcelain(input: &str) -> AppResult<Vec<GitWorktreeEntry>> {
    let mut entries = Vec::new();
    let mut current: Option<GitWorktreeEntry> = None;

    for line in input.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(GitWorktreeEntry {
                path: PathBuf::from(path),
                head: None,
                branch: None,
                detached: false,
                bare: false,
            });
            continue;
        }

        let Some(entry) = current.as_mut() else {
            return Err(AppError::Git(format!(
                "unexpected porcelain line before worktree header: {line}"
            )));
        };

        if let Some(head) = line.strip_prefix("HEAD ") {
            entry.head = Some(head.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.branch = branch
                .strip_prefix("refs/heads/")
                .or(Some(branch))
                .map(str::to_string);
        } else if line == "detached" {
            entry.detached = true;
        } else if line == "bare" {
            entry.bare = true;
        }
    }

    if let Some(entry) = current.take() {
        entries.push(entry);
    }

    Ok(entries)
}
