#!/usr/bin/env bash
# roster-account-test.sh: a Claude Code agent's start on one of the owner's own accounts (N-412 ask 12; roster's
# `account`, coordd's accounts/mover.rs). roster asks coordd for the account's folder at each start: with one, the
# session gets CLAUDE_CONFIG_DIR and nothing of the model route (no endpoint, no key helper), a settings file of its
# own, and its folder trusted in the account folder's .claude.json; with none, everything as before. With every account
# out a restart is refused before the running session is ended. Stand-ins for coordc and coord-pty record what the
# session would have been started with; no claude is run.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents/orch" "$t/home" "$t/w" "$t/acct/config"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/settings.json" "$here/claude/standing.md" "$here/claude/telemetry-token" "$here/claude/account-key" "$t/ab/claude/"
# coordc: `account-for` answers as $ANSWER says (a folder, nothing, or an error); `run NAME -- CMD` records CMD and the
# variables that matter
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
# up NAME ARGS: the session's program comes up (roster's came_up): coordd has NAME running, and its SessionStart writes
# session.json again, naming the session it resumed
up() { n=$1; shift; s=$COORD/agents/$n/session.json; r=$(printf '%s\n' "$@" | sed -n '/^--resume$/{n;p}'); echo idle >"$COORD/.state"; [ -z "$r" ] || [ ! -f "$s" ] || { jq --arg t "$r" '.session_id = $t | .source = "resume"' "$s" >"$s.up" && mv "$s.up" "$s"; }; }
echo "$1" >>"$CALLS"
case $1 in
ls) printf 'orch\tAGT-001\t%s\towner\n' "$(cat "$COORD/.state" 2>/dev/null || echo stopped)" ;;
hangup) echo stopped >"$COORD/.state" ;;
add) ;;
account-for) case $ANSWER in out) echo "coord account: every account is out; the first is back at 2026-11-18T18:00:00Z, and $2 is started then" >&2; exit 1 ;;
  old) echo "coordd: bad request" >&2; exit 1 ;; *) printf '%s' "$ANSWER" ;; esac ;;
run) n=$2; shift 3; { echo "config=${CLAUDE_CONFIG_DIR-unset}"; echo "url=${ANTHROPIC_BASE_URL-unset}"; echo "logs=${OTEL_LOGS_EXPORTER-unset}"; echo "prompts=${OTEL_LOG_USER_PROMPTS-unset}"; echo "headers=${OTEL_EXPORTER_OTLP_HEADERS-unset}"; printf 'arg=%s\n' "$@"; } >"$RAN"; up "$n" "$@" ;;
*) exit 3 ;;
esac
X
# coord-pty NAME [--confirm TEXT] -- CMD: the terminal's holder; here CMD is recorded, run and waited for
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
while [ "$1" != -- ]; do printf '%s\n' "$1" >>"$PTY.opts"; shift; done; shift
printf '%s\n' "$@" >"$PTY"
exec "$@"
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty"
echo key >"$t/coord/.proxy-key"
printf 'ANTHROPIC_BASE_URL=http://127.0.0.1:8317\n' >"$t/coord/route"
echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w\",\"model\":\"claude-opus-5-5\"}" >"$t/coord/agents/orch/session.json"
echo hello >"$t/prompt.md"; echo "$t/none/project.toml" >"$t/coord/project"
export AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran PTY=$t/pty HOME=$t/home
# the session it restarts has a transcript where Claude Code keeps it (roster's carried: a restart with none is refused)
mkdir -p "$HOME/.claude/projects/$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')"; echo '{}' >"$HOME/.claude/projects/$(printf '%s' "$t/w" | sed 's/[^A-Za-z0-9]/-/g')/s1.jsonl"
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
is() { if eval "$2"; then ok "$1"; else bad "$1 ($2)"; fi; }
settings_of() { sed -n '/^arg=--settings$/{n;s/^arg=//p}' "$RAN"; }

