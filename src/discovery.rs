use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{
    Cache, Config, DirtyState, IgnoredRepo, RepoRecord, RepoSource, RepoState, State,
    WorktreeRecord, WorktreeSource,
};
use crate::error::{AppError, AppResult};
use crate::git::{Git, GitWorktreeEntry, canonical_or_self};
use crate::resolver::Resolver;
use crate::storage::FileStorage;

const DEFAULT_SCAN_DEPTH: usize = 4;

pub fn discover(storage: &FileStorage, git: &Git, path: Option<&Path>) -> AppResult<String> {
    match path {
        Some(path) => discover_path(storage, git, path),
        None => scan(storage, git),
    }
}

pub fn adopt(storage: &FileStorage, git: &Git, path: &Path) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let mut state = storage.load_state()?;
    let mut cache = storage.load_cache()?;
    let root = git.verify(path)?;
    let adopted_path = canonical_or_self(path);
    if !state
        .adopted_paths
        .iter()
        .any(|known| known == &adopted_path)
    {
        state.adopted_paths.push(adopted_path);
    }

    let family = discover_family(git, &root.top_level, RepoSource::Adopted)?;
    merge_families(&mut state, &mut cache, vec![family]);
    storage.save_state(&state)?;
    storage.save_cache(&cache)?;

    Ok("adopted\n".to_string())
}

pub fn scan(storage: &FileStorage, git: &Git) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let config = storage.load_config()?;
    let mut state = storage.load_state()?;
    let mut cache = storage.load_cache()?;
    let candidates = scan_candidates(&scan_roots(&config)?, DEFAULT_SCAN_DEPTH);
    let mut seen_common_dirs = BTreeSet::new();
    let mut families = Vec::new();

    for candidate in candidates {
        let Ok(repo) = git.verify(&candidate) else {
            continue;
        };
        if is_ignored(&state, &repo.top_level, &repo.common_dir) {
            continue;
        }
        if !seen_common_dirs.insert(repo.common_dir.clone()) {
            continue;
        }
        families.push(discover_family(git, &repo.top_level, RepoSource::Scanned)?);
    }

    merge_families(&mut state, &mut cache, families);
    cache.last_scan_unix = Some(now_unix());
    storage.save_state(&state)?;
    storage.save_cache(&cache)?;

    Ok(format!(
        "scanned {} repos\n",
        cache.repos.iter().filter(|repo| !repo.stale).count()
    ))
}

pub fn ignore(storage: &FileStorage, git: &Git, input: &str) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let mut state = storage.load_state()?;
    let mut cache = storage.load_cache()?;
    let ignored = match git.verify(Path::new(input)) {
        Ok(root) => IgnoredRepo {
            alias: cache
                .repos
                .iter()
                .find(|repo| repo.git_common_dir == root.common_dir)
                .map(|repo| repo.alias.clone()),
            canonical_path: canonical_or_self(&root.top_level),
            git_common_dir: root.common_dir,
        },
        Err(_) => {
            let resolved = Resolver::new(cache.clone()).resolve_repo(input)?;
            IgnoredRepo {
                alias: Some(resolved.alias.clone()),
                canonical_path: resolved.canonical_path.clone(),
                git_common_dir: resolved.git_common_dir.clone(),
            }
        }
    };

    if !state
        .ignored_repos
        .iter()
        .any(|entry| entry.git_common_dir == ignored.git_common_dir)
    {
        state.ignored_repos.push(ignored.clone());
    }
    state
        .adopted_paths
        .retain(|path| path != &ignored.canonical_path);
    state
        .repos
        .retain(|_, repo| repo.git_common_dir != ignored.git_common_dir);
    state.sessions.retain(|session| {
        cache
            .repos
            .iter()
            .find(|repo| repo.alias == session.repo_alias)
            .map(|repo| repo.git_common_dir != ignored.git_common_dir)
            .unwrap_or(true)
    });
    cache
        .repos
        .retain(|repo| repo.git_common_dir != ignored.git_common_dir);
    cache.worktrees.retain(|worktree| {
        cache
            .repos
            .iter()
            .find(|repo| repo.alias == worktree.repo_alias)
            .map(|repo| repo.git_common_dir != ignored.git_common_dir)
            .unwrap_or(true)
    });

    storage.save_state(&state)?;
    storage.save_cache(&cache)?;

    Ok(format!("ignored {}\n", ignored.canonical_path.display()))
}

