use std::fs;
use std::path::Path;
use std::process::Command;

use workroot::branch::{branch_worktree, detach_worktree};
use workroot::discovery;
use workroot::domain::Config;
use workroot::git::Git;
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
    fs::write(path.join("README.md"), "hello\n").unwrap();
    git(&["add", "README.md"], path);
    git(
        &[
            "-c",
            "user.name=Workroot Test",
            "-c",
            "user.email=workroot@example.test",
            "commit",
            "-m",
            "init",
        ],
        path,
    );
}

#[test]
fn branch_converts_detached_worktree_to_branch_backed() {
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
    let worktree = root.join("repo").join("task");

    let output = branch_worktree(&storage, &Git::default(), "repo", "task", "feat/task").unwrap();

    assert!(output.contains("feat/task"));
    assert_eq!(
        git_stdout(&["branch", "--show-current"], &worktree),
        "feat/task"
    );
    let cache = storage.load_cache().unwrap();
    let record = cache
        .worktrees
        .iter()
        .find(|worktree| worktree.target == "task")
        .unwrap();
    assert_eq!(record.branch.as_deref(), Some("feat/task"));
    assert!(!record.detached);
}

#[test]
fn detach_converts_branch_worktree_to_detached_head() {
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
    discovery::new_branch_worktree(&storage, &Git::default(), "repo", "task", "feat/task").unwrap();
    let worktree = root.join("repo").join("task");

    let output = detach_worktree(&storage, &Git::default(), "repo", "task").unwrap();

    assert!(output.contains("detached"));
    assert_eq!(git_stdout(&["branch", "--show-current"], &worktree), "");
    let cache = storage.load_cache().unwrap();
    let record = cache
        .worktrees
        .iter()
        .find(|worktree| worktree.target == "task")
        .unwrap();
    assert!(record.detached);
    assert_eq!(record.branch, None);
}
