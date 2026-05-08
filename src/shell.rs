use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    pub fn parse(input: &str) -> AppResult<Self> {
        match input {
            "zsh" => Ok(Self::Zsh),
            "bash" => Ok(Self::Bash),
            "fish" => Ok(Self::Fish),
            other => Err(AppError::UnsupportedShell(other.to_string())),
        }
    }
}

pub fn shell_init(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => ZSH_INIT,
        Shell::Bash => BASH_INIT,
        Shell::Fish => FISH_INIT,
    }
}

const ZSH_INIT: &str = r#"# Workroot shell integration for zsh
workroot() {
  if [ "$1" = "cd" ]; then
    shift
    local dest
    dest="$(command workroot worktree path "$@")" || return
    cd "$dest"
  elif [ "$1" = "new" ]; then
    shift
    local dest
    dest="$(command workroot worktree new "$@")" || return
    printf '%s\n' "$dest"
    cd "$dest"
  elif { [ "$1" = "worktree" ] || [ "$1" = "workdir" ]; } && [ "$2" = "cd" ]; then
    shift 2
    local dest
    dest="$(command workroot worktree path "$@")" || return
    cd "$dest"
  elif { [ "$1" = "worktree" ] || [ "$1" = "workdir" ]; } && [ "$2" = "new" ]; then
    shift 2
    local dest
    dest="$(command workroot worktree new "$@")" || return
    printf '%s\n' "$dest"
    cd "$dest"
  else
    command workroot "$@"
  fi
}

wr() {
  workroot "$@"
}

_workroot_complete() {
  local -a candidates
  if [[ "${words[2]}" == "cd" || "${words[2]}" == "path" || "${words[2]}" == "status" || "${words[2]}" == "prune" || "${words[2]}" == "run" ]]; then
    if (( CURRENT == 3 )); then
      candidates=(${(f)"$(command workroot complete repos "${words[CURRENT]}" 2>/dev/null)"})
      compadd -- "$candidates[@]"
    elif (( CURRENT == 4 )); then
      candidates=(${(f)"$(command workroot complete targets "${words[3]}" "${words[CURRENT]}" 2>/dev/null)"})
      compadd -- "$candidates[@]"
    fi
  elif [[ "${words[2]}" == "new" ]]; then
    if (( CURRENT == 3 )); then
      candidates=(${(f)"$(command workroot complete repos "${words[CURRENT]}" 2>/dev/null)"})
      compadd -- "$candidates[@]"
    fi
  elif [[ ( "${words[2]}" == "worktree" || "${words[2]}" == "workdir" ) && ( "${words[3]}" == "cd" || "${words[3]}" == "path" || "${words[3]}" == "prune" || "${words[3]}" == "run" ) ]]; then
    if (( CURRENT == 4 )); then
      candidates=(${(f)"$(command workroot complete repos "${words[CURRENT]}" 2>/dev/null)"})
      compadd -- "$candidates[@]"
    elif (( CURRENT == 5 )); then
      candidates=(${(f)"$(command workroot complete targets "${words[4]}" "${words[CURRENT]}" 2>/dev/null)"})
      compadd -- "$candidates[@]"
    fi
  elif [[ ( "${words[2]}" == "worktree" || "${words[2]}" == "workdir" ) && "${words[3]}" == "new" ]]; then
    if (( CURRENT == 4 )); then
      candidates=(${(f)"$(command workroot complete repos "${words[CURRENT]}" 2>/dev/null)"})
      compadd -- "$candidates[@]"
    fi
  fi
}
compdef _workroot_complete workroot
compdef _workroot_complete wr
"#;

const BASH_INIT: &str = r#"# Workroot shell integration for bash
workroot() {
  if [ "$1" = "cd" ]; then
    shift
    local dest
    dest="$(command workroot worktree path "$@")" || return
    cd "$dest"
  elif [ "$1" = "new" ]; then
    shift
    local dest
    dest="$(command workroot worktree new "$@")" || return
    printf '%s\n' "$dest"
    cd "$dest"
  elif { [ "$1" = "worktree" ] || [ "$1" = "workdir" ]; } && [ "$2" = "cd" ]; then
    shift 2
    local dest
    dest="$(command workroot worktree path "$@")" || return
    cd "$dest"
  elif { [ "$1" = "worktree" ] || [ "$1" = "workdir" ]; } && [ "$2" = "new" ]; then
    shift 2
    local dest
    dest="$(command workroot worktree new "$@")" || return
    printf '%s\n' "$dest"
    cd "$dest"
  else
    command workroot "$@"
  fi
}

wr() {
  workroot "$@"
}

