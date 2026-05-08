use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::git::Git;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineageReport {
    pub status: LineageStatus,
    pub base_commit: String,
    pub head_commit: String,
    pub evidence: Vec<String>,
}

impl LineageReport {
    pub fn is_prune_safe(&self) -> bool {
        matches!(
            self.status,
            LineageStatus::MergedByAncestry
                | LineageStatus::MergedByGithubPr
                | LineageStatus::MergedByPatchId
        )
    }

    pub fn proof_label(&self) -> &'static str {
        match self.status {
            LineageStatus::MergedByAncestry => "merged-by-ancestry",
            LineageStatus::MergedByGithubPr => "merged-by-github-pr",
            LineageStatus::MergedByPatchId => "merged-by-patch-id",
            LineageStatus::NotProven => "not-proven-merged",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineageStatus {
    MergedByAncestry,
    MergedByGithubPr,
    MergedByPatchId,
    NotProven,
}

pub fn detect_lineage(
    git: &Git,
    repo_path: &Path,
    base_ref: &str,
    branch_name: Option<&str>,
    head_ref: &str,
) -> AppResult<LineageReport> {
    let base_commit = git
        .rev_parse(repo_path, base_ref)?
        .ok_or_else(|| AppError::Git(format!("base ref `{base_ref}` was not found")))?;
    let head_commit = git
        .rev_parse(repo_path, head_ref)?
        .ok_or_else(|| AppError::Git(format!("head ref `{head_ref}` was not found")))?;
    let mut evidence = vec![
        format!("base {base_ref}@{}", short(&base_commit)),
        format!("head {head_ref}@{}", short(&head_commit)),
    ];

    if git.is_ancestor(repo_path, &head_commit, &base_commit)? {
        evidence.push(format!(
            "`git merge-base --is-ancestor {} {}` succeeded",
            short(&head_commit),
            short(&base_commit)
        ));
        return Ok(LineageReport {
            status: LineageStatus::MergedByAncestry,
            base_commit,
            head_commit,
            evidence,
        });
    }
    evidence.push("head is not an ancestor of base".to_string());

    if let Some(branch_name) = branch_name {
        if let Some(github_evidence) =
            detect_github_pr_lineage(git, repo_path, base_ref, branch_name, &head_commit)?
        {
            evidence.extend(github_evidence.evidence);
            if github_evidence.status == LineageStatus::MergedByGithubPr {
                return Ok(LineageReport {
                    status: github_evidence.status,
                    base_commit,
                    head_commit,
                    evidence,
                });
            }
        }
    } else {
        evidence.push("GitHub PR lineage skipped: worktree is detached".to_string());
    }

    let cherry = git.cherry(repo_path, base_ref, head_ref)?;
    if !cherry.is_empty() && cherry.iter().all(|mark| mark.applied) {
        evidence.push(format!(
            "`git cherry {base_ref} {head_ref}` marks all {} branch commit(s) as applied upstream",
            cherry.len()
        ));
        return Ok(LineageReport {
            status: LineageStatus::MergedByPatchId,
            base_commit,
            head_commit,
            evidence,
        });
    }

    let unapplied = cherry.iter().filter(|mark| !mark.applied).count();
    if cherry.is_empty() {
        evidence.push(format!(
            "`git cherry {base_ref} {head_ref}` found no branch-only commits"
        ));
    } else {
        evidence.push(format!(
            "`git cherry {base_ref} {head_ref}` found {unapplied} unapplied branch commit(s)"
        ));
    }

    Ok(LineageReport {
        status: LineageStatus::NotProven,
        base_commit,
        head_commit,
        evidence,
    })
}

#[derive(Debug)]
struct GithubLineageEvidence {
    status: LineageStatus,
    evidence: Vec<String>,
}

fn detect_github_pr_lineage(
    git: &Git,
    repo_path: &Path,
    base_ref: &str,
    branch_name: &str,
    head_commit: &str,
) -> AppResult<Option<GithubLineageEvidence>> {
    let Some(repo_slug) = github_repo_slug(git, repo_path)? else {
        return Ok(Some(GithubLineageEvidence {
            status: LineageStatus::NotProven,
            evidence: vec!["GitHub PR lineage skipped: origin is not a GitHub remote".to_string()],
        }));
    };
    let owner = repo_slug.split('/').next().unwrap_or_default();
    let head_filter = format!("{owner}:{branch_name}");
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--repo",
            &repo_slug,
            "--state",
            "merged",
            "--head",
            &head_filter,
            "--json",
            "number,headRefOid,mergeCommit,mergedAt,url",
            "--limit",
            "20",
        ])
        .output();

