mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use support::FakeTool;
use workroot::domain::{
    Cache, DirtyState, RepoRecord, RepoSource, SessionBackend, SessionRecord, SessionStatus, State,
    WorktreeRecord, WorktreeSource,
};
use workroot::session::{parse_tmux_list_panes, sanitize_tmux_session_name};
use workroot::storage::{FileStorage, StoragePaths};

fn workroot() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_workroot"))
}

struct TestEnv {
    temp: tempfile::TempDir,
    worktree: PathBuf,
    bin: PathBuf,
    storage: FileStorage,
}

impl TestEnv {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let worktree = temp.path().join("work tree");
        fs::create_dir_all(&worktree).unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let storage = FileStorage::new(StoragePaths {
            config: temp
                .path()
                .join("config")
                .join("workroot")
                .join("config.toml"),
            state: temp
                .path()
                .join("state")
                .join("workroot")
                .join("state.json"),
            cache: temp
                .path()
                .join("cache")
                .join("workroot")
                .join("index.json"),
        });

        let cache = Cache {
            repos: vec![RepoRecord {
                alias: "jam repo".to_string(),
                display_name: "jam".to_string(),
                canonical_path: worktree.clone(),
                git_common_dir: worktree.join(".git"),
                base_branch: Some("main".to_string()),
                source: RepoSource::Adopted,
                stale: false,
            }],
            worktrees: vec![WorktreeRecord {
                repo_alias: "jam repo".to_string(),
                target: "auth/flow".to_string(),
                display_name: "auth".to_string(),
                branch: Some("auth/flow".to_string()),
                path: worktree.clone(),
                source: WorktreeSource::Manual,
                dirty: DirtyState::Unknown,
                last_seen_unix: None,
                stale: false,
                detached: false,
            }],
            ..Cache::default()
        };
        storage.save_cache(&cache).unwrap();

        Self {
            temp,
            worktree,
            bin,
            storage,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(workroot());
        command
            .env("WORKROOT_CONFIG_HOME", self.temp.path().join("config"))
            .env("WORKROOT_STATE_HOME", self.temp.path().join("state"))
            .env("WORKROOT_CACHE_HOME", self.temp.path().join("cache"))
            .env("PATH", &self.bin);
        command
    }

    fn install_tmux(&self) -> PathBuf {
        let log = self.temp.path().join("tmux.log");
        let state = self.temp.path().join("tmux-state");
        fs::create_dir_all(&state).unwrap();
        FakeTool::write(
            &self.bin,
            "tmux",
            r#"#!/bin/sh
set -eu
printf 'CMD' >> "$FAKE_TMUX_LOG"
for arg in "$@"; do
  printf '\t%s' "$arg" >> "$FAKE_TMUX_LOG"
done
printf '\n' >> "$FAKE_TMUX_LOG"
if [ "$1" = "-V" ]; then
  exit 0
fi
if [ "$1" = "has-session" ]; then
  test -f "$FAKE_TMUX_STATE/$3"
  exit $?
fi
if [ "$1" = "new-session" ]; then
  name=""
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "-s" ]; then name="$arg"; fi
    prev="$arg"
  done
  : > "$FAKE_TMUX_STATE/$name"
  exit 0
fi
if [ "$1" = "split-window" ]; then
  if [ "${FAKE_TMUX_SPLIT_FAIL:-}" = "1" ]; then
    exit 1
  fi
  exit 0
fi
if [ "$1" = "attach-session" ]; then
  test -f "$FAKE_TMUX_STATE/$3"
  exit $?
fi
if [ "$1" = "kill-session" ]; then
  name=""
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "-t" ]; then name="$arg"; fi
    prev="$arg"
  done
  rm -rf "$FAKE_TMUX_STATE/$name"
  exit 0
fi
if [ "$1" = "list-panes" ]; then
  if [ -n "${FAKE_TMUX_PANES:-}" ]; then
    /bin/cat "$FAKE_TMUX_PANES"
  fi
  exit 0
fi
exit 1
"#,
        )
        .unwrap();
        log
    }

    fn session_name(&self) -> String {
        sanitize_tmux_session_name("jam repo", "auth/flow")
    }

    fn mark_tmux_session_running(&self) {
        let state = self.temp.path().join("tmux-state");
        fs::create_dir_all(&state).unwrap();
        fs::write(state.join(self.session_name()), "").unwrap();
    }

    fn seed_session(&self, command: Vec<String>, status: SessionStatus) {
        self.storage
            .save_state(&State {
                sessions: vec![SessionRecord {
                    repo_alias: "jam repo".to_string(),
                    target: "auth/flow".to_string(),
                    worktree_path: self.worktree.clone(),
                    backend: SessionBackend::Tmux,
                    command,
                    tmux_session_name: self.session_name(),
                    status,
                }],
                ..State::default()
            })
            .unwrap();
    }
}

