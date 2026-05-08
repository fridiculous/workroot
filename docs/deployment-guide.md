# Workroot Deployment Guide

## Distribution overview

Workroot distributes the `workroot` binary through three paths:

1. GitHub Release tarballs for supported Linux and macOS targets
2. source install from the GitHub repository as a fallback
3. crates.io install of the `workroot-cli` crate as a final fallback

The public installer is `install.sh`. It prefers release artifacts and falls back automatically when an artifact does not exist for the current platform.

## CI pipeline

Workflow: `.github/workflows/ci.yml`

Checks currently include:
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- shell-focused test subsets
- package listing and publish dry-run checks

## Release artifact pipeline

Workflow: `.github/workflows/release.yml`

Triggers:
- manual `workflow_dispatch` for preview binary builds or final publish
- tags matching `v*` for final publish from an existing tag

Current target matrix:
- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Expected artifact names:
- `workroot-x86_64-unknown-linux-gnu.tar.gz`
- `workroot-x86_64-apple-darwin.tar.gz`
- `workroot-aarch64-apple-darwin.tar.gz`

On manual dispatch with `publish: false`, artifacts are uploaded to the workflow run for inspection.
On manual dispatch with `publish: true`, the workflow creates the `vX.Y.Z` GitHub Release from the selected `main` commit and uploads artifacts.
On tag pushes, artifacts are attached to the GitHub Release for the existing tag.

The release workflow validates that any final tag matches the `Cargo.toml` version.
For example, `Cargo.toml` version `0.0.2` must be released from tag `v0.0.2`.

## Actions-assisted release flow

Use these GitHub Actions to keep the release process simple:

1. Run **Release Gate** from `main`.
   - Optional input: expected version without the leading `v`.
   - It runs format, Clippy, tests, package listing, and publish dry run.
   - The job summary prints the exact tag commands for the tested commit.
2. Run **Release Binaries** from `main`.
   - Use `publish: false` to build preview tarballs without publishing a GitHub Release.
   - Use `publish: true` to create the final `vX.Y.Z` release and upload artifacts.
3. Optional fallback: push the final `vX.Y.Z` tag from the tested commit.
   - The tag-triggered **Release Binaries** run builds tarballs and attaches them to the GitHub Release for that tag.
4. Run **Verify Release** with the final tag.
   - It checks the release exists, required assets exist, tarballs contain both `workroot-<target>` and `wr-<target>`, and prints SHA256 values for downstream packaging.
5. Update Homebrew only after **Verify Release** passes.

## Installer behavior

`install.sh`:

1. detects shell from `$SHELL` unless `--shell zsh|bash|fish` is provided
2. supports `--no-shell` for binary-only install
3. detects platform target from `uname -s` and `uname -m`
4. attempts latest GitHub Release download from:

   ```text
   https://github.com/fridiculous/workroot/releases/latest/download/workroot-$target.tar.gz
   ```

5. installs `workroot-$target` into `${CARGO_HOME:-$HOME/.cargo}/bin/workroot`
   and `wr-$target` into `${CARGO_HOME:-$HOME/.cargo}/bin/wr` when present
6. falls back to `cargo install --git https://github.com/fridiculous/workroot.git workroot-cli --force`
7. falls back to `cargo install workroot-cli --force`
8. updates shell rc files unless disabled

## Local install path

For dogfooding before a release:

```bash
./scripts/install-workroot-shell.sh zsh
./scripts/install-workroot-shell.sh bash
./scripts/install-workroot-shell.sh fish
```

This helper installs the current checkout through Cargo, updates shell configuration, and prints the immediate shell-setup command.

## Homebrew status

Homebrew support should only be advertised once release assets and formula updates are verified. See `docs/homebrew.md` for packaging notes.

## Release checklist

1. ensure `Cargo.toml` version and metadata are correct
2. run locally:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   cargo package --locked --allow-dirty --list
   cargo publish --dry-run --locked --allow-dirty
   ```

3. run the **Release Gate** workflow from `main`
4. run **Release Binaries** from `main` with `publish: true`
5. wait for **Release Binaries** to create the release and attach release assets
6. run **Verify Release** for the final tag
7. test `install.sh` on a supported platform
8. update Homebrew only after release assets are verified

## Operational risks

- release artifact names must stay aligned across workflow, installer, and Homebrew formula
- source fallbacks require Rust and Cargo to be available
- shell rc-file edits are intentionally simple and may still need manual review in heavily customized shells
- Windows targets are not currently included
