#!/usr/bin/env bash
# The messaging gate (design decision 6) on roster sessions in a real box (BOX, required), with agent-base's
# settings and hooks:
#   1. a -> b over Claude Code's own cross-session messaging (SendMessage), while b is mid-turn;
#   2. host -> b with coord msg (the inbox socket), while b is mid-turn;
#   3. b -> a host session with coord msg HOST (a name in $COORD/hosts/), delivered by bin/coord-relay on the host.
# b runs eight 5-second ticks and logs each message with the tick it arrived after; PASS means before tick 8.
# HOST_SESSION is the hosts/ name of the host session that should receive step 3.
# KEEP=1 leaves a and b running.
set -uo pipefail
BOX=${BOX:?"set BOX to the box to test in"}
MODEL=${MODEL:-kiro-haiku-5.5[1m]}
HOST_SESSION=${HOST_SESSION:?name the host session that receives step 3}
AB=$(cd "$(dirname "$0")/.." && pwd)
COORD=$HOME/agent-coord/$BOX
DIR=$(cat "$COORD/repo")
T=$COORD/test; t1=sm-$RANDOM$RANDOM; t2=cm-$RANDOM$RANDOM
[ "${1:-}" = --yes ] || { sed -n '2,10p' "$0"; echo "Run again with --yes."; exit 2; }
systemctl --user is-active -q coord-relay || { echo "start bin/coord-relay on the host first"; exit 2; }
mkdir -p "$T" && rm -f "$T"/*; touch "$COORD/feed.jsonl"; feed0=$(wc -l <"$COORD/feed.jsonl")
newfeed() { tail -n +$((feed0 + 1)) "$COORD/feed.jsonl"; }
cat >"$T/b.md" <<EOF
You are b, a test agent. Run the shell command \`sleep 5; echo tick N\` eight times, as eight separate Bash tool
calls, for N = 1 to 8. Before each call, check whether a message from another session has arrived. For each one
that has, append one line to $T/got.txt: the message's exact text, then " after tick N", where N is the last
tick you finished. When all eight ticks are done, run this one Bash command:
coord msg $HOST_SESSION "b finished; got.txt: \$(tr '\n' ';' < $T/got.txt)"
Then reply DONE.
EOF
cat >"$T/a.md" <<EOF
You are a, a test agent. First run the Bash command \`sleep 20\`. Then use ListAgents, and send a message with
SendMessage to the session named b. The message text is exactly: $t1
Then reply SENT.
EOF
printf 'b %s b.md %s\na %s a.md %s\n' "$MODEL" "$DIR" "$MODEL" "$DIR" >"$T/roster"
box() { "$AB/box/box.sh" "$1" "$BOX" "${@:2}" 2>&1 | grep -v "sandbox $BOX"; }
echo "$(date '+%F %T') roster messaging gate in $BOX ($MODEL)"
box sh roster down a >/dev/null; box sh roster down b >/dev/null; rm -rf "$COORD/agents/a" "$COORD/agents/b"
box sh roster up "$T/roster"
waitfor() { local end=$(( $(date +%s) + $1 )); shift; until "$@"; do [ "$(date +%s)" -lt $end ] || return 1; sleep 2; done; }
# step 2 once b is ticking (its hook registers it; the inbox socket is inside the box)
sock_up() { local s; s=$(jq -r '.socket // empty' "$COORD/agents/b/session.json" 2>/dev/null) && [ -n "$s" ] && "$AB/box/box.sh" sh "$BOX" test -S "$s" 2>/dev/null; }
if waitfor 120 sock_up; then
  sleep 12; box sh coord msg b "$t2" | sed 's/^/  | /'
else echo "FAIL b never registered an inbox"; fi
# b's last step is its coord msg to the host; the relay answers it in the feed
waitfor 600 eval 'newfeed | grep -q "\"kind\": \"relay\""'
box sh tmux capture-pane -p -t box:a -S -60 >"$T/a.pane"
got=$(cat "$T/got.txt" 2>/dev/null)
for tok in "$t1:1 SendMessage a->b" "$t2:2 coord msg host->b"; do
  k=${tok%%:*}; what=${tok#*:}; line=$(grep "$k" <<<"$got")
  n=$(grep -o 'after tick [0-9]*' <<<"$line" | grep -o '[0-9]*$')
  if [ -z "$line" ]; then echo "FAIL $what: not received"
  elif [ -n "$n" ] && [ "$n" -lt 8 ]; then echo "PASS $what mid-turn (after tick $n of 8)"
  else echo "PARTIAL $what: delivered, not mid-turn ($line)"; fi
done
r=$(newfeed | grep '"kind": "relay"' | tail -1 | jq -r .status 2>/dev/null)
case $r in delivered*) echo "PASS 3 coord msg b->$HOST_SESSION: $r" ;; *) echo "FAIL 3 b->host: ${r:-no relay entry}" ;; esac
[ "${KEEP:-0}" = 1 ] || { box sh roster down a >/dev/null; box sh roster down b >/dev/null; echo "INFO a and b closed"; }
echo "$(date +%T) done"
