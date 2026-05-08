use std::fs;
use std::path::Path;
use std::process::Command;

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

fn add_worktree(repo: &Path, path: &Path, branch: &str) {
    if !branch_exists(repo, branch) {
        git(&["branch", branch], repo);
    }
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

fn branch_exists(repo: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show-ref", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .status()
        .unwrap()
        .success()
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

#[test]
fn adopt_base_discovers_linked_worktrees_and_preserves_paths_with_spaces() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo with spaces");
    let wt = temp.path().join("outside root").join("base");
    init_repo(&repo, "main");
    add_worktree(&repo, &wt, "feature");

    let storage = storage(&temp.path().join("workroot state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();
    let cache = storage.load_cache().unwrap();

    assert_eq!(cache.repos[0].alias, "repo with spaces");
    assert_eq!(cache.worktrees.len(), 2);
    assert!(cache.worktrees.iter().any(|worktree| {
        worktree.target == "base" && worktree.path == repo.canonicalize().unwrap()
    }));
    assert!(cache.worktrees.iter().any(|worktree| {
        worktree.target == "base-2" && worktree.path == wt.canonicalize().unwrap()
    }));
}

#[test]
fn scan_uses_configured_roots_and_discovers_siblings_outside_roots() {
    let temp = tempfile::tempdir().unwrap();
    let scan_root = temp.path().join("scan root with spaces");
    let repo = scan_root.join("a").join("b").join("c").join("repo");
    let sibling = temp.path().join("elsewhere").join("feature");
    init_repo(&repo, "main");
    add_worktree(&repo, &sibling, "feature");

    let storage = storage(&temp.path().join("state"));
    let config = Config {
        scan_roots: vec![scan_root],
        ..Config::default()
    };
    storage.save_config(&config).unwrap();
    discovery::scan(&storage, &Git::default()).unwrap();
    let cache = storage.load_cache().unwrap();

    assert!(
        cache
            .repos
            .iter()
            .any(|record| record.canonical_path == repo.canonicalize().unwrap())
    );
    assert!(
        cache
            .worktrees
            .iter()
            .any(|worktree| worktree.path == sibling.canonicalize().unwrap())
    );
}

#[test]
fn alias_and_target_collisions_receive_stable_suffixes() {
    let temp = tempfile::tempdir().unwrap();
    let repo_one = temp.path().join("one").join("jam");
    let repo_two = temp.path().join("two").join("jam");
    let dup_one = temp.path().join("worktrees-a").join("dup");
    let dup_two = temp.path().join("worktrees-b").join("dup");
    init_repo(&repo_one, "main");
    init_repo(&repo_two, "main");
    add_worktree(&repo_one, &dup_one, "feat-a");
    add_worktree(&repo_one, &dup_two, "feat-b");

    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo_one).unwrap();
    discovery::adopt(&storage, &Git::default(), &repo_two).unwrap();
    let cache = storage.load_cache().unwrap();

    assert!(cache.repos.iter().any(|repo| repo.alias == "jam"));
    assert!(cache.repos.iter().any(|repo| repo.alias == "jam-2"));
    let targets: Vec<_> = cache
        .worktrees
        .iter()
        .filter(|worktree| worktree.repo_alias == "jam")
        .map(|worktree| worktree.target.as_str())
        .collect();
    assert!(targets.contains(&"dup"));
    assert!(targets.contains(&"dup-2"));
}

#[test]
fn scan_marks_missing_worktree_paths_stale_without_deleting_state() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let wt = temp.path().join("wt");
    init_repo(&repo, "main");
    add_worktree(&repo, &wt, "feature");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    fs::remove_dir_all(&wt).unwrap();
    discovery::scan(&storage, &Git::default()).unwrap();
    let cache = storage.load_cache().unwrap();

    let stale = cache
        .worktrees
        .iter()
        .find(|worktree| worktree.branch.as_deref() == Some("feature"))
        .unwrap();
    assert!(stale.stale);
}

#[test]
fn discover_path_indexes_one_repo_family_as_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo with spaces");
    let wt = temp.path().join("outside root").join("base");
    init_repo(&repo, "main");
    add_worktree(&repo, &wt, "feature");

    let storage = storage(&temp.path().join("workroot state"));
    let output = discovery::discover(&storage, &Git::default(), Some(&repo)).unwrap();
    let cache = storage.load_cache().unwrap();

    assert_eq!(output, "discovered repo with spaces\n");
    assert_eq!(cache.repos[0].alias, "repo with spaces");
    assert_eq!(cache.repos[0].source, workroot::domain::RepoSource::Adopted);
    assert_eq!(cache.worktrees.len(), 2);
    assert!(cache.worktrees.iter().any(|worktree| {
        worktree.target == "base" && worktree.path == repo.canonicalize().unwrap()
    }));
    assert!(cache.worktrees.iter().any(|worktree| {
        worktree.target == "base-2" && worktree.path == wt.canonicalize().unwrap()
    }));
}

