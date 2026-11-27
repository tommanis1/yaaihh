#!/usr/bin/env bash
# roster-restart-up-test.sh: a Claude Code restart that ends is not reported as done, and a restart after a move to
# another account resumes its own session (docket-core N-2012 p2). A stand-in coordc keeps the agent's state in a file;
# its `run` is the session's program and does what $CLAUDE says: `up` comes up on the session it resumes (coordd's
# SessionStart: session.json written again), `ends` ends at once, `startup` ends at once after a SessionStart that names
# a session of its own with no transcript. No claude is run.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents/orch" "$t/home" "$t/w" "$t/old" "$t/acct"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/settings.json" "$here/claude/standing.md" "$here/claude/telemetry-token" "$here/claude/account-key" "$t/ab/claude/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$1" >>"$CALLS"
s=$COORD/agents/orch/session.json
case $1 in
ls) printf 'orch\tAGT-001\t%s\towner\n' "$(cat "$ST" 2>/dev/null || echo stopped)" ;;
add) ;;
hangup) echo stopped >"$ST" ;;
account-for) printf '%s' "$ANSWER" ;;
session) printf '%s' "$3" | jq -s '.[0] * .[1]' "$s" - >"$s.new" && mv "$s.new" "$s" ;;
run) shift 3; printf 'arg=%s\n' "$@" >"$RAN"
  sid=$(sed -n '/^arg=--resume$/{n;s/^arg=//p}' "$RAN")
  case $CLAUDE in
  up) echo idle >"$ST"; jq --arg t "$sid" '.session_id = $t | .source = "resume"' "$s" >"$s.new" && mv "$s.new" "$s" ;;
  ends) echo stopped >"$ST" ;;
  startup) jq --arg p "$COORD/none.jsonl" '.session_id = "s9" | .source = "startup" | .transcript_path = $p' "$s" >"$s.new" && mv "$s.new" "$s"; echo stopped >"$ST" ;;
  esac ;;
*) exit 3 ;;
esac
X
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
while [ "$1" != -- ]; do shift; done; shift
exec "$@"
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty"
echo key >"$t/coord/.proxy-key"; echo "$t/none/project.toml" >"$t/coord/project"
export AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran ST=$t/state HOME=$t/home ROSTER_UP_WAIT=2
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
is() { if eval "$2"; then ok "$1"; else bad "$1 ($2)"; fi; }
slug=$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')
# s1's transcript where it ran last: the old account's folder, as session.json names it
mkdir -p "$t/old/projects/$slug"; printf '{"sessionId":"s1","n":1}\n' >"$t/old/projects/$slug/s1.jsonl"
session() { echo "{\"session_id\":\"$1\",\"cwd\":\"$t/w\",\"model\":\"claude-opus-5-5\",\"transcript_path\":\"$2\"}" >"$t/coord/agents/orch/session.json"; }
restart() { : >"$CALLS"; rm -f "$RAN"; echo idle >"$ST"; set +e; out=$("$t/ab/bin/roster" restart orch 2>"$t/err"); rc=$?; set -e; }

# 1. moved to another account whose folder has no s1: the transcript is carried there and s1 resumed, and it comes up
session s1 "$t/old/projects/$slug/s1.jsonl"
ANSWER=$t/acct CLAUDE=up restart
is "moved: the restart is done" '[ "$rc" = 0 ] && [ "$out" = "orch: restarted (session s1)" ]'
is "moved: s1's transcript is under the new account's folder, as it was" 'cmp -s "$t/old/projects/$slug/s1.jsonl" "$t/acct/projects/$slug/s1.jsonl"'
is "moved: it resumed s1" 'grep -qx "arg=--resume" "$RAN" && grep -qx "arg=s1" "$RAN"'
is "moved: the old account's copy is left as it is" '[ -f "$t/old/projects/$slug/s1.jsonl" ]'

# 2. a start that ends at once: not reported as restarted, false, said and logged
session s1 "$t/acct/projects/$slug/s1.jsonl"
ANSWER=$t/acct CLAUDE=ends restart
is "ended: the restart is false" '[ "$rc" = 1 ]'
is "ended: it does not say restarted" 'case $out in *restarted*) false ;; *) true ;; esac'
is "ended: said, with its session and folder" 'grep -q "orch: ended at once on session s1 under $t/acct: not restarted" "$t/err"'
is "ended: one line in coordd.log" '[ "$(grep -c "roster: orch: ended at once on session s1" "$COORD/coordd.log")" = 1 ]'

# 3. a start that ended after naming a session of its own with no transcript: session.json names s1 again
ANSWER=$t/acct CLAUDE=startup restart
is "startup: the restart is false" '[ "$rc" = 1 ]'
is "startup: session.json names s1 again, at its transcript" '[ "$(jq -r .session_id "$COORD/agents/orch/session.json")" = s1 ] && [ "$(jq -r .transcript_path "$COORD/agents/orch/session.json")" = "$t/acct/projects/$slug/s1.jsonl" ]'

# 4. a session with no transcript anywhere: refused before the running session is ended
session s2 "$t/old/projects/$slug/s2.jsonl"
ANSWER=$t/acct CLAUDE=up restart
is "nowhere: the restart is false, and said" '[ "$rc" = 1 ] && grep -q "orch: its session s2 has no transcript here" "$t/err"'
is "nowhere: nothing hung up and nothing started" '! grep -q hangup "$CALLS" && [ ! -e "$RAN" ]'

# 5. on the route, its transcript where it is: as before
mkdir -p "$t/home/.claude/projects/$slug"; printf '{"sessionId":"s3"}\n' >"$t/home/.claude/projects/$slug/s3.jsonl"
session s3 "$t/home/.claude/projects/$slug/s3.jsonl"
ANSWER= CLAUDE=up restart
is "route: restarted on s3, nothing copied" '[ "$rc" = 0 ] && [ "$out" = "orch: restarted (session s3)" ] && [ ! -e "$t/acct/projects/$slug/s3.jsonl" ]'
[ "$fails" = 0 ]