#[test]
fn run_executes_foreground_in_selected_worktree_and_preserves_argv() {
    let env = TestEnv::new();
    let log = env.temp.path().join("argv.log");
    FakeTool::write(
        &env.bin,
        "capture-argv",
        r#"#!/bin/sh
printf 'cwd=%s\n' "$PWD" > "$CAPTURE_ARGV_LOG"
for arg in "$@"; do
  printf 'arg=%s\n' "$arg" >> "$CAPTURE_ARGV_LOG"
done
"#,
    )
    .unwrap();

    let output = env
        .command()
        .args([
            "worktree",
            "run",
            "jam repo",
            "auth/flow",
            "--",
            "capture-argv",
        ])
        .arg("space value")
        .arg("it's")
        .arg("$literal")
        .arg("semi;colon")
        .arg("")
        .env("CAPTURE_ARGV_LOG", &log)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(log).unwrap();
    let cwd = log.lines().next().unwrap().strip_prefix("cwd=").unwrap();
    assert_eq!(
        fs::canonicalize(cwd).unwrap(),
        fs::canonicalize(&env.worktree).unwrap()
    );
    assert!(log.contains("arg=space value\n"));
    assert!(log.contains("arg=it's\n"));
    assert!(log.contains("arg=$literal\n"));
    assert!(log.contains("arg=semi;colon\n"));
    assert!(log.ends_with("arg=\n"));
    assert!(env.storage.load_state().unwrap().sessions.is_empty());
}

