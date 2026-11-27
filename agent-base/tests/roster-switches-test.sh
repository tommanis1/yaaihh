#!/usr/bin/env bash
# roster-switches-test.sh: the owner's switches for a harness (bin/harness-settings; walk items 17 and 30) reach a
# Claude Code agent's start: harness.toml at the flavor's root (every project), beside the project file (the
# project), and in the agent's own folder (that agent), the later winning key by key. tools_off goes as
# --disallowedTools, parts_off as CLAUDE_CODE_REMOVE_PROMPT_STRINGS (a JSON list of the texts), safety_check off as
# CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS; the start's record line says them; a file that does not read
# stops the start. The safety check is off by default only in a box. Stand-ins for coordc and coord-pty record what the
# session would have been started with, its arguments and those two variables; no claude is run. What each does on
# the wire is harness-wire-test.sh's.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents/zed" "$t/home" "$t/w" "$t/flavor/projects/p"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"
cp "$here/claude/settings.json" "$here/claude/standing.md" "$t/ab/claude/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
# up NAME ARGS: the session's program comes up (roster's came_up): coordd has NAME running, and its SessionStart writes
# session.json again, naming the session it resumed
up() { n=$1; shift; s=$COORD/agents/$n/session.json; r=$(printf '%s\n' "$@" | sed -n '/^--resume$/{n;p}'); echo idle >"$COORD/.state"; [ -z "$r" ] || [ ! -f "$s" ] || { jq --arg t "$r" '.session_id = $t | .source = "resume"' "$s" >"$s.up" && mv "$s.up" "$s"; }; }
echo "$1" >>"$CALLS"
case $1 in
ls) printf 'zed\tAGT-001\t%s\towner\n' "$(cat "$COORD/.state" 2>/dev/null || echo stopped)" ;;
hangup) echo stopped >"$COORD/.state" ;;
add | start | account-for) ;;
run) n=$2; shift 3; { printf 'arg=%s\n' "$@"; env | grep -E '^CLAUDE_CODE_(REMOVE_PROMPT_STRINGS|EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS)=' | sed 's/^/env=/'; } >"$RAN"; up "$n" "$@" ;;
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
echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w\",\"model\":\"claude-opus-5-5\",\"kind\":\"claude\"}" >"$t/coord/agents/zed/session.json"
export AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran HOME=$t/home
unset ROSTER_BOX COORD_IN_BOX
# the session it restarts has a transcript where Claude Code keeps it (roster's carried: a restart with none is refused)
mkdir -p "$HOME/.claude/projects/$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')"; echo '{}' >"$HOME/.claude/projects/$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')/s1.jsonl"
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
is() { if eval "$2"; then ok "$1"; else bad "$1 ($2; ran: $(tr '\n' ' ' <"$RAN" 2>/dev/null))"; fi; }
start() { rm -f "$RAN"; : >"$CALLS"; out=$("$t/ab/bin/roster" restart zed --force 2>&1) && rc=0 || rc=$?; }
after() { grep -A1 -x -- "arg=$1" "$RAN" | tail -1 | sed 's/^arg=//'; }
pronouns="When you use a pronoun for someone, use they/them."

start
is "no settings anywhere: no tool off, no part off, the check on" '[ $rc = 0 ] && ! grep -qx -- "arg=--disallowedTools" "$RAN" && ! grep -q "^env=" "$RAN"'
is "its record line says the switches" 'grep -qx "arg=switches={\"safety_check\":true,\"tools_off\":\[\],\"parts_off\":0}" "$RAN"'

cat >"$t/flavor/harness.toml" <<X
[claude]
tools_off = ["WebSearch"]
parts_off = ["$pronouns"]
[codex]
tools_off = ["web_search"]
X
start
is "the flavor's root, for every project: the tool off as --disallowedTools" '[ "$(after --disallowedTools)" = WebSearch ]'
is "and the part off as CLAUDE_CODE_REMOVE_PROMPT_STRINGS, its text whole in a JSON list" '[ "$(sed -n "s/^env=CLAUDE_CODE_REMOVE_PROMPT_STRINGS=//p" "$RAN")" = "[\"$pronouns\"]" ]'
is "the check still on outside a box" '! grep -q "^env=CLAUDE_CODE_EVEN_MORE" "$RAN"'

printf '[claude]\ntools_off = ["WebSearch", "ScheduleWakeup"]\n' >"$t/flavor/projects/p/harness.toml"
start
is "the project's file wins over the flavor's, key by key: its tools" '[ "$(after --disallowedTools)" = WebSearch,ScheduleWakeup ]'
is "the flavor's part still off" 'grep -q "^env=CLAUDE_CODE_REMOVE_PROMPT_STRINGS=" "$RAN"'

printf '[claude]\nsafety_check = false\nparts_off = []\n' >"$t/coord/agents/zed/harness.toml"
start
is "the agent's own file: the check off, as CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS=1" 'grep -qx "env=CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS=1" "$RAN"'
is "and its empty parts_off turns the flavor's part back on" '! grep -q "^env=CLAUDE_CODE_REMOVE_PROMPT_STRINGS=" "$RAN"'
is "the project's tools stand" '[ "$(after --disallowedTools)" = WebSearch,ScheduleWakeup ]'
rm "$t/coord/agents/zed/harness.toml"

printf '[claude]\ntools_off = "WebSearch"\n' >"$t/flavor/projects/p/harness.toml"
start
is "a file that does not read as the keys want: the start refused, nothing run, and why" '[ $rc != 0 ] && [ ! -f "$RAN" ] && [[ $out == *"tools_off must be a list of texts"* ]]'
is "and the running session was not hung up first" '! grep -qx hangup "$CALLS" && [[ $out == *"not restarted, it runs as it was"* ]]'
printf '[claude]\ntool_off = ["x"]\n' >"$t/flavor/projects/p/harness.toml"
start
is "a key it does not know: refused, the keys named" '[ $rc != 0 ] && [[ $out == *"has no key tool_off"* ]]'
printf '[claude\n' >"$t/flavor/projects/p/harness.toml"
start
is "a file that is not TOML: refused" '[ $rc != 0 ] && [ ! -f "$RAN" ]'
rm "$t/flavor/projects/p/harness.toml"

# in a box the check is off unless a file turns it on (bin/harness-settings; the boxed start itself is coordd's)
boxed=$(ROSTER_BOX=1 "$t/ab/bin/harness-settings" claude zed)
is "in a box, nothing set: the check off" '[ "$(jq -r .safety_check <<<"$boxed")" = false ] && [ "$(jq -r .boxed <<<"$boxed")" = true ]'
printf '[claude]\nsafety_check = true\n' >"$t/coord/agents/zed/harness.toml"
boxed=$(ROSTER_BOX=1 "$t/ab/bin/harness-settings" claude zed)
is "in a box, the agent's file turns it on again, and says from where" '[ "$(jq -r .safety_check <<<"$boxed")" = true ] && [ "$(jq -r .from.safety_check <<<"$boxed")" = "$t/coord/agents/zed/harness.toml" ]'
codex=$("$t/ab/bin/harness-settings" codex zed)
is "another harness reads its own table only" '[ "$(jq -c .tools_off <<<"$codex")" = "[\"web_search\"]" ] && [ "$(jq -c .parts_off <<<"$codex")" = "[]" ]'

echo "roster-switches-test: $fails failed"
exit $((fails > 0))
