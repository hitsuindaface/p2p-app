#!/usr/bin/env bash
# Start-of-day routine for Linux, WSL and macOS. Run with: make recap
# Windows has the same routine in make.ps1 (.\make recap).
#
# It never changes your work: it only fast-forwards, so it cannot create merge
# commits or conflicts. Anything that needs a decision is printed for you to do.

set -u
cd "$(dirname "$0")/.."

step() { printf '\n\033[1;36m== %s\033[0m\n' "$1"; }
note() { printf '\033[33m%s\033[0m\n' "$1"; }

step "Code"
branch=$(git branch --show-current)
echo "You are on: $branch"
git fetch --prune --quiet origin || note "Could not reach GitHub. Continuing with the code you have."

before=$(git rev-parse HEAD)
if git rev-parse --abbrev-ref '@{u}' >/dev/null 2>&1; then
  git pull --ff-only --quiet ||
    note "Could not update $branch: it has commits that GitHub does not have, and the reverse. Merge or rebase it yourself."
else
  note "$branch is not on GitHub yet. Push it with: git push -u origin $branch"
fi

if [ "$before" != "$(git rev-parse HEAD)" ]; then
  echo "New commits on $branch:"
  git log --oneline "$before..HEAD"
else
  echo "$branch is up to date."
fi

if [ "$branch" != "main" ] && git rev-parse -q --verify origin/main >/dev/null; then
  behind=$(git rev-list --count HEAD..origin/main)
  [ "$behind" -gt 0 ] && note "main has $behind commit(s) that $branch does not have. To bring them in: git merge origin/main"
fi

changes=$(git status --porcelain | wc -l | tr -d ' ')
[ "$changes" -gt 0 ] && note "You have $changes file(s) with uncommitted changes."

if ! git diff --quiet "$before" HEAD -- .dependencies; then
  note ".dependencies changed in the new commits. Run: make setup"
fi

step "Rust"
rustup update --no-self-update 2>&1 | grep -E "updated|unchanged|error" || true

step "Tools"
make --no-print-directory doctor >/dev/null 2>&1 && echo "Everything in .dependencies is installed." ||
  note "Something in .dependencies is missing. Run: make doctor, then make setup"

step "Build"
if cargo build --quiet; then echo "Build OK."; else note "The build failed. Read the errors above."; fi

step "Pull requests"
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  gh pr status
else
  echo "Install and log in to the GitHub CLI (gh) to see pull requests waiting for your review here."
fi
