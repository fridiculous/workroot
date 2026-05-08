use std::path::{Path, PathBuf};
use std::process::Command;

use workroot::domain::{
    Cache, DirtyState, RepoRecord, RepoSource, SessionBackend, SessionRecord, SessionStatus, State,
    WorktreeRecord, WorktreeSource,
};
use workroot::git::Git;
use workroot::session::TmuxPane;
use workroot::status::{
    TmuxInventory, list_output, radar_json_output, radar_output, refresh_status, sessions_output,
    status_output,
};
use workroot::storage::{FileStorage, StoragePaths};

#[test]
fn list_shows_global_known_worktrees() {
    let cache = cache_with_worktrees(vec![worktree("jam", "base", Some("main"), "/tmp/jam")]);

    let output = list_output(&cache, None);

    assert!(output.contains("REPO"));
    assert!(output.contains("BASE BRANCH"));
    assert!(output.contains("WORKTREE BRANCH"));
    assert!(output.contains("HEAD"));
    assert!(output.contains("jam"));
    assert!(output.contains("main"));
    assert!(output.contains("/tmp/jam"));
}

#[test]
fn status_default_uses_cached_dirty_and_live_branch_checks() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_git_repo(&repo);

    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: repo.clone(),
        dirty: DirtyState::Dirty { files: 7 },
        branch: Some("deleted-branch".to_string()),
        ..worktree("jam", "base", Some("deleted-branch"), "/tmp/placeholder")
    }]);
    cache.repos[0].canonical_path = repo.clone();

    let output = status_output(&cache);

    assert!(output.contains("dirty(7)"));
    assert!(!output.contains("branch-missing"));
}

#[test]
fn status_default_displays_stale_and_detached_cache_states() {
    let mut wt = worktree(
        "jam",
        "detached",
        None,
        "/definitely/missing/workroot/worktree",
    );
    wt.stale = true;
    wt.detached = true;
    let cache = cache_with_worktrees(vec![wt]);

    let output = status_output(&cache);

    assert!(output.contains("stale"));
    assert!(output.contains("detached"));
}

#[test]
fn status_marks_worktree_stale_when_cached_repo_root_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: worktree_path,
        ..worktree("jam", "base", Some("main"), "/tmp/placeholder")
    }]);
    cache.repos[0].canonical_path = temp.path().join("missing-root");

    let output = status_output(&cache);

    assert!(output.contains("stale"));
}

#[test]
fn radar_status_adds_managed_session_columns_to_worktrees() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("jam");
    std::fs::create_dir_all(&worktree_path).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: worktree_path.clone(),
        ..worktree("jam", "base", Some("main"), "/tmp/jam")
    }]);
    cache.repos[0].canonical_path = worktree_path.clone();
    let sessions = vec![SessionRecord {
        repo_alias: "jam".to_string(),
        target: "base".to_string(),
        worktree_path: worktree_path.clone(),
        backend: SessionBackend::Tmux,
        command: vec!["make".to_string()],
        tmux_session_name: "workroot-jam-base".to_string(),
        status: SessionStatus::Running,
    }];

    let output = radar_output(
        &cache,
        &sessions,
        TmuxInventory {
            available: true,
            panes: vec![TmuxPane {
                session_name: "workroot-jam-base".to_string(),
                current_path: worktree_path,
                current_command: "make".to_string(),
            }],
        },
    );

    assert!(output.contains("SUMMARY"));
    assert!(output.contains("ACTIVE PROCESSES"));
    assert!(output.contains("STATE"));
    assert!(output.contains("RUN"));
    assert!(output.contains("active-panes=1"));
    assert!(output.contains("managed-running=1"));
    assert!(output.contains("SESSION"));
    assert!(output.contains("COMMAND"));
    assert!(output.contains("workroot-jam-base"));
    assert!(output.contains("make"));
}

#[test]
fn radar_json_output_is_stable_machine_contract() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("jam");
    std::fs::create_dir_all(&worktree_path).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: worktree_path.clone(),
        dirty: DirtyState::Clean,
        ..worktree("jam", "base", Some("main"), "/tmp/jam")
    }]);
    cache.repos[0].canonical_path = worktree_path.clone();
    let sessions = vec![SessionRecord {
        repo_alias: "jam".to_string(),
        target: "base".to_string(),
        worktree_path: worktree_path.clone(),
        backend: SessionBackend::Tmux,
        command: vec!["make".to_string()],
        tmux_session_name: "workroot-jam-base".to_string(),
        status: SessionStatus::Running,
    }];

    let output = radar_json_output(
        &cache,
        &sessions,
        TmuxInventory {
            available: true,
            panes: vec![TmuxPane {
                session_name: "workroot-jam-base".to_string(),
                current_path: worktree_path.clone(),
                current_command: "make".to_string(),
            }],
        },
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["summary"]["repos"], 1);
    assert_eq!(json["summary"]["worktrees"], 1);
    assert_eq!(json["summary"]["tmux_available"], true);
    assert_eq!(json["active"][0]["state"], "run");
    assert_eq!(json["active"][0]["repo"], "jam");
    assert_eq!(json["active"][0]["target"], "base");
    assert_eq!(json["active"][0]["session"], "workroot-jam-base");
    assert_eq!(json["active"][0]["command"], "make");
    assert_eq!(
        json["active"][0]["path"],
        worktree_path.display().to_string()
    );
}

