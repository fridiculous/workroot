use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use workroot::domain::{
    CURRENT_SCHEMA_VERSION, Cache, DirtyState, RepoRecord, RepoSource, WorktreeRecord,
    WorktreeSource,
};
use workroot::storage::{FileStorage, StoragePaths};

#[test]
fn path_prints_path_only_to_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("work tree $special [x]");
    fs::create_dir_all(&worktree_path).unwrap();
    write_cache(temp.path(), cache_with_worktrees(&worktree_path, &[]));

    let output = workroot_command(temp.path())
        .args(["path", "jam", "auth"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), format!("{}\n", worktree_path.display()));
    assert_eq!(stderr(&output), "");
}

#[test]
fn path_can_print_json_for_chaining() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("work tree $special [x]");
    fs::create_dir_all(&worktree_path).unwrap();
    write_cache(temp.path(), cache_with_worktrees(&worktree_path, &[]));

    let output = workroot_command(temp.path())
        .args(["path", "-o", "json", "jam", "auth"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "path");
    assert_eq!(json["repo"], "jam");
    assert_eq!(json["target"], "auth");
    assert_eq!(json["branch"], "auth");
    assert_eq!(json["path"], worktree_path.display().to_string());
}

#[test]
fn path_fails_ambiguity_without_tty_and_keeps_stdout_empty() {
    let temp = tempfile::tempdir().unwrap();
    let base_path = temp.path().join("base");
    let auth_path = temp.path().join("auth");
    fs::create_dir_all(&base_path).unwrap();
    fs::create_dir_all(&auth_path).unwrap();
    write_cache(
        temp.path(),
        cache_with_worktrees(&auth_path, &[(&base_path, "base")]),
    );

    let output = workroot_command(temp.path())
        .args(["worktree", "path", "jam"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("target for repo `jam` is ambiguous"));
}

#[test]
fn status_can_filter_to_one_target() {
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth");
    let docs_path = temp.path().join("docs");
    fs::create_dir_all(&auth_path).unwrap();
    fs::create_dir_all(&docs_path).unwrap();
    write_cache(
        temp.path(),
        cache_with_worktrees(&auth_path, &[(&docs_path, "docs")]),
    );

    let output = workroot_command(temp.path())
        .args(["status", "jam", "auth"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("worktrees=1"));
    assert!(stdout(&output).contains(&auth_path.display().to_string()));
    assert!(!stdout(&output).contains(&docs_path.display().to_string()));
}

#[test]
fn status_json_can_filter_to_one_target() {
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth");
    let docs_path = temp.path().join("docs");
    fs::create_dir_all(&auth_path).unwrap();
    fs::create_dir_all(&docs_path).unwrap();
    write_cache(
        temp.path(),
        cache_with_worktrees(&auth_path, &[(&docs_path, "docs")]),
    );

    let output = workroot_command(temp.path())
        .args(["status", "--json", "jam", "auth"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["summary"]["worktrees"], 1);
    assert_eq!(json["idle"][0]["repo"], "jam");
    assert_eq!(json["idle"][0]["target"], "auth");
    assert_eq!(json["idle"][0]["path"], auth_path.display().to_string());
    assert!(!stdout(&output).contains(&docs_path.display().to_string()));
}

#[test]
fn status_output_json_matches_legacy_json_flag() {
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth");
    fs::create_dir_all(&auth_path).unwrap();
    write_cache(temp.path(), cache_with_worktrees(&auth_path, &[]));

    let output = workroot_command(temp.path())
        .args(["status", "-o", "json", "jam", "auth"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["summary"]["worktrees"], 1);
    assert_eq!(json["idle"][0]["repo"], "jam");
    assert_eq!(json["idle"][0]["target"], "auth");
    assert_eq!(json["idle"][0]["path"], auth_path.display().to_string());
}

#[test]
fn status_json_uses_cached_dirty_until_refresh_requested() {
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth");
    init_git_repo(&auth_path);
    fs::write(auth_path.join("changed.txt"), "dirty").unwrap();
    let mut cache = cache_with_worktrees(&auth_path, &[]);
    cache.worktrees[0].dirty = DirtyState::Clean;
    write_cache(temp.path(), cache);

    let cached = workroot_command(temp.path())
        .args(["status", "--json", "jam", "auth"])
        .output()
        .unwrap();
    assert!(cached.status.success(), "stderr: {}", stderr(&cached));
    let json: serde_json::Value = serde_json::from_str(&stdout(&cached)).unwrap();
    assert_eq!(json["summary"]["dirty"], 0);
    assert_eq!(json["idle"][0]["dirty"], "clean");

    let refreshed = workroot_command(temp.path())
        .args(["status", "--refresh", "--json", "jam", "auth"])
        .output()
        .unwrap();
    assert!(refreshed.status.success(), "stderr: {}", stderr(&refreshed));
    let json: serde_json::Value = serde_json::from_str(&stdout(&refreshed)).unwrap();
    assert_eq!(json["summary"]["dirty"], 1);
    assert_eq!(json["attention"][0]["dirty"], "dirty(1)");
}

#[test]
fn complete_lists_repo_and_target_candidates_from_cache() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("auth");
    fs::create_dir_all(&worktree_path).unwrap();
    write_cache(temp.path(), cache_with_worktrees(&worktree_path, &[]));

    let repos = workroot_command(temp.path())
        .args(["complete", "repos", "ja"])
        .output()
        .unwrap();
    assert!(repos.status.success(), "stderr: {}", stderr(&repos));
    assert_eq!(stdout(&repos), "jam\n");

    let targets = workroot_command(temp.path())
        .args(["complete", "targets", "jam", "au"])
        .output()
        .unwrap();
    assert!(targets.status.success(), "stderr: {}", stderr(&targets));
    assert_eq!(stdout(&targets), "auth\n");
}

#[test]
fn complete_fails_clearly_for_corrupt_cache_without_git() {
    let temp = tempfile::tempdir().unwrap();
    let cache_path = temp
        .path()
        .join("cache")
        .join("workroot")
        .join("index.json");
    fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    fs::write(&cache_path, "not json").unwrap();

    let output = workroot_command(temp.path())
        .args(["complete", "repos", ""])
        .env("PATH", "")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("could not parse JSON cache file"));
}

#[test]
fn bash_shell_init_cd_works_with_command_substitution_when_available() {
    run_shell_cd_test(
        "bash",
        &["--noprofile", "--norc", "-c"],
        bash_script("bash"),
    );
}

#[test]
fn zsh_shell_init_cd_works_with_command_substitution_when_available() {
    run_shell_cd_test("zsh", &["-f", "-c"], bash_script("zsh"));
}

#[test]
fn fish_shell_init_cd_works_with_command_substitution_when_available() {
    run_shell_cd_test(
        "fish",
        &["-c"],
        "workroot shell-init fish | source; workroot cd jam auth; pwd".to_string(),
    );
}

#[test]
fn bash_shell_init_new_alias_prints_path_and_cds_into_worktree() {
    run_shell_new_test(
        "bash",
        &["--noprofile", "--norc", "-c"],
        "eval \"$(workroot shell-init bash)\"; workroot new repo feature; pwd",
    );
}

#[test]
fn bash_shell_init_wr_alias_prints_path_and_cds_into_worktree() {
    run_shell_new_test(
        "bash",
        &["--noprofile", "--norc", "-c"],
        "eval \"$(workroot shell-init bash)\"; wr new repo feature; pwd",
    );
}

#[test]
fn zsh_shell_init_new_alias_prints_path_and_cds_into_worktree() {
    run_shell_new_test(
        "zsh",
        &["-f", "-c"],
        "eval \"$(workroot shell-init zsh)\"; workroot new repo feature; pwd",
    );
}

#[test]
fn fish_shell_init_new_alias_prints_path_and_cds_into_worktree() {
    run_shell_new_test(
        "fish",
        &["-c"],
        "workroot shell-init fish | source; workroot new repo feature; pwd",
    );
}

#[test]
fn top_level_new_prints_path_when_not_interactive() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_git_repo(&repo);
    let adopt = workroot_command(temp.path())
        .args(["worktree", "adopt"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(adopt.status.success(), "stderr: {}", stderr(&adopt));

    let output = workroot_command(temp.path())
        .args(["new", "repo", "feature"])
        .output()
        .unwrap();
    let expected = temp.path().join(".worktrees").join("repo").join("feature");

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), format!("{}\n", expected.display()));
    assert!(expected.join(".git").exists());
}

#[test]
fn top_level_new_can_print_json_for_chaining() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_git_repo(&repo);
    let adopt = workroot_command(temp.path())
        .args(["worktree", "adopt"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(adopt.status.success(), "stderr: {}", stderr(&adopt));

    let output = workroot_command(temp.path())
        .args(["new", "-o", "json", "repo", "feature"])
        .output()
        .unwrap();
    let expected = temp.path().join(".worktrees").join("repo").join("feature");

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "new");
    assert_eq!(json["repo"], "repo");
    assert_eq!(json["target"], "feature");
    assert_eq!(json["branch"], "feature");
    assert_eq!(json["path"], expected.display().to_string());
    assert!(expected.join(".git").exists());
}

fn run_shell_cd_test(shell: &str, shell_args: &[&str], script: String) {
    if Command::new(shell).arg("--version").output().is_err() {
        eprintln!("skipping {shell} integration test; shell unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("work tree $special [x]");
    fs::create_dir_all(&worktree_path).unwrap();
    write_cache(temp.path(), cache_with_worktrees(&worktree_path, &[]));

    let mut command = Command::new(shell);
    command.args(shell_args).arg(script);
    workroot_env(&mut command, temp.path());

    let output = command.output().unwrap();
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), format!("{}\n", worktree_path.display()));
}

fn bash_script(shell_name: &str) -> String {
    format!("eval \"$(workroot shell-init {shell_name})\"; workroot cd jam auth; pwd")
}

fn run_shell_new_test(shell: &str, shell_args: &[&str], script: &str) {
    if Command::new(shell).arg("--version").output().is_err() {
        eprintln!("skipping {shell} integration test; shell unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_git_repo(&repo);
    let adopt = workroot_command(temp.path())
        .args(["worktree", "adopt"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(adopt.status.success(), "stderr: {}", stderr(&adopt));

    let mut command = Command::new(shell);
    command.args(shell_args).arg(script);
    workroot_env(&mut command, temp.path());
    let output = command.output().unwrap();
    let expected = temp.path().join(".worktrees").join("repo").join("feature");

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        format!("{}\n{}\n", expected.display(), expected.display())
    );
    assert!(expected.join(".git").exists());
}

fn workroot_command(root: &Path) -> Command {
    let mut command = Command::new(workroot_binary());
    workroot_env(&mut command, root);
    command
}

fn workroot_env(command: &mut Command, root: &Path) {
    command
        .env("WORKROOT_CONFIG_HOME", root.join("config"))
        .env("WORKROOT_STATE_HOME", root.join("state"))
        .env("WORKROOT_CACHE_HOME", root.join("cache"))
        .env("HOME", root)
        .env("PATH", test_path());
}

fn init_git_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-b", "main"]);
    fs::write(path.join("README.md"), "hello\n").unwrap();
    run_git(path, &["add", "README.md"]);
    run_git(
        path,
        &[
            "-c",
            "user.name=Workroot Test",
            "-c",
            "user.email=workroot@example.test",
            "commit",
            "-m",
            "init",
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

fn test_path() -> OsString {
    let mut paths = vec![workroot_binary().parent().unwrap().to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).unwrap()
}

fn workroot_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_workroot"))
}

fn write_cache(root: &Path, cache: Cache) {
    let storage = FileStorage::new(StoragePaths {
        config: root.join("config").join("workroot").join("config.toml"),
        state: root.join("state").join("workroot").join("state.json"),
        cache: root.join("cache").join("workroot").join("index.json"),
    });
    storage.save_cache(&cache).unwrap();
}

fn cache_with_worktrees(auth_path: &Path, extra: &[(&Path, &str)]) -> Cache {
    let mut worktrees = vec![worktree_record("auth", auth_path)];
    worktrees.extend(
        extra
            .iter()
            .map(|(path, target)| worktree_record(target, path)),
    );

    Cache {
        schema_version: CURRENT_SCHEMA_VERSION,
        repos: vec![RepoRecord {
            alias: "jam".to_string(),
            display_name: "jam".to_string(),
            canonical_path: auth_path.to_path_buf(),
            git_common_dir: auth_path.join(".git"),
            base_branch: Some("main".to_string()),
            source: RepoSource::Adopted,
            stale: false,
        }],
        worktrees,
        last_scan_unix: None,
    }
}

fn worktree_record(target: &str, path: &Path) -> WorktreeRecord {
    WorktreeRecord {
        repo_alias: "jam".to_string(),
        target: target.to_string(),
        display_name: target.to_string(),
        branch: Some(target.to_string()),
        path: path.to_path_buf(),
        source: WorktreeSource::Manual,
        dirty: DirtyState::Unknown,
        last_seen_unix: None,
        stale: false,
        detached: false,
    }
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}
