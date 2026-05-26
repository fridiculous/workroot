use std::collections::{BTreeMap, HashMap};
use std::io::IsTerminal;

use crate::domain::{Cache, RepoRecord, WorktreeRecord};

use super::{
    BranchDisplay, LiveWorktreeStatus, RadarState, RadarSummary, RadarTmuxRow, RadarView,
    RadarWorktreeRow, base_branch_label, radar_state_label, sorted_worktrees, status_dirty_label,
    status_head_label, worktree_key,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct TableCell {
    text: String,
    style: Option<AnsiStyle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnsiStyle {
    Green,
    Yellow,
    Red,
    Cyan,
    Blue,
    Magenta,
    Dim,
}

pub(super) fn render_status(
    cache: &Cache,
    statuses: &BTreeMap<String, LiveWorktreeStatus>,
) -> String {
    let repos_by_alias = cache
        .repos
        .iter()
        .map(|repo| (repo.alias.as_str(), repo))
        .collect::<HashMap<_, _>>();
    let mut by_repo = BTreeMap::<String, Vec<&WorktreeRecord>>::new();
    for worktree in sorted_worktrees(cache) {
        by_repo
            .entry(worktree.repo_alias.clone())
            .or_default()
            .push(worktree);
    }

    let mut output = String::new();
    for (repo_alias, worktrees) in by_repo {
        let repo: Option<&RepoRecord> = repos_by_alias.get(repo_alias.as_str()).copied();
        output.push_str(&format!("repo {repo_alias}\n"));
        output.push_str(&format!("  base {}\n", base_line(repo, "unknown")));
        for worktree in worktrees {
            let status = statuses
                .get(&worktree_key(worktree))
                .expect("status exists for every sorted worktree");
            output.push_str(&render_worktree_tree(
                worktree, status, "none", "-", "unknown",
            ));
        }
    }

    output
}

pub(super) fn render_radar_view(view: &RadarView) -> String {
    let mut output = String::new();
    output.push_str(&render_summary(&view.summary));
    output.push('\n');
    output.push('\n');

    let mut rows = view
        .attention
        .iter()
        .chain(view.active.iter())
        .chain(view.idle.iter())
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.repo
            .cmp(&right.repo)
            .then_with(|| left.target.cmp(&right.target))
    });
    let mut by_repo = BTreeMap::<String, Vec<&RadarWorktreeRow>>::new();
    for row in rows {
        by_repo.entry(row.repo.clone()).or_default().push(row);
    }

    for (repo, rows) in by_repo {
        output.push_str(&format!("repo {repo}\n"));
        if let Some(first) = rows.first() {
            output.push_str(&format!(
                "  base {} @ {}\n",
                first.base_branch, first.base_head
            ));
        }
        for row in rows {
            output.push_str(&render_radar_worktree_tree(row));
        }
    }

    output.push('\n');
    render_tmux_section(
        &mut output,
        "UNMAPPED TMUX",
        &view.unmapped,
        color_enabled(),
    );
    output
}

fn base_line(repo: Option<&RepoRecord>, head: &str) -> String {
    format!("{} @ {head}", base_branch_label(repo))
}

fn render_worktree_tree(
    worktree: &WorktreeRecord,
    status: &LiveWorktreeStatus,
    session: &str,
    command: &str,
    upstream: &str,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("  worktree {}\n", worktree.target));
    output.push_str(&format!("    path {}\n", worktree.path.display()));
    output.push_str(&format!("    {}\n", head_line(status)));
    if matches!(status.branch, BranchDisplay::Named(_)) {
        output.push_str("    branch locked here\n");
        output.push_str(&format!("    upstream {upstream}\n"));
    } else if status.unbranched_commits {
        output.push_str("    unbranched commits\n");
    }
    output.push_str(&format!("    state {}\n", status_dirty_label(status)));
    output.push_str(&format!("    session {session}\n"));
    if command != "-" {
        output.push_str(&format!("    command {command}\n"));
    }
    output
}

fn render_radar_worktree_tree(row: &RadarWorktreeRow) -> String {
    let status = LiveWorktreeStatus {
        branch: if row.branch == "detached" {
            BranchDisplay::Detached
        } else if row.branch == "unknown" {
            BranchDisplay::Unknown
        } else {
            BranchDisplay::Named(row.branch.clone())
        },
        head: Some(row.head.clone()).filter(|head| head != "unknown" && head != "stale"),
        upstream: Some(row.upstream.clone()).filter(|upstream| upstream != "none"),
        unbranched_commits: row.unbranched_commits,
        dirty: dirty_from_label(&row.dirty),
        stale: row.dirty == "stale",
    };
    let session = if row.session == "-" {
        "none"
    } else {
        row.session.as_str()
    };
    let upstream = if row.upstream == "none" {
        "none (unpushed)"
    } else {
        row.upstream.as_str()
    };
    let worktree = WorktreeRecord {
        repo_alias: row.repo.clone(),
        target: row.target.clone(),
        display_name: row.target.clone(),
        branch: match &status.branch {
            BranchDisplay::Named(branch) => Some(branch.clone()),
            _ => None,
        },
        path: row.path.clone().into(),
        source: crate::domain::WorktreeSource::Unknown,
        dirty: crate::domain::DirtyState::Unknown,
        last_seen_unix: None,
        stale: status.stale,
        detached: matches!(status.branch, BranchDisplay::Detached),
    };
    render_worktree_tree(&worktree, &status, session, &row.command, upstream)
}

