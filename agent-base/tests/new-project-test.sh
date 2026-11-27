#!/usr/bin/env bash
# new-project-test.sh DIR: agent-base/bin/new-project on a stand-in base in DIR (its box.sh and host-coordd only say
# what they were given) with DIR/home as HOME, so nothing real is made or started: the coord folder and its files,
# the start it hands over to, ~ in the paths, and each refusal with its exit code (an existing coord folder left as
# it was).
set -uo pipefail
D=${1:?new-project-test.sh DIR}
NP=$(cd "$(dirname "$0")/.." && pwd)/bin/new-project
mkdir -p "$D"
D=$(cd "$D" && pwd)
rm -rf "$D/base" "$D/home"
mkdir -p "$D/base/bin" "$D/base/box" "$D/home/code/repo"
cp "$NP" "$D/base/bin/new-project"
for t in bin/host-coordd box/box.sh; do
  printf '#!/usr/bin/env bash\necho "%s $*"\n[ -z "${STUB_FAIL:-}" ] || { echo "it broke" >&2; exit 3; }\n' "$(basename $t)" >"$D/base/$t"
  chmod +x "$D/base/$t"
done
: >"$D/home/p.toml"
export HOME=$D/home
# the machine's model route (made up: no real key command), which every project made gets a copy of
mkdir -p "$HOME/agent-coord" && printf 'ANTHROPIC_BASE_URL=http://127.0.0.1:9\nKEY_COMMAND=echo made-up\n' >"$HOME/agent-coord/route"
run() { out=$("$D/base/bin/new-project" "$@" 2>&1); rc=$?; }
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }

run host my-proj "~/code/repo" "~/p.toml"
[ $rc = 0 ] && [ "$out" = "host-coordd start my-proj" ] && ok "host: host-coordd start ($out)" || no "host: rc $rc, $out"
[ "$(cat "$HOME/agent-coord/my-proj/repo" 2>/dev/null)" = "$HOME/code/repo" ] && ok "host: repo, ~ expanded" || no "host: repo $(cat "$HOME/agent-coord/my-proj/repo" 2>&1)"
[ "$(cat "$HOME/agent-coord/my-proj/project" 2>/dev/null)" = "$HOME/p.toml" ] && ok "host: project" || no "host: project $(cat "$HOME/agent-coord/my-proj/project" 2>&1)"
[ ! -e "$HOME/agent-coord/my-proj/definition" ] && [ ! -s "$HOME/p.toml" ] && ok "host, his project file: used as it is, nothing beside it" || no "host, his file: $(ls -A "$HOME/agent-coord/my-proj") $(cat "$HOME/p.toml")"

# its agents' own Claude Code folder (roster own_home, paper-research N-1446 p3051): made empty and the owner's alone;
# nothing under the owner's ~/.claude
c=$HOME/agent-coord/my-proj/claude-home
[ -d "$c" ] && [ "$(stat -c %a "$c")" = 700 ] && [ -z "$(ls -A "$c")" ] && [ ! -e "$HOME/.claude" ] && [ ! -e "$HOME/.claude.json" ] && ok "host: claude-home, empty, 0700; nothing of the owner's" || no "host: claude-home $(stat -c %a "$c" 2>&1) $(ls -A "$c" 2>&1)"

# its route (Stream 6 item 3a): the machine's, so coordd's own calls reach the gateway; a host's a link to the one file,
# a box's a copy (its mount holds the coord folder alone), the owner's alone
r=$HOME/agent-coord/my-proj/route
[ "$(readlink "$r")" = "$HOME/agent-coord/route" ] && ok "host: route, a link to the machine's" || no "host: route $(ls -l "$r" 2>&1)"
run box boxed "~/code/repo"
r=$HOME/agent-coord/boxed/route
cmp -s "$r" "$HOME/agent-coord/route" && [ ! -L "$r" ] && [ "$(stat -c %a "$r")" = 600 ] && ok "box: route, a copy of the machine's, 0600" || no "box: route $(ls -l "$r" 2>&1); rc $rc, $out"
mv "$HOME/agent-coord/route" "$D/route.aside"
run host unrouted ""
[ $rc = 0 ] && [ ! -e "$HOME/agent-coord/unrouted/route" ] && grep -q "no $HOME/agent-coord/route: unrouted reaches no gateway" <<<"$out" && [ "$(tail -1 <<<"$out")" = "host-coordd start unrouted" ] \
  && ok "host, no machine route: none, it says the helpers have no model, and starts" || no "unrouted: rc $rc, $out"
