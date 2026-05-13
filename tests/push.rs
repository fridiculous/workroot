use std::fs;
use std::path::Path;
use std::process::Command;

use workroot::discovery;
use workroot::domain::Config;
use workroot::git::Git;
use workroot::push::{push_worktree, push_worktree_outcome};
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

fn git_output(args: &[&str], cwd: &Path) -> String {
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

fn git_status(args: &[&str], cwd: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .unwrap()
        .success()
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

fn init_bare_repo(path: &Path, branch: &str) {
    fs::create_dir_all(path).unwrap();
    let output = Command::new("git")
        .args(["init", "--bare", "-b", branch])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git init --bare failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn clone_repo(remote: &Path, path: &Path) {
    let output = Command::new("git")
        .arg("clone")
        .arg(remote)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_file(repo: &Path, name: &str, message: &str) {
    fs::write(repo.join(name), format!("{message}\n")).unwrap();
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

#[test]
fn push_sets_upstream_on_first_push() {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("origin.git");
    let seed = temp.path().join("seed");
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");

    init_bare_repo(&remote, "main");
    init_repo(&seed, "main");
    let remote_arg = remote.display().to_string();
    git(&["remote", "add", "origin", &remote_arg], &seed);
    git(&["push", "-u", "origin", "main"], &seed);
    clone_repo(&remote, &repo);

    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    let feature_path = root.join("repo").join("feature");
    discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();
    commit_file(&feature_path, "feature.txt", "feature commit");

    let output = push_worktree(&storage, &Git::default(), "repo", "feature").unwrap();

    assert!(
        output.contains("origin/feature"),
        "unexpected output: {output}"
    );
    assert_eq!(
        git_output(
            &["rev-parse", "--abbrev-ref", "feature@{upstream}"],
            &feature_path
        ),
        "origin/feature"
    );
    assert!(git_status(
        &[
            "show-ref",
            "--verify",
            "--quiet",
            "refs/remotes/origin/feature"
        ],
        &repo
    ));
}

#[test]
fn push_outcome_exposes_json_ready_fields() {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("origin.git");
    let seed = temp.path().join("seed");
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");

    init_bare_repo(&remote, "main");
    init_repo(&seed, "main");
    let remote_arg = remote.display().to_string();
    git(&["remote", "add", "origin", &remote_arg], &seed);
    git(&["push", "-u", "origin", "main"], &seed);
    clone_repo(&remote, &repo);

    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    let feature_path = root.join("repo").join("feature");
    discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();
    commit_file(&feature_path, "feature.txt", "feature commit");

    let outcome = push_worktree_outcome(&storage, &Git::default(), "repo", "feature").unwrap();

    assert_eq!(outcome.repo, "repo");
    assert_eq!(outcome.target, "feature");
    assert_eq!(outcome.branch, "feature");
    assert_eq!(outcome.upstream, "origin/feature");
    assert!(outcome.upstream_set);
    assert_eq!(outcome.path, fs::canonicalize(&feature_path).unwrap());
    assert_eq!(
        outcome.message(),
        "pushed `feature` to `origin/feature` and set upstream\n"
    );
}

#[test]
fn push_uses_existing_upstream_after_first_push() {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("origin.git");
    let seed = temp.path().join("seed");
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");

    init_bare_repo(&remote, "main");
    init_repo(&seed, "main");
    let remote_arg = remote.display().to_string();
    git(&["remote", "add", "origin", &remote_arg], &seed);
    git(&["push", "-u", "origin", "main"], &seed);
    clone_repo(&remote, &repo);

    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    let feature_path = root.join("repo").join("feature");
    discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();

    commit_file(&feature_path, "feature.txt", "first feature commit");
    push_worktree(&storage, &Git::default(), "repo", "feature").unwrap();

    commit_file(&feature_path, "feature.txt", "second feature commit");
    let head_before = git_output(&["rev-parse", "HEAD"], &feature_path);

    let output = push_worktree(&storage, &Git::default(), "repo", "feature").unwrap();

    assert!(
        output.contains("origin/feature"),
        "unexpected output: {output}"
    );
    let remote_head = git_output(&["rev-parse", "refs/remotes/origin/feature"], &repo);
    assert_eq!(remote_head, head_before);
}

#[test]
fn push_refuses_base_branch_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("origin.git");
    let seed = temp.path().join("seed");
    let repo = temp.path().join("repo");

    init_bare_repo(&remote, "main");
    init_repo(&seed, "main");
    let remote_arg = remote.display().to_string();
    git(&["remote", "add", "origin", &remote_arg], &seed);
    git(&["push", "-u", "origin", "main"], &seed);
    clone_repo(&remote, &repo);

    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let error = push_worktree(&storage, &Git::default(), "repo", "base")
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("base branch `main`"),
        "unexpected error: {error}"
    );
    assert!(
        error.contains("workroot new repo"),
        "unexpected error: {error}"
    );
}

#[test]
fn push_refuses_detached_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("origin.git");
    let seed = temp.path().join("seed");
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");

    init_bare_repo(&remote, "main");
    init_repo(&seed, "main");
    let remote_arg = remote.display().to_string();
    git(&["remote", "add", "origin", &remote_arg], &seed);
    git(&["push", "-u", "origin", "main"], &seed);
    clone_repo(&remote, &repo);

    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    let feature_path = root.join("repo").join("feature");
    discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();
    commit_file(&feature_path, "feature.txt", "feature commit");
    git(&["checkout", "--detach"], &feature_path);

    let error = push_worktree(&storage, &Git::default(), "repo", "feature")
        .unwrap_err()
        .to_string();

    assert!(error.contains("detached"), "unexpected error: {error}");
    assert!(
        error.contains("branch to push"),
        "unexpected error: {error}"
    );
}