    let output = match output {
        Ok(output) => output,
        Err(_) => {
            return Ok(Some(GithubLineageEvidence {
                status: LineageStatus::NotProven,
                evidence: vec!["GitHub PR lineage skipped: `gh` CLI is unavailable".to_string()],
            }));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Ok(Some(GithubLineageEvidence {
            status: LineageStatus::NotProven,
            evidence: vec![if stderr.is_empty() {
                "GitHub PR lineage skipped: `gh pr list` failed".to_string()
            } else {
                format!("GitHub PR lineage skipped: {stderr}")
            }],
        }));
    }

    let prs: Vec<GithubPr> =
        serde_json::from_slice(&output.stdout).map_err(|source| AppError::ParseJson {
            kind: "GitHub PR lineage",
            path: Path::new("gh pr list").to_path_buf(),
            source: Box::new(source),
        })?;

    if prs.is_empty() {
        return Ok(Some(GithubLineageEvidence {
            status: LineageStatus::NotProven,
            evidence: vec![format!(
                "GitHub PR lineage found no merged PR for `{head_filter}`"
            )],
        }));
    }

    let mut moved_prs = Vec::new();
    for pr in prs {
        let Some(merge_commit) = pr.merge_commit.and_then(|commit| commit.oid) else {
            continue;
        };
        if pr.head_ref_oid == head_commit && git.is_ancestor(repo_path, &merge_commit, base_ref)? {
            return Ok(Some(GithubLineageEvidence {
                status: LineageStatus::MergedByGithubPr,
                evidence: vec![
                    format!(
                        "GitHub PR #{} was merged at {}",
                        pr.number,
                        pr.merged_at.unwrap_or_else(|| "unknown time".to_string())
                    ),
                    format!("PR head matched worktree HEAD {}", short(head_commit)),
                    format!(
                        "PR merge/squash commit {} is reachable from {base_ref}",
                        short(&merge_commit)
                    ),
                    format!("PR URL: {}", pr.url),
                ],
            }));
        }
        moved_prs.push(pr.number);
    }

    Ok(Some(GithubLineageEvidence {
        status: LineageStatus::NotProven,
        evidence: vec![format!(
            "GitHub PR lineage found merged PR(s) for branch, but none matched current HEAD {} and reachable base: {:?}",
            short(head_commit),
            moved_prs
        )],
    }))
}

fn github_repo_slug(git: &Git, repo_path: &Path) -> AppResult<Option<String>> {
    let Some(url) = git.remote_url(repo_path, "origin")? else {
        return Ok(None);
    };
    Ok(parse_github_slug(&url))
}

fn parse_github_slug(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return Some(rest.to_string());
    }
    for prefix in [
        "https://github.com/",
        "http://github.com/",
        "ssh://git@github.com/",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest.to_string());
        }
    }
    None
}

fn short(commit: &str) -> String {
    commit.chars().take(8).collect()
}

#[derive(Debug, Deserialize)]
struct GithubPr {
    number: u64,
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
    #[serde(rename = "mergeCommit")]
    merge_commit: Option<GithubMergeCommit>,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct GithubMergeCommit {
    oid: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::parse_github_slug;

    #[test]
    fn parses_github_remote_urls() {
        assert_eq!(
            parse_github_slug("git@github.com:fridiculous/workroot.git").as_deref(),
            Some("fridiculous/workroot")
        );
        assert_eq!(
            parse_github_slug("https://github.com/fridiculous/workroot.git").as_deref(),
            Some("fridiculous/workroot")
        );
    }
}
