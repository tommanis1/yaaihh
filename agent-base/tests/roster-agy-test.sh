#!/usr/bin/env bash
# roster-agy-test.sh: an agy agent's start by roster (agy_agent). Stand-ins for coordc, coord-pty and agy-rpc record
# what would run; no agy and no agy-rpc is run (agy-rpc-test runs them). Checks: on a sign-in account the pinned agy
# on the account's sign-in, AGY_BARE said either way (0, or 1 for a bare start); a restart keeps the running server,
# and only its own: a pid on file that is another process's by now is neither taken for it nor stopped; a start that
# fails leaves no pid on file; an account with no sign-in and no key is refused; the owner's Yolo (session.json's
# `mode`) starts agy-rpc with --dangerously-skip-permissions last and Safe without it, and a restart after he changed it
# starts the server again in it, on its session, where one in the same mode keeps it.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
other=
# a space in every path, as a home folder may have (the brain server's path quoted: N-1998 p7, 5)
t=$(mktemp -d "${TMPDIR:-/tmp}/roster agy.XXXXXX"); trap 'pkill -f "agy-rpc serve ag[0-9] " 2>/dev/null || true; [ -z "$other" ] || kill "$other" 2>/dev/null || true; rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents" "$t/home" "$t/w" "$t/acct/home/profile"
cp "$here/bin/roster" "$here/bin/harness-settings" "$here/bin/coord-brain-mcp" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/standing.md" "$t/ab/claude/"
printf '#!/bin/sh\ncat >/dev/null\n' >"$t/ab/claude/hook"
# coordc: records each call; `account-for` answers $ANSWER; `run NAME -- CMD` records its environment and, as agy-rpc
# serve does, writes the session (none with FAIL set: it ends at once), then stays, its command line naming CMD
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$*" >>"$CALLS"
case $1 in
ls | add | hangup | session) ;;
account-for) printf '%s' "$ANSWER" ;;
run) env >"$RAN"; [ -z "${FAIL:-}" ] || exit 1; echo s1 >"$CODEX_SOCK/session"; while :; do sleep 1; done ;;
*) exit 3 ;;
esac
X
printf '#!/bin/sh\nwhile [ "$1" != -- ]; do shift; done; shift\nprintf "arg=%%s\\n" "$@" >"$PTY"\n' >"$t/ab/bin/coord-pty"
printf '#!/bin/sh\necho "$*" >>"$SENT"\n' >"$t/ab/bin/agy-rpc"
chmod +x "$t/ab/claude/hook" "$t/ab/bin/coordc" "$t/ab/bin/coord-pty" "$t/ab/bin/agy-rpc"
echo hello >"$t/prompt.md"; echo "$t/none/project.toml" >"$t/coord/project"
export CODEX_HOME=$t/coord/codex-home AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran PTY=$t/pty SENT=$t/sent HOME=$t/home
signin="$t/acct
AGY_SIGNIN=$t/acct/home/profile"
fails=0
is() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1 ($2)"; fails=$((fails + 1)); fi; }
pidof_() { cat "$CODEX_HOME/app/$1.pid" 2>/dev/null; }
runs() { grep -c "^run $1 " "$CALLS" || true; }
asked() { grep -c "^account-for $1" "$CALLS" || true; }

