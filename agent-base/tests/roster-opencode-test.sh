#!/usr/bin/env bash
# roster-opencode-test.sh: an opencode agent's start by roster (tool-scan, N-591 post 109). Stand-ins for coordc and
# coord-pty record what would run; the agent's server is opencode-standin.py at the coord's pinned path, run by a copy
# of opencode-rpc whose pinned hash is the stand-in's. No opencode is run. Checks: the server's environment (our config
# as OPENCODE_CONFIG_DIR, opencode's home the agent's own, the account's folders over the agent's), the config and the
# plugin written, the session told to coordd, the screen, the first prompt delivered, a restart on the same session,
# `roster down` ending the server, only its own server kept or stopped, and a start that fails leaving no pid on file.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
other=
t=$(mktemp -d); trap 'pkill -f "opencode-rpc serve oc1 " 2>/dev/null || true; [ -z "$other" ] || kill "$other" 2>/dev/null || true; rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/ab/opencode/plugin" "$t/coord/agents" "$t/coord/mcp" "$t/home" "$t/w" "$t/acct/home/data"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/standing.md" "$t/ab/claude/"
cp "$here/opencode/plugin/coord.js" "$t/ab/opencode/plugin/"
pinned=$t/coord/opt/opencode/1.18.34/bin/opencode
mkdir -p "$(dirname "$pinned")"; cp "$here/tests/opencode-standin.py" "$pinned"; chmod +x "$pinned"
sum=$(sha256sum "$pinned" | cut -d' ' -f1)
sed "s/^OPENCODE_SHA256 = '[0-9a-f]\{64\}'\$/OPENCODE_SHA256 = '$sum'/" "$here/bin/opencode-rpc" >"$t/ab/bin/opencode-rpc"; chmod +x "$t/ab/bin/opencode-rpc"
grep -q "^OPENCODE_SHA256 = '$sum'" "$t/ab/bin/opencode-rpc"
printf '#!/bin/sh\ncat >/dev/null\n' >"$t/ab/claude/hook"; chmod +x "$t/ab/claude/hook"
printf '#!/bin/sh\nexit 0\n' >"$t/coord/mcp/symbols"; chmod +x "$t/coord/mcp/symbols"
# coordc: records each call; `account-for` answers $ANSWER; `run NAME -- CMD` records the environment and runs CMD (FAIL: ends)
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$*" >>"$CALLS"
case $1 in
ls) ;;
add | hangup) ;;
state) echo "${STATE:-}" ;;
account-for) printf '%s' "$ANSWER" ;;
session) printf '%s\n' "$3" >"$SESSION" ;;
run) shift 3; env >"$RAN"; [ -z "${FAIL:-}" ] || exit 1; exec "$@" ;;
*) exit 3 ;;
esac
X
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
while [ "$1" != -- ]; do shift; done; shift
{ env; printf 'arg=%s\n' "$@"; } >"$PTY"
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty"
echo hello >"$t/prompt.md"; echo "$t/none/project.toml" >"$t/coord/project"
export CODEX_HOME=$t/coord/codex-home AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran PTY=$t/pty SESSION=$t/session HOME=$t/home STANDIN_STATE=$t/standin-sessions.json
fails=0
is() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1 ($2)"; fails=$((fails + 1)); fi; }
a=$t/coord/agents/oc1/opencode
pw() { cat "$a/coord/password"; }
url() { cat "$a/coord/url"; }