#[test]
fn pair_creates_command_and_shell_panes_with_tmux_boundary_quoting() {
    let env = TestEnv::new();
    let tmux_log = env.install_tmux();

    let output = env
        .command()
        .args(["tmux", "pair", "jam repo", "auth/flow", "--", "runner"])
        .arg("run this")
        .arg("it's")
        .arg("$literal")
        .arg("semi;colon")
        .env("FAKE_TMUX_LOG", &tmux_log)
        .env("FAKE_TMUX_STATE", env.temp.path().join("tmux-state"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(tmux_log).unwrap();
    let session_name = env.session_name();
    assert!(log.contains(&format!("CMD\tnew-session\t-d\t-s\t{session_name}\t-c")));
    assert!(log.contains(&env.worktree.display().to_string()));
    assert!(log.contains("runner 'run this' 'it'\\''s' '$literal' 'semi;colon'"));
    assert!(log.contains(&format!("CMD\tsplit-window\t-h\t-t\t{session_name}\t-c")));
    assert!(log.contains(&format!("CMD\tattach-session\t-t\t{session_name}")));

    let state = env.storage.load_state().unwrap();
    assert_eq!(state.sessions.len(), 1);
    assert_eq!(
        state.sessions[0].command,
        vec!["runner", "run this", "it's", "$literal", "semi;colon"]
    );
    assert_eq!(state.sessions[0].status, SessionStatus::Running);
}

#[test]
fn run_creates_managed_tmux_session_for_target() {
    let env = TestEnv::new();
    let tmux_log = env.install_tmux();

    let output = env
        .command()
        .args(["run", "jam repo", "auth/flow", "--", "runner"])
        .env("FAKE_TMUX_LOG", &tmux_log)
        .env("FAKE_TMUX_STATE", env.temp.path().join("tmux-state"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(tmux_log).unwrap();
    let session_name = env.session_name();
    assert!(log.contains(&format!("CMD\tnew-session\t-d\t-s\t{session_name}\t-c")));
    assert!(log.contains(&format!("CMD\tattach-session\t-t\t{session_name}")));
    assert_eq!(env.storage.load_state().unwrap().sessions.len(), 1);
}

#[test]
fn pair_existing_managed_session_warns_on_different_command_and_attaches() {
    let env = TestEnv::new();
    let tmux_log = env.install_tmux();
    env.mark_tmux_session_running();
    env.seed_session(vec!["existing-tool".to_string()], SessionStatus::Running);

    let output = env
        .command()
        .args(["tmux", "pair", "jam repo", "auth/flow", "--", "runner"])
        .env("FAKE_TMUX_LOG", &tmux_log)
        .env("FAKE_TMUX_STATE", env.temp.path().join("tmux-state"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning: existing Workroot session"));
    let log = fs::read_to_string(tmux_log).unwrap();
    assert!(!log.contains("new-session"));
    assert!(log.contains("attach-session"));
}

#[test]
fn pair_cleans_up_orphan_tmux_session_when_split_fails() {
    let env = TestEnv::new();
    let tmux_log = env.install_tmux();

    let output = env
        .command()
        .args(["tmux", "pair", "jam repo", "auth/flow", "--", "runner"])
        .env("FAKE_TMUX_LOG", &tmux_log)
        .env("FAKE_TMUX_STATE", env.temp.path().join("tmux-state"))
        .env("FAKE_TMUX_SPLIT_FAIL", "1")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let log = fs::read_to_string(tmux_log).unwrap();
    let session_name = env.session_name();
    assert!(log.contains(&format!("CMD\tkill-session\t-t\t{session_name}")));
    assert!(env.storage.load_state().unwrap().sessions.is_empty());
}

#[test]
fn attach_marks_killed_managed_session_exited() {
    let env = TestEnv::new();
    let tmux_log = env.install_tmux();
    env.seed_session(vec!["make".to_string()], SessionStatus::Running);

    let output = env
        .command()
        .args(["tmux", "attach", "jam repo", "auth/flow"])
        .env("FAKE_TMUX_LOG", &tmux_log)
        .env("FAKE_TMUX_STATE", env.temp.path().join("tmux-state"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(
        env.storage.load_state().unwrap().sessions[0].status,
        SessionStatus::Exited
    );
}

#[test]
fn missing_tmux_blocks_pair_but_run_still_works() {
    let env = TestEnv::new();
    let log = env.temp.path().join("ran");
    FakeTool::write(&env.bin, "ok-tool", "#!/bin/sh\n: > \"$RUN_LOG\"\n").unwrap();

    let pair = env
        .command()
        .args(["tmux", "pair", "jam repo", "auth/flow", "--", "ok-tool"])
        .output()
        .unwrap();
    assert!(!pair.status.success());
    assert!(String::from_utf8_lossy(&pair.stderr).contains("missing required dependency `tmux`"));

    let run = env
        .command()
        .args(["worktree", "run", "jam repo", "auth/flow", "--", "ok-tool"])
        .env("RUN_LOG", &log)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(Path::new(&log).exists());
}

#[test]
fn status_renders_managed_and_unmapped_tmux_inventory() {
    let env = TestEnv::new();
    let tmux_log = env.install_tmux();
    env.seed_session(vec!["make".to_string()], SessionStatus::Running);
    let panes = env.temp.path().join("panes.txt");
    fs::write(
        &panes,
        format!(
            "{}\t{}\tmake\nrandom\t/tmp/outside\tbash\n",
            env.session_name(),
            env.worktree.display()
        ),
    )
    .unwrap();

    let output = env
        .command()
        .args(["status"])
        .env("FAKE_TMUX_LOG", &tmux_log)
        .env("FAKE_TMUX_STATE", env.temp.path().join("tmux-state"))
        .env("FAKE_TMUX_PANES", &panes)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ACTIVE PROCESSES"));
    assert!(stdout.contains("RUN"));
    assert!(stdout.contains(&env.session_name()));
    assert!(stdout.contains("UNMAPPED TMUX"));
    assert!(stdout.contains("UNMAPPED"));
    assert!(stdout.contains("random"));
    assert!(stdout.contains("/tmp/outside"));
}

#[test]
fn tmux_names_avoid_separator_and_truncation_collisions() {
    let slash = sanitize_tmux_session_name("jam", "auth/flow");
    let underscore = sanitize_tmux_session_name("jam", "auth_flow");
    assert_ne!(slash, underscore);
    assert!(!slash.contains(':'));
    assert!(slash.len() <= 80);

    let long_a = sanitize_tmux_session_name("repo", &format!("{}a", "x".repeat(120)));
    let long_b = sanitize_tmux_session_name("repo", &format!("{}b", "x".repeat(120)));
    assert_ne!(long_a, long_b);
    assert!(long_a.len() <= 80);
    assert!(long_b.len() <= 80);
}

#[test]
fn parses_tmux_list_panes_inventory() {
    let panes = parse_tmux_list_panes(
        "workroot-jam-base\t/tmp/jam\tmake\nscratch\t/tmp/other src\tbash\nbad-row\n",
    );

    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0].session_name, "workroot-jam-base");
    assert_eq!(panes[0].current_path, PathBuf::from("/tmp/jam"));
    assert_eq!(panes[0].current_command, "make");
    assert_eq!(panes[1].session_name, "scratch");
    assert_eq!(panes[1].current_path, PathBuf::from("/tmp/other src"));
    assert_eq!(panes[1].current_command, "bash");
}