# 1. a new agent on a sign-in account
ANSWER=$signin "$t/ab/bin/roster" add ag1 agy/gemini-4-neon:low "$t/prompt.md" "$t/w" >/dev/null
is "on a sign-in: the pinned agy, on the account's sign-in, no key" 'grep -qx "AGY_BIN=$t/coord/opt/agy/1.5.18/bin/antigravity" "$RAN" && grep -qx "AGY_SIGNIN=$t/acct/home/profile" "$RAN" && ! grep -q "^AGY_KEY_FILE=" "$RAN"'
is "not bare: AGY_BARE is said, 0" 'grep -qx AGY_BARE=0 "$RAN"'
mcp_of() { sed -n 's/^AGY_MCP=//p' "$RAN"; }
is "not bare: its brain MCP server is wired, as this agent (N-1998)" '[ "$(mcp_of | jq -c "[.[] | select(.name == \"brain\") | [.command, (.env | from_entries | .AGENT_NAME, .COORD, .AGENT_KIND)]]")" = "$(jq -cn --arg c "$t/ab/bin/coord-brain-mcp" --arg co "$t/coord" "[[\$c, \"ag1\", \$co, \"agy\"]]")" ]'
is "the pid on file is its server" 'p=$(pidof_ ag1) && tr "\0" " " <"/proc/$p/cmdline" | grep -qF -- "agy-rpc serve ag1 "'
is "Safe (no mode in session.json): no --dangerously-skip-permissions" '! tr "\0" " " <"/proc/$(pidof_ ag1)/cmdline" | grep -qF -- --dangerously-skip-permissions'
is "its first words go to its session" 'grep -qx "send s1 next hello" "$SENT"'
echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w\",\"model\":\"agy/gemini-4-neon:low\",\"kind\":\"agy\"}" >"$t/coord/agents/ag1/session.json"
# no first words (New agent's empty first message, N-1997): its session made, nothing sent to it
printf '\n' >"$t/none.md"; : >"$SENT"
ANSWER=$signin "$t/ab/bin/roster" add ag6 agy/gemini-4-neon "$t/none.md" "$t/w" >/dev/null
is "no first words: its server and session started, nothing sent to it" 'p=$(pidof_ ag6) && tr "\0" " " <"/proc/$p/cmdline" | grep -qF -- "agy-rpc serve ag6 " && ! grep -q "^send " "$SENT"'

# 2. a restart keeps the running server: no new one, its account not asked again
ANSWER=$signin "$t/ab/bin/roster" restart ag1 >/dev/null
is "a restart keeps the running server and asks no account" '[ "$(runs ag1)" = 1 ] && [ "$(asked ag1)" = 1 ]'

# 3. down stops it; then a pid on file that is another process's (a pid taken again) is not its server
was=$(pidof_ ag1)
"$t/ab/bin/roster" down ag1 >/dev/null 2>&1 || true
for _ in $(seq 50); do kill -0 "$was" 2>/dev/null || break; sleep 0.1; done
is "roster down stops its server" '! kill -0 "$was" 2>/dev/null && [ -z "$(pidof_ ag1)" ]'
sleep 300 & other=$!
echo "$other" >"$CODEX_HOME/app/ag1.pid"
"$t/ab/bin/roster" down ag1 >/dev/null 2>&1 || true
is "down does not stop another process whose pid is on file" 'kill -0 "$other" 2>/dev/null'
ANSWER=$signin "$t/ab/bin/roster" restart ag1 >/dev/null
is "a restart does not take another process for its server: a new one, on its account asked again" '[ "$(runs ag1)" = 2 ] && [ "$(asked ag1)" = 2 ] && [ "$(pidof_ ag1)" != "$other" ] && kill -0 "$other" 2>/dev/null'
"$t/ab/bin/roster" down ag1 >/dev/null 2>&1 || true

# 4. a start that fails leaves no pid on file
out=$(FAIL=1 ANSWER=$signin "$t/ab/bin/roster" add ag2 agy/gemini-4-neon "$t/prompt.md" "$t/w" 2>&1) && r=0 || r=$?
is "a start that fails says so and leaves no pid on file" '[ "$r" != 0 ] && [[ $out == *"ag2: agy did not start"* ]] && [ ! -e "$CODEX_HOME/app/ag2.pid" ]'

# 5. bare: AGY_BARE 1
ROSTER_BARE=1 ANSWER=$signin "$t/ab/bin/roster" add ag3 agy/gemini-4-neon "$t/prompt.md" "$t/w" >/dev/null
is "a bare start says AGY_BARE 1" 'grep -qx AGY_BARE=1 "$RAN"'
is "bare: no MCP server, not even its brain" '[ "$(mcp_of)" = "[]" ]'
"$t/ab/bin/roster" down ag3 >/dev/null 2>&1 || true

