#!/usr/bin/env bash
# roster-no-first-words-test.sh: an agent added with no first words (New agent's empty first message, the owner's
# N-1997) is started with none of ours in their place and waits at its prompt; with words, they are its first turn as
# before. Claude Code: no prompt argument. Codex: no thread started with a turn on empty words; its TUI makes the
# thread, which roster finds loaded in its app-server and tells coordd as ready; a later restart of a thread that never
# had a turn (no rollout) gives a new one. Stand-ins for coordc, coord-pty, claude and codex-rpc; nothing is run.
# opencode and agy send nothing on empty words already (`[ -z "$prompt" ] ||`): roster-opencode-test, roster-agy-test.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'pkill -f "codex-standin-listen $t" 2>/dev/null || true; rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents" "$t/home" "$t/w" "$t/fakebin"
cp "$here/bin/roster" "$here/bin/harness-settings" "$here/bin/prompt-recipe" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"
cp -r "$here/prompts" "$t/ab/"; cp "$here/claude/settings.json" "$here/claude/standing.md" "$t/ab/claude/"
printf '#!/bin/sh\ncat >/dev/null\n' >"$t/ab/claude/hook"; chmod +x "$t/ab/claude/hook"
# coordc: each call recorded; a Codex app-server's `run` runs it, a Claude Code session's records its arguments only
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$*" >>"$CALLS"
case $1 in
ls | add | hangup | session | act | start | account-for) ;;
state) echo "" ;;
terms) for f in "$TESTDIR"/term-*; do [ -f "$f" ] && cat "$f"; done ;;
run) shift 3; case "$*" in *"codex-rpc serve"*) exec "$@" ;; *) printf 'arg=%s\n' "$@" >"$RAN" ;; esac ;;
*) exit 3 ;;
esac
X
# coord-pty: the terminal's command recorded; Codex's TUI only marks itself up, Claude Code's command goes on to coordc
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
NAME=$1; while [ "$1" != -- ]; do shift; done; shift
printf 'arg=%s\n' "$@" >"$TESTDIR/pty"
case "$*" in
*" codex "*)
  if [ -n "${TUI_ENDS:-}" ]; then printf '%s\t80x24\tcodex\tcodex\t(ended: 1)\n' "$NAME" >"$TESTDIR/term-$NAME"
  elif [ -n "${TUI_STALLS:-}" ]; then printf '%s\t80x24\tcodex\tcodex\n' "$NAME" >"$TESTDIR/term-$NAME"
  else printf '%s\t80x24\tcodex\tcodex\n' "$NAME" >"$TESTDIR/term-$NAME"; touch "$TESTDIR/tui-up"; fi ;;
*) exec "$@" ;;
esac
X
# codex-rpc: `serve` listens on the agent's socket; th-old is loaded from the start and th-tui once the TUI is up; a
# thread th-gone has no rollout
cat >"$t/ab/bin/codex-rpc" <<'X'
#!/bin/sh
echo "codex-rpc $*" >>"$CALLS"
case $1 in
serve) exec python3 -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(); time.sleep(600)' "$CODEX_SOCK" "codex-standin-listen $TESTDIR" ;;
new-thread) echo th-1 ;;
loaded) echo th-old; [ ! -f "$TESTDIR/tui-up" ] || echo th-tui ;;
status) [ "$2" != th-gone ] || { echo "RuntimeError: thread/resume: no rollout found for thread id th-gone" >&2; exit 1; }; echo idle ;;
*) ;;
esac
X
printf '#!/bin/sh\necho "2.1.332 (Claude Code)"\n' >"$t/fakebin/claude"
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty" "$t/ab/bin/codex-rpc" "$t/fakebin/claude"
printf '\n' >"$t/none.md"; printf 'Write the lexer.\n' >"$t/words.md"; echo "$t/none/project.toml" >"$t/coord/project"
export CODEX_HOME=$t/coord/codex-home AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran HOME=$t/home TESTDIR=$t PATH=$t/fakebin:$PATH
unset AGENT_NAME ROSTER_BOX COORD_IN_BOX CLAUDE_CONFIG_DIR
fails=0
is() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1 ($2)"; fails=$((fails + 1)); fi; }
# roster under a bound of the test's own, so a start that never ends fails here instead of hanging the test
add() { : >"$CALLS"; rm -f "$t/ran" "$t/pty" "$t/tui-up"; timeout 45 "$t/ab/bin/roster" add "$@" "$t/w" >"$t/out" 2>&1 && rc=0 || rc=$?; }

add zed claude-opus-5-5 "$t/none.md"
is "Claude Code, no first words: started, with no prompt argument (an empty one included)" '[ $rc = 0 ] && grep -qx "arg=claude" "$t/ran" && ! grep -qx "arg=" "$t/ran"'
add zoe claude-opus-5-5 "$t/words.md"
is "Claude Code, first words: they are its prompt, as before" '[ $rc = 0 ] && grep -qx "arg=Write the lexer." "$t/ran"'

add cx1 gpt-6-astra "$t/none.md"
is "Codex, no first words: no thread started with a turn" '[ $rc = 0 ] && ! grep -q "^codex-rpc new-thread" "$CALLS" && ! grep -q "^codex-rpc send" "$CALLS"'
is "its TUI makes the thread: no resume" 'grep -qx "arg=--remote" "$t/pty" && ! grep -qx "arg=resume" "$t/pty"'
is "on the model he picked (-m), not Codex's default" 'grep -A1 -x "arg=-m" "$t/pty" | tail -1 | grep -qx "arg=gpt-6-astra"'
is "the thread the TUI made told coordd, as ready" 'grep -qF "session cx1 {\"session_id\":\"th-tui\",\"cwd\":\"$t/w\",\"socket\":\"$CODEX_HOME/app/cx1.sock\",\"ready\":true}" "$CALLS"'
add cx2 gpt-6-astra "$t/words.md"
is "Codex, first words: a thread with them as its first turn, resumed by the TUI, as before" '[ $rc = 0 ] && grep -q "^codex-rpc new-thread $t/w gpt-6-astra Write the lexer.$" "$CALLS" && grep -qx "arg=resume" "$t/pty" && ! grep -q "\"ready\"" "$CALLS"'

TUI_ENDS=1 add cx3 gpt-6-astra "$t/none.md"
is "a TUI that ends with no thread: the start fails at once, and says why" '[ $rc != 0 ] && [ $rc != 124 ] && grep -q "cx3: its codex TUI ended without opening a thread" "$t/out"'
s=$SECONDS; TUI_STALLS=1 add cx4 gpt-6-astra "$t/none.md"
is "a TUI that stays up and opens no thread: the start fails after 30 s, and says why" '[ $rc != 0 ] && [ $rc != 124 ] && [ $((SECONDS - s)) -ge 29 ] && grep -q "cx4: codex opened no thread in 30 s" "$t/out"'

echo "{\"session_id\":\"th-gone\",\"cwd\":\"$t/w\",\"model\":\"gpt-6-astra\",\"kind\":\"codex\"}" >"$t/coord/agents/cx1/session.json"
: >"$CALLS"; rm -f "$t/pty" "$t/tui-up"
"$t/ab/bin/roster" restart cx1 >"$t/out" 2>&1 && rc=0 || rc=$?
is "a restart of a thread that never had a turn: a new one, made by its TUI, and said" '[ $rc = 0 ] && grep -q "th-gone never had a turn" "$t/out" && grep -qx "arg=--remote" "$t/pty" && grep -q "\"session_id\":\"th-tui\".*\"ready\":true" "$CALLS"'
echo "roster-no-first-words-test: $fails failed"
[ "$fails" = 0 ]