#[test]
fn ignore_removes_repo_and_blocks_future_discovery_until_unignored() {
    let temp = tempfile::tempdir().unwrap();
    let scan_root = temp.path().join("scan-root");
    let repo = scan_root.join("repo");
    init_repo(&repo, "main");

    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            scan_roots: vec![scan_root],
            ..Config::default()
        })
        .unwrap();

    discovery::discover(&storage, &Git::default(), None).unwrap();
    let output = discovery::ignore(&storage, &Git::default(), "repo").unwrap();
    assert!(output.contains("ignored"));

    let state = storage.load_state().unwrap();
    let cache = storage.load_cache().unwrap();
    assert_eq!(state.ignored_repos.len(), 1);
    assert!(!cache.repos.iter().any(|record| record.alias == "repo"));

    discovery::discover(&storage, &Git::default(), None).unwrap();
    let cache_after_rediscover = storage.load_cache().unwrap();
    assert!(
        !cache_after_rediscover
            .repos
            .iter()
            .any(|record| record.alias == "repo")
    );

    let unignore_output =
        discovery::unignore(&storage, &Git::default(), &repo.display().to_string()).unwrap();
    assert!(unignore_output.contains("unignored"));

    discovery::discover(&storage, &Git::default(), None).unwrap();
    let cache_after_unignore = storage.load_cache().unwrap();
    assert!(
        cache_after_unignore
            .repos
            .iter()
            .any(|record| record.alias == "repo")
    );
}

#[test]
fn discover_path_rejects_ignored_repo_until_unignored() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo, "main");

    let storage = storage(&temp.path().join("state"));
    discovery::discover(&storage, &Git::default(), Some(&repo)).unwrap();
    discovery::ignore(&storage, &Git::default(), &repo.display().to_string()).unwrap();

    let error = discovery::discover(&storage, &Git::default(), Some(&repo))
        .unwrap_err()
        .to_string();
    assert!(error.contains("is ignored"));
    assert!(error.contains("workroot unignore"));
}

#[test]
fn unignore_rejects_unknown_entry() {
    let temp = tempfile::tempdir().unwrap();
    let storage = storage(&temp.path().join("state"));

    let error = discovery::unignore(&storage, &Git::default(), "missing")
        .unwrap_err()
        .to_string();
    assert!(error.contains("ignored repo `missing` was not found"));
}

#[test]
fn new_creates_branch_and_worktree_under_configured_root() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed worktrees");
    init_repo(&repo, "main");
    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let output = discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();
    let expected = root.join("repo").join("feature");

    assert_eq!(output.trim(), expected.display().to_string());
    assert!(expected.join(".git").exists());
    assert!(branch_exists(&repo, "feature"));
}

