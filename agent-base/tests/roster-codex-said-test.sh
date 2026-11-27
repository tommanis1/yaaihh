#!/usr/bin/env bash
# roster-codex-said-test.sh: a Codex agent's start is in its coord's record as every kind's is (`roster said` before its
# app-server; paper-research N-1446 p3174): kind codex, its model and effort, its folder and its first words. Stand-ins
# for coordc, coord-pty and codex-rpc (whose `serve` only listens on the agent's socket); no Codex is run.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'pkill -f "codex-standin-listen $t" 2>/dev/null || true; rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents" "$t/home" "$t/w"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/standing.md" "$t/ab/claude/"
printf '#!/bin/sh\ncat >/dev/null\n' >"$t/ab/claude/hook"; chmod +x "$t/ab/claude/hook"
# coordc: records each call; `run NAME -- CMD` runs CMD; `act` (the record line, from roster said) is recorded only
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$*" >>"$CALLS"
case $1 in
ls | add | hangup | session | act) ;;
state) echo "" ;;
run) shift 3; exec "$@" ;;
*) exit 3 ;;
esac
X
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
exit 0
X
# codex-rpc: `serve` listens on the agent's socket and stays; a new thread is th-1; the rest answers nothing
cat >"$t/ab/bin/codex-rpc" <<'X'
#!/bin/sh
case $1 in
serve) printf '%s\n' "$@" >"$TESTDIR/served"
  exec python3 -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(); time.sleep(600)' "$CODEX_SOCK" "codex-standin-listen $TESTDIR" "unix://$CODEX_SOCK" ;;
new-thread) echo th-1 ;;
*) ;;
esac
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty" "$t/ab/bin/codex-rpc"
printf 'Write the lexer.' >"$t/prompt.md"; echo "$t/none/project.toml" >"$t/coord/project"
export CODEX_HOME=$t/coord/codex-home AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls HOME=$t/home TESTDIR=$t
unset AGENT_NAME
fails=0
is() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1 ($2)"; fails=$((fails + 1)); fi; }
"$t/ab/bin/roster" add cx1 gpt-6-astra:medium "$t/prompt.md" "$t/w" >/dev/null
for _ in $(seq 50); do grep -q "^act began " "$CALLS" && break; sleep 0.1; done
began=$(grep "^act began " "$CALLS" | head -1 || true)
is "a Codex start's record line: kind codex, its model and effort, its folder" '[[ $began == "act began start: codex gpt-6-astra effort medium in $t/w"* ]] && [[ $began == *" kind=codex "* ]] && [[ $began == *" effort=medium "* ]]'
is "its first words whole, beside their hash (they name no brief)" '[[ $began == *" prompt=Write the lexer."* ]] && [[ $began == *" prompt_sha=$(printf "Write the lexer." | sha256sum | cut -c1-64)"* ]]'
# its system text (system_prompt): the standing text appended, as its developer instructions
std=$(printf "%s" "$(cat "$t/ab/claude/standing.md")" | jq -Rs .)
is "the standing text, as its developer instructions" 'grep -qxF -- "developer_instructions=$std" "$t/served" && ! grep -q "^model_instructions_file=" "$t/served"'
# a version of his in place of Codex's own: restart --system stops the app-server, which reads it only as it starts,
# and the new one is given it as a file (model_instructions_file), the developer instructions gone
mkdir -p "$t/coord/prompts/cx1.system"; printf 'You are only a calculator.\n' >"$t/coord/prompts/cx1.system/1.md"
echo "{\"session_id\":\"th-1\",\"cwd\":\"$t/w\",\"model\":\"gpt-6-astra:medium\",\"kind\":\"codex\"}" >"$t/coord/agents/cx1/session.json"
old=$(cat "$CODEX_HOME/app/cx1.pid"); : >"$CALLS"
"$t/ab/bin/roster" restart cx1 --system 1 replace >/dev/null
new=$(cat "$CODEX_HOME/app/cx1.pid")
is "--system 1 replace: its app-server stopped and a new one started" '! kill -0 "$old" 2>/dev/null && [ "$new" != "$old" ] && kill -0 "$new"'
is "the new one given the version as a file, in place of its own" 'f=$(sed -n "s/^model_instructions_file=//p" "$t/served" | jq -r .) && [ "$(cat "$f")" = "You are only a calculator." ] && ! grep -q "^developer_instructions=" "$t/served"'
is "named for later starts, phase system" '[ "$(cat "$t/coord/prompts/cx1.system/current")" = "1 replace" ] && grep -q "^act began system: codex .* system=1 replace=1 " "$CALLS"'
echo "roster-codex-said-test: $fails failed"
[ "$fails" = 0 ]