# 1. a new agent on no account
ANSWER= "$t/ab/bin/roster" add oc1 anthropic/claude-sonnet-5-5 "$t/prompt.md" "$t/w" >/dev/null
is "coordd is told the agent's kind is opencode" 'grep -qx "add oc1 opencode anthropic/claude-sonnet-5-5" "$CALLS"'
is "its server runs the pinned opencode, as kind opencode" 'grep -qx "OPENCODE_BIN=$t/coord/opt/opencode/1.18.34/bin/opencode" "$RAN" && grep -qx "AGENT_KIND=opencode" "$RAN"'
is "our config is opencode's config folder, its home the agent's own" 'grep -qx "OPENCODE_CONFIG_DIR=$a/config/opencode" "$RAN" && grep -qx "OPENCODE_TEST_HOME=$a/home" "$RAN"'
is "no account: opencode's data is the agent's folder" 'grep -qx "XDG_DATA_HOME=$a/data" "$RAN"'
is "the repository's config, the owner's ~/.claude and the update are off" 'grep -qx OPENCODE_DISABLE_PROJECT_CONFIG=1 "$RAN" && grep -qx OPENCODE_DISABLE_CLAUDE_CODE=1 "$RAN" && grep -qx OPENCODE_DISABLE_AUTOUPDATE=1 "$RAN"'
is "no password in the server's environment from roster (opencode-rpc makes it)" '! grep -q "^OPENCODE_SERVER_PASSWORD=" "$RAN"'
c=$a/config/opencode/opencode.json
is "the config: every ask allowed but question, no update, no share, the standing text" 'jq -e ".permission == {\"*\": \"allow\", question: \"deny\"} and .autoupdate == false and .share == \"disabled\" and (.instructions[0] | endswith(\"/system-prompt.md\"))" "$c" >/dev/null && [ -s "$(jq -r ".instructions[0]" "$c")" ]'
is "the config: the coord's MCP servers, not a coord channel" 'jq -e ".mcp.symbols.command == [\"$t/coord/mcp/symbols\"] and (.mcp | has(\"coord\") | not)" "$c" >/dev/null'
is "the plugin is in the config folder" 'cmp -s "$a/config/opencode/plugin/coord.js" "$t/ab/opencode/plugin/coord.js"'
sid=$(cat "$a/coord/session")
is "coordd is told the session, its folder and the coord folder" '[ "$(jq -r .session_id "$SESSION")" = "$sid" ] && [ "$(jq -r .cwd "$SESSION")" = "$t/w" ] && [ "$(jq -r .socket "$SESSION")" = "$a/coord" ]'
is "the terminal is opencode's screen through opencode-rpc, with the same folders" 'grep -qx "arg=$t/ab/bin/opencode-rpc" "$PTY" && grep -qx "arg=screen" "$PTY" && grep -qx "arg=OPENCODE_CONFIG_DIR=$a/config/opencode" "$PTY" && grep -qx "arg=COORD=$t/coord" "$PTY"'
for _ in $(seq 100); do curl -s -u "opencode:$(pw)" "$(url)/test/prompts" | jq -e "length == 1" >/dev/null && break; sleep 0.1; done
is "the first prompt is sent to the session, with its model" 'curl -s -u "opencode:$(pw)" "$(url)/test/prompts" | jq -e ".[0].parts[0].text == \"hello\" and .[0].model.modelID == \"claude-sonnet-5-5\"" >/dev/null'

# its start in the record as a Claude Code start's is (`roster said` before the server, one start path for every
# kind; paper-research N-1446 p3164): kind, model and folder, and its first words whole, which name no brief
for _ in $(seq 50); do grep -q "^act began " "$CALLS" && break; sleep 0.1; done
began=$(grep "^act began " "$CALLS" | head -1 || true)
is "its start's record line: kind opencode, its model and folder, its words whole beside their hash" '[[ $began == "act began start: opencode anthropic/claude-sonnet-5-5 in $t/w"* ]] && [[ $began == *" kind=opencode "* ]] && [[ $began == *" prompt=hello"* ]] && [[ $began == *" prompt_sha=$(printf hello | sha256sum | cut -c1-64)"* ]]'
# 2. restart: the server holds the session; only the screen is new
echo "{\"session_id\":\"$sid\",\"cwd\":\"$t/w\",\"model\":\"anthropic/claude-sonnet-5-5\",\"kind\":\"opencode\"}" >"$t/coord/agents/oc1/session.json"
rm -f "$PTY"
ANSWER= "$t/ab/bin/roster" restart oc1 >/dev/null
is "a restart keeps the running server and its session, and opens the screen again" '[ "$(cat "$a/coord/session")" = "$sid" ] && [ -s "$PTY" ] && [ "$(grep -c "^run " "$CALLS")" = 1 ]'
is "a kept server's account is not asked again (accounts-3, N-946 post 14)" '[ "$(grep -c "^account-for oc1" "$CALLS")" = 1 ]'

# 3. down ends the server
"$t/ab/bin/roster" down oc1 >/dev/null 2>&1 || true
for _ in $(seq 50); do pgrep -f "opencode-rpc serve oc1 " >/dev/null || break; sleep 0.1; done
is "roster down ends the server" '! pgrep -f "opencode-rpc serve oc1 " >/dev/null'

