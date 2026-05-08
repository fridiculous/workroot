use std::path::PathBuf;

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(
        "missing required dependency `{name}`\nfix: install `{name}` and retry\nnext: run `workroot status` to confirm Workroot can see your worktrees"
    )]
    MissingDependency { name: &'static str },

    #[error(
        "repo `{input}` is ambiguous; candidates: {candidates}\nfix: pass the exact repo alias shown in `workroot status`"
    )]
    AmbiguousRepo { input: String, candidates: String },

    #[error(
        "target for repo `{repo}` is ambiguous; candidates: {candidates}\nfix: pass the exact target name shown in `workroot status {repo}`"
    )]
    AmbiguousTarget { repo: String, candidates: String },

    #[error("interactive target selection failed: {0}")]
    Picker(String),

    #[error(
        "repo `{0}` was not found\nfix: run `workroot discover` or `workroot discover <path-to-repo>`\nthen: run `workroot status`"
    )]
    RepoNotFound(String),

    #[error(
        "target `{target}` was not found for repo `{repo}`\nfix: run `workroot status {repo}` to see known targets"
    )]
    TargetNotFound { repo: String, target: String },

    #[error("worktree path is stale: {0}\nfix: run `workroot discover` to refresh the index")]
    StaleWorktree(PathBuf),

    #[error(
        "state schema version {found} is newer than supported version {supported}; upgrade Workroot"
    )]
    FutureSchema { found: u32, supported: u32 },

    #[error("could not read {kind} file at {path}: {source}")]
    ReadFile {
        kind: &'static str,
        path: PathBuf,
        source: Box<std::io::Error>,
    },

    #[error("could not write {kind} file at {path}: {source}")]
    WriteFile {
        kind: &'static str,
        path: PathBuf,
        source: Box<std::io::Error>,
    },

    #[error("could not parse JSON {kind} file at {path}: {source}")]
    ParseJson {
        kind: &'static str,
        path: PathBuf,
        source: Box<serde_json::Error>,
    },

    #[error("could not parse TOML {kind} file at {path}: {source}")]
    ParseToml {
        kind: &'static str,
        path: PathBuf,
        source: Box<toml::de::Error>,
    },

    #[error("could not serialize {kind}: {source}")]
    SerializeToml {
        kind: &'static str,
        source: Box<toml::ser::Error>,
    },

    #[error("could not serialize {kind}: {source}")]
    SerializeJson {
        kind: &'static str,
        source: Box<serde_json::Error>,
    },

    #[error("git command failed: {0}")]
    Git(String),

    #[error("command `{0}` failed")]
    CommandFailed(String),

    #[error("I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("tmux command failed: {0}")]
    Tmux(String),

    #[error(
        "no Workroot-managed tmux session for `{repo}` `{target}`\nfix: run `workroot run {repo} {target} -- <command...>` first"
    )]
    ManagedSessionNotFound { repo: String, target: String },

    #[error("unsupported shell `{0}`; supported shells: zsh, bash, fish")]
    UnsupportedShell(String),

    #[error("`{0}` is not implemented in the core foundation yet")]
    NotImplemented(&'static str),

    #[error("invalid command: {0}")]
    InvalidCommand(String),

    #[error("HOME is not set and no XDG directory override was provided")]
    MissingHome,
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::InvalidCommand(_) => 2,
            AppError::AmbiguousRepo { .. } | AppError::AmbiguousTarget { .. } => 3,
            AppError::RepoNotFound(_) | AppError::TargetNotFound { .. } => 4,
            AppError::StaleWorktree(_) => 5,
            AppError::FutureSchema { .. } => 6,
            AppError::ReadFile { .. }
            | AppError::WriteFile { .. }
            | AppError::ParseJson { .. }
            | AppError::ParseToml { .. }
            | AppError::SerializeJson { .. }
            | AppError::SerializeToml { .. }
            | AppError::MissingHome => 7,
            AppError::MissingDependency { .. } => 8,
            AppError::Git(_) | AppError::CommandFailed(_) | AppError::Tmux(_) => 9,
            AppError::Io(_) => 9,
            AppError::ManagedSessionNotFound { .. } => 11,
            AppError::UnsupportedShell(_) => 10,
            AppError::Picker(_) => 11,
            AppError::NotImplemented(_) => 64,
        }
    }
}
