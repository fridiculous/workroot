#!/usr/bin/env bash
set -euo pipefail

crate="workroot-cli"
bin="workroot"
shorthand="wr"
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
repo="fridiculous/workroot"
shell_name="$(basename "${SHELL:-zsh}")"
install_shell="true"

usage() {
  cat <<'EOF'
Workroot installer

Usage:
  curl -fsSL https://raw.githubusercontent.com/fridiculous/workroot/main/install.sh | bash
  curl -fsSL https://raw.githubusercontent.com/fridiculous/workroot/main/install.sh | bash -s -- --shell zsh

Options:
  --shell zsh|bash|fish   Shell integration to install. Defaults to current shell.
  --no-shell              Install binary only.
  -h, --help              Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --shell)
      shell_name="${2:-}"
      shift 2
      ;;
    --shell=*)
      shell_name="${1#--shell=}"
      shift
      ;;
    --no-shell)
      install_shell="false"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$shell_name" in
  zsh|bash|fish) ;;
  *)
    echo "error: unsupported shell: $shell_name" >&2
    echo "supported shells: zsh, bash, fish" >&2
    exit 2
    ;;
esac

release_target=""
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) release_target="x86_64-unknown-linux-gnu" ;;
  Darwin-x86_64) release_target="x86_64-apple-darwin" ;;
  Darwin-arm64|Darwin-aarch64) release_target="aarch64-apple-darwin" ;;
esac

install_from_release() {
  [ -n "$release_target" ] || return 1
  command -v curl >/dev/null 2>&1 || return 1
  command -v tar >/dev/null 2>&1 || return 1

  url="https://github.com/$repo/releases/latest/download/workroot-$release_target.tar.gz"
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT

  curl -fsSL "$url" -o "$tmp_dir/workroot.tar.gz" || return 1
  tar -xzf "$tmp_dir/workroot.tar.gz" -C "$tmp_dir" || return 1
  [ -f "$tmp_dir/workroot-$release_target" ] || return 1

  mkdir -p "$cargo_bin"
  install -m 0755 "$tmp_dir/workroot-$release_target" "$cargo_bin/$bin" || return 1
  if [ -f "$tmp_dir/wr-$release_target" ]; then
    install -m 0755 "$tmp_dir/wr-$release_target" "$cargo_bin/$shorthand" || return 1
  fi
}

ensure_shorthand() {
  if [ -x "$cargo_bin/$shorthand" ]; then
    return 0
  fi
  if [ ! -x "$cargo_bin/$bin" ]; then
    return 0
  fi
  ln -sf "$bin" "$cargo_bin/$shorthand" 2>/dev/null || cp "$cargo_bin/$bin" "$cargo_bin/$shorthand"
}

find_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    cargo_cmd=(cargo)
  elif command -v rustup >/dev/null 2>&1; then
    toolchain_bin="$(rustup which rustc | xargs dirname)"
    cargo_cmd=("$toolchain_bin/cargo")
  else
    echo "error: no release binary matched this system and Rust is not available for source fallback." >&2
    echo "Install Rust first: https://rustup.rs" >&2
    exit 1
  fi
}

install_from_git() {
  find_cargo
  "${cargo_cmd[@]}" install --git "https://github.com/$repo.git" "$crate" --force
}

install_from_cargo() {
  find_cargo
  "${cargo_cmd[@]}" install "$crate" --force
}

if install_from_release; then
  install_source="GitHub Releases"
elif install_from_git; then
  install_source="GitHub source"
else
  install_from_cargo
  install_source="crates.io"
fi

ensure_shorthand

if [ "$install_shell" = "false" ]; then
  echo "Installed $bin and $shorthand from $install_source to: $cargo_bin"
  exit 0
fi

case "$shell_name" in
  zsh)
    rc_file="$HOME/.zshrc"
    # shellcheck disable=SC2016
    path_line='export PATH="$HOME/.cargo/bin:$PATH"'
    # shellcheck disable=SC2016
    init_line='eval "$(workroot shell-init zsh)"'
    reload_command="exec zsh"
    ;;
  bash)
    rc_file="$HOME/.bashrc"
    # shellcheck disable=SC2016
    path_line='export PATH="$HOME/.cargo/bin:$PATH"'
    # shellcheck disable=SC2016
    init_line='eval "$(workroot shell-init bash)"'
    reload_command="exec bash"
    ;;
  fish)
    rc_file="$HOME/.config/fish/config.fish"
    mkdir -p "$(dirname "$rc_file")"
    # shellcheck disable=SC2016
    path_line='fish_add_path "$HOME/.cargo/bin"'
    init_line='workroot shell-init fish | source'
    reload_command="exec fish"
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

echo "Installed $bin from $install_source to: $cargo_bin/$bin"
echo "Installed shorthand: $cargo_bin/$shorthand"
echo "Updated shell config: $rc_file"
echo "Reload with: $reload_command"