pub fn unignore(storage: &FileStorage, git: &Git, input: &str) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let mut state = storage.load_state()?;
    let lookup = git
        .verify(Path::new(input))
        .ok()
        .map(|root| root.common_dir);
    let canonical_input = canonical_or_self(Path::new(input));
    let before = state.ignored_repos.len();
    state.ignored_repos.retain(|entry| {
        let path_match = entry.canonical_path == canonical_input;
        let common_dir_match = lookup
            .as_ref()
            .map(|common_dir| &entry.git_common_dir == common_dir)
            .unwrap_or(false);
        let alias_match = entry.alias.as_deref() == Some(input);
        !(path_match || common_dir_match || alias_match)
    });

    if state.ignored_repos.len() == before {
        return Err(AppError::InvalidCommand(format!(
            "ignored repo `{input}` was not found"
        )));
    }

    storage.save_state(&state)?;
    Ok(format!("unignored {input}\n"))
}

pub fn list(storage: &FileStorage) -> AppResult<String> {
    let cache = storage.load_cache()?;
    let mut rows = Vec::new();
    for repo in &cache.repos {
        for worktree in cache
            .worktrees
            .iter()
            .filter(|worktree| worktree.repo_alias == repo.alias)
        {
            let stale = if repo.stale || worktree.stale {
                " stale"
            } else {
                ""
            };
            let branch = worktree.branch.as_deref().unwrap_or("detached");
            rows.push(format!(
                "{} {} {} {}{}\n",
                repo.alias,
                worktree.target,
                branch,
                worktree.path.display(),
                stale
            ));
        }
    }
    Ok(rows.concat())
}

pub fn path(storage: &FileStorage, repo: &str, target: Option<&str>) -> AppResult<String> {
    let resolver = Resolver::new(storage.load_cache()?);
    let resolved = resolver.resolve_worktree(repo, target)?;
    Ok(format!("{}\n", resolved.path.display()))
}