# 1. no account: the model route, as before
ANSWER= "$t/ab/bin/roster" add one claude-opus-5-5 "$t/prompt.md" "$t/w" >/dev/null
is "no account: the route's endpoint and no config folder" 'grep -qx "url=http://127.0.0.1:8317" "$RAN" && grep -qx "config=unset" "$RAN"'
is "no account: the coord's settings, with the key helper" '[ "$(settings_of)" = "$COORD/claude-settings.json" ] && jq -e .apiKeyHelper "$COORD/claude-settings.json" >/dev/null'
is "no account: trusted in the owner's own .claude.json" 'jq -e --arg d "$t/w" ".projects[\$d].hasTrustDialogAccepted" "$HOME/.claude.json" >/dev/null'
cp "$COORD/claude-settings.json" "$t/shared-before"; cp "$HOME/.claude.json" "$t/home-before"

# 2. an account: its folder, nothing of the route, though roster's own environment has an endpoint
ANSWER=$t/acct/config ANTHROPIC_BASE_URL=http://127.0.0.1:8317 "$t/ab/bin/roster" add two claude-opus-5-5 "$t/prompt.md" "$t/w" >/dev/null
is "account: CLAUDE_CONFIG_DIR is the account's folder, and no endpoint" 'grep -qx "config=$t/acct/config" "$RAN" && grep -qx "url=unset" "$RAN"'
s=$(settings_of)
is "account: a settings file of its own, without the key helper and the gateway's models" '[ "$s" = "$COORD/agents/two/claude-settings.json" ] && jq -e "(has(\"apiKeyHelper\") or has(\"modelPicker\") or (.env | has(\"CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY\"))) | not" "$s" >/dev/null'
is "account: the hooks are the same" 'jq -e ".hooks.Stop and .hooks.StopFailure and .hooks.SessionStart" "$s" >/dev/null'
is "account: the status line and the quota notices go to coordd, and only there" 'jq -e ".statusLine.command == \"\\\"\$AGENT_BASE/claude/statusline\\\"\" and (.hooks.Notification[0].hooks[0].command | endswith(\"hook\\\" notification\"))" "$s" >/dev/null && jq -e "(has(\"statusLine\") or (.hooks | has(\"Notification\"))) | not" "$COORD/claude-settings.json" >/dev/null'
is "account: trusted, and the first-start screens seen, in the account folder's .claude.json (0600)" 'jq -e --arg d "$t/w" ".projects[\$d].hasTrustDialogAccepted and .hasCompletedOnboarding and .lspRecommendationDisabled" "$t/acct/config/.claude.json" >/dev/null && [ "$(stat -c %a "$t/acct/config/.claude.json")" = 600 ]'
is "account: the coord's settings and the owner's .claude.json are as they were" 'cmp -s "$COORD/claude-settings.json" "$t/shared-before" && cmp -s "$HOME/.claude.json" "$t/home-before"'

# 3. a restart on an account: asked before the session is ended, resumed on its session under the folder
: >"$CALLS"
ANSWER=$t/acct/config "$t/ab/bin/roster" restart orch >/dev/null
is "restart: the account is asked before the hang-up" '[ "$(grep -n -m1 account-for "$CALLS" | cut -d: -f1)" -lt "$(grep -n -m1 hangup "$CALLS" | cut -d: -f1)" ] && [ "$(grep -c account-for "$CALLS")" = 1 ]'
is "restart: its own session under the account's folder" 'grep -qx "config=$t/acct/config" "$RAN" && grep -qx "arg=--resume" "$RAN" && grep -qx "arg=s1" "$RAN"'

