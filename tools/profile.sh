#!/usr/bin/env bash
# Run one entry of the built plugin (dist/gear-kit) with solidcraft-cli's profiler and show where
# the time goes.
#
#   tools/profile.sh ENTRY 'JSON params' [lines]
set -euo pipefail
cd "$(dirname "$0")/.."
cli="${SOLIDCRAFT_CLI:-/Applications/SolidCraft.app/Contents/MacOS/solidcraft-cli}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
script="$tmp/run.json"
python3 - "$1" "$2" "$script" <<'PY'
import json, os, sys
sys.path.insert(0, "tools")
import run as gk
pkg = os.path.abspath(gk.PKG)
step = {"command": "plugin.run", "params": {"path": pkg, "approval": gk.approval(pkg), "entry": sys.argv[1], "params": json.loads(sys.argv[2])}}
with open(sys.argv[3], "w") as f:
    json.dump([step], f)
PY
"$cli" run "$script" --quiet --profile 2>&1 | grep -E '^ +[0-9]|wall|error' | sort -k1 -rn | head -"${3:-10}"