mv "$D/route.aside" "$HOME/agent-coord/route"
run host bare ""
[ $rc = 0 ] && [ "$out" = "host-coordd start bare" ] && ok "host, no folder: host-coordd start ($out)" || no "host, no folder: rc $rc, $out"
[ ! -e "$HOME/agent-coord/bare/repo" ] && [ ! -e "$HOME/agent-coord/bare/work" ] && [ "$(cat "$HOME/agent-coord/bare/project" 2>/dev/null)" = "$HOME/agent-coord/bare/definition/project.toml" ] && [ "$(cat "$HOME/agent-coord/bare/definition/project.toml" 2>/dev/null)" = $'[project]\nhost_relay = true\nnorms = "clingo"' ] && ok "host, no folder: no repo and no work folder; its own project file, host_relay = true, norms = clingo" || no "host, no folder: $(ls -A "$HOME/agent-coord/bare" 2>&1) repo $(cat "$HOME/agent-coord/bare/repo" 2>&1)"
run host bare2 "" "~/p.toml"
[ $rc = 0 ] && [ "$(cat "$HOME/agent-coord/bare2/project" 2>/dev/null)" = "$HOME/p.toml" ] && [ ! -e "$HOME/agent-coord/bare2/work" ] && ok "host, no folder, a project file" || no "host, no folder, file: rc $rc, $out"
run box b0 ""
[ $rc = 1 ] && [[ $out == "new-project: a boxed project clones a repository: name it" ]] && [ ! -e "$HOME/agent-coord/b0" ] && ok "box, no repository: refused, nothing made" || no "box, no repository: rc $rc, $out"

run box b1 "$HOME/code/repo"
[ $rc = 0 ] && [ "$out" = "box.sh up b1 $HOME/code/repo" ] && ok "box: box.sh up ($out)" || no "box: rc $rc, $out"
[ -d "$HOME/agent-coord/b1" ] && [ ! -e "$HOME/agent-coord/b1/repo" ] && [ ! -e "$HOME/agent-coord/b1/project" ] && [ ! -e "$HOME/agent-coord/b1/claude-home" ] && ok "box: coord folder, no repo, no project, no claude-home (its box's home is its own)" || no "box: $(ls -A "$HOME/agent-coord/b1" 2>&1)"

STUB_FAIL=1 run host h2 "$HOME/code/repo"
[ $rc = 3 ] && [[ $out == *"it broke"* ]] && ok "a failed start: its code (3) and its words" || no "failed start: rc $rc, $out"

mkdir -p "$HOME/agent-coord/old/x"; echo kept >"$HOME/agent-coord/old/repo"
run host old "$HOME/code/repo"
[ $rc = 1 ] && [[ $out == "new-project: there is a coord folder $HOME/agent-coord/old already" ]] && [ "$(cat "$HOME/agent-coord/old/repo")" = kept ] && ok "a coord folder there already: refused, untouched" || no "existing: rc $rc, $out"
run host my-proj "$HOME/code/repo"
[ $rc = 1 ] && [[ $out == *"there is a coord folder"* ]] && ok "the same id twice: the second refused" || no "twice: rc $rc, $out"

run host h3 "~/nothing"
[ $rc = 1 ] && [[ $out == "new-project: no folder $HOME/nothing" ]] && [ ! -e "$HOME/agent-coord/h3" ] && ok "no folder: refused, nothing made" || no "no folder: rc $rc, $out"
run host h4 "$HOME/code/repo" /no/file
[ $rc = 1 ] && [[ $out == "new-project: no file /no/file" ]] && [ ! -e "$HOME/agent-coord/h4" ] && ok "no file: refused, nothing made" || no "no file: rc $rc, $out"
for bad in My-Proj a_b -a "a--b" "../x" ""; do
  run host "$bad" "$HOME/code/repo"
  [ $rc = 1 ] && [[ $out == *"not a project id"* ]] || no "id '$bad': rc $rc, $out"
done
ok "ids refused: My-Proj a_b -a a--b ../x (empty)"
run vm x "$HOME/code/repo"
[ $rc = 2 ] && ok "an unknown kind: usage, 2" || no "kind: rc $rc, $out"
run host x
[ $rc = 2 ] && ok "too few arguments: usage, 2" || no "args: rc $rc, $out"

[ $fails = 0 ] && echo "new-project-test: all ok" || echo "new-project-test: $fails failed"
exit $fails
