use std::collections::{BTreeMap, HashMap};
use std::io::IsTerminal;

use crate::domain::{Cache, RepoRecord};

use super::{
    LiveWorktreeStatus, RadarState, RadarSummary, RadarTmuxRow, RadarView, RadarWorktreeRow,
    base_branch_label, branch_label, radar_state_label, sorted_worktrees, status_dirty_label,
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
    let mut rows = Vec::new();
    rows.push(vec![
        "REPO".to_string(),
        "BASE BRANCH".to_string(),
        "WORKTREE BRANCH".to_string(),
        "HEAD".to_string(),
        "DIRTY".to_string(),
        "PATH".to_string(),
    ]);

    for worktree in sorted_worktrees(cache) {
        let status = statuses
            .get(&worktree_key(worktree))
            .expect("status exists for every sorted worktree");
        let repo: Option<&RepoRecord> = repos_by_alias.get(worktree.repo_alias.as_str()).copied();
        rows.push(vec![
            worktree.repo_alias.clone(),
            base_branch_label(repo),
            branch_label(&status.branch),
            status_head_label(status),
            status_dirty_label(status),
            worktree.path.display().to_string(),
        ]);
    }

    render_table(rows)
}

pub(super) fn render_radar_view(view: &RadarView) -> String {
    let colors = color_enabled();
    let mut output = String::new();
    output.push_str(&render_summary(&view.summary));
    output.push('\n');
    output.push('\n');
    render_worktree_section(&mut output, "ATTENTION", &view.attention, colors);
    output.push('\n');
    render_worktree_section(&mut output, "ACTIVE PROCESSES", &view.active, colors);
    output.push('\n');
    render_worktree_section(&mut output, "IDLE WORKTREES", &view.idle, colors);
    output.push('\n');
    render_tmux_section(&mut output, "UNMAPPED TMUX", &view.unmapped, colors);
    output
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

fn render_worktree_section(
    output: &mut String,
    title: &str,
    rows: &[RadarWorktreeRow],
    colors: bool,
) {
    output.push_str(title);
    output.push('\n');
    if rows.is_empty() {
        output.push_str("  none\n");
        return;
    }

    output.push_str(&render_styled_table(
        std::iter::once(vec![
            plain_cell("STATE"),
            plain_cell("REPO"),
            plain_cell("TARGET"),
            plain_cell("BASE"),
            plain_cell("BRANCH"),
            plain_cell("HEAD"),
            plain_cell("DIRTY"),
            plain_cell("SESSION"),
            plain_cell("COMMAND"),
            plain_cell("PATH"),
        ])
        .chain(rows.iter().map(|row| {
            vec![
                state_cell(row.state),
                plain_cell(&row.repo),
                plain_cell(&row.target),
                plain_cell(&row.base_branch),
                plain_cell(&row.branch),
                plain_cell(&row.head),
                plain_cell(&row.dirty),
                plain_cell(&row.session),
                plain_cell(&row.command),
                plain_cell(&row.path),
            ]
        }))
        .collect(),
        colors,
    ));
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