# 3b. coordd's answer carries the session's telemetry environment: set and removed as it says, whatever the starter has
: >"$CALLS"
ANSWER=$(printf '%s\n' "$t/acct/config" OTEL_LOGS_EXPORTER=otlp -OTEL_LOG_USER_PROMPTS) OTEL_LOG_USER_PROMPTS=1 "$t/ab/bin/roster" restart orch >/dev/null
is "telemetry: the folder is the answer's first line" 'grep -qx "config=$t/acct/config" "$RAN"'
is "telemetry: what coordd names is set, and what it removes is not inherited" 'grep -qx "logs=otlp" "$RAN" && grep -qx "prompts=unset" "$RAN"'
ANSWER= OTEL_LOG_USER_PROMPTS=1 "$t/ab/bin/roster" restart orch >/dev/null
is "telemetry: an agent on the route keeps the starter's environment as before" 'grep -qx "logs=unset" "$RAN" && grep -qx "prompts=1" "$RAN"'

# 3c. the coord's telemetry token (a made-up one) reaches the session's environment from its file: the header its
# exporter sends is that and nothing of the starter's, and the token is not in the holder's command line
echo made-up-token >"$COORD/telemetry.token"
ANSWER=$(printf '%s\n' "$t/acct/config" -OTEL_EXPORTER_OTLP_HEADERS) OTEL_EXPORTER_OTLP_HEADERS=x=1 "$t/ab/bin/roster" restart orch >/dev/null
is "token: the session's exporter sends the coord's token" 'grep -qx "headers=x-coord-token=made-up-token" "$RAN"'
is "token: it is not in the holder's command line" '[ -s "$PTY" ] && ! grep -q made-up-token "$PTY"'
ANSWER= OTEL_EXPORTER_OTLP_HEADERS=x=1 "$t/ab/bin/roster" restart orch >/dev/null
is "token: an agent on the route gets none" 'grep -qx "headers=x=1" "$RAN"'
rm "$COORD/telemetry.token"

# 3d. an account that runs on an API key (a made-up one): coordd's answer names the key's file, and the session's own
# settings ask the helper for it with that file; the key is in no settings file, environment or command line, and the
# next start on a sign-in account has no helper again
mkdir -p "$t/acct k"; (umask 077; echo made-up-key >"$t/acct k/key")
ANSWER=$(printf '%s\n' "$t/acct/config" "@key $t/acct k/key") "$t/ab/bin/roster" restart orch >/dev/null
s=$(settings_of)
is "key: the session's settings ask the helper, with the key's file" '[ "$(jq -r .apiKeyHelper "$s")" = "\"\$AGENT_BASE/claude/account-key\" '"'"'$t/acct k/key'"'"'" ]'
is "key: the helper's command gives the key, as Claude Code runs it" '[ "$(sh -c "$(jq -r .apiKeyHelper "$s")")" = made-up-key ]'
is "key: it is in no settings file, environment or command line" '! grep -q made-up-key "$s" "$RAN" "$PTY"'
is "key: nothing of the route all the same" 'grep -qx "url=unset" "$RAN" && jq -e "has(\"modelPicker\") | not" "$s" >/dev/null'
chmod 644 "$t/acct k/key"
is "key: a file others can read is not given" '! out=$(sh -c "$(jq -r .apiKeyHelper "$s")" 2>&1) && case $out in *"not the owner'"'"'s alone"*) true ;; *) false ;; esac && case $out in *made-up-key*) false ;; *) true ;; esac'
ANSWER=$t/acct/config "$t/ab/bin/roster" restart orch >/dev/null
is "key: the next start on a sign-in account has no helper" 'jq -e "has(\"apiKeyHelper\") | not" "$(settings_of)" >/dev/null'

# 4. every account out: refused in coordd's words, and the running session is not ended
: >"$CALLS"; rm -f "$RAN"
if out=$(ANSWER=out "$t/ab/bin/roster" restart orch 2>&1); then bad "all out: the restart went through"; else
  is "all out: the restart is refused with until when" 'case $out in *"every account is out"*) true ;; *) false ;; esac'
fi
is "all out: nothing was hung up and nothing started" '! grep -q hangup "$CALLS" && [ ! -e "$RAN" ]'

