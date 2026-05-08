#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
shell_name="$(basename "${SHELL:-zsh}")"

case "${1:-}" in
  zsh|bash|fish)
    shell_name="$1"
    ;;
  "")
    ;;
  *)
    echo "usage: $0 [zsh|bash|fish]" >&2
    exit 2
    ;;
esac

if command -v cargo >/dev/null 2>&1; then
  cargo_cmd=(cargo)
elif command -v rustup >/dev/null 2>&1; then
  toolchain_bin="$(rustup which rustc | xargs dirname)"
  cargo_cmd=("$toolchain_bin/cargo")
else
  echo "error: cargo/rustup not found; install Rust first: https://rustup.rs" >&2
  exit 1
fi

"${cargo_cmd[@]}" install --path "$repo_root" --force

case "$shell_name" in
  zsh)
    rc_file="$HOME/.zshrc"
    # shellcheck disable=SC2016
    path_line='export PATH="$HOME/.cargo/bin:$PATH"'
    # shellcheck disable=SC2016
    init_line='eval "$(workroot shell-init zsh)"'
    ;;
  bash)
    rc_file="$HOME/.bashrc"
    # shellcheck disable=SC2016
    path_line='export PATH="$HOME/.cargo/bin:$PATH"'
    # shellcheck disable=SC2016
    init_line='eval "$(workroot shell-init bash)"'
    ;;
  fish)
    rc_file="$HOME/.config/fish/config.fish"
    mkdir -p "$(dirname "$rc_file")"
    # shellcheck disable=SC2016
    path_line='fish_add_path "$HOME/.cargo/bin"'
    init_line='workroot shell-init fish | source'
    ;;
  *)
    echo "error: unsupported shell: $shell_name" >&2
    echo "usage: $0 [zsh|bash|fish]" >&2
    exit 2
    ;;
esac

touch "$rc_file"

if ! grep -Fq "$path_line" "$rc_file"; then
  {
    printf '\n# Workroot local install\n'
    printf '%s\n' "$path_line"
  } >> "$rc_file"
fi

if ! grep -Fq "$init_line" "$rc_file"; then
  {
    printf '\n# Workroot shell integration\n'
    printf '%s\n' "$init_line"
  } >> "$rc_file"
fi

export PATH="$cargo_bin:$PATH"

echo "Installed workroot to: $cargo_bin/workroot"
echo "Installed shorthand to: $cargo_bin/wr"
echo "Updated shell config: $rc_file"
echo "Reload with: exec $shell_name"
echo "Or for this shell now, run:"
# shellcheck disable=SC2016
case "$shell_name" in
  zsh) echo '  export PATH="$HOME/.cargo/bin:$PATH"; eval "$(workroot shell-init zsh)"' ;;
  bash) echo '  export PATH="$HOME/.cargo/bin:$PATH"; eval "$(workroot shell-init bash)"' ;;
  fish) echo '  fish_add_path "$HOME/.cargo/bin"; workroot shell-init fish | source' ;;
esac
