# Homebrew Packaging Notes

Homebrew should only be treated as live once release artifacts are published and the formula has been verified against those exact assets.

## Expected shape

- tap repo separate from the main repository
- formula downloads release tarballs, not source builds
- formula installs the `workroot` binary
- formula update happens immediately after release asset verification

## Expected release asset names

- `workroot-aarch64-apple-darwin.tar.gz`
- `workroot-x86_64-apple-darwin.tar.gz`
- `workroot-x86_64-unknown-linux-gnu.tar.gz`

## Formula outline

```ruby
class Workroot < Formula
  desc "Machine-wide switchboard for git worktrees"
  homepage "https://github.com/fridiculous/workroot"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/fridiculous/workroot/releases/download/v0.1.0/workroot-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    else
      url "https://github.com/fridiculous/workroot/releases/download/v0.1.0/workroot-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  def install
    bin.install Dir["workroot-*"][0] => "workroot"
  end
end
```

## Verification

After release assets exist:

```bash
brew untap fridiculous/tap || true
brew tap fridiculous/tap
brew reinstall fridiculous/tap/workroot
/opt/homebrew/bin/workroot --help
```

Do not update README install text until this path has been verified.