# 4b. every account out in a coord with the gateway: coordd answers no folder (an empty first line) and the lines that
# go on with a cut turn or keep one from being run again. The session starts through the route, with those lines
: >"$CALLS"; rm -f "$RAN"
ANSWER=$(printf '%s\n' '' OTEL_LOGS_EXPORTER=otlp -OTEL_LOG_USER_PROMPTS) OTEL_LOG_USER_PROMPTS=1 "$t/ab/bin/roster" restart orch >/dev/null
is "through the gateway meanwhile: the route's endpoint, no config folder, the coord's settings with the key helper" 'grep -qx "url=http://127.0.0.1:8317" "$RAN" && grep -qx "config=unset" "$RAN" && [ "$(settings_of)" = "$COORD/claude-settings.json" ] && jq -e .apiKeyHelper "$COORD/claude-settings.json" >/dev/null'
is "through the gateway meanwhile: what coordd names is set, and what it removes is not inherited" 'grep -qx "logs=otlp" "$RAN" && grep -qx "prompts=unset" "$RAN"'
is "through the gateway meanwhile: its own session" 'grep -qx "arg=--resume" "$RAN" && grep -qx "arg=s1" "$RAN"'

# 5. a coordd from before accounts refuses the request in other words: the route, as before
ANSWER=old "$t/ab/bin/roster" add five claude-opus-5-5 "$t/prompt.md" "$t/w" >/dev/null 2>&1
is "an older coordd: the route" 'grep -qx "url=http://127.0.0.1:8317" "$RAN" && grep -qx "config=unset" "$RAN"'

# 6. roster run by an agent that is itself on an account: the agent it starts gets none of it
ANSWER= CLAUDE_CONFIG_DIR=/elsewhere "$t/ab/bin/roster" add six claude-opus-5-5 "$t/prompt.md" "$t/w" >/dev/null
is "a starter's own account folder does not reach the agent it starts" 'grep -qx "config=unset" "$RAN"'

# 7. the agents moved onto one account start together (38 within seconds on 20 Nov): every start's folder is trusted,
# in one valid file. roster's write is under one lock; without it, two starts that each read the file before the other
# wrote it leave only one of the two folders trusted, and the other session stops at "Do you trust this folder?"
rm -f "$t/acct/config/.claude.json"
for i in 1 2 3 4 5 6 7 8; do mkdir -p "$t/w7/$i"; done
for i in 1 2 3 4 5 6 7 8; do ANSWER=$t/acct/config RAN=$t/ran7.$i PTY=$t/pty7.$i "$t/ab/bin/roster" add seven$i claude-opus-5-5 "$t/prompt.md" "$t/w7/$i" >/dev/null 2>&1 & done
wait
n=0
for i in 1 2 3 4 5 6 7 8; do jq -e --arg d "$t/w7/$i" '.projects[$d].hasTrustDialogAccepted' "$t/acct/config/.claude.json" >/dev/null 2>&1 && n=$((n + 1)); done
is "together: eight starts on one account at once leave all eight folders trusted, in one valid file" '[ "$n" = 8 ]'
is "together: no copy of the file and no lock is left beside it" '[ -z "$(find "$t/acct/config" -maxdepth 1 -name ".claude.json.roster.*" -o -maxdepth 1 -name ".claude.json.lock")" ]'

