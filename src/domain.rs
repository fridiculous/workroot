use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub trait Versioned {
    fn schema_version(&self) -> u32;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub scan_roots: Vec<PathBuf>,
    pub default_worktree_root: Option<PathBuf>,
    #[serde(default)]
    pub repos: BTreeMap<String, RepoConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            scan_roots: Vec::new(),
            default_worktree_root: None,
            repos: BTreeMap::new(),
        }
    }
}

impl Versioned for Config {
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoConfig {
    pub alias: String,
    pub canonical_path: PathBuf,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub schema_version: u32,
    #[serde(default)]
    pub adopted_paths: Vec<PathBuf>,
    #[serde(default)]
    pub ignored_repos: Vec<IgnoredRepo>,
    #[serde(default)]
    pub repos: BTreeMap<String, RepoState>,
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            adopted_paths: Vec::new(),
            ignored_repos: Vec::new(),
            repos: BTreeMap::new(),
            sessions: Vec::new(),
        }
    }
}

impl Versioned for State {
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoState {
    pub alias: String,
    pub canonical_path: PathBuf,
    pub git_common_dir: PathBuf,
    pub base_branch: Option<String>,
    pub source: RepoSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredRepo {
    #[serde(default)]
    pub alias: Option<String>,
    pub canonical_path: PathBuf,
    pub git_common_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {
    pub schema_version: u32,
    #[serde(default)]
    pub repos: Vec<RepoRecord>,
    #[serde(default)]
    pub worktrees: Vec<WorktreeRecord>,
    pub last_scan_unix: Option<i64>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            repos: Vec::new(),
            worktrees: Vec::new(),
            last_scan_unix: None,
        }
    }
}

impl Versioned for Cache {
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRecord {
    pub alias: String,
    pub display_name: String,
    pub canonical_path: PathBuf,
    pub git_common_dir: PathBuf,
    pub base_branch: Option<String>,
    pub source: RepoSource,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRecord {
    pub repo_alias: String,
    pub target: String,
    pub display_name: String,
    pub branch: Option<String>,
    pub path: PathBuf,
    pub source: WorktreeSource,
    pub dirty: DirtyState,
    pub last_seen_unix: Option<i64>,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub detached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub repo_alias: String,
    pub target: String,
    pub worktree_path: PathBuf,
    pub backend: SessionBackend,
    pub command: Vec<String>,
    pub tmux_session_name: String,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoSource {
    Adopted,
    Scanned,
    Workroot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeSource {
    Workroot,
    Workmux,
    Manual,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyState {
    Unknown,
    Clean,
    Dirty { files: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBackend {
    Tmux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Exited,
    Unknown,
}