#[test]
fn radar_status_surfaces_missing_managed_session_as_attention_exit() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("jam");
    std::fs::create_dir_all(&worktree_path).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: worktree_path.clone(),
        dirty: DirtyState::Clean,
        ..worktree("jam", "base", Some("main"), "/tmp/jam")
    }]);
    cache.repos[0].canonical_path = worktree_path.clone();
    let sessions = vec![SessionRecord {
        repo_alias: "jam".to_string(),
        target: "base".to_string(),
        worktree_path,
        backend: SessionBackend::Tmux,
        command: vec!["make".to_string()],
        tmux_session_name: "workroot-jam-base".to_string(),
        status: SessionStatus::Running,
    }];

    let output = radar_output(
        &cache,
        &sessions,
        TmuxInventory {
            available: true,
            panes: Vec::new(),
        },
    );

    assert!(output.contains("ATTENTION"));
    assert!(output.contains("EXIT"));
    assert!(output.contains("exited=1"));
    assert!(output.contains("workroot-jam-base"));
    assert!(output.contains("make"));
}

#[test]
fn radar_status_maps_unmanaged_tmux_cwd_inside_known_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("jam");
    let nested = worktree_path.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: worktree_path.clone(),
        ..worktree("jam", "base", Some("main"), "/tmp/jam")
    }]);
    cache.repos[0].canonical_path = worktree_path;

    let output = radar_output(
        &cache,
        &[],
        TmuxInventory {
            available: true,
            panes: vec![TmuxPane {
                session_name: "scratch".to_string(),
                current_path: nested,
                current_command: "vim".to_string(),
            }],
        },
    );

    assert!(output.contains("ACTIVE PROCESSES"));
    assert!(output.contains("MAP"));
    assert!(output.contains("scratch"));
    assert!(output.contains("vim"));
    assert!(output.contains("unmapped=0"));
    assert!(output.contains("UNMAPPED TMUX"));
}

#[test]
fn radar_status_renders_unmapped_tmux_sessions_separately() {
    let cache = cache_with_worktrees(vec![worktree("jam", "base", Some("main"), "/tmp/jam")]);

    let output = radar_output(
        &cache,
        &[],
        TmuxInventory {
            available: true,
            panes: vec![TmuxPane {
                session_name: "random".to_string(),
                current_path: PathBuf::from("/tmp/outside"),
                current_command: "bash".to_string(),
            }],
        },
    );

    assert!(output.contains("UNMAPPED TMUX"));
    assert!(output.contains("UNMAPPED"));
    assert!(output.contains("TMUX"));
    assert!(output.contains("random"));
    assert!(output.contains("/tmp/outside"));
    assert!(output.contains("bash"));
}

#[test]
fn radar_status_counts_dirty_and_stale_worktrees_in_attention() {
    let temp = tempfile::tempdir().unwrap();
    let dirty_path = temp.path().join("dirty");
    std::fs::create_dir_all(&dirty_path).unwrap();
    let missing_path = temp.path().join("missing");
    let mut stale = WorktreeRecord {
        path: missing_path.clone(),
        dirty: DirtyState::Clean,
        ..worktree("jam", "gone", Some("main"), "/tmp/missing")
    };
    stale.stale = true;
    let mut cache = cache_with_worktrees(vec![
        WorktreeRecord {
            path: dirty_path.clone(),
            dirty: DirtyState::Dirty { files: 3 },
            ..worktree("jam", "dirty", Some("main"), "/tmp/dirty")
        },
        stale,
    ]);
    cache.repos[0].canonical_path = dirty_path;

    let output = radar_output(
        &cache,
        &[],
        TmuxInventory {
            available: true,
            panes: Vec::new(),
        },
    );

    assert!(output.contains("dirty=1"));
    assert!(output.contains("stale=1"));
    assert!(output.contains("ATTENTION"));
    assert!(output.contains("DIRTY"));
    assert!(output.contains("dirty(3)"));
    assert!(output.contains("STALE"));
    assert!(output.contains(&missing_path.display().to_string()));
}