# 8. the write waits for Claude Code's own lock on the file (the folder .claude.json.lock, which a running session makes
# while it writes the file and refreshes while it holds it), and takes away one left untouched for 30 s
mkdir "$t/acct/config/.claude.json.lock"; mkdir -p "$t/w8"
ANSWER=$t/acct/config RAN=$t/ran8 PTY=$t/pty8 "$t/ab/bin/roster" add eight claude-opus-5-5 "$t/prompt.md" "$t/w8" >/dev/null 2>&1 &
sleep 0.5
is "a held lock: the folder is not written while a session holds it" '! jq -e --arg d "$t/w8" ".projects[\$d]" "$t/acct/config/.claude.json" >/dev/null'
rmdir "$t/acct/config/.claude.json.lock"; wait
is "a held lock: written once it is let go, and the lock is not left" 'jq -e --arg d "$t/w8" ".projects[\$d].hasTrustDialogAccepted" "$t/acct/config/.claude.json" >/dev/null && [ ! -e "$t/acct/config/.claude.json.lock" ]'
mkdir "$t/acct/config/.claude.json.lock"; touch -d '-40 seconds' "$t/acct/config/.claude.json.lock"; mkdir -p "$t/w9"
ANSWER=$t/acct/config RAN=$t/ran9 PTY=$t/pty9 "$t/ab/bin/roster" add nine claude-opus-5-5 "$t/prompt.md" "$t/w9" >/dev/null 2>&1
is "a lock left untouched for 30 s is taken away, and the folder written" 'jq -e --arg d "$t/w9" ".projects[\$d].hasTrustDialogAccepted" "$t/acct/config/.claude.json" >/dev/null && [ ! -e "$t/acct/config/.claude.json.lock" ]'
# an account folder that is gone: no lock can be made there, so nothing is waited for; said, and not started
mkdir -p "$t/w10"
ANSWER=$t/acct/gone RAN=$t/ran10 PTY=$t/pty10 timeout 10 "$t/ab/bin/roster" add ten claude-opus-5-5 "$t/prompt.md" "$t/w10" >/dev/null 2>"$t/err10"; echo "rc=$?" >>"$t/err10"
is "an account folder that is gone: not waited for (no timeout), said, and not started" '! grep -q "^rc=124" "$t/err10" && grep -q "ten: the lock on $t/acct/gone/.claude.json cannot be taken" "$t/err10" && [ ! -e "$t/pty10" ] && [ ! -e "$t/acct/gone" ]'

# 9. the project's own Claude Code folder, $COORD/claude-home (new-project makes it; roster's own_home, paper-research
# N-1446 p3051): its folder, with the route as ever; trusted and the first-start screens seen there; nothing of the
# owner's written. With an account, the account's folder. With neither a route nor an account: said, and not started.
mkdir -m 700 "$t/coord/claude-home"; mkdir -p "$t/w11"; cp "$HOME/.claude.json" "$t/home-before9"
ANSWER= RAN=$t/ran11 PTY=$t/pty11 "$t/ab/bin/roster" add eleven claude-opus-5-5 "$t/prompt.md" "$t/w11" >/dev/null
is "own folder: CLAUDE_CONFIG_DIR is the project's claude-home, with the route's endpoint and the key helper" 'grep -qx "config=$t/coord/claude-home" "$t/ran11" && grep -qx "url=http://127.0.0.1:8317" "$t/ran11" && [ "$(RAN=$t/ran11 settings_of)" = "$COORD/claude-settings.json" ] && jq -e .apiKeyHelper "$COORD/claude-settings.json" >/dev/null'
is "own folder: trusted, and the first-start screens seen, in its .claude.json (0600)" 'jq -e --arg d "$t/w11" ".projects[\$d].hasTrustDialogAccepted and .hasCompletedOnboarding and .lspRecommendationDisabled" "$t/coord/claude-home/.claude.json" >/dev/null && [ "$(stat -c %a "$t/coord/claude-home/.claude.json")" = 600 ]'
is "own folder: the owner's .claude.json is as it was" 'cmp -s "$HOME/.claude.json" "$t/home-before9"'
mkdir -p "$t/w12"
ANSWER=$t/acct/config RAN=$t/ran12 PTY=$t/pty12 "$t/ab/bin/roster" add twelve claude-opus-5-5 "$t/prompt.md" "$t/w12" >/dev/null
is "own folder and an account: the account's folder" 'grep -qx "config=$t/acct/config" "$t/ran12"'
mv "$t/coord/.proxy-key" "$t/proxy-key-aside"; mkdir -p "$t/w13"
ANSWER= RAN=$t/ran13 PTY=$t/pty13 "$t/ab/bin/roster" add thirteen claude-opus-5-5 "$t/prompt.md" "$t/w13" >/dev/null 2>"$t/err13" || true
is "own folder, no route and no account: said, and not started (never the owner's login)" 'grep -qx "thirteen: this project has no account yet: add one under Accounts" "$t/err13" && [ ! -e "$t/pty13" ] && [ ! -e "$t/ran13" ] && ! jq -e --arg d "$t/w13" ".projects[\$d]" "$HOME/.claude.json" >/dev/null'
rmdir "$t/coord/claude-home" 2>/dev/null || rm -rf "$t/coord/claude-home"; mkdir -p "$t/w14"
ANSWER= RAN=$t/ran14 PTY=$t/pty14 "$t/ab/bin/roster" add fourteen claude-opus-5-5 "$t/prompt.md" "$t/w14" >/dev/null
is "no own folder and no route: as before (no config folder)" 'grep -qx "config=unset" "$t/ran14"'
mv "$t/proxy-key-aside" "$t/coord/.proxy-key"

