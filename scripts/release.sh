#!/usr/bin/env bash
#
# Cut a release.
#
#   ./scripts/release.sh 0.2.0
#
# Bumps the version in the three files that carry it, commits, tags and pushes.
# GitHub Actions does the rest: it builds the DMG and publishes it, and the
# in-app update check picks it up within a day.
set -euo pipefail

VERSION="${1:-}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: $0 <version>   e.g. $0 0.2.0" >&2
  exit 1
fi

cd "$(dirname "$0")/.."

if [[ -n "$(git status --porcelain)" ]]; then
  echo "You have uncommitted changes. Commit or stash them first." >&2
  git status --short >&2
  exit 1
fi

# Three files carry the version and they must agree: Cargo.toml is what the
# update check compares against, tauri.conf.json names the DMG, package.json
# keeps npm honest.
python3 - "$VERSION" <<'PY'
import json, pathlib, re, sys
version = sys.argv[1]

cargo = pathlib.Path("src-tauri/Cargo.toml")
cargo.write_text(re.sub(r'^version = "[^"]+"', f'version = "{version}"',
                        cargo.read_text(), count=1, flags=re.M))

for path in ("package.json", "src-tauri/tauri.conf.json"):
    p = pathlib.Path(path)
    d = json.loads(p.read_text())
    d["version"] = version
    p.write_text(json.dumps(d, indent=2) + "\n")

print(f"version set to {version}")
PY

echo "Checking it still builds and passes…"
npm run build >/dev/null
( cd src-tauri && cargo test --lib >/dev/null )

git add -A
git commit -m "Release v${VERSION}"
git tag "v${VERSION}"
git push origin HEAD --tags

echo
echo "Pushed v${VERSION}. GitHub Actions is building the DMG now:"
echo "  https://github.com/ArmanKundu/retain/actions"
echo
echo "When it finishes, the release appears here and the in-app check will find it:"
echo "  https://github.com/ArmanKundu/retain/releases"