#[test]
fn radar_status_keeps_worktrees_when_tmux_is_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_path = temp.path().join("jam");
    std::fs::create_dir_all(&worktree_path).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: worktree_path.clone(),
        dirty: DirtyState::Clean,
        ..worktree("jam", "base", Some("main"), "/tmp/jam")
    }]);
    cache.repos[0].canonical_path = worktree_path.clone();
    let sessions = vec![SessionRecord {
        repo_alias: "jam".to_string(),
        target: "base".to_string(),
        worktree_path,
        backend: SessionBackend::Tmux,
        command: vec!["make".to_string()],
        tmux_session_name: "workroot-jam-base".to_string(),
        status: SessionStatus::Running,
    }];

    let output = radar_output(
        &cache,
        &sessions,
        TmuxInventory {
            available: false,
            panes: Vec::new(),
        },
    );

    assert!(output.contains("jam"));
    assert!(output.contains("tmux=unavailable"));
    assert!(output.contains("active-panes=unknown"));
    assert!(output.contains("UNKNOWN"));
    assert!(output.contains("workroot-jam-base"));
    assert!(output.contains("UNMAPPED TMUX"));
}

#[test]
fn sessions_are_reported_in_separate_view() {
    let cache = cache_with_worktrees(vec![worktree("jam", "base", Some("main"), "/tmp/jam")]);
    let sessions = vec![SessionRecord {
        repo_alias: "jam".to_string(),
        target: "base".to_string(),
        worktree_path: PathBuf::from("/tmp/jam"),
        backend: SessionBackend::Tmux,
        command: vec!["make".to_string()],
        tmux_session_name: "definitely-missing-workroot-test-session".to_string(),
        status: SessionStatus::Running,
    }];

    let output = sessions_output(&cache, &sessions);

    assert!(output.contains("STATUS"));
    assert!(output.contains("make"));
    assert!(output.contains("exited") || output.contains("unknown"));
}

#[test]
fn status_refresh_updates_dirty_cache_across_multiple_worktrees() {
    let temp = tempfile::tempdir().unwrap();
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    init_git_repo(&one);
    init_git_repo(&two);
    std::fs::write(one.join("changed.txt"), "dirty").unwrap();

    let storage = FileStorage::new(StoragePaths {
        config: temp.path().join("config.toml"),
        state: temp.path().join("state.json"),
        cache: temp.path().join("cache.json"),
    });
    storage.save_state(&State::default()).unwrap();
    let mut cache = cache_with_worktrees(vec![
        WorktreeRecord {
            path: one.clone(),
            ..worktree("jam", "one", Some("main"), "/tmp/one")
        },
        WorktreeRecord {
            path: two.clone(),
            ..worktree("jam", "two", Some("main"), "/tmp/two")
        },
    ]);
    cache.repos[0].canonical_path = one.clone();
    storage.save_cache(&cache).unwrap();

    let output = refresh_status(&storage, &Git::default()).unwrap();
    let refreshed = storage.load_cache().unwrap();

    assert!(output.contains("dirty(1)"));
    assert_eq!(refreshed.worktrees[0].dirty, DirtyState::Dirty { files: 1 });
    assert_eq!(refreshed.worktrees[1].dirty, DirtyState::Clean);
}

#[test]
fn list_refreshes_dirty_cache_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_git_repo(&repo);
    std::fs::write(repo.join("changed.txt"), "dirty").unwrap();

    let storage = FileStorage::new(StoragePaths {
        config: temp.path().join("config.toml"),
        state: temp.path().join("state.json"),
        cache: temp.path().join("cache.json"),
    });
    storage.save_state(&State::default()).unwrap();
    let mut cache = cache_with_worktrees(vec![WorktreeRecord {
        path: repo.clone(),
        ..worktree("jam", "base", Some("main"), "/tmp/repo")
    }]);
    cache.repos[0].canonical_path = repo.clone();
    storage.save_cache(&cache).unwrap();

    let output =
        workroot::status::list_with_refresh(&storage, &Git::default(), Some("jam")).unwrap();

    assert!(output.contains("dirty(1)"));
}

fn init_git_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-b", "main"]);
}

fn run_git(path: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn cache_with_worktrees(worktrees: Vec<WorktreeRecord>) -> Cache {
    Cache {
        schema_version: 1,
        repos: vec![RepoRecord {
            alias: "jam".to_string(),
            display_name: "jam".to_string(),
            canonical_path: PathBuf::from("/tmp/jam"),
            git_common_dir: PathBuf::from("/tmp/jam/.git"),
            base_branch: Some("main".to_string()),
            source: RepoSource::Adopted,
            stale: false,
        }],
        worktrees,
        last_scan_unix: Some(1),
    }
}

fn worktree(repo: &str, target: &str, branch: Option<&str>, path: &str) -> WorktreeRecord {
    WorktreeRecord {
        repo_alias: repo.to_string(),
        target: target.to_string(),
        display_name: target.to_string(),
        branch: branch.map(str::to_string),
        path: PathBuf::from(path),
        source: WorktreeSource::Manual,
        dirty: DirtyState::Unknown,
        last_seen_unix: Some(1),
        stale: false,
        detached: false,
    }
}