fn dirty_from_label(label: &str) -> crate::domain::DirtyState {
    if label == "clean" {
        crate::domain::DirtyState::Clean
    } else if let Some(files) = label
        .strip_prefix("dirty(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<u32>().ok())
    {
        crate::domain::DirtyState::Dirty { files }
    } else {
        crate::domain::DirtyState::Unknown
    }
}

fn head_line(status: &LiveWorktreeStatus) -> String {
    match &status.branch {
        BranchDisplay::Named(branch) => {
            format!("head -> branch {branch} @ {}", status_head_label(status))
        }
        BranchDisplay::Detached => format!("head detached @ {}", status_head_label(status)),
        BranchDisplay::Unknown => format!("head unknown @ {}", status_head_label(status)),
    }
}

fn render_summary(summary: &RadarSummary) -> String {
    format!(
        "SUMMARY repos={} worktrees={} tmux={} active-panes={} managed-running={} exited={} unmapped={} dirty={} stale={}",
        summary.repos,
        summary.worktrees,
        if summary.tmux_available {
            "ok"
        } else {
            "unavailable"
        },
        optional_count(summary.active_panes),
        optional_count(summary.managed_running),
        optional_count(summary.exited),
        optional_count(summary.unmapped),
        summary.dirty,
        summary.stale
    )
}

fn render_tmux_section(output: &mut String, title: &str, rows: &[RadarTmuxRow], colors: bool) {
    output.push_str(title);
    output.push('\n');
    if rows.is_empty() {
        output.push_str("  none\n");
        return;
    }

    output.push_str(&render_styled_table(
        std::iter::once(vec![
            plain_cell("STATE"),
            plain_cell("TMUX"),
            plain_cell("COMMAND"),
            plain_cell("CWD"),
        ])
        .chain(rows.iter().map(|row| {
            vec![
                state_cell(row.state),
                plain_cell(&row.session),
                plain_cell(&row.command),
                plain_cell(&row.cwd),
            ]
        }))
        .collect(),
        colors,
    ));
}

fn optional_count(count: Option<usize>) -> String {
    count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn radar_state_style(state: RadarState) -> AnsiStyle {
    match state {
        RadarState::Run => AnsiStyle::Green,
        RadarState::Exit => AnsiStyle::Red,
        RadarState::Map => AnsiStyle::Cyan,
        RadarState::Unmapped => AnsiStyle::Blue,
        RadarState::Idle => AnsiStyle::Dim,
        RadarState::Dirty => AnsiStyle::Yellow,
        RadarState::Stale => AnsiStyle::Red,
        RadarState::Unknown => AnsiStyle::Magenta,
    }
}

fn plain_cell(text: impl Into<String>) -> TableCell {
    TableCell {
        text: text.into(),
        style: None,
    }
}

fn state_cell(state: RadarState) -> TableCell {
    TableCell {
        text: radar_state_label(state).to_string(),
        style: Some(radar_state_style(state)),
    }
}

fn render_styled_table(rows: Vec<Vec<TableCell>>, colors: bool) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0; column_count];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.text.len());
        }
    }

    let mut output = String::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                output.push_str("  ");
            }
            output.push_str(&render_cell(cell, colors));
            if index + 1 < row.len() {
                output.push_str(&" ".repeat(widths[index] - cell.text.len()));
            }
        }
        output.push('\n');
    }
    output
}

fn render_cell(cell: &TableCell, colors: bool) -> String {
    match (colors, cell.style) {
        (true, Some(style)) => paint(&cell.text, style),
        _ => cell.text.clone(),
    }
}

fn paint(text: &str, style: AnsiStyle) -> String {
    let code = match style {
        AnsiStyle::Green => "32",
        AnsiStyle::Yellow => "33",
        AnsiStyle::Red => "31",
        AnsiStyle::Cyan => "36",
        AnsiStyle::Blue => "34",
        AnsiStyle::Magenta => "35",
        AnsiStyle::Dim => "2",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

pub(super) fn render_table(rows: Vec<Vec<String>>) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0; column_count];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    let mut output = String::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                output.push_str("  ");
            }
            output.push_str(cell);
            if index + 1 < row.len() {
                output.push_str(&" ".repeat(widths[index] - cell.len()));
            }
        }
        output.push('\n');
    }
    output
}
