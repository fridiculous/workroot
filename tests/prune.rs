use std::fs;
use std::path::Path;
use std::process::Command;

use workroot::discovery;
use workroot::git::Git;
use workroot::prune::{prune_merged_interactive, prune_report};
use workroot::storage::{FileStorage, StoragePaths};

#[test]
fn prune_reports_worktree_behind_base_branch() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    commit_file(&repo, "base.txt", "base moved\n", "move base");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let output = prune_report(&storage, &Git::default()).unwrap();

    assert!(output.contains("feature"));
    assert!(output.contains("behind-base"));
}

#[test]
fn prune_reports_diverged_worktree_when_base_and_feature_moved() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    commit_file(&feature, "feature.txt", "feature moved\n", "move feature");
    commit_file(&repo, "base.txt", "base moved\n", "move base");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let output = prune_report(&storage, &Git::default()).unwrap();

    assert!(output.contains("feature"));
    assert!(output.contains("diverged-base"));
}

#[test]
fn prune_treats_feature_commits_on_current_base_as_fresh() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    commit_file(&feature, "feature.txt", "feature moved\n", "move feature");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let output = prune_report(&storage, &Git::default()).unwrap();

    assert!(output.contains("feature"));
    assert!(output.contains("fresh"));
    assert!(!output.contains("behind-base"));
    assert!(!output.contains("diverged-base"));
}

#[test]
fn prune_reports_missing_worktree_paths_as_stale_without_deleting_cache() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    let expected = feature.canonicalize().unwrap();
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    fs::remove_dir_all(&feature).unwrap();

    let output = prune_report(&storage, &Git::default()).unwrap();
    let cache = storage.load_cache().unwrap();

    assert!(output.contains("feature"));
    assert!(output.contains("stale"));
    assert!(
        cache
            .worktrees
            .iter()
            .any(|worktree| worktree.path == expected)
    );
}

#[test]
fn prune_merged_prompts_with_trunk_and_branch_commits_then_removes_yes() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    commit_file(&repo, "base.txt", "base moved\n", "move base");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let mut input = std::io::Cursor::new(b"y\n".to_vec());
    let mut output = Vec::new();
    prune_merged_interactive(
        &storage,
        &Git::default(),
        &mut input,
        &mut output,
        None,
        None,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("trunk  main:"));
    assert!(output.contains("move base"));
    assert!(output.contains("branch feature:"));
    assert!(output.contains("init"));
    assert!(output.contains("proof: merged-by-ancestry"));
    assert!(output.contains("git merge-base --is-ancestor"));
    assert!(output.contains("Remove this worktree? [y/N]"));
    assert!(output.contains("removed"));
    assert!(!feature.exists());
    assert!(
        !storage
            .load_cache()
            .unwrap()
            .worktrees
            .iter()
            .any(|worktree| worktree.path.ends_with("feature"))
    );
}

#[test]
fn prune_merged_can_target_one_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    let other = temp.path().join("other");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    add_worktree(&repo, &other, "other");
    commit_file(&repo, "base.txt", "base moved\n", "move base");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let mut input = std::io::Cursor::new(b"y\n".to_vec());
    let mut output = Vec::new();
    prune_merged_interactive(
        &storage,
        &Git::default(),
        &mut input,
        &mut output,
        Some("repo"),
        Some("feature"),
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("repo feature"));
    assert!(!output.contains("repo other"));
    assert!(!feature.exists());
    assert!(other.exists());
}

#[test]
fn prune_merged_detects_squash_merge_by_patch_id() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    commit_file(&feature, "feature.txt", "feature moved\n", "move feature");
    run_git(&repo, &["cherry-pick", "--no-commit", "feature"]);
    run_git(
        &repo,
        &[
            "-c",
            "user.name=Workroot Test",
            "-c",
            "user.email=workroot@example.test",
            "commit",
            "-m",
            "squash feature",
        ],
    );
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let mut input = std::io::Cursor::new(b"n\n".to_vec());
    let mut output = Vec::new();
    prune_merged_interactive(
        &storage,
        &Git::default(),
        &mut input,
        &mut output,
        None,
        None,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("proof: merged-by-patch-id"));
    assert!(output.contains("git cherry main"));
    assert!(feature.exists());
}

#[test]
fn prune_merged_skips_when_answer_is_not_yes() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let feature = temp.path().join("feature");
    init_repo(&repo);
    add_worktree(&repo, &feature, "feature");
    commit_file(&repo, "base.txt", "base moved\n", "move base");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let mut input = std::io::Cursor::new(b"n\n".to_vec());
    let mut output = Vec::new();
    prune_merged_interactive(
        &storage,
        &Git::default(),
        &mut input,
        &mut output,
        None,
        None,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("skipped"));
    assert!(feature.exists());
    assert!(
        storage
            .load_cache()
            .unwrap()
            .worktrees
            .iter()
            .any(|worktree| worktree.path.ends_with("feature"))
    );
}

fn storage(root: &Path) -> FileStorage {
    FileStorage::new(StoragePaths {
        config: root.join("config.toml"),
        state: root.join("state.json"),
        cache: root.join("cache.json"),
    })
}

fn init_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-b", "main"]);
    commit_file(path, "README.md", "hello\n", "init");
}

fn add_worktree(repo: &Path, path: &Path, branch: &str) {
    run_git(repo, &["branch", branch]);
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "add"])
        .arg(path)
        .arg(branch)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_file(path: &Path, file: &str, contents: &str, message: &str) {
    fs::write(path.join(file), contents).unwrap();
    run_git(path, &["add", file]);
    run_git(
        path,
        &[
            "-c",
            "user.name=Workroot Test",
            "-c",
            "user.email=workroot@example.test",
            "commit",
            "-m",
            message,
        ],
    );
}

fn run_git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
