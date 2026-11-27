#!/usr/bin/env bash
# roster-recipe-test.sh: a Claude Code agent's start gives it the system prompt bin/prompt-recipe composed (owner's
# Stream 4A, N-1979): on a version with a pinned base, --system-prompt-file with the base less its cuts plus the
# model's packs, then those his harness.toml adds by role, and the standing text; its memory folder that of its account's folder and its own; its parts in
# system-prompt.md.diff.json; the record line's `recipe` says how. On a version with no base, the standing text and pack
# appended, and the record says why. Stand-ins for coordc and coord-pty record what the session would have been
# started with; no claude is run. What Claude Code sends of it is prompt-recipe-wire-test.sh's.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents/zed" "$t/home" "$t/w" "$t/flavor/projects/p" "$t/versions" "$t/fakebin"
cp "$here/bin/roster" "$here/bin/harness-settings" "$here/bin/prompt-recipe" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"
cp -r "$here/prompts" "$t/ab/"
cp "$here/claude/settings.json" "$here/claude/standing.md" "$t/ab/claude/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
# up NAME ARGS: the session's program comes up (roster's came_up): coordd has NAME running, and its SessionStart writes
# session.json again, naming the session it resumed
up() { n=$1; shift; s=$COORD/agents/$n/session.json; r=$(printf '%s\n' "$@" | sed -n '/^--resume$/{n;p}'); echo idle >"$COORD/.state"; [ -z "$r" ] || [ ! -f "$s" ] || { jq --arg t "$r" '.session_id = $t | .source = "resume"' "$s" >"$s.up" && mv "$s.up" "$s"; }; }
case $1 in
ls) printf 'zed\tAGT-001\t%s\towner\n' "$(cat "$COORD/.state" 2>/dev/null || echo stopped)" ;;
hangup) echo stopped >"$COORD/.state" ;;
add | start | account-for) ;;
run) n=$2; shift 3; printf 'arg=%s\n' "$@" >"$RAN"; up "$n" "$@" ;;
*) exit 3 ;;
esac
X
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
while [ "$1" != -- ]; do shift; done; shift
exec "$@"
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty"
echo "# the project" >"$t/flavor/projects/p/project.toml"
echo "$t/flavor/projects/p/project.toml" >"$t/coord/project"
echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w\",\"model\":\"claude-fable-5-2\",\"kind\":\"claude\"}" >"$t/coord/agents/zed/session.json"
export AGENT_BASE=$t/ab COORD=$t/coord RAN=$t/ran HOME=$t/home PATH=$t/fakebin:$PATH
unset ROSTER_BOX COORD_IN_BOX CLAUDE_CONFIG_DIR
# the session it restarts has a transcript where Claude Code keeps it (roster's carried: a restart with none is refused)
mkdir -p "$HOME/.claude/projects/$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')"; echo '{}' >"$HOME/.claude/projects/$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')/s1.jsonl"
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
is() { if eval "$2"; then ok "$1"; else bad "$1 ($2; out: $out)"; fi; }
# claude VERSION: the claude on PATH is an installed VERSION, as Claude Code's own installer lays it out
claude() { printf '#!/bin/sh\necho "%s (Claude Code)"\n' "$1" >"$t/versions/$1"; chmod +x "$t/versions/$1"; ln -sfn "$t/versions/$1" "$t/fakebin/claude"; }
start() { rm -f "$RAN"; out=$("$t/ab/bin/roster" restart zed --force 2>&1) && rc=0 || rc=$?; }
after() { grep -A1 -x -- "arg=$1" "$RAN" | tail -1 | sed 's/^arg=//'; }
sp=$t/coord/agents/zed/system-prompt.md