pub fn new_worktree(storage: &FileStorage, git: &Git, repo: &str, name: &str) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let config = storage.load_config()?;
    let cache = storage.load_cache()?;
    let resolver = Resolver::new(cache);
    let repo_record = resolver.resolve_repo(repo)?;
    if repo_record.stale {
        return Err(AppError::StaleWorktree(repo_record.canonical_path));
    }

    let target_path = default_worktree_root(&config)?
        .join(&repo_record.alias)
        .join(name);
    let family = git.worktrees(&repo_record.canonical_path)?;
    if let Some(existing) = family
        .iter()
        .find(|entry| entry.branch.as_deref() == Some(name))
    {
        let existing_path = canonical_or_self(&existing.path);
        if existing_path == canonical_or_self(&target_path) {
            refresh_after_new(storage, git, &repo_record.canonical_path)?;
            return Ok(format!("{}\n", existing.path.display()));
        }
        return Err(AppError::Git(format!(
            "branch `{name}` is already checked out at {}; refusing to force",
            existing.path.display()
        )));
    }

    if target_path.exists() {
        let canonical_target = canonical_or_self(&target_path);
        if let Some(existing) = family
            .iter()
            .find(|entry| canonical_or_self(&entry.path) == canonical_target)
        {
            if existing.branch.as_deref() == Some(name) {
                refresh_after_new(storage, git, &repo_record.canonical_path)?;
                return Ok(format!("{}\n", target_path.display()));
            }
            return Err(AppError::InvalidCommand(format!(
                "target path exists as worktree for branch `{}`; refusing to report it as `{name}`",
                existing.branch.as_deref().unwrap_or("detached")
            )));
        }
        if let Ok(existing) = git.verify(&target_path)
            && existing.common_dir == repo_record.git_common_dir
        {
            return Err(AppError::InvalidCommand(format!(
                "target path exists as a worktree for this repo but is not registered in `git worktree list`: {}",
                target_path.display()
            )));
        }
        return Err(AppError::InvalidCommand(format!(
            "target path already exists and is not the expected worktree: {}",
            target_path.display()
        )));
    }

    let base = infer_base_branch(&config, git, &repo_record)?;
    update_base_before_new(git, &repo_record, &base, name)?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|source| AppError::WriteFile {
            kind: "worktree root",
            path: parent.to_path_buf(),
            source: Box::new(source),
        })?;
    }
    let created_branch = !git.branch_exists(&repo_record.canonical_path, name)?;
    if created_branch {
        git.create_branch(&repo_record.canonical_path, name, &base)?;
    }
    if let Err(error) = git.add_worktree(&repo_record.canonical_path, &target_path, name) {
        if created_branch {
            let _ = git.delete_branch(&repo_record.canonical_path, name);
        }
        return Err(error);
    }
    refresh_after_new(storage, git, &repo_record.canonical_path)?;

    Ok(format!("{}\n", target_path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BasePull {
    Tracking,
    Origin,
}

fn update_base_before_new(git: &Git, repo: &RepoRecord, base: &str, target: &str) -> AppResult<()> {
    let pull = if git.branch_upstream(&repo.canonical_path, base)?.is_some() {
        Some(BasePull::Tracking)
    } else if git.remote_url(&repo.canonical_path, "origin")?.is_some() {
        Some(BasePull::Origin)
    } else {
        None
    };
    let Some(pull) = pull else {
        return Ok(());
    };

    let current = git.current_branch(&repo.canonical_path)?;
    if current.as_deref() != Some(base) {
        let current = current.unwrap_or_else(|| "detached HEAD".to_string());
        return Err(AppError::InvalidCommand(format!(
            "base worktree is on `{current}`, not `{base}`: {}\nfix: run `git -C {} checkout {base}`\nthen: run `workroot new {} {target}`",
            repo.canonical_path.display(),
            repo.canonical_path.display(),
            repo.alias
        )));
    }

    if git.is_dirty(&repo.canonical_path)? {
        return Err(AppError::InvalidCommand(format!(
            "base worktree has uncommitted changes: {}\nfix: run `git -C {} status --short`, then commit or stash those changes\nthen: run `workroot new {} {target}`",
            repo.canonical_path.display(),
            repo.canonical_path.display(),
            repo.alias
        )));
    }

    let recovery = match pull {
        BasePull::Tracking => format!("git -C {} pull --ff-only", repo.canonical_path.display()),
        BasePull::Origin => format!(
            "git -C {} pull --ff-only origin {base}",
            repo.canonical_path.display()
        ),
    };
    let result = match pull {
        BasePull::Tracking => git.pull_ff_only(&repo.canonical_path),
        BasePull::Origin => git.pull_ff_only_from(&repo.canonical_path, "origin", base),
    };
    result.map_err(|error| {
        AppError::Git(format!(
            "could not fast-forward base branch `{base}` before creating `{target}`\nfix: run `{recovery}`\nthen: run `workroot new {} {target}`\ncause: {error}",
            repo.alias
        ))
    })
}

fn refresh_after_new(storage: &FileStorage, git: &Git, path: &Path) -> AppResult<()> {
    let mut state = storage.load_state()?;
    let mut cache = storage.load_cache()?;
    let family = discover_family(git, path, RepoSource::Workroot)?;
    merge_families(&mut state, &mut cache, vec![family]);
    storage.save_state(&state)?;
    storage.save_cache(&cache)
}

fn infer_base_branch(config: &Config, git: &Git, repo: &RepoRecord) -> AppResult<String> {
    if let Some(branch) = config
        .repos
        .get(&repo.alias)
        .and_then(|repo| repo.base_branch.clone())
    {
        return Ok(branch);
    }
    if let Some(branch) = git.remote_default_branch(&repo.canonical_path)? {
        return Ok(branch);
    }
    for branch in ["main", "master", "trunk"] {
        if git.branch_exists(&repo.canonical_path, branch)? {
            return Ok(branch.to_string());
        }
    }
    if let Some(branch) = git.current_branch(&repo.canonical_path)? {
        return Ok(branch);
    }
    Err(AppError::Git(
        "could not infer a base branch; configure a repo base branch before running `workroot new`"
            .to_string(),
    ))
}

#[derive(Debug, Clone)]
struct Family {
    common_dir: PathBuf,
    canonical_path: PathBuf,
    display_name: String,
    base_branch: Option<String>,
    source: RepoSource,
    worktrees: Vec<GitWorktreeEntry>,
}

fn discover_family(git: &Git, path: &Path, source: RepoSource) -> AppResult<Family> {
    let root = git.verify(path)?;
    let worktrees = git.worktrees(&root.top_level)?;
    let canonical_path = canonical_or_self(
        &worktrees
            .first()
            .map(|entry| entry.path.clone())
            .unwrap_or_else(|| root.top_level.clone()),
    );
    let display_name = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo")
        .to_string();
    let base_branch = worktrees.first().and_then(|entry| entry.branch.clone());

    Ok(Family {
        common_dir: root.common_dir,
        canonical_path,
        display_name,
        base_branch,
        source,
        worktrees,
    })
}

fn merge_families(state: &mut State, cache: &mut Cache, families: Vec<Family>) {
    for repo in &mut cache.repos {
        repo.stale = !repo.canonical_path.exists();
    }
    for worktree in &mut cache.worktrees {
        worktree.stale = !worktree.path.exists();
    }

    let seen: BTreeSet<_> = families
        .iter()
        .map(|family| family.common_dir.clone())
        .collect();
    for repo in &mut cache.repos {
        if seen.contains(&repo.git_common_dir) {
            repo.stale = !repo.canonical_path.exists();
        }
    }
    for worktree in &mut cache.worktrees {
        if let Some(repo) = cache
            .repos
            .iter()
            .find(|repo| repo.alias == worktree.repo_alias)
            && seen.contains(&repo.git_common_dir)
        {
            worktree.stale = !worktree.path.exists();
        }
    }

    for family in families {
        let alias = alias_for_family(cache, &family);
        let repo = RepoRecord {
            alias: alias.clone(),
            display_name: family.display_name.clone(),
            canonical_path: family.canonical_path.clone(),
            git_common_dir: family.common_dir.clone(),
            base_branch: family.base_branch.clone(),
            source: family.source,
            stale: !family.canonical_path.exists(),
        };
        upsert_repo(cache, repo.clone());
        state.repos.insert(
            alias.clone(),
            RepoState {
                alias: alias.clone(),
                canonical_path: repo.canonical_path.clone(),
                git_common_dir: repo.git_common_dir.clone(),
                base_branch: repo.base_branch.clone(),
                source: repo.source,
            },
        );

        let existing_targets: BTreeMap<PathBuf, String> = cache
            .worktrees
            .iter()
            .filter(|worktree| worktree.repo_alias == alias)
            .map(|worktree| (canonical_or_self(&worktree.path), worktree.target.clone()))
            .collect();
        let mut target_counts = BTreeMap::<String, usize>::new();
        let mut records = Vec::new();
        for (index, worktree) in family.worktrees.iter().enumerate() {
            let path = canonical_or_self(&worktree.path);
            let display = if index == 0 {
                "base".to_string()
            } else {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("worktree")
                    .to_string()
            };
            let target = if index == 0 {
                "base".to_string()
            } else {
                existing_targets
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| unique_target(&display, &mut target_counts))
            };
            target_counts.entry(target.clone()).or_insert(1);
            records.push(WorktreeRecord {
                repo_alias: alias.clone(),
                target,
                display_name: display,
                branch: worktree.branch.clone(),
                path,
                source: worktree_source(&worktree.path, family.source),
                dirty: DirtyState::Unknown,
                last_seen_unix: Some(now_unix()),
                stale: !worktree.path.exists(),
                detached: worktree.detached,
            });
        }
        cache
            .worktrees
            .retain(|worktree| worktree.repo_alias != alias);
        cache.worktrees.extend(records);
    }
    cache
        .repos
        .sort_by(|left, right| left.alias.cmp(&right.alias));
    cache.worktrees.sort_by(|left, right| {
        left.repo_alias
            .cmp(&right.repo_alias)
            .then_with(|| left.target.cmp(&right.target))
    });
}

fn alias_for_family(cache: &Cache, family: &Family) -> String {
    if let Some(existing) = cache
        .repos
        .iter()
        .find(|repo| repo.git_common_dir == family.common_dir)
    {
        return existing.alias.clone();
    }
    unique_alias(cache, &family.display_name)
}

fn unique_alias(cache: &Cache, display: &str) -> String {
    let used: BTreeSet<_> = cache.repos.iter().map(|repo| repo.alias.as_str()).collect();
    if !used.contains(display) {
        return display.to_string();
    }
    for suffix in 2.. {
        let candidate = format!("{display}-{suffix}");
        if !used.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!()
}

fn unique_target(display: &str, counts: &mut BTreeMap<String, usize>) -> String {
    let base = if display == "base" {
        "base-2".to_string()
    } else {
        display.to_string()
    };
    let count = counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count)
    }
}

