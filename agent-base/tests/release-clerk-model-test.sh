#!/usr/bin/env bash
# release-clerk-model-test.sh: ops/release/clerk-model on a stand-in release, whose coordd keeps what it is called with
# and answers. No model, account or coordd of anyone's is reached.
set -uo pipefail
HERE=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
fails=0
check() { if eval "$2"; then echo "ok: $1"; else echo "FAIL: $1"; fails=$((fails + 1)); fi; }
mkdir -p "$D/rel" "$D/p/agents/rd"
cat >"$D/rel/coordd" <<STAND_IN
#!/bin/bash
printf '%s\n' "\$@" >"$D/args"; cat >"$D/prompt"; echo '{"answer": "a text", "tokens": 26}'
STAND_IN
chmod +x "$D/rel/coordd"
echo '{"kind": "claude", "model": "claude-sonnet-5-5:high"}' >"$D/p/agents/rd/session.json"
run() { echo "the prompt" | SPAWN_COORD="$D/p" SPAWN_PROGS="$D/rel" "$HERE/ops/release/clerk-model" "$@" 2>"$D/err"; }

out=$(run rd); rc=$?
check "the party's model, its effort cut, to the release's coordd call with the coord and agent-base" \
  '[ "$(paste -sd" " "$D/args")" = "call $D/p $D/rel/agent-base claude-sonnet-5-5" ]'
check "the prompt on its input, its answer printed as it gave it" \
  '[ $rc = 0 ] && [ "$(cat "$D/prompt")" = "the prompt" ] && [ "$out" = "{\"answer\": \"a text\", \"tokens\": 26}" ]'
rm -f "$D/args"
run nobody >/dev/null; rc=$?
check "no session: no call" '[ $rc = 1 ] && [ ! -e "$D/args" ] && grep -q "nobody has no session with a model" "$D/err"'
echo '{"kind": "agy"}' >"$D/p/agents/rd/session.json"
run rd >/dev/null; rc=$?
check "a session naming no model: no call" '[ $rc = 1 ] && [ ! -e "$D/args" ] && grep -q "rd has no session with a model" "$D/err"'
run ../rd >/dev/null; rc=$?
check "a name that is no agent's: no call" '[ $rc = 1 ] && [ ! -e "$D/args" ] && grep -q "is no agent" "$D/err"'

echo "release-clerk-model-test: $fails failed"
[ "$fails" = 0 ]