# 10. the project's repositories are the session's working folders (the owner's walk 13; paper-research N-1446 p3535):
# one --add-dir each, in the file's order, ~/ and a path beside the file taken as coordd takes them; one that is not
# there, and the start folder itself, left out; with no project file, none
adds() { sed -n '/^arg=--add-dir$/{n;s/^arg=//p}' "$1" | paste -sd' '; }
is "no project file: no added folder" '[ -z "$(adds "$t/ran14")" ]'
mkdir -p "$t/def" "$t/home/code/second" "$t/def/beside" "$t/w15"
printf '[project]\nrepos = { first = "%s", second = "~/code/second", gone = "%s", beside = "beside", start = "%s" }\n' "$t/w11" "$t/nothing" "$t/w15" >"$t/def/project.toml"
echo "$t/def/project.toml" >"$t/coord/project"
ANSWER= RAN=$t/ran15 PTY=$t/pty15 "$t/ab/bin/roster" add fifteen claude-opus-5-5 "$t/prompt.md" "$t/w15" >/dev/null
is "repositories: one --add-dir each that is there, in the file's order, not the start folder" '[ "$(adds "$t/ran15")" = "$t/w11 $t/home/code/second $t/def/beside" ]'
# each trusted before the start, beside the start folder, and no other folder (N-1863: Claude Code's "Adding a new
# directory" popup for a working folder with no trust of its own); the terminal's holder answers Yes for these alone
trusted() { jq -r --arg t "$t" '.projects | keys[] | select(startswith($t))' "$1" | sort | paste -sd' '; }
is "repositories: trusted in .claude.json with the start folder, the one not there and the project's own folder not" '[ "$(trusted "$HOME/.claude.json")" = "$(printf "%s\n" "$t/def/beside" "$t/home/code/second" "$t/w" "$t/w11" "$t/w14" "$t/w15" | sort | paste -sd" ")" ]'
is "repositories: the holder told to trust the start folder and them, nothing else" '[ "$(sed -n "/^--trust$/{n;p}" "$t/pty15.opts" | paste -sd" ")" = "$t/w15 $t/w11 $t/home/code/second $t/def/beside" ]'
is "repositories: the holder told the agent's home, for a popup's ~" '[ "$(sed -n "/^--home$/{n;p}" "$t/pty15.opts")" = "$HOME" ]'
: >"$CALLS"
echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w15\",\"model\":\"claude-opus-5-5\"}" >"$t/coord/agents/orch/session.json"
ANSWER= RAN=$t/ran16 PTY=$t/pty16 "$t/ab/bin/roster" restart orch >/dev/null
is "repositories: a restart gets them too" '[ "$(adds "$t/ran16")" = "$t/w11 $t/home/code/second $t/def/beside" ] && grep -qx "arg=--resume" "$t/ran16"'
echo "$t/none/project.toml" >"$t/coord/project"
[ "$fails" = 0 ]
