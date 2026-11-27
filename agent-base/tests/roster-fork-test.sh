#!/usr/bin/env bash
# roster-fork-test.sh: `roster fork NAME` and `roster ask NAME TEXT` (the owner's spin-off, his walk item 19, N-1794).
# A fork must find NAME's session where its own Claude Code looks, or it starts an empty one: a fresh agent under a
# fork's name. So NAME's transcript is put under the fork's folder's projects entry for the folder it runs in (its
# account's folder, else the project's claude-home, else ~/.claude), nothing copied when it is there already, and no
# agent started when NAME has no transcript. The fork is registered as a copy of NAME (`coord add ... --from NAME`).
# A --cut fork (a busy agent's, cut where its calls are answered) puts its cut copy there the same way. `roster ask`
# runs one `claude -p` on a copy of NAME's session in its folder, with no hooks, and prints the answer. Stand-ins for
# coordc, coord-pty and claude record what would have run; no claude is run.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents/orch" "$t/home" "$t/w" "$t/sessions" "$t/fake"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/settings.json" "$here/claude/standing.md" "$t/ab/claude/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$*" >>"$CALLS"
case $1 in
ls) printf 'orch\tAGT-001\tidle\towner\n' ;;
add | hangup | start) ;;
account-for) [ ! -s "$ACCOUNT" ] || cat "$ACCOUNT" ;;
run) shift 3; printf 'arg=%s\n' "$@" >"$RAN" ;;
*) exit 3 ;;
esac
X
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
while [ "$1" != -- ]; do shift; done; shift
exec "$@"
X
cat >"$t/fake/claude" <<'X'
#!/bin/sh
{ pwd; printf 'arg=%s\n' "$@"; env | grep -E '^(AGENT_NAME|CLAUDE_CONFIG_DIR)=' || true; } >"$ASKED"
for a in "$@"; do case $a in */claude-settings.ask.json) cp "$a" "$ASKED.settings" ;; esac; done
echo "the fix is in parser.rs:40, untested"
X
printf '#!/bin/sh\nexec "$@"\n' >"$t/ab/claude/telemetry-token"
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty" "$t/fake/claude" "$t/ab/claude/telemetry-token"
echo "$t/none/project.toml" >"$t/coord/project"
f=$t/sessions/s1.jsonl
printf '%s\n' '{"type":"user","message":{"role":"user","content":"the work"},"uuid":"u0","sessionId":"s1"}' \
  '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]},"uuid":"u1","sessionId":"s1"}' >"$f"
session() { echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w\",\"model\":\"claude-opus-5-5\",\"kind\":\"claude\"${1:+,\"transcript_path\":\"$1\"}}" >"$t/coord/agents/orch/session.json"; }
session "$f"
entry=$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')
export AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran HOME=$t/home ACCOUNT=$t/account ASKED=$t/asked PATH=$t/fake:$PATH
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
is() { if eval "$2"; then ok "$1"; else bad "$1 ($2)"; fi; }
fork() { : >"$CALLS"; rm -f "$RAN"; "$t/ab/bin/roster" fork orch "$@"; }

# 1. no account, no claude-home: the transcript goes under ~/.claude, the fork resumes it, registered as orch's copy
out=$(fork --name a1)
is "the transcript put where the fork's Claude Code finds it (~/.claude)" 'cmp -s "$f" "$t/home/.claude/projects/$entry/s1.jsonl"'
is "registered as a copy of orch" 'grep -qx "add a1 claude claude-opus-5-5 --from orch" "$CALLS"'
is "resumed on orch's session, forked" '[ "$(grep -A1 -x -- "arg=--resume" "$RAN" | tail -1)" = "arg=s1" ] && grep -qx -- "arg=--fork-session" "$RAN"'
is "it says so" '[ "$out" = "a1: forked from orch (session s1)" ]'

# 2. on an account whose folder links no projects: there, not ~/.claude
mkdir -p "$t/acct"; echo "$t/acct" >"$ACCOUNT"
fork --name a2 >/dev/null
is "on an account: under the account's folder" 'cmp -s "$f" "$t/acct/projects/$entry/s1.jsonl"'
rm -f "$ACCOUNT"

# 3. already where it is looked for (the projects entry is a link to where it is): nothing copied
rm -rf "$t/home/.claude/projects/$entry"; ln -s "$t/sessions" "$t/home/.claude/projects/$entry"
fork --name a3 >/dev/null
is "already there: nothing copied, the one file" '[ ! -e "$t/sessions/s1.jsonl.fork" ] && [ "$(ls "$t/sessions")" = s1.jsonl ] && grep -qx -- "arg=--fork-session" "$RAN"'
rm "$t/home/.claude/projects/$entry"

# 4. no transcript: refused, nothing started
session "$t/sessions/gone.jsonl"
rc=0; err=$(fork --name a4 2>&1 >/dev/null) || rc=$?
is "no transcript: refused, no agent started" '[ "$rc" != 0 ] && [[ $err == *"has no transcript here"* ]] && [ ! -e "$RAN" ]'
session "$f"

# 5. a busy agent's fork, cut where its calls are answered: the cut copy is what is put there and resumed
upto=$(head -n 1 "$f" | wc -c)
fork --cut "$f" "$upto" --name a5 >/dev/null
id=$(grep -A1 -x -- "arg=--resume" "$RAN" | tail -1); id=${id#arg=}
is "cut: a copy of the first line, under ~/.claude, resumed" '[ "$id" != s1 ] && [ "$(wc -l <"$t/home/.claude/projects/$entry/$id.jsonl")" = 1 ] && grep -q "\"sessionId\":\"$id\"" "$t/home/.claude/projects/$entry/$id.jsonl"'

# 6. roster ask: one claude -p on a copy of orch's session, in its folder, no hooks, the answer printed
: >"$CALLS"; rm -f "$RAN"
out=$("$t/ab/bin/roster" ask orch "What did you find?")
is "ask: the answer printed" '[ "$out" = "the fix is in parser.rs:40, untested" ]'
is "ask: claude -p on a copy of its session, in its folder" '[ "$(head -n 1 "$ASKED")" = "$t/w" ] && grep -qx -- "arg=-p" "$ASKED" && [ "$(grep -A1 -x -- "arg=--resume" "$ASKED" | tail -1)" = "arg=s1" ] && grep -qx -- "arg=--fork-session" "$ASKED" && grep -qx "arg=What did you find?" "$ASKED"'
is "ask: no hooks, no status line, no terminal, not as the agent" '[ "$(jq "has(\"hooks\") or has(\"statusLine\")" "$ASKED.settings")" = false ] && [ ! -e "$RAN" ] && ! grep -q "^AGENT_NAME=" "$ASKED"'

[ "$fails" = 0 ] && echo "roster-fork-test: all ok" || { echo "roster-fork-test: $fails failed"; exit 1; }