fn upsert_repo(cache: &mut Cache, repo: RepoRecord) {
    if let Some(existing) = cache
        .repos
        .iter_mut()
        .find(|existing| existing.git_common_dir == repo.git_common_dir)
    {
        *existing = repo;
    } else {
        cache.repos.push(repo);
    }
}

fn worktree_source(path: &Path, source: RepoSource) -> WorktreeSource {
    if source == RepoSource::Workroot {
        return WorktreeSource::Workroot;
    }
    if path.components().any(|part| part.as_os_str() == "workmux") {
        WorktreeSource::Workmux
    } else {
        WorktreeSource::Manual
    }
}

fn scan_roots(config: &Config) -> AppResult<Vec<PathBuf>> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".worktrees"));
        roots.push(home.join(".worktree"));
        roots.push(home.join("worktrees"));
    }
    roots.extend(config.scan_roots.iter().cloned());
    if roots.is_empty() {
        return Err(AppError::MissingHome);
    }
    Ok(roots)
}

fn default_worktree_root(config: &Config) -> AppResult<PathBuf> {
    if let Some(root) = config.default_worktree_root.clone() {
        return Ok(root);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(AppError::MissingHome)?;
    Ok(home.join(".worktrees"))
}

fn scan_candidates(roots: &[PathBuf], max_depth: usize) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for root in roots {
        walk_candidates(root, 0, max_depth, &mut candidates);
    }
    candidates
}

