use std::fs;
use std::path::Path;
use std::process::Command;

use workroot::discovery;
use workroot::domain::Config;
use workroot::git::Git;
use workroot::merge::merge_worktree;
use workroot::storage::{FileStorage, StoragePaths};

fn storage(root: &Path) -> FileStorage {
    FileStorage::new(StoragePaths {
        config: root.join("config.toml"),
        state: root.join("state.json"),
        cache: root.join("cache.json"),
    })
}

fn git(args: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(args: &[&str], cwd: &Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repo(path: &Path, branch: &str) {
    fs::create_dir_all(path).unwrap();
    let output = Command::new("git")
        .args(["init", "-b", branch])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    commit_file(path, "README.md", "hello", "init");
}

fn commit_file(repo: &Path, name: &str, contents: &str, message: &str) {
    fs::write(repo.join(name), format!("{contents}\n")).unwrap();
    git(&["add", name], repo);
    git(
        &[
            "-c",
            "user.name=Workroot Test",
            "-c",
            "user.email=workroot@example.test",
            "commit",
            "-m",
            message,
        ],
        repo,
    );
}

fn setup_merge_env() -> (tempfile::TempDir, FileStorage, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");
    init_repo(&repo, "main");
    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    discovery::new_worktree(&storage, &Git::default(), "repo", "task").unwrap();
    discovery::new_branch_worktree(
        &storage,
        &Git::default(),
        "repo",
        "integration",
        "integration",
    )
    .unwrap();
    (temp, storage, root)
}

#[test]
fn merge_merges_detached_source_head_into_destination_branch_worktree() {
    let (_temp, storage, root) = setup_merge_env();
    let source = root.join("repo").join("task");
    let destination = root.join("repo").join("integration");
    commit_file(&source, "agent.txt", "agent work", "agent work");

    let output = merge_worktree(&storage, &Git::default(), "repo", "task", "integration").unwrap();

    assert!(output.contains("merged `task` into `integration`"));
    assert_eq!(
        git_stdout(&["branch", "--show-current"], &destination),
        "integration"
    );
    assert!(destination.join("agent.txt").exists());
    assert_eq!(
        git_stdout(&["rev-parse", "HEAD"], &destination),
        git_stdout(&["rev-parse", "HEAD"], &source)
    );
}

#[test]
fn merge_refuses_dirty_source() {
    let (_temp, storage, root) = setup_merge_env();
    let source = root.join("repo").join("task");
    fs::write(source.join("agent.txt"), "dirty\n").unwrap();

    let error = merge_worktree(&storage, &Git::default(), "repo", "task", "integration")
        .unwrap_err()
        .to_string();

    assert!(error.contains("source target `task` has uncommitted changes"));
}

#[test]
fn merge_refuses_dirty_destination() {
    let (_temp, storage, root) = setup_merge_env();
    let source = root.join("repo").join("task");
    let destination = root.join("repo").join("integration");
    commit_file(&source, "agent.txt", "agent work", "agent work");
    fs::write(destination.join("dirty.txt"), "dirty\n").unwrap();

    let error = merge_worktree(&storage, &Git::default(), "repo", "task", "integration")
        .unwrap_err()
        .to_string();

    assert!(error.contains("destination branch `integration` has uncommitted changes"));
}

#[test]
fn merge_refuses_missing_destination_branch_worktree() {
    let (_temp, storage, _root) = setup_merge_env();

    let error = merge_worktree(&storage, &Git::default(), "repo", "task", "missing")
        .unwrap_err()
        .to_string();

    assert!(error.contains("destination branch `missing` is not checked out"));
    assert!(error.contains("workroot new repo <target> --branch missing"));
}

#[test]
fn merge_leaves_conflict_state_in_destination_worktree() {
    let (_temp, storage, root) = setup_merge_env();
    let source = root.join("repo").join("task");
    let destination = root.join("repo").join("integration");
    commit_file(&source, "README.md", "source change", "source change");
    commit_file(
        &destination,
        "README.md",
        "destination change",
        "destination change",
    );

    let error = merge_worktree(&storage, &Git::default(), "repo", "task", "integration")
        .unwrap_err()
        .to_string();

    assert!(error.contains("has conflicts"));
    assert!(error.contains(&destination.display().to_string()));
    assert!(git_stdout(&["ls-files", "-u", "README.md"], &destination).contains("README.md"));
    assert!(git_stdout(&["status", "--porcelain"], &destination).contains("UU README.md"));
}