# 4. on an account: its folders over the agent's
ANSWER="$t/acct
XDG_DATA_HOME=$t/acct/home/data
TMPDIR=$t/acct/home/tmp" "$t/ab/bin/roster" restart oc1 >/dev/null
is "on an account the server runs with the account's folders" 'grep -qx "XDG_DATA_HOME=$t/acct/home/data" "$RAN" && grep -qx "TMPDIR=$t/acct/home/tmp" "$RAN" && grep -qx "OPENCODE_CONFIG_DIR=$a/config/opencode" "$RAN"'
is "a new server asks for its account" '[ "$(grep -c "^account-for oc1" "$CALLS")" = 2 ]'
is "the screen runs on the agent's own folders, not the account's" 'grep -qx "arg=XDG_DATA_HOME=$a/data" "$PTY" && ! grep -q "^arg=.*$t/acct" "$PTY"'
is "and resumes its session" '[ "$(cat "$a/coord/session")" = "$sid" ]'

# 5. a version of his in place of opencode's own (restart --system): the running server stopped, since opencode reads
# its config only as it starts, and the new one's default agent has the version's file as its prompt, no instructions
mkdir -p "$t/coord/prompts/oc1.system"; printf 'You are only a calculator.\n' >"$t/coord/prompts/oc1.system/1.md"
was=$(cat "$CODEX_HOME/app/oc1.pid")
ANSWER= "$t/ab/bin/roster" restart oc1 --system 1 replace >/dev/null
is "--system 1 replace: the server stopped and a new one started on its session" '! kill -0 "$was" 2>/dev/null && [ "$(cat "$CODEX_HOME/app/oc1.pid")" != "$was" ] && [ "$(cat "$a/coord/session")" = "$sid" ]'
is "its default agent's prompt is the version's file, in place of opencode's own; no instructions" 'p=$(jq -r ".agent.build.prompt" "$c") && [[ $p == "{file:"*"}" ]] && f=${p#\{file:} && [ "$(cat "${f%\}}")" = "You are only a calculator." ] && jq -e ".instructions == []" "$c" >/dev/null'
"$t/ab/bin/roster" down oc1 >/dev/null 2>&1 || true

# 6. only its own server: a pid on file that is another process's by now (a pid taken again), its session file left
sleep 300 & other=$!
echo "$other" >"$CODEX_HOME/app/oc1.pid"
"$t/ab/bin/roster" down oc1 >/dev/null 2>&1 || true
is "down does not stop another process whose pid is on file" 'kill -0 "$other" 2>/dev/null'
runs=$(grep -c "^run " "$CALLS"); asked=$(grep -c "^account-for oc1" "$CALLS")
ANSWER= "$t/ab/bin/roster" restart oc1 >/dev/null
is "a restart does not take another process for its server: a new one, on its account asked again" '[ "$(grep -c "^run " "$CALLS")" = $((runs + 1)) ] && [ "$(grep -c "^account-for oc1" "$CALLS")" = $((asked + 1)) ] && [ "$(cat "$CODEX_HOME/app/oc1.pid")" != "$other" ] && kill -0 "$other" 2>/dev/null'
"$t/ab/bin/roster" down oc1 >/dev/null 2>&1 || true

# no first words (New agent's empty first message, N-1997): its session made and told coordd, nothing sent to it
printf '\n' >"$t/none.md"
ANSWER= "$t/ab/bin/roster" add oc3 anthropic/claude-sonnet-5-5 "$t/none.md" "$t/w" >/dev/null
a3=$t/coord/agents/oc3/opencode
is "no first words: its session started, nothing sent to it" '[ -s "$a3/coord/session" ] && curl -s -u "opencode:$(cat "$a3/coord/password")" "$(cat "$a3/coord/url")/test/prompts" | jq -e "length == 0" >/dev/null'
"$t/ab/bin/roster" down oc3 >/dev/null 2>&1 || true

# 7. a start that fails leaves no pid on file
out=$(FAIL=1 ANSWER= "$t/ab/bin/roster" add oc2 anthropic/claude-sonnet-5-5 "$t/prompt.md" "$t/w" 2>&1) && r=0 || r=$?
is "a start that fails says so and leaves no pid on file" '[ "$r" != 0 ] && [[ $out == *"oc2: opencode did not start"* ]] && [ ! -e "$CODEX_HOME/app/oc2.pid" ]'
[ $fails = 0 ] && echo "all passed" || { echo "$fails failed"; exit 1; }