# 5b. a project with a brain of its own ($COORD/mcp/brain): that one alone
mkdir -p "$t/coord/mcp"; printf '#!/bin/sh\n' >"$t/coord/mcp/brain"; chmod +x "$t/coord/mcp/brain"
ANSWER=$signin "$t/ab/bin/roster" add ag7 agy/gemini-4-neon "$t/prompt.md" "$t/w" >/dev/null
is "the project's own brain stands for coord-brain-mcp" '[ "$(mcp_of | jq -c "[.[] | select(.name == \"brain\") | .command]")" = "[\"$t/coord/mcp/brain\"]" ]'
"$t/ab/bin/roster" down ag7 >/dev/null 2>&1 || true
rm -r "$t/coord/mcp"

# 7. the owner's Yolo: set while it runs in Safe, a restart starts it again with the flag last, on its session; a
# restart in the same mode keeps it; set back to Safe, it starts again without
cmd() { tr "\0" " " <"/proc/$(pidof_ "$1")/cmdline"; }
ANSWER=$signin "$t/ab/bin/roster" add ag5 agy/gemini-4-neon "$t/prompt.md" "$t/w" >/dev/null
safe5='{"session_id":"s1","cwd":"'$t/w'","model":"agy/gemini-4-neon","kind":"agy"}'
echo "$safe5" >"$t/coord/agents/ag5/session.json"
was=$(pidof_ ag5)
jq -c '. + {mode: "yolo", mode_from: "agent"}' <<<"$safe5" >"$t/coord/agents/ag5/session.json"
ANSWER=$signin "$t/ab/bin/roster" restart ag5 >/dev/null
is "Yolo set: the Safe server is stopped" '! kill -0 "$was" 2>/dev/null'
is "and started again with --dangerously-skip-permissions last, on its session" '[ "$(runs ag5)" = 2 ] && [[ $(cmd ag5) == *"agy-rpc serve ag5 $t/w gemini-4-neon s1 --dangerously-skip-permissions " ]]'
yolo5=$(pidof_ ag5)
ANSWER=$signin "$t/ab/bin/roster" restart ag5 >/dev/null
is "a restart in Yolo keeps the server in Yolo" '[ "$(runs ag5)" = 2 ] && [ "$(pidof_ ag5)" = "$yolo5" ]'
echo "$safe5" >"$t/coord/agents/ag5/session.json"
ANSWER=$signin "$t/ab/bin/roster" restart ag5 >/dev/null
is "Safe again: started again without the flag, on its session" '! kill -0 "$yolo5" 2>/dev/null && [ "$(runs ag5)" = 3 ] && [[ $(cmd ag5) == *"agy-rpc serve ag5 $t/w gemini-4-neon s1 " ]] && [[ $(cmd ag5) != *--dangerously-skip-permissions* ]]'
jq -c '. + {mode: "yolo"}' <<<"$safe5" >"$t/coord/agents/ag5/session.json"
"$t/ab/bin/roster" down ag5 >/dev/null 2>&1 || true
ANSWER=$signin "$t/ab/bin/roster" restart ag5 >/dev/null
is "a stopped one in Yolo starts with the flag" '[ "$(runs ag5)" = 4 ] && [[ $(cmd ag5) == *" --dangerously-skip-permissions " ]]'
"$t/ab/bin/roster" down ag5 >/dev/null 2>&1 || true

# 6. an account with neither a sign-in nor a key
out=$(ANSWER="$t/acct
AGY_SIGNINX=1" "$t/ab/bin/roster" add ag4 agy/gemini-4-neon "$t/prompt.md" "$t/w" 2>&1) && r=0 || r=$?
is "an account with no sign-in and no key is refused (a name that only begins the same is none)" '[ "$r" != 0 ] && [[ $out == *"no sign-in, no key"* ]] && [ "$(runs ag4)" = 0 ]'
[ $fails = 0 ] && echo "all passed" || { echo "$fails failed"; exit 1; }
