#!/usr/bin/env bash
# Smoke test of the box pieces end to end, in a fresh sandbox (box/box.sh up).
#
# Two agents start from a roster in tmux, then:
#   1. registration: both write session.json with a live inbox socket;
#   2. coord msg: a token posted to b mid-turn reaches it (b writes it out);
#   3. trailers: b's commit carries Agent:, Session: and Ruling: trailers, and
#      the host fetches it from sandbox-BOX;
#   4. coord status: a row per agent and no alarm.
# Creates the test repo ~/agent-test-repo if missing. KEEP=1 leaves the box running.
set -uo pipefail
BOX=${BOX:-ab-smoke}
A_MODEL=${A_MODEL:-kiro-haiku-5.5[1m]}
B_MODEL=${B_MODEL:-claude-haiku-5-5}
AB=$(cd "$(dirname "$0")/.." && pwd)
REPO=$HOME/agent-test-repo
COORD=$HOME/agent-coord/$BOX
box() { "$AB/box/box.sh" "$1" "$BOX" "${@:2}"; }
pass=0 fail=0
ok() { echo "PASS $*"; pass=$((pass + 1)); }
no() { echo "FAIL $*"; fail=$((fail + 1)); }
waitfor() {  # waitfor SECONDS TEST...
  local end=$(( $(date +%s) + $1 )); shift
  until "$@"; do [ "$(date +%s)" -lt $end ] || return 1; sleep 3; done
}

[ "${1:-}" = --yes ] || { sed -n '2,12p' "$0"; echo "Run again with --yes to go ahead."; exit 2; }
if [ ! -d "$REPO/.git" ]; then
  git init -q -b main "$REPO" && echo test >"$REPO/README" && git -C "$REPO" add README \
    && git -C "$REPO" -c user.name=owner -c user.email=owner@local commit -qm init
fi
rm -rf "$COORD"
echo "$(date '+%F %T') smoke test, box $BOX"
box up "$REPO" 8g >"/tmp/$BOX-up.log" 2>&1 || { no "box up (see /tmp/$BOX-up.log)"; exit 1; }
mkdir -p "$COORD/test" "$COORD/roster"
echo "INFO tools: $(box sh bash -c 'command -v tmux socat jq git claude | xargs -n1 basename | tr "\n" " "')"

cat >"$COORD/roster/a.md" <<'EOF'
You are agent a in a smoke test. Run these as separate Bash tool calls, one at a time: `sleep 4; echo tick 1`, then tick 2, and so on up to tick 12.
EOF
cat >"$COORD/roster/b.md" <<'EOF'
You are agent b in a smoke test. Run these as separate Bash tool calls, one at a time: `sleep 4; echo tick 1`, then tick 2, and so on up to tick 8.
If a message from another session arrives meanwhile, write its exact text to $COORD/test/b.txt with one Bash call, then continue the ticks.
After the ticks: in the current directory (a git repo), run `coord ack 7`, then create b.txt containing hello and commit it with the message "smoke: b". Then stop.
EOF
printf 'a %s a.md\nb %s b.md\n' "$A_MODEL" "$B_MODEL" >"$COORD/roster/roster"
wd=$(box sh bash -c 'pwd')
box sh bash -c "cd '$wd' && roster up '$COORD/roster/roster'"

registered() { for n in a b; do jq -e '.socket | length > 0' "$COORD/agents/$n/session.json" >/dev/null 2>&1 || return 1; done; }
if waitfor 120 registered; then ok "registration: a and b wrote session.json with an inbox socket"
else no "registration: $(ls "$COORD/agents" 2>/dev/null | tr '\n' ' ')"; box sh tmux capture-pane -p -t box:a | tail -15; fi

sleep 10
token=tok-$RANDOM$RANDOM
box sh coord msg b "$token" && echo "INFO sent $token to b"

if waitfor 180 grep -q "$token" "$COORD/test/b.txt" 2>/dev/null; then ok "coord msg: b got the token mid-turn"
else no "coord msg: b.txt has '$(cat "$COORD/test/b.txt" 2>/dev/null)'"; fi

committed() { box sh bash -c "cd '$wd' && git log -1 --format=%s | grep -q 'smoke: b'"; }
if waitfor 240 committed; then
  tr=$(box sh bash -c "cd '$wd' && git log -1 --format=%B")
  if grep -q '^Agent: b$' <<<"$tr" && grep -q '^Session: ' <<<"$tr" && grep -q '^Ruling: 7$' <<<"$tr"; then ok "trailers: Agent, Session, Ruling on b's commit"
  else no "trailers: $(tr '\n' '|' <<<"$tr")"; fi
  if box fetch >/dev/null 2>&1 && git -C "$REPO" log -1 --format=%s "sandbox-$BOX/main" | grep -q 'smoke: b'; then ok "host fetch: sandbox-$BOX/main has b's commit"
  else no "host fetch from sandbox-$BOX"; fi
else no "b never committed"; box sh tmux capture-pane -p -t box:b | tail -15; fi

st=$(box sh coord status); echo "$st"
if grep -q '^a ' <<<"$st" && grep -q '^b ' <<<"$st" && ! grep -q ALARM <<<"$st"; then ok "coord status: both rows, no alarm"
else no "coord status"; fi

[ "${KEEP:-0}" = 1 ] && echo "INFO kept: box.sh attach $BOX" || { box down >/dev/null 2>&1; echo "INFO box stopped (sbx rm $BOX removes it)"; }
echo "$(date '+%T') $pass passed, $fail failed"
[ $fail -eq 0 ]