#[test]
fn new_fast_forwards_base_before_creating_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("origin.git");
    let seed = temp.path().join("seed");
    let repo = temp.path().join("repo");
    let other = temp.path().join("other");
    let root = temp.path().join("managed");
    init_bare_repo(&remote, "main");
    init_repo(&seed, "main");
    let remote_arg = remote.display().to_string();
    git(&["remote", "add", "origin", &remote_arg], &seed);
    git(&["push", "-u", "origin", "main"], &seed);
    clone_repo(&remote, &repo);
    clone_repo(&remote, &other);
    fs::write(other.join("remote.txt"), "latest\n").unwrap();
    git(&["add", "remote.txt"], &other);
    git(
        &[
            "-c",
            "user.name=Workroot Test",
            "-c",
            "user.email=workroot@example.test",
            "commit",
            "-m",
            "remote update",
        ],
        &other,
    );
    git(&["push"], &other);

    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();
    let expected = root.join("repo").join("feature");

    assert!(repo.join("remote.txt").exists());
    assert!(expected.join("remote.txt").exists());
    assert_eq!(
        git_stdout(&["rev-parse", "main"], &repo),
        git_stdout(&["rev-parse", "feature"], &repo)
    );
}

#[test]
fn new_refuses_dirty_base_before_pulling() {
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
    fs::write(repo.join("local.txt"), "dirty\n").unwrap();
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let error = discovery::new_worktree(&storage, &Git::default(), "repo", "feature")
        .unwrap_err()
        .to_string();

    assert!(error.contains("base worktree has uncommitted changes"));
    assert!(error.contains("git -C"));
    assert!(!branch_exists(&repo, "feature"));
}

#[test]
fn new_reuses_existing_branch_without_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");
    init_repo(&repo, "main");
    git(&["branch", "feature"], &repo);
    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root.clone()),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    discovery::new_worktree(&storage, &Git::default(), "repo", "feature").unwrap();

    assert!(root.join("repo").join("feature").join(".git").exists());
}

#[test]
fn new_refuses_existing_target_path_for_different_branch() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = temp.path().join("managed");
    let target_path = root.join("repo").join("feature");
    init_repo(&repo, "main");
    add_worktree(&repo, &target_path, "other");
    let storage = storage(&temp.path().join("state"));
    storage
        .save_config(&Config {
            default_worktree_root: Some(root),
            ..Config::default()
        })
        .unwrap();
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let error = discovery::new_worktree(&storage, &Git::default(), "repo", "feature")
        .unwrap_err()
        .to_string();

    assert!(error.contains("target path exists as worktree for branch `other`"));
}

#[test]
fn new_refuses_branch_checked_out_elsewhere() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let elsewhere = temp.path().join("elsewhere-feature");
    init_repo(&repo, "main");
    add_worktree(&repo, &elsewhere, "feature");
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let error = discovery::new_worktree(&storage, &Git::default(), "repo", "feature")
        .unwrap_err()
        .to_string();

    assert!(error.contains("already checked out"));
}

#[test]
fn new_rejects_detached_repo_when_no_base_branch_can_be_inferred() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo, "topic");
    git(&["checkout", "--detach"], &repo);
    let storage = storage(&temp.path().join("state"));
    discovery::adopt(&storage, &Git::default(), &repo).unwrap();

    let error = discovery::new_worktree(&storage, &Git::default(), "repo", "feature")
        .unwrap_err()
        .to_string();

    assert!(error.contains("could not infer a base branch"));
}

#[test]
fn adopt_linked_worktree_registers_the_base_family() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let wt = temp.path().join("feature wt");
    init_repo(&repo, "main");
    add_worktree(&repo, &wt, "feature");
    let storage = storage(&temp.path().join("state"));

    discovery::adopt(&storage, &Git::default(), &wt).unwrap();
    let cache = storage.load_cache().unwrap();

    assert_eq!(cache.repos[0].canonical_path, repo.canonicalize().unwrap());
    assert_eq!(cache.worktrees.len(), 2);
}
