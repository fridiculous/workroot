use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::domain::{CURRENT_SCHEMA_VERSION, Cache, Config, State, Versioned};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePaths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
}

impl StoragePaths {
    pub fn default_for_user() -> AppResult<Self> {
        let config_root = root_from_env(
            "WORKROOT_CONFIG_HOME",
            "ROSTRUM_CONFIG_HOME",
            "XDG_CONFIG_HOME",
            &[".config"],
        )?;
        let state_root = root_from_env(
            "WORKROOT_STATE_HOME",
            "ROSTRUM_STATE_HOME",
            "XDG_STATE_HOME",
            &[".local", "state"],
        )?;
        let cache_root = root_from_env(
            "WORKROOT_CACHE_HOME",
            "ROSTRUM_CACHE_HOME",
            "XDG_CACHE_HOME",
            &[".cache"],
        )?;

        Ok(Self {
            config: config_root.join("workroot").join("config.toml"),
            state: state_root.join("workroot").join("state.json"),
            cache: cache_root.join("workroot").join("index.json"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FileStorage {
    paths: StoragePaths,
}

#[derive(Debug)]
pub struct StorageTransaction {
    file: File,
}

impl Drop for StorageTransaction {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl FileStorage {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn for_user() -> AppResult<Self> {
        Ok(Self::new(StoragePaths::default_for_user()?))
    }

    pub fn paths(&self) -> &StoragePaths {
        &self.paths
    }

    pub fn transaction(&self) -> AppResult<StorageTransaction> {
        let transaction_path = self.paths.state.with_file_name("transaction.lock");
        let file = open_lock_file("transaction", &transaction_path)?;
        file.lock_exclusive()
            .map_err(|source| AppError::WriteFile {
                kind: "transaction",
                path: transaction_path,
                source: Box::new(source),
            })?;
        Ok(StorageTransaction { file })
    }

    pub fn load_config(&self) -> AppResult<Config> {
        load_toml_or_default("config", &self.paths.config)
    }

    pub fn save_config(&self, value: &Config) -> AppResult<()> {
        save_toml_atomic("config", &self.paths.config, value)
    }

    pub fn load_state(&self) -> AppResult<State> {
        load_json_or_default("state", &self.paths.state)
    }

    pub fn save_state(&self, value: &State) -> AppResult<()> {
        save_json_atomic("state", &self.paths.state, value)
    }

    pub fn load_cache(&self) -> AppResult<Cache> {
        load_json_or_default("cache", &self.paths.cache)
    }

    pub fn save_cache(&self, value: &Cache) -> AppResult<()> {
        save_json_atomic("cache", &self.paths.cache, value)
    }
}

fn load_json_or_default<T>(kind: &'static str, path: &Path) -> AppResult<T>
where
    T: DeserializeOwned + Default + Versioned,
{
    if !path.exists() {
        return Ok(T::default());
    }

    let lock = lock_file(kind, path, false)?;
    let mut contents = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut contents))
        .map_err(|source| AppError::ReadFile {
            kind,
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
    unlock(lock);

    if contents.trim().is_empty() {
        return Ok(T::default());
    }

    let value: T = serde_json::from_str(&contents).map_err(|source| AppError::ParseJson {
        kind,
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    reject_future_schema(value.schema_version())?;
    Ok(value)
}

fn save_json_atomic<T>(kind: &'static str, path: &Path, value: &T) -> AppResult<()>
where
    T: Serialize,
{
    let lock = lock_file(kind, path, true)?;
    ensure_parent(kind, path)?;
    let contents = serde_json::to_string_pretty(value)
        .map(|json| format!("{json}\n"))
        .map_err(|source| AppError::SerializeJson {
            kind,
            source: Box::new(source),
        })?;
    write_atomic(kind, path, contents.as_bytes())?;
    unlock(lock);
    Ok(())
}

fn load_toml_or_default<T>(kind: &'static str, path: &Path) -> AppResult<T>
where
    T: DeserializeOwned + Default + Versioned,
{
    if !path.exists() {
        return Ok(T::default());
    }

    let lock = lock_file(kind, path, false)?;
    let mut contents = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut contents))
        .map_err(|source| AppError::ReadFile {
            kind,
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
    unlock(lock);

    if contents.trim().is_empty() {
        return Ok(T::default());
    }

    let value: T = toml::from_str(&contents).map_err(|source| AppError::ParseToml {
        kind,
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    reject_future_schema(value.schema_version())?;
    Ok(value)
}

fn save_toml_atomic<T>(kind: &'static str, path: &Path, value: &T) -> AppResult<()>
where
    T: Serialize,
{
    let lock = lock_file(kind, path, true)?;
    ensure_parent(kind, path)?;
    let contents = toml::to_string_pretty(value).map_err(|source| AppError::SerializeToml {
        kind,
        source: Box::new(source),
    })?;
    write_atomic(kind, path, contents.as_bytes())?;
    unlock(lock);
    Ok(())
}

fn write_atomic(kind: &'static str, path: &Path, contents: &[u8]) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| AppError::WriteFile {
        kind,
        path: path.to_path_buf(),
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent",
        )),
    })?;
    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workroot"),
        std::process::id()
    ));

    let write_result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok::<(), std::io::Error>(())
    })();

    if let Err(source) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(AppError::WriteFile {
            kind,
            path: path.to_path_buf(),
            source: Box::new(source),
        });
    }

    Ok(())
}

fn lock_file(kind: &'static str, path: &Path, exclusive: bool) -> AppResult<File> {
    ensure_parent(kind, path)?;
    let lock_path = path.with_file_name(format!(
        "{}.lock",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workroot")
    ));
    let file = open_lock_file(kind, &lock_path)?;

    let result = if exclusive {
        file.lock_exclusive()
    } else {
        file.lock_shared()
    };
    result.map_err(|source| AppError::WriteFile {
        kind,
        path: lock_path,
        source: Box::new(source),
    })?;
    Ok(file)
}

fn open_lock_file(kind: &'static str, path: &Path) -> AppResult<File> {
    ensure_parent(kind, path)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| AppError::WriteFile {
            kind,
            path: path.to_path_buf(),
            source: Box::new(source),
        })
}

fn unlock(file: File) {
    let _ = file.unlock();
}

fn ensure_parent(kind: &'static str, path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AppError::WriteFile {
            kind,
            path: parent.to_path_buf(),
            source: Box::new(source),
        })?;
    }
    Ok(())
}

fn reject_future_schema(found: u32) -> AppResult<()> {
    if found > CURRENT_SCHEMA_VERSION {
        return Err(AppError::FutureSchema {
            found,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn root_from_env(
    primary_key: &str,
    legacy_key: &str,
    xdg_key: &str,
    home_suffix: &[&str],
) -> AppResult<PathBuf> {
    if let Some(value) = env::var_os(primary_key) {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = env::var_os(legacy_key) {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = env::var_os(xdg_key) {
        return Ok(PathBuf::from(value));
    }

    let mut home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(AppError::MissingHome)?;
    for part in home_suffix {
        home.push(part);
    }
    Ok(home)
}
