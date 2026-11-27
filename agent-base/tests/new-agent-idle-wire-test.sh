#!/usr/bin/env bash
# new-agent-idle-wire-test.sh: the real Claude Code, started as roster starts a New agent given no first words (no
# prompt argument; N-1997), sends no request while it waits at its prompt; given first words, its first request's
# last user message is them. Each on a terminal for 20 s (harness-wire/harness-wire-check.sh's TTY_SECONDS: sealed,
# loopback only, a made-up home). Needs the real claude and unshare; else exits 77. OUT keeps the requests.
set -u
here=$(cd "$(dirname "$0")" && pwd)
command -v claude >/dev/null || { echo "new-agent-idle-wire-test: no claude: skipped"; exit 77; }
unshare -rn true 2>/dev/null || { echo "new-agent-idle-wire-test: no network namespace: skipped"; exit 77; }
out=${OUT:-$(mktemp -d /tmp/new-agent-idle.XXXXXX)}; mkdir -p "$out"; out=$(cd "$out" && pwd); echo "new-agent-idle-wire-test: requests kept in $out"
# the first-start screens seen and the folder trusted, as roster's `home` leaves an agent's
jq -n --arg d "$out/run/work" '{hasCompletedOnboarding: true, customApiKeyResponses: {approved: ["stand-in-not-a-key"], rejected: []}, projects: {($d): {hasTrustDialogAccepted: true}}}' >"$out/cfg.json"
fails=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi; }
TTY_SECONDS=20 CFG_JSON=$out/cfg.json bash "$here/harness-wire/harness-wire-check.sh" idle "$out" -- --name try-8 >/dev/null
check "no first words: no request in 20 s at its prompt" '[ -f "$out/idle.jsonl" ] && ! grep -q "/v1/messages" "$out/idle.jsonl"'
check "and it was at its prompt, no screen holding it" 'grep -q "try-8" "$out/idle.out" && grep -q "auto mode on\|bypass permissions on\|for shortcuts" "$out/idle.out"'
TTY_SECONDS=20 CFG_JSON=$out/cfg.json bash "$here/harness-wire/harness-wire-check.sh" words "$out" -- --name try-9 "Read the lexer" >/dev/null
check "first words: its first request's last user message is them" 'python3 -I -c "
import json, sys
r = next(json.loads(l) for l in open(sys.argv[1]) if \"/v1/messages\" in l and \"count_tokens\" not in l)
m = [x for x in json.loads(r[\"body\"])[\"messages\"] if x[\"role\"] == \"user\"][-1][\"content\"]
sys.exit(0 if any(c.get(\"text\") == \"Read the lexer\" for c in (m if isinstance(m, list) else [{\"text\": m}])) else 1)" "$out/words.jsonl"'
echo "new-agent-idle-wire-test: $fails failed"
[ $fails = 0 ]
