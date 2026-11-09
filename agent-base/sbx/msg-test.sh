#!/usr/bin/env bash
# Messaging part of the gate test, against a running sandbox (default ab-gate).
#
# gate-b runs eight slow tool calls. While it does, gate-a sends it a token over
# cross-session messaging. PASS means the token reached gate-b between its tool
# calls, before its turn ended.
#
# Needs: sandbox network rule localhost:8317, and the proxy's client key, read
# from the proxy's config.yaml and passed as an environment variable.
set -uo pipefail
NAME=${NAME:-ab-gate}
B_MODEL=${B_MODEL:-kiro-haiku-5.5}
A_MODEL=${A_MODEL:-claude-haiku-5-5}
WORK=$(cd "$(dirname "$0")/.." && pwd)/scratch/gate
OUT=$WORK/results/msg
TOKEN=ping-$RANDOM$RANDOM
mkdir -p "$OUT" && rm -f "$OUT"/*
ANTHROPIC_API_KEY=$(python3 -I -c '
import re, sys
t = open(sys.argv[1]).read()
print(re.search(r"^access:\n(?:.*\n)*?  api-keys:\n    - [\x27\"]?([^\x27\"\n]+)", t, re.M).group(1))' \
  "$HOME/.local/opt/EasyCLIProxyAPI/cpa-core/config.yaml")
export ANTHROPIC_API_KEY

printf '%s\n' '{"crossSessionInbound":"accept"}' >"$OUT/accept.json"
cat >"$OUT/prompt-b.txt" <<EOF
Run the shell command \`sleep 5; echo tick N\` eight times, as eight separate Bash tool calls, for N = 1 to 8.
Before each call, check whether a message from another session has arrived. As soon as one has, write one line
to the file $OUT/got.txt: the message's exact text, then " after tick N", where N is the last tick you finished.
Then carry on with the remaining ticks. When all eight ticks are done, reply DONE.
EOF
cat >"$OUT/prompt-a.txt" <<EOF
Use ListAgents. Send a message with SendMessage to the session named gate-b; if no session has that name,
send it to the one other session listed. The message text is exactly: $TOKEN
Then reply SENT.
EOF
run() { # SESSION MODEL PROMPT_FILE
  timeout 400 sbx exec -e ANTHROPIC_API_KEY -e ANTHROPIC_BASE_URL=http://host.docker.internal:8317 "$NAME" bash -lc \
    "cd '$WORK' && claude -p --name $1 --model '$2' --dangerously-skip-permissions --settings '$OUT/accept.json' \"\$(cat '$3')\""
}
run gate-b "$B_MODEL" "$OUT/prompt-b.txt" >"$OUT/b.out" 2>&1 &
b_pid=$!
sleep 25
run gate-a "$A_MODEL" "$OUT/prompt-a.txt" >"$OUT/a.out" 2>&1
wait "$b_pid"
got=$(cat "$OUT/got.txt" 2>/dev/null)
tick=$(grep -o 'after tick [0-9]*' <<<"$got" | grep -o '[0-9]*$')
echo "a: $(tail -2 "$OUT/a.out" | tr '\n' ' ' | cut -c1-300)"
echo "b: $(tail -2 "$OUT/b.out" | tr '\n' ' ' | cut -c1-300)"
if [ -z "$got" ]; then echo "FAIL messaging: nothing received"
elif ! grep -q "$TOKEN" <<<"$got"; then echo "FAIL messaging: wrong text: $got"
elif [ -n "$tick" ] && [ "$tick" -lt 8 ]; then echo "PASS messaging mid-turn (arrived after tick $tick of 8)"
else echo "PARTIAL messaging delivered, not mid-turn: $got"; fi
