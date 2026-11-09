#!/usr/bin/env bash
# Can Claude Code's background sessions (`claude --bg`, agent view) replace
# tmux windows in the box? Questions, in a fresh sandbox:
#   1. a --bg session gets the roster environment (its hooks register it);
#   2. `claude agents --json` lists it with state and status;
#   3. coord msg wakes it while it is idle with its process alive;
#   4. after `claude stop`, its inbox is gone, and `claude --resume ID --bg
#      PROMPT` brings it back with the conversation (the fallback coord would
#      need, since the supervisor stops idle sessions after about an hour).
# KEEP=1 leaves the box running.
set -uo pipefail
BOX=${BOX:-ab-bg}
MODEL=${MODEL:-kiro-haiku-5.5[1m]}
AB=$(cd "$(dirname "$0")/.." && pwd)
REPO=$HOME/agent-test-repo
COORD=$HOME/agent-coord/$BOX
box() { "$AB/box/box.sh" "$1" "$BOX" "${@:2}"; }
waitfor() { local end=$(( $(date +%s) + $1 )); shift; until "$@"; do [ "$(date +%s)" -lt $end ] || return 1; sleep 3; done; }
say() { echo "$*"; }

[ "${1:-}" = --yes ] || { sed -n '2,10p' "$0"; echo "Run again with --yes to go ahead."; exit 2; }
[ -d "$REPO/.git" ] || { echo "run sbx/smoke-test.sh first (it creates $REPO)"; exit 2; }
rm -rf "$COORD"
say "$(date '+%F %T') background-session test, box $BOX"
box up "$REPO" 8g >"/tmp/$BOX-up.log" 2>&1 || { say "FAIL box up (see /tmp/$BOX-up.log)"; exit 1; }
mkdir -p "$COORD/test"
cat >"$COORD/test/w.md" <<'EOF'
You are a test agent. Reply with the single word ready and end your turn. Later messages will arrive: for each one, append its exact text as one line to $COORD/test/w.txt with one Bash call, then end your turn.
EOF
in_box() { box sh bash -c "cd '$REPO' && $1"; }  # the clone sits at its host path in the box
launch="env -u ANTHROPIC_API_KEY AGENT_NAME=w AGENT_MODEL='$MODEL' COORD='$COORD' AGENT_BASE='$AB' PATH='$AB/bin':\$PATH \
  GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0='$AB/git/hooks' \
  claude --bg --name w --model '$MODEL' --settings '$AB/claude/settings.json' \
  --dangerously-skip-permissions \"\$(cat '$COORD/test/w.md')\""
in_box "$launch" 2>&1 | grep -v 'sandbox ab' | sed 's/^/  | /'

if waitfor 90 test -f "$COORD/agents/w/session.json"; then say "PASS 1 env: the hook registered w"
else say "FAIL 1 env: no registration"; fi
js=$(in_box 'claude agents --json --all' 2>/dev/null | grep -v 'sandbox ab')
row=$(jq -c '.[] | select(.name == "w") | {id, state, status, kind, sessionId}' <<<"$js" 2>/dev/null)
[ -n "$row" ] && say "PASS 2 listing: $row" || say "FAIL 2 listing: $(head -c 300 <<<"$js")"
id=$(jq -r '.id // empty' <<<"$row"); sid=$(jq -r '.sessionId // empty' <<<"$row")

idle() { in_box 'claude agents --json' 2>/dev/null | grep -v 'sandbox ab' | jq -e '.[] | select(.name == "w") | .state == "done"' >/dev/null; }
waitfor 120 idle && say "INFO w is idle (state done)" || say "INFO w never reached done"
box sh coord msg w tok-idle-$$ 2>&1 | grep -v 'sandbox ab'
if waitfor 120 grep -q "tok-idle-$$" "$COORD/test/w.txt" 2>/dev/null; then say "PASS 3 idle wake: coord msg started a turn"
else say "FAIL 3 idle wake: w.txt has '$(cat "$COORD/test/w.txt" 2>/dev/null)'"; fi

if [ -n "$id" ]; then
  in_box "claude stop $id" >/dev/null 2>&1; sleep 5
  st=$(in_box 'claude agents --json --all' 2>/dev/null | grep -v 'sandbox ab' | jq -r '.[] | select(.name == "w") | .state')
  if box sh coord msg w tok-stopped-$$ >/dev/null 2>&1; then say "INFO coord msg to stopped w was accepted (state $st)"
  else say "INFO coord msg to stopped w refused: no live inbox (state $st)"; fi
  in_box "env -u ANTHROPIC_API_KEY AGENT_NAME=w AGENT_MODEL='$MODEL' COORD='$COORD' AGENT_BASE='$AB' PATH='$AB/bin':\$PATH \
    claude --resume $sid --bg --settings '$AB/claude/settings.json' --dangerously-skip-permissions \
    '[from owner] tok-resume-$$'" 2>&1 | grep -v 'sandbox ab' | sed 's/^/  | /'
  if waitfor 150 grep -q "tok-resume-$$" "$COORD/test/w.txt" 2>/dev/null; then
    say "PASS 4 resume: --resume ID --bg delivered a prompt to the stopped session"
    grep -q "tok-stopped-$$" "$COORD/test/w.txt" && say "INFO the message sent while stopped also arrived"
  else say "FAIL 4 resume: w.txt has '$(tr '\n' '|' <"$COORD/test/w.txt" 2>/dev/null)'"; fi
  in_box 'claude agents --json --all' 2>/dev/null | grep -v 'sandbox ab' | jq -c '.[] | {name, id, state, status}'
fi
[ "${KEEP:-0}" = 1 ] && say "INFO kept" || { box down >/dev/null 2>&1; say "INFO box stopped"; }
say "$(date '+%T') done"