_workroot_complete() {
  local cur prev cmd
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  cmd="${COMP_WORDS[1]}"
  if [[ "$cmd" == "cd" || "$cmd" == "path" || "$cmd" == "status" || "$cmd" == "prune" || "$cmd" == "run" ]]; then
    if [[ $COMP_CWORD -eq 2 ]]; then
      while IFS= read -r candidate; do COMPREPLY+=("$candidate"); done < <(command workroot complete repos "$cur" 2>/dev/null)
    elif [[ $COMP_CWORD -eq 3 ]]; then
      prev="${COMP_WORDS[2]}"
      while IFS= read -r candidate; do COMPREPLY+=("$candidate"); done < <(command workroot complete targets "$prev" "$cur" 2>/dev/null)
    fi
  elif [[ "$cmd" == "new" ]]; then
    if [[ $COMP_CWORD -eq 2 ]]; then
      while IFS= read -r candidate; do COMPREPLY+=("$candidate"); done < <(command workroot complete repos "$cur" 2>/dev/null)
    fi
  elif [[ ( "$cmd" == "worktree" || "$cmd" == "workdir" ) && ( "${COMP_WORDS[2]}" == "cd" || "${COMP_WORDS[2]}" == "path" || "${COMP_WORDS[2]}" == "prune" || "${COMP_WORDS[2]}" == "run" ) ]]; then
    if [[ $COMP_CWORD -eq 3 ]]; then
      while IFS= read -r candidate; do COMPREPLY+=("$candidate"); done < <(command workroot complete repos "$cur" 2>/dev/null)
    elif [[ $COMP_CWORD -eq 4 ]]; then
      prev="${COMP_WORDS[3]}"
      while IFS= read -r candidate; do COMPREPLY+=("$candidate"); done < <(command workroot complete targets "$prev" "$cur" 2>/dev/null)
    fi
  elif [[ ( "$cmd" == "worktree" || "$cmd" == "workdir" ) && "${COMP_WORDS[2]}" == "new" ]]; then
    if [[ $COMP_CWORD -eq 3 ]]; then
      while IFS= read -r candidate; do COMPREPLY+=("$candidate"); done < <(command workroot complete repos "$cur" 2>/dev/null)
    fi
  fi
}
complete -F _workroot_complete workroot
complete -F _workroot_complete wr
"#;

const FISH_INIT: &str = r#"# Workroot shell integration for fish
function workroot
  if test "$argv[1]" = "cd"
    set -l dest (command workroot worktree path $argv[2..-1])
    or return
    cd "$dest"
  else if test "$argv[1]" = "new"
    set -l dest (command workroot worktree new $argv[2..-1])
    or return
    printf '%s\n' "$dest"
    cd "$dest"
  else if contains -- "$argv[1]" worktree workdir; and test "$argv[2]" = "cd"
    set -l dest (command workroot worktree path $argv[3..-1])
    or return
    cd "$dest"
  else if contains -- "$argv[1]" worktree workdir; and test "$argv[2]" = "new"
    set -l dest (command workroot worktree new $argv[3..-1])
    or return
    printf '%s\n' "$dest"
    cd "$dest"
  else
    command workroot $argv
  end
end

function wr
  workroot $argv
end

function __workroot_complete_repos
  command workroot complete repos (commandline -ct) 2>/dev/null
end

function __workroot_complete_targets
  set -l tokens (commandline -opc)
  if contains -- "$tokens[2]" worktree workdir
    command workroot complete targets $tokens[4] (commandline -ct) 2>/dev/null
  else
    command workroot complete targets $tokens[3] (commandline -ct) 2>/dev/null
  end
end
complete -c workroot -f -n '__fish_seen_subcommand_from cd path status prune run; and test (count (commandline -opc)) -le 2' -a '(__workroot_complete_repos)'
complete -c workroot -f -n '__fish_seen_subcommand_from cd path status prune run; and test (count (commandline -opc)) -eq 3' -a '(__workroot_complete_targets)'
complete -c workroot -f -n '__fish_seen_subcommand_from new; and test (count (commandline -opc)) -le 2' -a '(__workroot_complete_repos)'
complete -c workroot -f -n 'set -l tokens (commandline -opc); contains -- "$tokens[2]" worktree workdir; and contains -- "$tokens[3]" cd path prune run; and test (count $tokens) -le 3' -a '(__workroot_complete_repos)'
complete -c workroot -f -n 'set -l tokens (commandline -opc); contains -- "$tokens[2]" worktree workdir; and contains -- "$tokens[3]" cd path prune run; and test (count $tokens) -eq 4' -a '(__workroot_complete_targets)'
complete -c workroot -f -n 'set -l tokens (commandline -opc); contains -- "$tokens[2]" worktree workdir; and test "$tokens[3]" = new; and test (count $tokens) -le 3' -a '(__workroot_complete_repos)'
complete -c wr -w workroot
"#;