claude 2.1.332
start
is "a version with a base: the composed text as --system-prompt-file" '[ $rc = 0 ] && [ "$(after --system-prompt-file)" = "$sp" ] && ! grep -qx -- "arg=--append-system-prompt-file" "$RAN"'
is "less its cuts" '! grep -q "When you use a pronoun\|Do build original games\|# Session-specific guidance" "$sp"'
is "with the model's packs and the standing text last" 'grep -q "^# Finishing work" "$sp" && grep -q "^## Simplicity first" "$sp" && [ "$(tail -c 200 "$sp")" = "$(tail -c 200 "$t/ab/claude/standing.md")" ]'
is "its memory folder: its own, under the default config folder" 'grep -qF "memory at \`$t/home/.claude/projects/$(echo "$t/w" | sed "s/[^A-Za-z0-9]/-/g")/memory/\`" "$sp"'
is "its parts beside it, for the window" '[ "$(jq -r .how "$sp.diff.json")" = replace ] && [ "$(jq -c .packs "$sp.diff.json")" = "[\"claude\",\"fable\"]" ]'
is "the record line says how" 'grep -qx "arg=recipe=replace" "$RAN"'
is "and its sha is the text given, the same as the diff's" 'grep -qx "arg=system_sha=$(jq -r .system_sha "$sp.diff.json")" "$RAN" && grep -qx "arg=system_sha=$(printf "%s" "$(cat "$sp")" | sha256sum | cut -c1-64)" "$RAN"'

printf '[claude]\npacks = ["../base/claude-code-2.1.332"]\n' >"$t/coord/agents/zed/harness.toml"
start
is "a pack named as a path: the start refused by harness-settings, nothing run" '[ $rc != 0 ] && [ ! -f "$RAN" ] && [[ $out == *"packs must be plain names"* ]]'

mkdir -p "$t/coord/prompts/zed.system"; printf 'His own words.' >"$t/coord/prompts/zed.system/1.md"; echo "1 replace" >"$t/coord/prompts/zed.system/current"
rm "$t/coord/agents/zed/harness.toml"; start
is "a version of his: his text, and the earlier diff removed so none stands beside it" '[ $rc = 0 ] && [ "$(cat "$sp")" = "His own words." ] && [ ! -e "$sp.diff.json" ]'
rm -r "$t/coord/prompts/zed.system"

printf '[claude]\npacks = ["fable-owner"]\n' >"$t/coord/agents/zed/harness.toml"
start
is "a pack his settings add by role: after the model's" '[ $rc = 0 ] && [ "$(jq -c .packs "$sp.diff.json")" = "[\"claude\",\"fable\",\"fable-owner\"]" ] && grep -q "^# Ownership" "$sp"'
rm "$t/coord/agents/zed/harness.toml"

echo '{"autoMemoryEnabled": false}' >"$t/flavor/projects/p/claude-settings.json"
start
is "memory off in the project's settings: the memory part cut too" '[ $rc = 0 ] && ! grep -q "^# Memory" "$sp" && grep -q "#8 Memory" "$sp.diff.json"'
rm "$t/flavor/projects/p/claude-settings.json"

claude 9.9.9
start
is "a version with no base: the standing text and the pack appended" '[ $rc = 0 ] && [ "$(after --append-system-prompt-file)" = "$sp" ] && grep -q "^# Finishing work" "$sp"'
is "and the record says why" 'grep -qx "arg=recipe=append;no base text for claude-code 9.9.9: its own text is kept and the rest appended" "$RAN"'

rm "$t/ab/prompts/packs/packs.toml"
start
is "a composer that fails: the standing text appended alone, and the record says it failed" '[ $rc = 0 ] && [ "$(after --append-system-prompt-file)" = "$sp" ] && ! grep -q "^# Finishing work" "$sp" && grep -q "^arg=recipe=failed: prompt-recipe:" "$RAN"'
is "and no diff of an earlier start stays beside it" '[ ! -e "$sp.diff.json" ]'

[ $fails = 0 ] && echo "roster-recipe-test: all ok" || { echo "roster-recipe-test: $fails failed"; exit 1; }
