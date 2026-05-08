use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use serde::Serialize;

use crate::domain::{
    Cache, DirtyState, RepoRecord, SessionBackend, SessionRecord, SessionStatus, WorktreeRecord,
};
use crate::error::AppResult;
use crate::git::Git;
use crate::session::{Tmux, TmuxPane};
use crate::storage::FileStorage;

mod json;
mod render;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveWorktreeStatus {
    pub branch: BranchDisplay,
    pub head: Option<String>,
    pub dirty: DirtyState,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchDisplay {
    Named(String),
    Detached,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxInventory {
    pub panes: Vec<TmuxPane>,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessAttachment {
    kind: ProcessAttachmentKind,
    session: String,
    command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessAttachmentKind {
    Managed(SessionStatus),
    Mapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RadarState {
    Run,
    Exit,
    Map,
    Unmapped,
    Idle,
    Dirty,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RadarSummary {
    pub(crate) repos: usize,
    pub(crate) worktrees: usize,
    pub(crate) active_panes: Option<usize>,
    pub(crate) managed_running: Option<usize>,
    pub(crate) exited: Option<usize>,
    pub(crate) unmapped: Option<usize>,
    pub(crate) dirty: usize,
    pub(crate) stale: usize,
    pub(crate) tmux_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RadarWorktreeRow {
    pub(crate) state: RadarState,
    pub(crate) repo: String,
    pub(crate) target: String,
    pub(crate) base_branch: String,
    pub(crate) branch: String,
    pub(crate) head: String,
    pub(crate) dirty: String,
    pub(crate) session: String,
    pub(crate) command: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RadarTmuxRow {
    pub(crate) state: RadarState,
    pub(crate) session: String,
    pub(crate) command: String,
    pub(crate) cwd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RadarView {
    pub(crate) summary: RadarSummary,
    pub(crate) attention: Vec<RadarWorktreeRow>,
    pub(crate) active: Vec<RadarWorktreeRow>,
    pub(crate) idle: Vec<RadarWorktreeRow>,
    pub(crate) unmapped: Vec<RadarTmuxRow>,
}

pub fn list_output(cache: &Cache, repo_filter: Option<&str>) -> String {
    let cache = filtered_cache(cache, repo_filter, None);
    status_output(&cache)
}

pub fn status_output(cache: &Cache) -> String {
    let statuses = live_statuses(cache);
    render::render_status(cache, &statuses)
}

pub fn radar_output(cache: &Cache, sessions: &[SessionRecord], inventory: TmuxInventory) -> String {
    let statuses = live_statuses(cache);
    render::render_radar_view(&build_radar_view(cache, sessions, &statuses, &inventory))
}

pub fn radar_json_output(
    cache: &Cache,
    sessions: &[SessionRecord],
    inventory: TmuxInventory,
) -> AppResult<String> {
    let statuses = live_statuses(cache);
    json::render_radar_json(&build_radar_view(cache, sessions, &statuses, &inventory))
}

pub fn sessions_output(cache: &Cache, sessions: &[SessionRecord]) -> String {
    let worktrees_by_key = cache
        .worktrees
        .iter()
        .map(|worktree| {
            (
                (worktree.repo_alias.as_str(), worktree.target.as_str()),
                worktree,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut sessions = sessions.iter().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        left.repo_alias
            .cmp(&right.repo_alias)
            .then(left.target.cmp(&right.target))
            .then(left.tmux_session_name.cmp(&right.tmux_session_name))
    });

    let mut rows = vec![vec![
        "REPO".to_string(),
        "TARGET".to_string(),
        "STATUS".to_string(),
        "TMUX".to_string(),
        "COMMAND".to_string(),
        "PATH".to_string(),
    ]];

    for session in sessions {
        let path = worktrees_by_key
            .get(&(session.repo_alias.as_str(), session.target.as_str()))
            .map(|worktree| worktree.path.as_path())
            .unwrap_or(&session.worktree_path);
        rows.push(vec![
            session.repo_alias.clone(),
            session.target.clone(),
            session_label(Some(live_session_status(session))),
            session.tmux_session_name.clone(),
            session.command.join(" "),
            path.display().to_string(),
        ]);
    }

    render::render_table(rows)
}

pub fn refresh_status(storage: &FileStorage, git: &Git) -> AppResult<String> {
    radar_with_refresh(storage, git, None, None)
}

pub fn list_with_refresh(
    storage: &FileStorage,
    git: &Git,
    repo_filter: Option<&str>,
) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let mut cache = storage.load_cache()?;
    let dirty = refresh_dirty_states(&cache, git.clone());

    for worktree in &mut cache.worktrees {
        if let Some(refreshed) = dirty.get(&worktree_key(worktree)) {
            worktree.dirty = *refreshed;
        }
    }

    storage.save_cache(&cache)?;
    let cache = filtered_cache(&cache, repo_filter, None);
    let statuses = live_statuses(&cache);
    Ok(render::render_status(&cache, &statuses))
}

pub fn radar_with_refresh(
    storage: &FileStorage,
    git: &Git,
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> AppResult<String> {
    let view = refreshed_radar_view(storage, git, repo_filter, target_filter)?;
    Ok(render::render_radar_view(&view))
}

pub fn radar_json_with_refresh(
    storage: &FileStorage,
    git: &Git,
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> AppResult<String> {
    let view = refreshed_radar_view(storage, git, repo_filter, target_filter)?;
    json::render_radar_json(&view)
}

pub fn radar_with_storage(
    storage: &FileStorage,
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> AppResult<String> {
    let view = stored_radar_view(storage, repo_filter, target_filter)?;
    Ok(render::render_radar_view(&view))
}

pub fn radar_json_with_storage(
    storage: &FileStorage,
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> AppResult<String> {
    let view = stored_radar_view(storage, repo_filter, target_filter)?;
    json::render_radar_json(&view)
}

pub fn complete_repos(cache: &Cache) -> String {
    cache
        .repos
        .iter()
        .map(|repo| repo.alias.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn complete_targets(cache: &Cache, repo: Option<&str>) -> String {
    cache
        .worktrees
        .iter()
        .filter(|worktree| repo.map(|repo| worktree.repo_alias == repo).unwrap_or(true))
        .map(|worktree| worktree.target.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn live_statuses(cache: &Cache) -> BTreeMap<String, LiveWorktreeStatus> {
    let repos_by_alias = cache
        .repos
        .iter()
        .map(|repo| (repo.alias.as_str(), repo))
        .collect::<HashMap<_, _>>();
    let mut statuses = BTreeMap::new();
    for worktree in sorted_worktrees(cache) {
        let repo_stale = repos_by_alias
            .get(worktree.repo_alias.as_str())
            .map(|repo| repo.stale || !repo.canonical_path.exists())
            .unwrap_or(true);
        let path_exists = worktree.path.exists();
        let stale = repo_stale || worktree.stale || !path_exists;
        let branch = if stale {
            cached_branch_display(worktree)
        } else {
            live_branch_display(&worktree.path).unwrap_or_else(|| cached_branch_display(worktree))
        };
        let head = if stale {
            None
        } else {
            live_head_display(&worktree.path)
        };
        statuses.insert(
            worktree_key(worktree),
            LiveWorktreeStatus {
                branch,
                head,
                dirty: worktree.dirty,
                stale,
            },
        );
    }
    statuses
}

fn build_radar_view(
    cache: &Cache,
    sessions: &[SessionRecord],
    statuses: &BTreeMap<String, LiveWorktreeStatus>,
    inventory: &TmuxInventory,
) -> RadarView {
    let repos_by_alias = cache
        .repos
        .iter()
        .map(|repo| (repo.alias.as_str(), repo))
        .collect::<HashMap<_, _>>();
    let panes_by_session = panes_by_session(&inventory.panes);
    let managed_sessions = managed_sessions_by_worktree(cache, sessions);
    let mapped_panes = mapped_panes_by_worktree(cache, inventory);
    let mut attention = Vec::new();
    let mut active = Vec::new();
    let mut idle = Vec::new();
    let mut dirty = 0;
    let mut stale = 0;
    let mut managed_running = 0;
    let mut exited = 0;

    for worktree in sorted_worktrees(cache) {
        let key = worktree_key(worktree);
        let status = statuses
            .get(&key)
            .expect("status exists for every sorted worktree");
        let repo = repos_by_alias.get(worktree.repo_alias.as_str()).copied();
        let process = managed_sessions
            .get(&key)
            .map(|session| managed_process(session, &panes_by_session, inventory))
            .or_else(|| mapped_panes.get(&key).map(|pane| mapped_process(pane)));

        if status.stale {
            stale += 1;
        } else if matches!(status.dirty, DirtyState::Dirty { .. }) {
            dirty += 1;
        }

        if let Some(ProcessAttachment {
            kind: ProcessAttachmentKind::Managed(live_status),
            ..
        }) = process.as_ref()
        {
            match live_status {
                SessionStatus::Running => managed_running += 1,
                SessionStatus::Exited => exited += 1,
                SessionStatus::Unknown => {}
            }
        }

        let state = radar_state(status, process.as_ref());
        let row = RadarWorktreeRow {
            state,
            repo: worktree.repo_alias.clone(),
            target: worktree.target.clone(),
            base_branch: base_branch_label(repo),
            branch: branch_label(&status.branch),
            head: status_head_label(status),
            dirty: status_dirty_label(status),
            session: process
                .as_ref()
                .map(|process| process.session.clone())
                .unwrap_or_else(|| "-".to_string()),
            command: process
                .as_ref()
                .map(|process| process.command.clone())
                .unwrap_or_else(|| "-".to_string()),
            path: worktree.path.display().to_string(),
        };

        match row.state {
            RadarState::Stale | RadarState::Dirty | RadarState::Exit | RadarState::Unknown => {
                attention.push(row)
            }
            RadarState::Run | RadarState::Map => active.push(row),
            RadarState::Idle => idle.push(row),
            RadarState::Unmapped => unreachable!("worktree rows cannot be unmapped"),
        }
    }

    attention.sort_by(sort_worktree_rows);
    active.sort_by(sort_worktree_rows);
    idle.sort_by(sort_worktree_rows);
    let unmapped = unmapped_tmux_panes(cache, sessions, inventory);
    let summary = RadarSummary {
        repos: cache.repos.len(),
        worktrees: cache.worktrees.len(),
        active_panes: inventory.available.then_some(inventory.panes.len()),
        managed_running: inventory.available.then_some(managed_running),
        exited: inventory.available.then_some(exited),
        unmapped: inventory.available.then_some(unmapped.len()),
        dirty,
        stale,
        tmux_available: inventory.available,
    };

    RadarView {
        summary,
        attention,
        active,
        idle,
        unmapped,
    }
}

fn tmux_inventory() -> TmuxInventory {
    match Tmux::default().list_panes() {
        Ok(panes) => TmuxInventory {
            panes,
            available: true,
        },
        Err(_) => TmuxInventory {
            panes: Vec::new(),
            available: false,
        },
    }
}

fn radar_state(status: &LiveWorktreeStatus, process: Option<&ProcessAttachment>) -> RadarState {
    if status.stale {
        return RadarState::Stale;
    }
    if matches!(status.dirty, DirtyState::Dirty { .. }) {
        return RadarState::Dirty;
    }

    match process.map(|process| process.kind) {
        Some(ProcessAttachmentKind::Managed(SessionStatus::Running)) => RadarState::Run,
        Some(ProcessAttachmentKind::Managed(SessionStatus::Exited)) => RadarState::Exit,
        Some(ProcessAttachmentKind::Managed(SessionStatus::Unknown)) => RadarState::Unknown,
        Some(ProcessAttachmentKind::Mapped) => RadarState::Map,
        None => RadarState::Idle,
    }
}

fn managed_process(
    session: &SessionRecord,
    panes_by_session: &HashMap<&str, &TmuxPane>,
    inventory: &TmuxInventory,
) -> ProcessAttachment {
    let live_pane = panes_by_session
        .get(session.tmux_session_name.as_str())
        .copied();
    let live_status = if inventory.available {
        if live_pane.is_some() {
            SessionStatus::Running
        } else {
            SessionStatus::Exited
        }
    } else {
        SessionStatus::Unknown
    };

    ProcessAttachment {
        kind: ProcessAttachmentKind::Managed(live_status),
        session: session.tmux_session_name.clone(),
        command: live_pane
            .map(|pane| pane.current_command.clone())
            .filter(|command| !command.is_empty())
            .unwrap_or_else(|| session.command.join(" ")),
    }
}

fn mapped_process(pane: &TmuxPane) -> ProcessAttachment {
    ProcessAttachment {
        kind: ProcessAttachmentKind::Mapped,
        session: pane.session_name.clone(),
        command: pane.current_command.clone(),
    }
}

fn managed_sessions_by_worktree<'a>(
    cache: &Cache,
    sessions: &'a [SessionRecord],
) -> BTreeMap<String, &'a SessionRecord> {
    let worktrees_by_pair = cache
        .worktrees
        .iter()
        .map(|worktree| {
            (
                (worktree.repo_alias.as_str(), worktree.target.as_str()),
                worktree,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut output = BTreeMap::new();

    for session in sessions {
        let Some(worktree) =
            worktrees_by_pair.get(&(session.repo_alias.as_str(), session.target.as_str()))
        else {
            continue;
        };
        output.entry(worktree_key(worktree)).or_insert(session);
    }

    output
}

fn mapped_panes_by_worktree<'a>(
    cache: &'a Cache,
    inventory: &'a TmuxInventory,
) -> BTreeMap<String, &'a TmuxPane> {
    let mut output = BTreeMap::new();
    for pane in &inventory.panes {
        let Some(worktree) = worktree_for_path(cache, &pane.current_path) else {
            continue;
        };
        output.entry(worktree_key(worktree)).or_insert(pane);
    }
    output
}

fn unmapped_tmux_panes(
    cache: &Cache,
    sessions: &[SessionRecord],
    inventory: &TmuxInventory,
) -> Vec<RadarTmuxRow> {
    if !inventory.available {
        return Vec::new();
    }

    let managed = sessions
        .iter()
        .map(|session| session.tmux_session_name.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut unmapped = Vec::new();

    for pane in &inventory.panes {
        if managed.contains(pane.session_name.as_str())
            || worktree_for_path(cache, &pane.current_path).is_some()
        {
            continue;
        }
        let key = (
            pane.session_name.as_str(),
            pane.current_path.as_path(),
            pane.current_command.as_str(),
        );
        if seen.insert(key) {
            unmapped.push(RadarTmuxRow {
                state: RadarState::Unmapped,
                session: pane.session_name.clone(),
                cwd: pane.current_path.display().to_string(),
                command: pane.current_command.clone(),
            });
        }
    }

    unmapped.sort_by(|left, right| {
        left.session
            .cmp(&right.session)
            .then(left.cwd.cmp(&right.cwd))
            .then(left.command.cmp(&right.command))
    });
    unmapped
}

fn sort_worktree_rows(left: &RadarWorktreeRow, right: &RadarWorktreeRow) -> std::cmp::Ordering {
    radar_state_priority(left.state)
        .cmp(&radar_state_priority(right.state))
        .then(left.repo.cmp(&right.repo))
        .then(left.target.cmp(&right.target))
        .then(left.path.cmp(&right.path))
}

fn radar_state_priority(state: RadarState) -> u8 {
    match state {
        RadarState::Stale => 0,
        RadarState::Exit => 1,
        RadarState::Unknown => 2,
        RadarState::Dirty => 3,
        RadarState::Run => 4,
        RadarState::Map => 5,
        RadarState::Idle => 6,
        RadarState::Unmapped => 7,
    }
}

pub(crate) fn radar_state_label(state: RadarState) -> &'static str {
    match state {
        RadarState::Run => "RUN",
        RadarState::Exit => "EXIT",
        RadarState::Map => "MAP",
        RadarState::Unmapped => "UNMAPPED",
        RadarState::Idle => "IDLE",
        RadarState::Dirty => "DIRTY",
        RadarState::Stale => "STALE",
        RadarState::Unknown => "UNKNOWN",
    }
}

fn panes_by_session(panes: &[TmuxPane]) -> HashMap<&str, &TmuxPane> {
    let mut output = HashMap::new();
    for pane in panes {
        output.entry(pane.session_name.as_str()).or_insert(pane);
    }
    output
}

fn worktree_for_path<'a>(cache: &'a Cache, path: &Path) -> Option<&'a WorktreeRecord> {
    let path = canonical_or_self(path);
    cache
        .worktrees
        .iter()
        .filter(|worktree| path.starts_with(canonical_or_self(&worktree.path)))
        .max_by_key(|worktree| worktree.path.as_os_str().len())
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn sorted_worktrees(cache: &Cache) -> Vec<&WorktreeRecord> {
    let mut worktrees = cache.worktrees.iter().collect::<Vec<_>>();
    worktrees.sort_by(|left, right| {
        left.repo_alias
            .cmp(&right.repo_alias)
            .then(left.target.cmp(&right.target))
            .then(left.path.cmp(&right.path))
    });
    worktrees
}

fn filtered_cache(cache: &Cache, repo_filter: Option<&str>, target_filter: Option<&str>) -> Cache {
    let Some(filter) = repo_filter else {
        if let Some(target) = target_filter {
            return Cache {
                schema_version: cache.schema_version,
                repos: cache.repos.clone(),
                worktrees: cache
                    .worktrees
                    .iter()
                    .filter(|worktree| worktree.target == target || worktree.display_name == target)
                    .cloned()
                    .collect(),
                last_scan_unix: cache.last_scan_unix,
            };
        }
        return cache.clone();
    };
    let repos = cache
        .repos
        .iter()
        .filter(|repo| repo.alias == filter || repo.display_name == filter)
        .cloned()
        .collect::<Vec<_>>();
    let aliases = repos
        .iter()
        .map(|repo| repo.alias.clone())
        .collect::<std::collections::BTreeSet<_>>();

    Cache {
        schema_version: cache.schema_version,
        repos,
        worktrees: cache
            .worktrees
            .iter()
            .filter(|worktree| aliases.contains(&worktree.repo_alias))
            .filter(|worktree| {
                target_filter
                    .map(|target| worktree.target == target || worktree.display_name == target)
                    .unwrap_or(true)
            })
            .cloned()
            .collect(),
        last_scan_unix: cache.last_scan_unix,
    }
}

fn filtered_inventory(
    cache: &Cache,
    sessions: &[SessionRecord],
    inventory: TmuxInventory,
) -> TmuxInventory {
    if !inventory.available {
        return inventory;
    }

    let worktree_keys = cache
        .worktrees
        .iter()
        .map(|worktree| (worktree.repo_alias.as_str(), worktree.target.as_str()))
        .collect::<HashSet<_>>();
    let managed_sessions = sessions
        .iter()
        .filter(|session| {
            worktree_keys.contains(&(session.repo_alias.as_str(), session.target.as_str()))
        })
        .map(|session| session.tmux_session_name.as_str())
        .collect::<HashSet<_>>();

    TmuxInventory {
        available: inventory.available,
        panes: inventory
            .panes
            .into_iter()
            .filter(|pane| {
                managed_sessions.contains(pane.session_name.as_str())
                    || worktree_for_path(cache, &pane.current_path).is_some()
            })
            .collect(),
    }
}

fn refreshed_radar_view(
    storage: &FileStorage,
    git: &Git,
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> AppResult<RadarView> {
    let _transaction = storage.transaction()?;
    let mut cache = storage.load_cache()?;
    let state = storage.load_state()?;
    let dirty = refresh_dirty_states(&cache, git.clone());

    for worktree in &mut cache.worktrees {
        if let Some(refreshed) = dirty.get(&worktree_key(worktree)) {
            worktree.dirty = *refreshed;
        }
    }

    storage.save_cache(&cache)?;
    Ok(radar_view_from_cache(
        &cache,
        &state.sessions,
        repo_filter,
        target_filter,
    ))
}

fn stored_radar_view(
    storage: &FileStorage,
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> AppResult<RadarView> {
    let _transaction = storage.transaction()?;
    let cache = storage.load_cache()?;
    let state = storage.load_state()?;
    Ok(radar_view_from_cache(
        &cache,
        &state.sessions,
        repo_filter,
        target_filter,
    ))
}

fn radar_view_from_cache(
    cache: &Cache,
    sessions: &[SessionRecord],
    repo_filter: Option<&str>,
    target_filter: Option<&str>,
) -> RadarView {
    let cache = filtered_cache(cache, repo_filter, target_filter);
    let mut inventory = tmux_inventory();
    if repo_filter.is_some() || target_filter.is_some() {
        inventory = filtered_inventory(&cache, sessions, inventory);
    }
    let statuses = live_statuses(&cache);
    build_radar_view(&cache, sessions, &statuses, &inventory)
}

fn refresh_dirty_states(cache: &Cache, git: Git) -> BTreeMap<String, DirtyState> {
    let handles = cache
        .worktrees
        .iter()
        .cloned()
        .map(|worktree| {
            let git = git.clone();
            thread::spawn(move || {
                let state = if worktree.path.exists() {
                    dirty_state(&worktree.path, &git)
                } else {
                    DirtyState::Unknown
                };
                (worktree_key(&worktree), state)
            })
        })
        .collect::<Vec<_>>();

    handles
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .collect()
}

fn dirty_state(path: &Path, git: &Git) -> DirtyState {
    let output = Command::new(git.executable())
        .args(["-C"])
        .arg(path)
        .args(["status", "--porcelain"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let files = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count() as u32;
            if files == 0 {
                DirtyState::Clean
            } else {
                DirtyState::Dirty { files }
            }
        }
        _ => DirtyState::Unknown,
    }
}

fn live_branch_display(path: &Path) -> Option<BranchDisplay> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() {
            Some(BranchDisplay::Unknown)
        } else {
            Some(BranchDisplay::Named(branch))
        }
    } else {
        Some(BranchDisplay::Detached)
    }
}

fn live_head_display(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!head.is_empty()).then_some(head)
    } else {
        None
    }
}

pub(crate) fn base_branch_label(repo: Option<&RepoRecord>) -> String {
    repo.and_then(|repo| repo.base_branch.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

fn live_session_status(session: &SessionRecord) -> SessionStatus {
    match session.backend {
        SessionBackend::Tmux => match Command::new("tmux")
            .args(["has-session", "-t"])
            .arg(&session.tmux_session_name)
            .status()
        {
            Ok(status) if status.success() => SessionStatus::Running,
            Ok(_) => SessionStatus::Exited,
            Err(_) => SessionStatus::Unknown,
        },
    }
}

fn cached_branch_display(worktree: &WorktreeRecord) -> BranchDisplay {
    if worktree.detached {
        BranchDisplay::Detached
    } else {
        worktree
            .branch
            .clone()
            .map(BranchDisplay::Named)
            .unwrap_or(BranchDisplay::Unknown)
    }
}

pub(crate) fn branch_label(branch: &BranchDisplay) -> String {
    match branch {
        BranchDisplay::Named(branch) => branch.clone(),
        BranchDisplay::Detached => "detached".to_string(),
        BranchDisplay::Unknown => "unknown".to_string(),
    }
}

pub(crate) fn dirty_label(dirty: DirtyState) -> String {
    match dirty {
        DirtyState::Unknown => "unknown".to_string(),
        DirtyState::Clean => "clean".to_string(),
        DirtyState::Dirty { files } => format!("dirty({files})"),
    }
}

pub(crate) fn status_dirty_label(status: &LiveWorktreeStatus) -> String {
    if status.stale {
        "stale".to_string()
    } else {
        dirty_label(status.dirty)
    }
}

pub(crate) fn status_head_label(status: &LiveWorktreeStatus) -> String {
    if status.stale {
        "stale".to_string()
    } else {
        status.head.clone().unwrap_or_else(|| "unknown".to_string())
    }
}

fn session_label(session: Option<SessionStatus>) -> String {
    match session {
        Some(SessionStatus::Running) => "running".to_string(),
        Some(SessionStatus::Exited) => "exited".to_string(),
        Some(SessionStatus::Unknown) => "unknown".to_string(),
        None => "-".to_string(),
    }
}

pub(crate) fn worktree_key(worktree: &WorktreeRecord) -> String {
    format!(
        "{}\0{}\0{}",
        worktree.repo_alias,
        worktree.target,
        worktree.path.display()
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::{
        Cache, DirtyState, RepoRecord, RepoSource, SessionBackend, SessionRecord, SessionStatus,
        WorktreeRecord, WorktreeSource,
    };

    use super::{complete_repos, complete_targets, list_output, sessions_output, status_output};

    #[test]
    fn renders_list_from_cached_index() {
        let cache = sample_cache(false, false, DirtyState::Clean);
        let output = list_output(&cache, None);

        assert!(output.contains("REPO"));
        assert!(output.contains("BASE BRANCH"));
        assert!(output.contains("WORKTREE BRANCH"));
        assert!(output.contains("HEAD"));
        assert!(output.contains("jam"));
        assert!(output.contains("missing-branch"));
        assert!(output.contains("stale"));
        assert!(!output.contains("FLAGS"));
    }

    #[test]
    fn list_filters_by_repo_alias() {
        let mut cache = sample_cache(false, false, DirtyState::Clean);
        cache.repos.push(RepoRecord {
            alias: "other".to_string(),
            display_name: "other".to_string(),
            canonical_path: PathBuf::from("/tmp/other"),
            git_common_dir: PathBuf::from("/tmp/other/.git"),
            base_branch: Some("main".to_string()),
            source: RepoSource::Adopted,
            stale: false,
        });
        cache.worktrees.push(WorktreeRecord {
            repo_alias: "other".to_string(),
            target: "base".to_string(),
            display_name: "base".to_string(),
            branch: Some("main".to_string()),
            path: PathBuf::from("/tmp/other"),
            source: WorktreeSource::Manual,
            dirty: DirtyState::Clean,
            last_seen_unix: None,
            stale: false,
            detached: false,
        });

        let output = list_output(&cache, Some("jam"));

        assert!(output.contains("/missing/auth"));
        assert!(!output.contains("/tmp/other"));
    }

    #[test]
    fn renders_stale_rows_without_dash_placeholders() {
        let cache = sample_cache(true, true, DirtyState::Dirty { files: 2 });
        let output = status_output(&cache);

        assert!(output.contains("stale"));
        assert!(output.contains("detached"));
        assert!(!output.contains("SESSION"));
        assert!(!output.contains(" - "));
    }

    #[test]
    fn renders_sessions_separately() {
        let cache = sample_cache(false, false, DirtyState::Clean);
        let sessions = vec![SessionRecord {
            repo_alias: "jam".to_string(),
            target: "auth-flow".to_string(),
            worktree_path: PathBuf::from("/missing/auth"),
            backend: SessionBackend::Tmux,
            command: vec!["make".to_string()],
            tmux_session_name: "definitely-missing-workroot-test-session".to_string(),
            status: SessionStatus::Running,
        }];
        let output = sessions_output(&cache, &sessions);

        assert!(output.contains("SESSION") || output.contains("STATUS"));
        assert!(output.contains("make"));
        assert!(output.contains("exited") || output.contains("unknown"));
    }

    #[test]
    fn completion_reads_cache_only() {
        let cache = sample_cache(false, false, DirtyState::Unknown);

        assert_eq!(complete_repos(&cache), "jam");
        assert_eq!(complete_targets(&cache, Some("jam")), "auth-flow");
    }

    fn sample_cache(stale: bool, detached: bool, dirty: DirtyState) -> Cache {
        Cache {
            schema_version: 1,
            repos: vec![RepoRecord {
                alias: "jam".to_string(),
                display_name: "jam".to_string(),
                canonical_path: PathBuf::from("/tmp/jam"),
                git_common_dir: PathBuf::from("/tmp/jam/.git"),
                base_branch: Some("main".to_string()),
                source: RepoSource::Adopted,
                stale,
            }],
            worktrees: vec![WorktreeRecord {
                repo_alias: "jam".to_string(),
                target: "auth-flow".to_string(),
                display_name: "auth-flow".to_string(),
                branch: Some("missing-branch".to_string()),
                path: PathBuf::from("/missing/auth"),
                source: WorktreeSource::Manual,
                dirty,
                last_seen_unix: None,
                stale,
                detached,
            }],
            last_scan_unix: None,
        }
    }
}