fn walk_candidates(path: &Path, depth: usize, max_depth: usize, candidates: &mut Vec<PathBuf>) {
    if depth > max_depth || !path.is_dir() {
        return;
    }
    if path.join(".git").exists() {
        candidates.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            walk_candidates(&child, depth + 1, max_depth, candidates);
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn discover_path(storage: &FileStorage, git: &Git, path: &Path) -> AppResult<String> {
    let _transaction = storage.transaction()?;
    let mut state = storage.load_state()?;
    let mut cache = storage.load_cache()?;
    let root = git.verify(path)?;
    if is_ignored(&state, &root.top_level, &root.common_dir) {
        return Err(AppError::InvalidCommand(format!(
            "repo at `{}` is ignored\nfix: run `workroot unignore {}` first",
            root.top_level.display(),
            root.top_level.display()
        )));
    }
    let adopted_path = canonical_or_self(&root.top_level);
    if !state
        .adopted_paths
        .iter()
        .any(|known| known == &adopted_path)
    {
        state.adopted_paths.push(adopted_path);
    }
    let family = discover_family(git, &root.top_level, RepoSource::Adopted)?;
    let alias = family.display_name.clone();
    merge_families(&mut state, &mut cache, vec![family]);
    storage.save_state(&state)?;
    storage.save_cache(&cache)?;
    Ok(format!("discovered {}\n", alias))
}

fn is_ignored(state: &State, canonical_path: &Path, git_common_dir: &Path) -> bool {
    let canonical_path = canonical_or_self(canonical_path);
    state.ignored_repos.iter().any(|entry| {
        entry.git_common_dir == git_common_dir || entry.canonical_path == canonical_path
    })
}
