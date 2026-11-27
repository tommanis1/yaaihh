#!/bin/bash
# clerk-model's test: stand-in `claude` and Codex in a scratch coord of its own, never a harness or route of his.
set -uo pipefail
here=$(cd "$(dirname "$0")/../.." && pwd); t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
fails=0; ok() { echo "ok: $1"; }; no() { echo "FAIL: $1"; fails=$((fails + 1)); }
R=$t/rel C=$t/coord; mkdir -p "$R" "$C/agents"/{w1,w2,w3} "$C/claude-home" "$C/codex-home" "$C/opt/codex/bin" "$t/bin"
echo 'model_provider = "route"' > "$C/codex-home/config.toml"; echo '{}' > "$C/codex-home/auth.json"
ln -s "$here/agent-base" "$R/agent-base"
echo '{"kind": "claude", "model": "claude-x:high"}' > "$C/agents/w1/session.json"
echo '{"kind": "codex", "model": "gpt-y:medium"}' > "$C/agents/w2/session.json"
echo '{"kind": "agy"}' > "$C/agents/w3/session.json"
echo 'ANTHROPIC_BASE_URL = http://127.0.0.1:9' > "$C/route"; echo k1 > "$C/.proxy-key"
cat > "$t/bin/claude" <<'EOF'
#!/bin/bash
{ printf '%s\n' "$@"; echo "url=${ANTHROPIC_BASE_URL:-} home=${CLAUDE_CONFIG_DIR:-} coord=${COORD:-}"; echo "in=$(cat)"; echo "at=$PWD $(ls -A | wc -l)"; } > "$SEEN"
grep -qx in=fail "$SEEN" && { echo '{"type": "result", "is_error": true, "result": "the route said no"}'; exit 0; }
echo '{"type": "result", "is_error": false, "result": "the claude answer", "usage": {"input_tokens": 3, "cache_creation_input_tokens": 4, "cache_read_input_tokens": 5, "output_tokens": 6}}'
EOF
cat > "$C/opt/codex/bin/codex" <<'EOF'
#!/bin/bash
{ printf '%s\n' "$@"; echo "key=${CPA_API_KEY:-} files=$(ls "$CODEX_HOME" | tr '\n' ' ')"; echo "in=$(cat)"; echo "at=$PWD home=$CODEX_HOME"; } > "$SEEN"
while [ $# -gt 0 ]; do [ "$1" = -o ] && printf 'the codex answer' > "$2"; shift; done
echo '{"type": "thread.started"}'
grep -qx in=nocount "$SEEN" || echo '{"type": "turn.completed", "usage": {"input_tokens": 30, "cached_input_tokens": 10, "output_tokens": 5}}'
EOF
chmod +x "$t/bin/claude" "$C/opt/codex/bin/codex"
m() { SEEN=$t/seen SPAWN_COORD=$C SPAWN_PROGS=$R PATH="$t/bin:$PATH" CODEX_HOME=$t/elsewhere CODEX_KEY_FILE=$t/elsewhere/key bash "$here/ops/release/clerk-model" "$@"; }
gone() { local d; d=$(sed -n 's/^at=\([^ ]*\).*/\1/p' "$t/seen"); [ -n "$d" ] && [ ! -e "$d" ]; }

out=$(echo "Mark the map." | m w1)
[ "$out" = '{"answer":"the claude answer","tokens":18}' ] && ok "claude: its answer and every token it counted" || no "claude printed '$out'"
grep -qx -- '-p' "$t/seen" && grep -qx -- 'claude-x' "$t/seen" && ! grep -q 'high' "$t/seen" && ok "claude: print mode, its session's model, no effort" || no "claude's args: $(cat "$t/seen")"
grep -qx -- '--tools' "$t/seen" && grep -qx '' "$t/seen" && grep -qx -- '--no-session-persistence' "$t/seen" && ok "claude: no tool, no session kept" || no "claude's tools: $(cat "$t/seen")"
grep -qx -- --bare "$t/seen" && grep -qxF '{"apiKeyHelper":"cat \"$COORD/.proxy-key\""}' "$t/seen" && ok "claude: bare, the key through apiKeyHelper only" || no "claude's settings: $(cat "$t/seen")"
grep -qx 'at=.* 0' "$t/seen" && gone && ok "claude: in an empty folder of its own, gone after" || no "claude's folder: $(grep at= "$t/seen")"
grep -qx "url=http://127.0.0.1:9 home=$C/claude-home coord=$C" "$t/seen" && ok "claude: the route's endpoint and the project's claude-home" || no "claude's env: $(grep url= "$t/seen")"
grep -qx 'in=Mark the map.' "$t/seen" && ok "claude: the prompt on its stdin" || no "claude's stdin: $(grep in= "$t/seen")"
err=$(echo fail | m w1 2>&1 >/dev/null); rc=$?; [ $rc = 1 ] && [ "$err" = "clerk-model: w1's model: the route said no" ] && ok "claude: an error it answers fails the call, with its words" || no "a failing claude: rc $rc, '$err'"

out=$(echo "And this." | m w2)
[ "$out" = '{"answer":"the codex answer","tokens":35}' ] && ok "codex: its last message and its count" || no "codex printed '$out'"
grep -qx exec "$t/seen" && grep -qx gpt-y "$t/seen" && grep -qx read-only "$t/seen" && grep -qx -- --ephemeral "$t/seen" && grep -qx -- --json "$t/seen" && ok "codex: exec, its session's model, read-only, nothing kept" || no "codex's args: $(cat "$t/seen")"
grep -qx "key=k1 files=config.toml " "$t/seen" && grep -qx 'in=And this.' "$t/seen" && ok "codex: a home of its own with the project's config, the route's key, no login" || no "codex's env: $(tail -3 "$t/seen")"
grep -qx "at=\(.*\)/in home=\1/home" "$t/seen" && gone && ok "codex: its home in its own folder, gone after" || no "codex's folder: $(grep at= "$t/seen")"
err=$(echo nocount | m w2 2>&1 >/dev/null); rc=$?; [ $rc = 1 ] && [[ $err == *"with no count of its tokens"* ]] && ok "codex: an answer with no count fails, never costs nothing" || no "codex with no count: rc $rc, '$err'"

rm "$C/.proxy-key" "$t/seen"; err=$(echo x | m w1 2>&1 >/dev/null); rc=$?
[ $rc = 1 ] && [[ $err == *"no route for the clerk's call"* ]] && [ ! -e "$t/seen" ] && ok "no route key: no call at all, and no login tried" || no "no key: rc $rc, '$err'"
echo k1 > "$C/.proxy-key"
err=$(echo x | m w3 2>&1 >/dev/null); [ $? -ne 0 ] && [[ $err == *"no print mode for w3's harness agy"* ]] && ok "a harness with no print mode fails, saying so" || no "agy: '$err'"
err=$(echo x | m w9 2>&1 >/dev/null); [ $? -ne 0 ] && [[ $err == *"w9 has no session"* ]] && ok "a party with no session fails, saying so" || no "w9: '$err'"
err=$(echo x | m "../w1" 2>&1 >/dev/null); [ $? -ne 0 ] && [[ $err == *"is no agent's name"* ]] && ok "a name that is a path is refused" || no "../w1: '$err'"
[ $fails = 0 ] && echo "clerk-model-test: all held" || { echo "clerk-model-test: $fails failed"; exit 1; }
