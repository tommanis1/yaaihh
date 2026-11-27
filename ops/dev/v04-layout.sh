#!/bin/bash
# v04-layout.sh [REPO]: stages yaaihh's v0.4 layout (owner N-1831) by v04-layout.map; safe to run again after a merge.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "${1:-.}"
[ -z "$(git status --porcelain --untracked-files=no)" ] || { echo "v04-layout: $(pwd) has changes not committed" >&2; exit 1; }
map=$here/v04-layout.map py=$here/v04-paths.py
orig=$(git rev-parse HEAD)

# the moves, file by file, so files a later merge put in an old place follow the rest
git ls-files | python3 -I "$py" plan "$map" | while IFS=$'\t' read -r old new; do
  if [ "$new" = - ]; then git rm -q -- "$old"; else mkdir -p "$(dirname "$new")" && git mv -- "$old" "$new"; fi
done

# yaaihh/'s ignores now the root's
grep -qx '.cargo/config.toml' .gitignore || printf '%s\n' "# a developer's own overrides (a local rune checkout)" .cargo/config.toml >>.gitignore
grep -qx '/target' .gitignore || echo /target >>.gitignore
[ ! -f yaaihh/.cargo/config.toml ] || [ -f .cargo/config.toml ] || { mkdir -p .cargo && mv yaaihh/.cargo/config.toml .cargo/; }

python3 -I "$py" members
python3 -I "$py" fix "$map" "$orig"

# symbols' dependencies join the workspace's lock (no compile: cargo reads the registry already on this machine)
"$(command -v cargo || echo "$HOME/.cargo/bin/cargo")" metadata --offline --format-version 1 >/dev/null
git add -u
# folders the moves emptied (untracked files keep theirs)
for d in yaaihh symbols docs/design; do [ ! -d "$d" ] || find "$d" -depth -type d -empty -delete; done
git status --short | awk '{print $1}' | sort | uniq -c | sed 's/^/v04-layout: /'
