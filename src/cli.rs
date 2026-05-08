use std::path::Path;

use clap::{Parser, Subcommand, ValueEnum};

use crate::discovery;
use crate::domain::SessionStatus;
use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::prune::{prune_merged_interactive, prune_report};
use crate::resolver::Resolver;
use crate::session::{
    CommandSpec, ExistingSession, Tmux, find_session_mut, sanitize_tmux_session_name,
    upsert_running_session,
};
use crate::shell::{Shell, shell_init};
use crate::status::{
    list_with_refresh, radar_json_with_refresh, radar_json_with_storage, radar_with_refresh,
    radar_with_storage, sessions_output,
};
use crate::storage::FileStorage;

const GLOBAL_HELP: &str = r#"Worktree lifecycle:
  [1mnew[0m          Create a target worktree from the repo base branch
               workroot new <project> <worktree>

  [1mpush[0m         Push a target branch to its remote
               workroot push <project> <worktree>

  [1mprune[0m        Remove merged worktrees with proof and confirmation
               workroot prune [<project> [<worktree>]]

Navigation:
  [1mstatus[0m       Show worktrees; --json for scripts
               workroot status [--json] [--refresh] [<project> [<worktree>]]

  [1mpath[0m         Print a target path for scripts and command substitution
               workroot path <project> [<worktree>]

  [1mcd[0m           Change directory through shell integration
               workroot cd <project> [<worktree>]

Repo management:
  [1mdiscover[0m     Index repos from configured roots or from one explicit path
               workroot discover [<path>]

  [1mignore[0m       Hide a repo from Workroot and future discovery
               workroot ignore <project-or-path>

  [1munignore[0m     Allow a previously ignored repo to appear again
               workroot unignore <project-or-path>

Execution:
  [1mrun[0m          Start or rejoin a managed tmux session
               workroot run <project> <worktree> -- <CMD>...

Shell integration:
  [1mshell-init[0m   Print shell integration for zsh, bash, or fish
               workroot shell-init <shell>
  [1mcomplete[0m     Print completion candidates for shell wrappers

Getting started

  workroot discover ~/projects/my-app
  workroot new my-app my-feature
  workroot run my-app my-feature -- make test
  workroot push my-app my-feature

Run `workroot shell-init <shell>` to set up directory switching.
GitHub: https://github.com/fridiculous/workroot"#;

const STATUS_HELP: &str = r#"Command shapes:
  workroot status [--json] [--refresh]
  workroot status [--json] [--refresh] <project>
  workroot status [--json] [--refresh] <project> <worktree>

Examples:
  workroot status --json my-app my-feature
  workroot status --refresh my-app"#;

const PATH_HELP: &str = r#"Command shapes:
  workroot path <project>
  workroot path <project> <worktree>

Examples:
  workroot path my-app my-feature"#;

const CD_HELP: &str = r##"Command shapes:
  workroot cd <project>
  workroot cd <project> <worktree>

Examples:
  workroot cd my-app my-feature

Install shell integration first:
  eval "$(workroot shell-init zsh)""##;

const NEW_HELP: &str = r#"Command shape:
  workroot new <project> <worktree>

Examples:
  workroot new my-app my-feature

If the repo is not known yet, first run:
  workroot discover
or point Workroot directly at it:
  workroot discover /path/to/repo

Shell integration changes the current shell directory after creating the worktree. Direct binary use prints the path because a child process cannot cd its parent shell."#;

const RUN_HELP: &str = r#"Command shape:
  workroot run <project> <worktree> -- <CMD>...

Examples:
  workroot run my-app my-feature -- make test
  workroot run my-app my-feature -- ./dev.sh

If a managed session already exists, Workroot attaches instead of replacing it."#;

const PUSH_HELP: &str = r#"Command shape:
  workroot push <project> <worktree>

Examples:
  workroot push my-app my-feature

If the branch has no upstream, Workroot pushes with `-u origin <branch>`. Otherwise it runs a normal `git push`."#;

const PRUNE_HELP: &str = r#"Command shapes:
  workroot prune
  workroot prune <project>
  workroot prune <project> <worktree>

Examples:
  workroot prune my-app my-feature

Workroot shows merge proof before each removal and asks for confirmation."#;

#[derive(Debug, Parser)]
#[command(
    name = "workroot",
    bin_name = "workroot",
    about = "Machine-wide switchboard for git worktrees",
    help_template = "{about}\n\nUsage: {usage}{after-help}\n\nOptions:\n{options}",
    after_help = GLOBAL_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommand,
    },
    #[command(hide = true)]
    Workdir {
        #[command(subcommand)]
        command: WorktreeCommand,
    },
    Tmux {
        #[command(subcommand)]
        command: TmuxCommand,
    },
    ShellInit {
        shell: ShellName,
    },
    #[command(hide = true)]
    List {
        #[arg(long, hide = true)]
        refresh: bool,
        #[arg(long, alias = "repo", value_name = "REPO")]
        project: Option<String>,
        repo: Option<String>,
    },
    #[command(
        about = "Show the global worktree and process radar",
        override_usage = "workroot status [--json] [--refresh] [<project> [<worktree>]]",
        after_help = STATUS_HELP
    )]
    Status {
        #[arg(long, help = "Refresh cached dirty state before rendering")]
        refresh: bool,
        #[arg(long, help = "Print stable machine-readable JSON")]
        json: bool,
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name to filter"
        )]
        repo: Option<String>,
        #[arg(
            value_name = "WORKTREE",
            help = "Worktree target or display name to filter"
        )]
        target: Option<String>,
    },
    #[command(hide = true)]
    Audit,
    #[command(hide = true)]
    Sessions,
    #[command(
        about = "Push a target branch to its remote",
        override_usage = "workroot push <project> <worktree>",
        after_help = PUSH_HELP
    )]
    Push {
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name"
        )]
        repo: String,
        #[arg(value_name = "WORKTREE", help = "Worktree target or display name")]
        target: String,
    },
    #[command(
        about = "Safely remove worktrees proven merged",
        override_usage = "workroot prune [<project> [<worktree>]]",
        after_help = PRUNE_HELP
    )]
    Prune {
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name to filter"
        )]
        repo: Option<String>,
        #[arg(
            value_name = "WORKTREE",
            help = "Worktree target or display name to filter"
        )]
        target: Option<String>,
    },
    #[command(
        about = "Print a worktree path for scripts",
        override_usage = "workroot path <project> [<worktree>]",
        after_help = PATH_HELP
    )]
    Path {
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name"
        )]
        repo: String,
        #[arg(value_name = "WORKTREE", help = "Worktree target or display name")]
        target: Option<String>,
    },
    #[command(
        about = "Change into a worktree through shell integration",
        override_usage = "workroot cd <project> [<worktree>]",
        after_help = CD_HELP
    )]
    Cd {
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name"
        )]
        repo: String,
        #[arg(value_name = "WORKTREE", help = "Worktree target or display name")]
        target: Option<String>,
    },
    #[command(
        about = "Create a new worktree target",
        override_usage = "workroot new <project> <worktree>",
        after_help = NEW_HELP
    )]
    New {
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name"
        )]
        repo: String,
        #[arg(value_name = "WORKTREE", help = "New worktree target and branch name")]
        target: String,
    },
    #[command(
        about = "Index repos from configured roots or one explicit path",
        override_usage = "workroot discover [<path>]"
    )]
    Discover {
        #[arg(
            value_name = "PATH",
            help = "Optional path to one repo or worktree family"
        )]
        path: Option<std::path::PathBuf>,
    },
    #[command(
        about = "Hide a repo from Workroot and future discovery",
        override_usage = "workroot ignore <project-or-path>"
    )]
    Ignore {
        #[arg(
            value_name = "PROJECT_OR_PATH",
            help = "Repo alias, display name, or filesystem path"
        )]
        repo: String,
    },
    #[command(
        about = "Allow a previously ignored repo to appear again",
        override_usage = "workroot unignore <project-or-path>"
    )]
    Unignore {
        #[arg(
            value_name = "PROJECT_OR_PATH",
            help = "Ignored repo alias or filesystem path"
        )]
        repo: String,
    },
    #[command(hide = true)]
    Adopt {
        path: std::path::PathBuf,
    },
    #[command(
        about = "Start or rejoin managed work in tmux",
        override_usage = "workroot run <project> <worktree> -- <CMD>...",
        after_help = RUN_HELP
    )]
    Run {
        #[arg(
            value_name = "PROJECT",
            help = "Project name, repo alias, or display name"
        )]
        repo: String,
        #[arg(value_name = "WORKTREE", help = "Worktree target or display name")]
        target: String,
        #[arg(
            required = true,
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "CMD",
            help = "Command to run inside the worktree"
        )]
        command: Vec<String>,
    },
    #[command(hide = true)]
    Pair {
        repo: String,
        target: String,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    #[command(hide = true)]
    Attach {
        repo: String,
        target: String,
    },
    #[command(hide = true)]
    Complete {
        kind: CompleteKind,
        repo: Option<String>,
        prefix: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum WorktreeCommand {
    Scan,
    List {
        #[arg(long, hide = true)]
        refresh: bool,
        #[arg(long, alias = "repo", value_name = "REPO")]
        project: Option<String>,
        repo: Option<String>,
    },
    Audit,
    #[command(alias = "prune-merged")]
    Prune {
        repo: Option<String>,
        target: Option<String>,
    },
    Path {
        repo: String,
        target: Option<String>,
    },
    Cd {
        repo: String,
        target: Option<String>,
    },
    New {
        repo: String,
        target: String,
    },
    Push {
        repo: String,
        target: String,
    },
    Adopt {
        path: std::path::PathBuf,
    },
    Run {
        repo: String,
        target: String,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum TmuxCommand {
    List,
    Pair {
        repo: String,
        target: String,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    Attach {
        repo: String,
        target: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ShellName {
    Zsh,
    Bash,
    Fish,
}

impl From<ShellName> for Shell {
    fn from(value: ShellName) -> Self {
        match value {
            ShellName::Zsh => Shell::Zsh,
            ShellName::Bash => Shell::Bash,
            ShellName::Fish => Shell::Fish,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CompleteKind {
    Repos,
    Targets,
}

pub fn run(cli: Cli) -> AppResult<Option<String>> {
    let storage = FileStorage::for_user()?;
    match cli.command {
        Commands::Worktree { command } | Commands::Workdir { command } => {
            run_worktree(&storage, command)
        }
        Commands::Tmux { command } => run_tmux(&storage, command),
        Commands::ShellInit { shell } => Ok(Some(shell_init(shell.into()).to_string())),
        Commands::Complete { kind, repo, prefix } => {
            complete(&storage, kind, repo, prefix).map(Some)
        }
        Commands::List {
            refresh: _,
            project,
            repo,
        } => run_worktree(
            &storage,
            WorktreeCommand::List {
                refresh: false,
                project,
                repo,
            },
        ),
        Commands::Status {
            refresh,
            json,
            repo,
            target,
        } => {
            if json && refresh {
                Ok(Some(radar_json_with_refresh(
                    &storage,
                    &Git::default(),
                    repo.as_deref(),
                    target.as_deref(),
                )?))
            } else if json {
                Ok(Some(radar_json_with_storage(
                    &storage,
                    repo.as_deref(),
                    target.as_deref(),
                )?))
            } else if refresh {
                Ok(Some(radar_with_refresh(
                    &storage,
                    &Git::default(),
                    repo.as_deref(),
                    target.as_deref(),
                )?))
            } else {
                Ok(Some(radar_with_storage(
                    &storage,
                    repo.as_deref(),
                    target.as_deref(),
                )?))
            }
        }
        Commands::Audit => run_worktree(&storage, WorktreeCommand::Audit),
        Commands::Push { repo, target } => {
            run_worktree(&storage, WorktreeCommand::Push { repo, target })
        }
        Commands::Prune { repo, target } => {
            run_worktree(&storage, WorktreeCommand::Prune { repo, target })
        }
        Commands::Sessions => run_tmux(&storage, TmuxCommand::List),
        Commands::Path { repo, target } => {
            run_worktree(&storage, WorktreeCommand::Path { repo, target })
        }
        Commands::Cd { repo, target } => {
            run_worktree(&storage, WorktreeCommand::Cd { repo, target })
        }
        Commands::New { repo, target } => {
            run_worktree(&storage, WorktreeCommand::New { repo, target })
        }
        Commands::Discover { path } => {
            discovery::discover(&storage, &Git::default(), path.as_deref()).map(Some)
        }
        Commands::Ignore { repo } => discovery::ignore(&storage, &Git::default(), &repo).map(Some),
        Commands::Unignore { repo } => {
            discovery::unignore(&storage, &Git::default(), &repo).map(Some)
        }
        Commands::Adopt { path } => run_worktree(&storage, WorktreeCommand::Adopt { path }),
        Commands::Run {
            repo,
            target,
            command,
        } => run_pair(&storage, repo, target, command),
        Commands::Pair {
            repo,
            target,
            command,
        } => run_tmux(
            &storage,
            TmuxCommand::Pair {
                repo,
                target,
                command,
            },
        ),
        Commands::Attach { repo, target } => {
            run_tmux(&storage, TmuxCommand::Attach { repo, target })
        }
    }
}

fn run_worktree(storage: &FileStorage, command: WorktreeCommand) -> AppResult<Option<String>> {
    match command {
        WorktreeCommand::Scan => discovery::scan(storage, &Git::default()).map(Some),
        WorktreeCommand::List {
            refresh: _,
            project,
            repo,
        } => {
            let repo_filter = list_repo_filter(project.as_deref(), repo.as_deref())?;
            Ok(Some(list_with_refresh(
                storage,
                &Git::default(),
                repo_filter,
            )?))
        }
        WorktreeCommand::Audit => prune_report(storage, &Git::default()).map(Some),
        WorktreeCommand::Prune { repo, target } => {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            let stdout = std::io::stdout();
            let mut output = stdout.lock();
            prune_merged_interactive(
                storage,
                &Git::default(),
                &mut input,
                &mut output,
                repo.as_deref(),
                target.as_deref(),
            )?;
            Ok(None)
        }
        WorktreeCommand::Path { repo, target } | WorktreeCommand::Cd { repo, target } => {
            resolved_path_output(storage, &repo, target.as_deref()).map(Some)
        }
        WorktreeCommand::New { repo, target } => {
            discovery::new_worktree(storage, &Git::default(), &repo, &target).map(Some)
        }
        WorktreeCommand::Push { repo, target } => {
            crate::push::push_worktree(storage, &Git::default(), &repo, &target).map(Some)
        }
        WorktreeCommand::Adopt { path } => {
            discovery::adopt(storage, &Git::default(), &path).map(Some)
        }
        WorktreeCommand::Run {
            repo,
            target,
            command,
        } => {
            let resolved =
                Resolver::new(storage.load_cache()?).resolve_worktree(&repo, Some(&target))?;
            let command = CommandSpec::new(command)?;
            run_foreground(&resolved.path, &command)?;
            Ok(None)
        }
    }
}

fn run_tmux(storage: &FileStorage, command: TmuxCommand) -> AppResult<Option<String>> {
    match command {
        TmuxCommand::List => {
            let cache = storage.load_cache()?;
            let state = storage.load_state()?;
            Ok(Some(sessions_output(&cache, &state.sessions)))
        }
        TmuxCommand::Pair {
            repo,
            target,
            command,
        } => run_pair(storage, repo, target, command),
        TmuxCommand::Attach { repo, target } => run_attach(storage, repo, target),
    }
}

fn run_pair(
    storage: &FileStorage,
    repo: String,
    target: String,
    command: Vec<String>,
) -> AppResult<Option<String>> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(&repo, Some(&target))?;
    let command = CommandSpec::new(command)?;
    let session_name = sanitize_tmux_session_name(&resolved.repo.alias, &resolved.worktree.target);
    let tmux = Tmux::default();
    tmux.ensure_available()?;

    let _transaction = storage.transaction()?;
    let mut state = storage.load_state()?;
    let existing = tmux.session_state(&session_name)?;
    if existing == ExistingSession::Running {
        if let Some(session) =
            find_session_mut(&mut state, &resolved.repo.alias, &resolved.worktree.target)
        {
            if session.command != command.argv() {
                eprintln!(
                    "warning: existing Workroot session `{session_name}` was started with `{}`; attaching instead of replacing it",
                    CommandSpec::new(session.command.clone())?.to_posix_shell_command()
                );
            }
            session.status = SessionStatus::Running;
            storage.save_state(&state)?;
            tmux.attach(&session_name)?;
            return Ok(None);
        }

        return Err(AppError::ManagedSessionNotFound {
            repo: resolved.repo.alias,
            target: resolved.worktree.target,
        });
    }

    if let Some(session) =
        find_session_mut(&mut state, &resolved.repo.alias, &resolved.worktree.target)
    {
        session.status = SessionStatus::Exited;
    }

    tmux.create_pair_session(&session_name, &resolved.path, &command)?;
    upsert_running_session(
        &mut state,
        resolved.repo.alias,
        resolved.worktree.target,
        resolved.path,
        &command,
        session_name.clone(),
    );
    storage.save_state(&state)?;
    tmux.attach(&session_name)?;
    Ok(None)
}

fn run_attach(storage: &FileStorage, repo: String, target: String) -> AppResult<Option<String>> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(&repo, Some(&target))?;
    let tmux = Tmux::default();
    tmux.ensure_available()?;

    let _transaction = storage.transaction()?;
    let mut state = storage.load_state()?;
    let session = find_session_mut(&mut state, &resolved.repo.alias, &resolved.worktree.target)
        .ok_or_else(|| AppError::ManagedSessionNotFound {
            repo: resolved.repo.alias.clone(),
            target: resolved.worktree.target.clone(),
        })?;
    let session_name = session.tmux_session_name.clone();
    match tmux.session_state(&session_name)? {
        ExistingSession::Running => {
            session.status = SessionStatus::Running;
            storage.save_state(&state)?;
            tmux.attach(&session_name)?;
            Ok(None)
        }
        ExistingSession::Missing => {
            session.status = SessionStatus::Exited;
            storage.save_state(&state)?;
            Err(AppError::ManagedSessionNotFound {
                repo: resolved.repo.alias,
                target: resolved.worktree.target,
            })
        }
    }
}

fn list_repo_filter<'a>(
    project: Option<&'a str>,
    repo: Option<&'a str>,
) -> AppResult<Option<&'a str>> {
    match (project, repo) {
        (Some(_), Some(_)) => Err(AppError::InvalidCommand(
            "pass either `workroot list <repo>` or `workroot list --project <repo>`, not both"
                .to_string(),
        )),
        (Some(project), None) => Ok(Some(project)),
        (None, Some(repo)) => Ok(Some(repo)),
        (None, None) => Ok(None),
    }
}

fn resolved_path_output(
    storage: &FileStorage,
    repo: &str,
    target: Option<&str>,
) -> AppResult<String> {
    let resolved = Resolver::new(storage.load_cache()?).resolve_worktree(repo, target)?;
    Ok(format!("{}\n", resolved.path.display()))
}

fn complete(
    storage: &FileStorage,
    kind: CompleteKind,
    repo_or_prefix: Option<String>,
    prefix: Option<String>,
) -> AppResult<String> {
    let resolver = Resolver::new(storage.load_cache()?);
    let candidates = match kind {
        CompleteKind::Repos => resolver.complete_repos(repo_or_prefix.as_deref()),
        CompleteKind::Targets => {
            let repo = repo_or_prefix.ok_or_else(|| {
                AppError::InvalidCommand("expected repo for target completion".to_string())
            })?;
            resolver.complete_targets(&repo, prefix.as_deref())?
        }
    };

    Ok(if candidates.is_empty() {
        String::new()
    } else {
        format!("{}\n", candidates.join("\n"))
    })
}

fn run_foreground(path: &Path, command: &CommandSpec) -> AppResult<()> {
    let program = &command.argv()[0];
    let status = std::process::Command::new(program)
        .args(&command.argv()[1..])
        .current_dir(path)
        .status()
        .map_err(|source| AppError::CommandFailed(format!("{program}: {source}")))?;

    if !status.success() {
        return Err(AppError::CommandFailed(program.clone()));
    }

    Ok(())
}
