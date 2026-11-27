#!/usr/bin/env bash
# release-web-test.sh [DIR]: each release carries ops/web as committed (N-1994): `build --from` packs its tracked files
# into NAME/ops/web, byte for byte, and nothing untracked beside them (a .venv, a key file); a tree whose ops/web holds a
# link in git cuts no release and leaves no NAME and no NAME.part; install-mcp.sh run from releases/current links a
# coord's mcp/web through `current`, so a move of current moves it. A copy of this checkout's release.sh runs in
# DIR/tree, a git repository of its own; the programs are made up, HOME is DIR's, nothing of anyone's is reached.
# Without DIR, a mktemp folder, removed after.
set -uo pipefail
HERE=$(cd "$(dirname "$0")/../.." && pwd)
if [ -n "${1:-}" ]; then D=$1; rm -rf "$D"; mkdir -p "$D"; else D=$(mktemp -d); trap 'rm -rf "$D"' EXIT; fi
T=$D/tree
mkdir -p "$T/agent-base/bin" "$T/releases" "$T/ops/release" "$D/from" "$D/home/agent-coord/w/mcp" "$D/bin"
cp "$HERE/ops/release/release.sh" "$HERE/ops/release/put.sh" "$HERE/ops/release/bundle-v2.sh" "$T/ops/release/"
git -C "$HERE" archive HEAD ops/web | tar -x -C "$T"
echo "#!/bin/sh" >"$T/agent-base/bin/host-coordd"
printf '#!/bin/sh\nexit 0\n' >"$D/bin/logger" && chmod +x "$D/bin/logger"
export HOME=$D/home PATH=$D/bin:$PATH GIT_CONFIG_NOSYSTEM=1 RELEASE_ACTS=0
unset YAAIHH_WIN_DIR
g() { git -C "$T" -c core.hooksPath=/dev/null -c user.name=test -c user.email=test@example.invalid "$@"; }
g init -q -b main && g add agent-base ops && g commit -qm A
# beside the tracked files, what a working copy holds and no release may carry
mkdir -p "$T/ops/web/.venv/bin" && echo "made-up python" >"$T/ops/web/.venv/bin/python"
(umask 077 && echo "made-up key" >"$T/ops/web/exa.key")
for p in coordd coordc coord-pty docket-ls; do echo "made-up $p" >"$D/from/$p"; done

fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
run() { out=$("$T/ops/release/release.sh" "$@" 2>&1); rc=$?; }
# build --from NAME: the made-up exe carries NAME and a speech engine, as one built for it does
cut() { echo "made-up exe ggml $1" >"$D/from/yaaihh.exe"; run build --from "$D/from" "$1"; }

N1=20261127-110000-$(g rev-parse --short HEAD)
cut "$N1"
[ "$rc" = 0 ] && [ -d "$T/releases/$N1/ops/web" ] && ok "build --from: $N1/ops/web there" || no "build --from: rc $rc: $out"
mkdir -p "$D/want" && g archive HEAD ops/web | tar -x -C "$D/want"
diff -r "$D/want/ops/web" "$T/releases/$N1/ops/web" >/dev/null && ok "$N1/ops/web is the committed ops/web, byte for byte" ||
  no "$N1/ops/web differs from the commit: $(diff -rq "$D/want/ops/web" "$T/releases/$N1/ops/web")"
[ ! -e "$T/releases/$N1/ops/web/.venv" ] && [ ! -e "$T/releases/$N1/ops/web/exa.key" ] && ok "no .venv and no key file packed" ||
  no "an untracked file packed: $(ls -a "$T/releases/$N1/ops/web")"
[ -z "$(find "$T/releases/$N1/ops/web" -type l)" ] && ok "no link in the packed ops/web" || no "a link packed: $(find "$T/releases/$N1/ops/web" -type l)"
[ -x "$T/releases/$N1/ops/web/yaaihh-web" ] && [ -x "$T/releases/$N1/ops/web/install-mcp.sh" ] && ok "yaaihh-web and install-mcp.sh executable" ||
  no "modes lost: $(ls -l "$T/releases/$N1/ops/web")"

# install-mcp.sh from releases/current: the coord's mcp/web goes through current, and follows its move
ln -sfn "$N1" "$T/releases/current"
"$T/releases/current/ops/web/install-mcp.sh" "$D/home/agent-coord/w" >/dev/null
[ "$(readlink "$D/home/agent-coord/w/mcp/web")" = "$T/releases/current/ops/web/yaaihh-web" ] && ok "mcp/web links through releases/current" ||
  no "mcp/web links to $(readlink "$D/home/agent-coord/w/mcp/web")"

# a link committed in ops/web: nothing cut, no NAME, no NAME.part, the releases there as they were
ln -s /etc/passwd "$T/ops/web/leak" && g add ops/web/leak && g commit -qm B
N2=20261127-120000-$(g rev-parse --short HEAD)
before=$(ls -A "$T/releases")
cut "$N2"
[ "$rc" != 0 ] && [[ $out == *"ops/web holds a link in git"* ]] && ok "a link in ops/web refused: rc $rc" || no "a link in ops/web: rc $rc: $out"
[ ! -e "$T/releases/$N2" ] && [ ! -e "$T/releases/$N2.part" ] && [ "$(ls -A "$T/releases")" = "$before" ] &&
  ok "nothing left of the refused cut, releases/ as it was" || no "the refused cut left: $(ls -A "$T/releases")"

[ "$fails" = 0 ] && echo "release-web-test: all passed" || echo "release-web-test: $fails failed"
[ "$fails" = 0 ]
