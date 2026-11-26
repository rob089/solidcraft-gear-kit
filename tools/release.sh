#!/usr/bin/env bash
# Publish the version in Cargo.toml as a GitHub release: tag v<version>, asset
# gear-kit-<version>.zip (made by build.sh). SolidCraft offers it as an update to installed copies
# (the manifest's `updates` field names this repository).
#
#     tools/release.sh            # from a clean, pushed master, after the tests
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$(dirname "$(rustup which rustc)"):$PATH"

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
repo=$(sed -n 's/^pub const GITHUB_REPO: &str = "\(.*\)";$/\1/p' src/schema.rs)
tag="v$version"

[ -z "$(git status --porcelain)" ] || { echo "commit your changes first" >&2; exit 1; }
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/master)" ] || { echo "push master first" >&2; exit 1; }
if gh release view "$tag" -R "$repo" >/dev/null 2>&1; then
    echo "$tag is already released: raise the version in Cargo.toml" >&2
    exit 1
fi

./build.sh
gh release create "$tag" "dist/gear-kit-$version.zip" -R "$repo" --target "$(git rev-parse HEAD)" \
    --title "Gear Kit $version" --generate-notes
