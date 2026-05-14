use std::path::PathBuf;
use std::process::Command;

use clap::{CommandFactory, Parser};
use workroot::cli::{Cli, Commands, OutputFormat, TmuxCommand, WorktreeCommand};
use workroot::domain::{
    CURRENT_SCHEMA_VERSION, Cache, DirtyState, RepoRecord, RepoSource, State, WorktreeRecord,
    WorktreeSource,
};
use workroot::git::parse_worktree_porcelain;
use workroot::resolver::Resolver;
use workroot::session::{CommandSpec, sanitize_tmux_session_name};
use workroot::shell::{Shell, shell_init};
use workroot::storage::{FileStorage, StoragePaths};

#[test]
fn shell_init_wraps_cd_for_supported_shells() {
    for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
        let init = shell_init(shell);
        assert!(init.contains("workroot"));
        assert!(init.contains("command workroot worktree path"));
        assert!(init.contains("command workroot worktree new"));
        assert!(init.contains("cd"));
        assert!(
            init.contains("wr"),
            "{shell:?} init missing wr alias:\n{init}"
        );
    }
}

#[test]
fn workroot_binary_help_uses_workroot() {
    let output = Command::new(env!("CARGO_BIN_EXE_workroot"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Usage: workroot <COMMAND>"));
    assert!(help.contains("workroot new [-o json] <project> <worktree>"));
}

#[test]
fn wr_binary_is_available_as_shorthand() {
    let output = Command::new(env!("CARGO_BIN_EXE_wr"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
}

#[test]
fn command_spec_preserves_argv_and_serializes_only_for_shell_boundary() {
    let spec = CommandSpec::new(vec![
        "runner".to_string(),
        "run this".to_string(),
        "it's".to_string(),
        "$literal".to_string(),
    ])
    .unwrap();

    assert_eq!(spec.argv()[1], "run this");
    assert_eq!(
        spec.to_posix_shell_command(),
        "runner 'run this' 'it'\\''s' '$literal'"
    );
}

#[test]
fn empty_command_spec_is_rejected() {
    assert!(CommandSpec::new(Vec::new()).is_err());
}

#[test]
fn tmux_session_names_are_sanitized_but_alias_based() {
    let name = sanitize_tmux_session_name("my repo", "auth/flow");
    assert!(name.starts_with("workroot-my_repo-auth_flow-"));
    assert!(!name.contains(':'));
    assert!(name.len() <= 80);
}

#[test]
fn parses_git_worktree_porcelain() {
    let entries = parse_worktree_porcelain(
        "worktree /tmp/base\nHEAD abc123\nbranch refs/heads/main\n\nworktree /tmp/wt\nHEAD def456\ndetached\n\n",
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, PathBuf::from("/tmp/base"));
    assert_eq!(entries[0].branch.as_deref(), Some("main"));
    assert!(entries[1].detached);
}

#[test]
fn storage_round_trips_state_with_atomic_json() {
    let temp = tempfile::tempdir().unwrap();
    let storage = FileStorage::new(StoragePaths {
        config: temp.path().join("config.toml"),
        state: temp.path().join("state.json"),
        cache: temp.path().join("cache.json"),
    });

    let mut state = State::default();
    state.adopted_paths.push(PathBuf::from("/tmp/project"));
    storage.save_state(&state).unwrap();

    let loaded = storage.load_state().unwrap();
    assert_eq!(loaded.adopted_paths, vec![PathBuf::from("/tmp/project")]);
}

#[test]
fn cli_accepts_trailing_command_after_double_dash() {
    let cli =
        Cli::try_parse_from(["workroot", "run", "jam", "auth", "--", "runner", "--flag"]).unwrap();
    match cli.command {
        Commands::Run {
            repo,
            target,
            command,
        } => {
            assert_eq!(repo, "jam");
            assert_eq!(target, "auth");
            assert_eq!(command, vec!["runner", "--flag"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn cli_accepts_cd_as_direct_path_lookup() {
    let cli = Cli::try_parse_from(["workroot", "worktree", "cd", "jam", "auth"]).unwrap();
    match cli.command {
        Commands::Worktree {
            command: WorktreeCommand::Cd { repo, target },
        } => {
            assert_eq!(repo, "jam");
            assert_eq!(target.as_deref(), Some("auth"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn cli_accepts_list_repo_filters() {
    let positional = Cli::try_parse_from(["workroot", "worktree", "list", "jam"]).unwrap();
    match positional.command {
        Commands::Worktree {
            command: WorktreeCommand::List { repo, project, .. },
        } => {
            assert_eq!(repo.as_deref(), Some("jam"));
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let long = Cli::try_parse_from(["workroot", "worktree", "list", "--project", "jam"]).unwrap();
    match long.command {
        Commands::Worktree {
            command: WorktreeCommand::List { repo, project, .. },
        } => {
            assert_eq!(repo, None);
            assert_eq!(project.as_deref(), Some("jam"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn public_top_level_commands_parse_and_appear_in_help() {
    let help = Cli::command().render_help().to_string();
    assert!(help.contains("Machine-wide switchboard for git worktrees"));
    assert!(help.contains("Usage: workroot <COMMAND>"));
    assert!(help.contains("Worktree lifecycle:"));
    assert!(help.contains("Navigation:"));
    assert!(help.contains("Execution:"));
    assert!(help.contains("Shell integration:"));
    assert!(help.contains("Getting started"));
    assert!(help.contains("workroot discover ~/projects/my-app"));
    assert!(help.contains("workroot new my-app my-feature"));
    assert!(help.contains("GitHub: https://github.com/fridiculous/workroot"));
    assert!(help.contains("new          Create a target worktree from the repo base branch"));
    assert!(help.contains("workroot new [-o json] <project> <worktree>"));
    assert!(help.contains("push         Push a target branch to its remote"));
    assert!(help.contains("workroot push [-o json] <project> <worktree>"));
    assert!(help.contains("status       Show worktrees; --json for scripts"));
    assert!(help.contains("workroot status [-o json|--json] [--refresh] [<project> [<worktree>]]"));
    assert!(
        help.contains("discover     Index repos from configured roots or from one explicit path")
    );
    assert!(help.contains("workroot discover [<path>]"));
    assert!(help.contains("ignore       Hide a repo from Workroot and future discovery"));
    assert!(help.contains("workroot ignore <project-or-path>"));
    assert!(help.contains("unignore     Allow a previously ignored repo to appear again"));
    assert!(help.contains("workroot unignore <project-or-path>"));
    assert!(help.contains("shell-init   Print shell integration for zsh, bash, or fish"));
    assert!(help.contains("complete     Print completion candidates for shell wrappers"));
    assert!(!help.contains("Setup:"));
    assert!(!help.contains("Commands:"));
    assert!(!help.contains("Daily flow:"));
    assert!(!help.contains("Move:"));
    assert!(!help.contains("Names:"));
    assert!(!help.contains("Examples:"));
    for example in [
        "workroot discover ~/projects/my-app",
        "workroot new my-app my-feature",
        "workroot run my-app my-feature -- make test",
        "workroot push my-app my-feature",
    ] {
        assert!(help.contains(example), "help missing example {example}");
    }
    assert!(!help.contains("Automation:"));
    for command in [
        "status", "discover", "ignore", "unignore", "cd", "path", "new", "run", "push", "prune",
    ] {
        assert!(help.contains(command), "help missing {command}");
    }
    assert!(
        !help.contains("workroot worktree"),
        "help exposes worktree namespace"
    );
    assert!(
        !help.contains("workroot tmux"),
        "help exposes tmux namespace"
    );
    assert!(
        !help.contains("workroot scan"),
        "help still exposes workroot scan"
    );
    assert!(
        !help.contains("workroot prune-merged"),
        "help still exposes workroot prune-merged"
    );
    assert!(
        !help.contains("workroot pair <REPO>"),
        "help still exposes top-level workroot pair"
    );
    assert!(
        !help.contains("workroot attach <REPO>"),
        "help still exposes top-level workroot attach"
    );

    assert!(Cli::try_parse_from(["workroot", "scan"]).is_err());
    assert!(Cli::try_parse_from(["workroot", "discover"]).is_ok());
    assert!(Cli::try_parse_from(["workroot", "discover", "/tmp/repo"]).is_ok());
    assert!(Cli::try_parse_from(["workroot", "ignore", "jam"]).is_ok());
    assert!(Cli::try_parse_from(["workroot", "unignore", "jam"]).is_ok());
    assert!(Cli::try_parse_from(["workroot", "prune-merged"]).is_err());
    match Cli::try_parse_from(["workroot", "status", "jam", "auth"])
        .unwrap()
        .command
    {
        Commands::Status { repo, target, .. } => {
            assert_eq!(repo.as_deref(), Some("jam"));
            assert_eq!(target.as_deref(), Some("auth"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert!(matches!(
        Cli::try_parse_from(["workroot", "cd", "jam", "auth"])
            .unwrap()
            .command,
        Commands::Cd { .. }
    ));
    assert!(matches!(
        Cli::try_parse_from(["workroot", "path", "jam", "auth"])
            .unwrap()
            .command,
        Commands::Path { .. }
    ));
    match Cli::try_parse_from(["workroot", "path", "-o", "json", "jam", "auth"])
        .unwrap()
        .command
    {
        Commands::Path {
            output,
            repo,
            target,
        } => {
            assert_eq!(output, Some(OutputFormat::Json));
            assert_eq!(repo, "jam");
            assert_eq!(target.as_deref(), Some("auth"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert!(matches!(
        Cli::try_parse_from(["workroot", "new", "jam", "feature"])
            .unwrap()
            .command,
        Commands::New { .. }
    ));
    match Cli::try_parse_from(["workroot", "new", "--output", "json", "jam", "feature"])
        .unwrap()
        .command
    {
        Commands::New {
            output,
            repo,
            target,
        } => {
            assert_eq!(output, Some(OutputFormat::Json));
            assert_eq!(repo, "jam");
            assert_eq!(target, "feature");
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert!(matches!(
        Cli::try_parse_from(["workroot", "run", "jam", "auth", "--", "runner"])
            .unwrap()
            .command,
        Commands::Run { .. }
    ));
    assert!(matches!(
        Cli::try_parse_from(["workroot", "push", "jam", "auth"])
            .unwrap()
            .command,
        Commands::Push { .. }
    ));
    match Cli::try_parse_from(["workroot", "push", "-o", "json", "jam", "auth"])
        .unwrap()
        .command
    {
        Commands::Push {
            output,
            repo,
            target,
        } => {
            assert_eq!(output, Some(OutputFormat::Json));
            assert_eq!(repo, "jam");
            assert_eq!(target, "auth");
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert!(matches!(
        Cli::try_parse_from(["workroot", "prune", "jam", "auth"])
            .unwrap()
            .command,
        Commands::Prune { .. }
    ));
}

#[test]
fn top_level_discover_ignore_and_unignore_parse() {
    assert!(matches!(
        Cli::try_parse_from(["workroot", "discover"])
            .unwrap()
            .command,
        Commands::Discover { path: None }
    ));
    assert!(matches!(
        Cli::try_parse_from(["workroot", "discover", "/tmp/repo"])
            .unwrap()
            .command,
        Commands::Discover { path: Some(_) }
    ));
    assert!(matches!(
        Cli::try_parse_from(["workroot", "ignore", "jam"])
            .unwrap()
            .command,
        Commands::Ignore { .. }
    ));
    assert!(matches!(
        Cli::try_parse_from(["workroot", "unignore", "jam"])
            .unwrap()
            .command,
        Commands::Unignore { .. }
    ));
}

#[test]
fn command_help_teaches_examples_and_json() {
    let mut command = Cli::command();
    let status_help = command
        .find_subcommand_mut("status")
        .unwrap()
        .render_help()
        .to_string();
    assert!(status_help.contains("--json"));
    assert!(status_help.contains("--output <FORMAT>"));
    assert!(
        status_help.contains("workroot status [-o json|--json] [--refresh] <project> <worktree>")
    );

    let run_help = command
        .find_subcommand_mut("run")
        .unwrap()
        .render_help()
        .to_string();
    assert!(run_help.contains("workroot run <project> <worktree> -- <CMD>..."));

    let push_help = command
        .find_subcommand_mut("push")
        .unwrap()
        .render_help()
        .to_string();
    assert!(push_help.contains("workroot push [-o json] <project> <worktree>"));
}

#[test]
fn nested_tmux_commands_remain_available() {
    let cli = Cli::try_parse_from(["workroot", "tmux", "list"]).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Tmux {
            command: TmuxCommand::List
        }
    ));
}

#[test]
fn resolver_contract_types_can_represent_collisions_and_stale_entries() {
    let repo = RepoRecord {
        alias: "jam".to_string(),
        display_name: "jam".to_string(),
        canonical_path: PathBuf::from("/tmp/jam"),
        git_common_dir: PathBuf::from("/tmp/jam/.git"),
        base_branch: Some("main".to_string()),
        source: RepoSource::Adopted,
        stale: false,
    };
    let worktree = WorktreeRecord {
        repo_alias: repo.alias.clone(),
        target: "base-2".to_string(),
        display_name: "base".to_string(),
        branch: Some("feature".to_string()),
        path: PathBuf::from("/tmp/jam/base"),
        source: WorktreeSource::Manual,
        dirty: DirtyState::Unknown,
        last_seen_unix: None,
        stale: true,
        detached: false,
    };

    assert_eq!(repo.alias, "jam");
    assert_eq!(worktree.target, "base-2");
    assert!(worktree.stale);
}

#[test]
fn resolver_resolves_exact_repo_and_target_from_cache() {
    let repo = repo_record("jam", "jam", "/tmp/jam");
    let worktree = worktree_record("jam", "auth", "/tmp/jam-auth", false);
    let resolver = Resolver::new(Cache {
        schema_version: CURRENT_SCHEMA_VERSION,
        repos: vec![repo],
        worktrees: vec![worktree],
        last_scan_unix: None,
    });

    let resolved = resolver.resolve_worktree("jam", Some("auth")).unwrap();
    assert_eq!(resolved.path, PathBuf::from("/tmp/jam-auth"));
}

#[test]
fn resolver_reports_ambiguous_repo_matches() {
    let resolver = Resolver::new(Cache {
        schema_version: CURRENT_SCHEMA_VERSION,
        repos: vec![
            repo_record("jam", "shared", "/tmp/jam-one"),
            repo_record("jam-2", "shared", "/tmp/jam-two"),
        ],
        worktrees: Vec::new(),
        last_scan_unix: None,
    });

    let error = resolver.resolve_repo("shared").unwrap_err().to_string();
    assert!(error.contains("repo `shared` is ambiguous"));
    assert!(error.contains("fix: pass the exact repo alias"));
    assert!(error.contains("jam"));
    assert!(error.contains("jam-2"));
}

#[test]
fn resolver_prefers_exact_alias_over_display_name_collision() {
    let resolver = Resolver::new(Cache {
        schema_version: CURRENT_SCHEMA_VERSION,
        repos: vec![
            repo_record("jam", "jam", "/tmp/jam-one"),
            repo_record("jam-2", "jam", "/tmp/jam-two"),
        ],
        worktrees: vec![
            worktree_record("jam", "base", "/tmp/jam-one", false),
            worktree_record("jam-2", "base", "/tmp/jam-two", false),
        ],
        last_scan_unix: None,
    });

    let resolved = resolver.resolve_worktree("jam", Some("base")).unwrap();
    assert_eq!(resolved.repo.alias, "jam");
    assert_eq!(resolved.path, PathBuf::from("/tmp/jam-one"));
}

#[test]
fn resolver_reports_non_tty_target_ambiguity_without_stdout_choice() {
    let resolver = Resolver::new(Cache {
        schema_version: CURRENT_SCHEMA_VERSION,
        repos: vec![repo_record("jam", "jam", "/tmp/jam")],
        worktrees: vec![
            worktree_record("jam", "base", "/tmp/jam", false),
            worktree_record("jam", "auth", "/tmp/jam-auth", false),
        ],
        last_scan_unix: None,
    });

    let error = resolver
        .resolve_worktree("jam", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("target for repo `jam` is ambiguous"));
    assert!(error.contains("fix: pass the exact target name"));
    assert!(error.contains("base"));
    assert!(error.contains("auth"));
}

#[test]
fn resolver_completion_uses_cache_records_only() {
    let resolver = Resolver::new(Cache {
        schema_version: CURRENT_SCHEMA_VERSION,
        repos: vec![
            repo_record("jam", "jam", "/tmp/jam"),
            repo_record("other", "other", "/tmp/other"),
        ],
        worktrees: vec![
            worktree_record("jam", "base", "/tmp/jam", false),
            worktree_record("jam", "auth", "/tmp/jam-auth", false),
            worktree_record("jam", "old", "/tmp/jam-old", true),
        ],
        last_scan_unix: None,
    });

    assert_eq!(resolver.complete_repos(Some("ja")), vec!["jam"]);
    assert_eq!(
        resolver.complete_targets("jam", Some("a")).unwrap(),
        vec!["auth"]
    );
}

fn repo_record(alias: &str, display_name: &str, path: &str) -> RepoRecord {
    RepoRecord {
        alias: alias.to_string(),
        display_name: display_name.to_string(),
        canonical_path: PathBuf::from(path),
        git_common_dir: PathBuf::from(path).join(".git"),
        base_branch: Some("main".to_string()),
        source: RepoSource::Adopted,
        stale: false,
    }
}

fn worktree_record(repo_alias: &str, target: &str, path: &str, stale: bool) -> WorktreeRecord {
    WorktreeRecord {
        repo_alias: repo_alias.to_string(),
        target: target.to_string(),
        display_name: target.to_string(),
        branch: Some(target.to_string()),
        path: PathBuf::from(path),
        source: WorktreeSource::Manual,
        dirty: DirtyState::Unknown,
        last_seen_unix: None,
        stale,
        detached: false,
    }
}
