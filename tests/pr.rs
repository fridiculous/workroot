use std::fs;
use std::path::Path;
use std::process::Command;

use workroot::discovery;
use workroot::domain::Config;
use workroot::git::Git;
use workroot::pr::create_pr;
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
fn pr_refuses_detached_worktree_with_branch_guidance() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");
    init_repo(&repo, "main");
    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    discovery::new_worktree(&storage, &Git::default(), "repo", "task").unwrap();

    let error = create_pr(&storage, &Git::default(), "repo", "task")
        .unwrap_err()
        .to_string();

    assert!(error.contains("detached"));
    assert!(error.contains("workroot switch repo task -c <branch>"));
}

#[test]
fn pr_refuses_unpushed_branch_with_push_guidance() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");
    init_repo(&repo, "main");
    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    discovery::new_branch_worktree(&storage, &Git::default(), "repo", "task", "feat/task").unwrap();

    let error = create_pr(&storage, &Git::default(), "repo", "task")
        .unwrap_err()
        .to_string();

    assert!(error.contains("no upstream"));
    assert!(error.contains("workroot push repo task"));
}
