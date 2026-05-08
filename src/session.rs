use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::domain::{SessionBackend, SessionRecord, SessionStatus, State};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    argv: Vec<String>,
}

impl CommandSpec {
    pub fn new(argv: Vec<String>) -> AppResult<Self> {
        if argv.is_empty() {
            return Err(AppError::InvalidCommand(
                "expected command after `--`".to_string(),
            ));
        }
        Ok(Self { argv })
    }

    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    pub fn to_posix_shell_command(&self) -> String {
        self.argv
            .iter()
            .map(|arg| posix_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingSession {
    Running,
    Missing,
}

#[derive(Debug, Clone)]
pub struct Tmux {
    executable: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxPane {
    pub session_name: String,
    pub current_path: PathBuf,
    pub current_command: String,
}

impl Default for Tmux {
    fn default() -> Self {
        Self::new("tmux")
    }
}

impl Tmux {
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn ensure_available(&self) -> AppResult<()> {
        let status = Command::new(&self.executable)
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    AppError::MissingDependency { name: "tmux" }
                } else {
                    AppError::Tmux(source.to_string())
                }
            })?;
        if !status.success() {
            return Err(AppError::Tmux("tmux -V failed".to_string()));
        }
        Ok(())
    }

    pub fn session_state(&self, session_name: &str) -> AppResult<ExistingSession> {
        let status = Command::new(&self.executable)
            .args(["has-session", "-t", session_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    AppError::MissingDependency { name: "tmux" }
                } else {
                    AppError::Tmux(source.to_string())
                }
            })?;

        if status.success() {
            Ok(ExistingSession::Running)
        } else {
            Ok(ExistingSession::Missing)
        }
    }

    pub fn create_pair_session(
        &self,
        session_name: &str,
        cwd: &Path,
        command: &CommandSpec,
    ) -> AppResult<()> {
        let command = command.to_posix_shell_command();
        let status = Command::new(&self.executable)
            .args(["new-session", "-d", "-s", session_name, "-c"])
            .arg(cwd)
            .arg(command)
            .status()
            .map_err(|source| AppError::Tmux(source.to_string()))?;
        if !status.success() {
            return Err(AppError::Tmux(format!(
                "new-session failed for `{session_name}`"
            )));
        }

        let status = Command::new(&self.executable)
            .args(["split-window", "-h", "-t", session_name, "-c"])
            .arg(cwd)
            .status()
            .map_err(|source| AppError::Tmux(source.to_string()))?;
        if !status.success() {
            let _ = self.kill_session(session_name);
            return Err(AppError::Tmux(format!(
                "split-window failed for `{session_name}`"
            )));
        }

        Ok(())
    }

    pub fn kill_session(&self, session_name: &str) -> AppResult<()> {
        let status = Command::new(&self.executable)
            .args(["kill-session", "-t", session_name])
            .status()
            .map_err(|source| AppError::Tmux(source.to_string()))?;
        if !status.success() {
            return Err(AppError::Tmux(format!(
                "kill-session failed for `{session_name}`"
            )));
        }
        Ok(())
    }

    pub fn attach(&self, session_name: &str) -> AppResult<()> {
        let status = Command::new(&self.executable)
            .args(["attach-session", "-t", session_name])
            .status()
            .map_err(|source| AppError::Tmux(source.to_string()))?;
        if !status.success() {
            return Err(AppError::Tmux(format!(
                "attach-session failed for `{session_name}`"
            )));
        }
        Ok(())
    }

    pub fn list_panes(&self) -> AppResult<Vec<TmuxPane>> {
        let output = Command::new(&self.executable)
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name}\t#{pane_current_path}\t#{pane_current_command}",
            ])
            .output()
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    AppError::MissingDependency { name: "tmux" }
                } else {
                    AppError::Tmux(source.to_string())
                }
            })?;

        if !output.status.success() {
            return Err(AppError::Tmux("list-panes failed".to_string()));
        }

        Ok(parse_tmux_list_panes(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }
}

pub fn parse_tmux_list_panes(input: &str) -> Vec<TmuxPane> {
    input
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let session_name = parts.next()?.trim();
            let current_path = parts.next()?.trim();
            let current_command = parts.next()?.trim();
            if session_name.is_empty() {
                return None;
            }
            Some(TmuxPane {
                session_name: session_name.to_string(),
                current_path: PathBuf::from(current_path),
                current_command: current_command.to_string(),
            })
        })
        .collect()
}

pub fn find_session_mut<'a>(
    state: &'a mut State,
    repo_alias: &str,
    target: &str,
) -> Option<&'a mut SessionRecord> {
    state
        .sessions
        .iter_mut()
        .find(|session| session.repo_alias == repo_alias && session.target == target)
}

pub fn upsert_running_session(
    state: &mut State,
    repo_alias: String,
    target: String,
    worktree_path: std::path::PathBuf,
    command: &CommandSpec,
    tmux_session_name: String,
) {
    if let Some(session) = find_session_mut(state, &repo_alias, &target) {
        session.worktree_path = worktree_path;
        session.command = command.argv().to_vec();
        session.tmux_session_name = tmux_session_name;
        session.status = SessionStatus::Running;
        return;
    }

    state.sessions.push(SessionRecord {
        repo_alias,
        target,
        worktree_path,
        backend: SessionBackend::Tmux,
        command: command.argv().to_vec(),
        tmux_session_name,
        status: SessionStatus::Running,
    });
}

pub fn posix_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'=' | b'+' | b',')
    }) {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn sanitize_tmux_session_name(repo_alias: &str, target: &str) -> String {
    let raw = format!("{repo_alias}-{target}");
    let mut sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        sanitized.push_str("session");
    }

    let hash = stable_hash(&format!("{repo_alias}\0{target}"));
    let suffix = format!("-{hash:08x}");
    let max_prefix_len = 80usize.saturating_sub("workroot-".len() + suffix.len());
    if sanitized.len() > max_prefix_len {
        sanitized.truncate(max_prefix_len);
        sanitized = sanitized.trim_end_matches(['-', '_', '.']).to_string();
    }

    format!("workroot-{sanitized}{suffix}")
}

fn stable_hash(input: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in input.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
