#!/usr/bin/env bash
# roster-cut-test.sh: `roster restart NAME --cut FILE BYTES`, the restart coordd runs for a `fork` (part 4, docket-core
# N-1446 p2096): NAME goes on under its own name from a copy of its session, FILE's first BYTES bytes, made beside FILE
# as a new session (every line's sessionId its own, 0600), and the old session stays as it was; its start's record line
# says phase fork, from the old session, cut after BYTES of it, on the new one. Refused, with nothing hung up and no copy made, when the cut
# cannot be made: BYTES not at a line's end or past the file, FILE not NAME's own session's, a Codex session, a boxed
# project. A restart without --cut is as before. Stand-ins for coordc and coord-pty record what the session would have
# been started with (as roster-account-test.sh); no claude is run.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/ab/claude" "$t/coord/agents/orch" "$t/home" "$t/w" "$t/projects/-w"
cp "$here/bin/roster" "$here/bin/harness-settings" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"; cp "$here/claude/settings.json" "$here/claude/standing.md" "$t/ab/claude/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
# up NAME ARGS: the session's program comes up (roster's came_up): coordd has NAME running, and its SessionStart writes
# session.json again, naming the session it resumed
up() { n=$1; shift; s=$COORD/agents/$n/session.json; r=$(printf '%s\n' "$@" | sed -n '/^--resume$/{n;p}'); echo idle >"$COORD/.state"; [ -z "$r" ] || [ ! -f "$s" ] || { jq --arg t "$r" '.session_id = $t | .source = "resume"' "$s" >"$s.up" && mv "$s.up" "$s"; }; }
echo "$1" >>"$CALLS"
case $1 in
ls) printf 'orch\tAGT-001\t%s\towner\n' "$(cat "$COORD/.state" 2>/dev/null || echo stopped)" ;;
hangup) echo stopped >"$COORD/.state" ;;
add | start) ;;
account-for) ;;
run) n=$2; shift 3; printf 'arg=%s\n' "$@" >"$RAN"; up "$n" "$@" ;;
*) exit 3 ;;
esac
X
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
while [ "$1" != -- ]; do shift; done; shift
exec "$@"
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty"
echo "$t/none/project.toml" >"$t/coord/project"
# as a session's own record has it: where its transcript is (a fork puts it where its own Claude Code looks, N-1794)
session() { echo "{\"session_id\":\"s1\",\"cwd\":\"$t/w\",\"model\":\"claude-opus-5-5\",\"kind\":\"$1\",\"transcript_path\":\"$t/projects/-w/s1.jsonl\"}" >"$t/coord/agents/orch/session.json"; }
session claude
f=$t/projects/-w/s1.jsonl
printf '%s\n' '{"type":"user","message":{"role":"user","content":"before"},"uuid":"u0","sessionId":"s1"}' \
  '{"type":"user","isMeta":true,"message":{"role":"user","content":"<channel source=\"coord\" id=\"N-1.p4\">the work</channel>"},"uuid":"u1","sessionId":"s1"}' \
  '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]},"uuid":"u2","sessionId":"s1"}' >"$f"
cp "$f" "$t/s1.before"
upto=$(head -n 2 "$f" | wc -c)
export AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls RAN=$t/ran HOME=$t/home
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
is() { if eval "$2"; then ok "$1"; else bad "$1 ($2)"; fi; }
copies() { find "$t/projects/-w" -name '*.jsonl*' ! -name s1.jsonl | sort; }

# 1. the cut: a copy of the first two lines as a new session, resumed under its own name
: >"$CALLS"; rm -f "$t/ran"
out=$("$t/ab/bin/roster" restart orch --cut "$f" "$upto")
new=$(copies); id=${new##*/}; id=${id%.jsonl}
is "one copy beside the transcript, a session id of its own" '[ "$(copies | wc -l)" = 1 ] && [[ $id =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]'
is "the copy is the first BYTES bytes, each line's sessionId its own" '[ "$(head -c "$upto" "$f" | sed "s/\"sessionId\":\"s1\"/\"sessionId\":\"$id\"/")" = "$(cat "$new")" ] && [ "$(grep -c "\"sessionId\":\"$id\"" "$new")" = 2 ] && ! grep -q "\"sessionId\":\"s1\"" "$new"'
is "the copy is the owner's alone (0600)" '[ "$(stat -c %a "$new")" = 600 ]'
is "the old session stays as it was" 'cmp -s "$f" "$t/s1.before"'
is "resumed on the copy" 'grep -qx -- "arg=--resume" "$RAN" && [ "$(grep -A1 -x -- "arg=--resume" "$RAN" | tail -1)" = "arg=$id" ]'
is "its record line: phase fork, from the old session, cut after BYTES of it, on the new one" 'grep -qx "arg=phase=fork" "$RAN" && grep -qx "arg=from=s1" "$RAN" && grep -qx "arg=cut=$upto" "$RAN" && grep -qx "arg=session=$id" "$RAN"'
is "its old session was hung up" 'grep -qx hangup "$CALLS"'
is "it says so" '[ "$out" = "orch: restarted on session $id, session s1 cut after $upto bytes" ]'
rm -f "$new"
# its SessionStart named the copy (as coordd writes it); the cases below restart s1 again
session claude

# 2. refused before anything is hung up, no copy made
refused() {  # refused WHAT WORDS ARGS...: roster restart orch ARGS fails, saying WORDS, with no hang-up and no copy
  local what=$1 words=$2 err rc=0; shift 2
  : >"$CALLS"
  err=$("$t/ab/bin/roster" restart orch "$@" 2>&1 >/dev/null) || rc=$?
  if [ "$rc" != 0 ] && [[ $err == *"$words"* ]] && ! grep -qx hangup "$CALLS" && ! grep -qx start "$CALLS" && [ -z "$(copies)" ]; then ok "refused: $what"
  else bad "refused: $what (rc $rc: $err; calls $(tr '\n' ' ' <"$CALLS"); copies $(copies))"; fi
}
refused "a cut inside a line" "not the end of a line" --cut "$f" $((upto - 1))
refused "a cut past the file" "past the" --cut "$f" $(($(stat -c %s "$f") + 1))
refused "no number" "not a number" --cut "$f" 12b
mkdir -p "$t/other"; cp "$f" "$t/other/s2.jsonl"
refused "another session's transcript" "is not session s1's transcript" --cut "$t/other/s2.jsonl" "$upto"
session codex
refused "a Codex session" "a codex session is not restarted on a cut copy" --cut "$f" "$upto"
session claude
touch "$t/coord/boxed"
refused "a boxed project" "a boxed agent is not restarted on a cut copy" --cut "$f" "$upto"
rm "$t/coord/boxed"
refused "an unknown word" "restart orch: --cutt?" --cutt "$f" "$upto"

# 3. without --cut, as before; --force still taken
: >"$CALLS"
"$t/ab/bin/roster" restart orch --force >/dev/null
is "a restart without --cut resumes its own session" '[ "$(grep -A1 -x -- "arg=--resume" "$RAN" | tail -1)" = "arg=s1" ] && grep -qx "arg=phase=restart" "$RAN" && grep -qx "arg=from=" "$RAN" && grep -qx "arg=cut=" "$RAN" && [ -z "$(copies)" ]'

# 4. --system (coordd's SystemPrompt, accounts-lead N-1446 p2447): a version of its own system prompt, appended or in
# place of Claude Code's, named for every later start; with --cut on the cut copy; refused like --cut
# system_sha: the sha256 of the text given as its system prompt, as prompt_sha is made (docket-edge6 N-1446 p2529)
sha() { printf '%s' "$1" | sha256sum | cut -c1-64; }
# given FLAG: the text of the file the last start gave Claude Code after FLAG (--system-prompt-file,
# --append-system-prompt-file): the text goes as a file, never as one argument, so Linux's 128 KiB bounds none
given() { local f; f=$(grep -A1 -x -- "arg=$1" "$RAN" | tail -1); f=${f#arg=}; [ -n "$f" ] && [ -f "$f" ] && cat "$f"; }
[ "$(given --append-system-prompt-file)" = "$(cat "$t/ab/claude/standing.md")" ] && grep -qx "arg=system=" "$RAN" && grep -qx "arg=system_sha=$(sha "$(cat "$t/ab/claude/standing.md")")" "$RAN" \
  && ok "with no version of its own: the standing text appended, its system field empty, its system_sha the standing text's" || bad "standing (ran: $(tr '\n' ' ' <"$RAN"))"
sp=$t/coord/prompts/orch.system; mkdir -p "$sp"; echo "Be brief." >"$sp/1.md"; echo "You are only a calculator." >"$sp/2.md"
: >"$CALLS"
"$t/ab/bin/roster" restart orch --system 1 append >/dev/null
is "--system 1 append: its text appended, from a file" '[ "$(given --append-system-prompt-file)" = "Be brief." ] && ! grep -qx -- "arg=--system-prompt-file" "$RAN" && ! grep -qx -- "arg=Be brief." "$RAN"'
is "its record line: phase system, version 1, its text's system_sha, its own session" 'grep -qx "arg=phase=system" "$RAN" && grep -qx "arg=system=1" "$RAN" && grep -qx "arg=replace=" "$RAN" && grep -qx "arg=system_sha=$(sha "Be brief.")" "$RAN" && [ "$(grep -A1 -x -- "arg=--resume" "$RAN" | tail -1)" = "arg=s1" ]'
is "current names version 1" '[ "$(cat "$sp/current")" = "1 append" ]'
# the resumed session takes version 1, not the text Claude Code recorded on its first request (roster's system_prompt)
is "a resumed start rebuilds its system prompt: --system-prompt-snapshot off" '[ "$(grep -A1 -x -- "arg=--system-prompt-snapshot" "$RAN" | tail -1)" = "arg=off" ]'
"$t/ab/bin/roster" restart orch --system 2 replace >/dev/null
is "--system 2 replace: in place of Claude Code's own, from a file" '[ "$(given --system-prompt-file)" = "You are only a calculator." ] && ! grep -qx -- "arg=--append-system-prompt-file" "$RAN" && grep -qx "arg=replace=1" "$RAN" && grep -qx "arg=system_sha=$(sha "You are only a calculator.")" "$RAN"'
"$t/ab/bin/roster" restart orch >/dev/null
is "a later restart keeps version 2" '[ "$(given --system-prompt-file)" = "You are only a calculator." ] && grep -qx "arg=phase=restart" "$RAN" && grep -qx "arg=system=2" "$RAN"'
out=$("$t/ab/bin/roster" restart orch --cut "$f" "$upto" --system 1 append)
new=$(copies); id=${new##*/}; id=${id%.jsonl}
is "--system with --cut: on the cut copy, phase system, from the old session, cut after BYTES of it" '[ -n "$id" ] && grep -qx "arg=phase=system" "$RAN" && grep -qx "arg=from=s1" "$RAN" && grep -qx "arg=cut=$upto" "$RAN" && grep -qx "arg=session=$id" "$RAN" && grep -qx "arg=system=1" "$RAN" && [ "$out" = "orch: restarted on session $id, session s1 cut after $upto bytes" ]'
rm -f "$new"
was=$(cat "$sp/current")
refused "a version it does not have" "no system prompt 9" --system 9 append
refused "a mode not append or replace" "append or replace" --system 1 instead
# agy takes a system text only appended (agy_agent): a version in place of its own is refused before it is named
session agy
refused "agy, a version in place of its own" "agy takes a system text only appended" --system 1 replace
session claude
is "nothing refused moved current" '[ "$(cat "$sp/current")" = "$was" ]'
# a text past Linux's 128 KiB on one argument goes whole, as its file: once refused at 100000 bytes
{ head -c 200001 /dev/zero | tr '\0' a; echo; } >"$sp/3.md"
"$t/ab/bin/roster" restart orch --system 3 append >/dev/null
is "a version of 200001 bytes: given whole, through its file" '[ "$(given --append-system-prompt-file | wc -c)" = 200001 ] && [ "$(cat "$sp/current")" = "3 append" ]'
"$t/ab/bin/roster" restart orch --system 1 append >/dev/null
# a boxed agent (N-1788): `current` names the version before coordd's start in the box; a start refused puts it back
printf '%s\n' "$was" >"$sp/current"
touch "$t/coord/boxed"
: >"$CALLS"
"$t/ab/bin/roster" restart orch --system 2 replace >/dev/null
is "boxed: current names it, then coordd starts it" '[ "$(cat "$sp/current")" = "2 replace" ] && grep -qx start "$CALLS" && ! grep -qx run "$CALLS"'
printf '%s\n' "$was" >"$sp/current"
sed -i 's/^add | start) ;;/add) ;;\nstart) exit 4 ;;/' "$t/ab/bin/coordc"
rc=0; "$t/ab/bin/roster" restart orch --system 2 replace >/dev/null 2>&1 || rc=$?
is "boxed: a start refused puts current back" '[ "$rc" != 0 ] && [ "$(cat "$sp/current")" = "$was" ]'
sed -i 's/^start) exit 4 ;;/start) ;;/' "$t/ab/bin/coordc"
rm "$t/coord/boxed"
# a fork goes on with the version its source runs with (p2447: "on restart, resume and fork"); a fork's name whose
# agent has versions of its own is refused, nothing made
: >"$CALLS"
"$t/ab/bin/roster" fork orch --name orch-f >/dev/null
is "a fork starts with its source's version, its own copy naming it" '[ "$(given --append-system-prompt-file)" = "Be brief." ] && grep -qx "arg=phase=fork" "$RAN" && grep -qx "arg=system=1" "$RAN" && [ "$(cat "$t/coord/prompts/orch-f.system/current")" = "1 append" ] && [ "$(cat "$t/coord/prompts/orch-f.system/1.md")" = "Be brief." ]'
# its words name no brief, so its record line holds them whole beside their hash; words that name one: the hash alone
# (paper-research N-1446 p3041)
words="You are now orch-f: a fork of orch, made at this point of its conversation (a copy of it; orch goes on on its own, and nothing you do changes its session). Your name for mail is orch-f; mail to orch reaches orch, not you. Say in one line that you are the fork, then wait for mail."
is "a fork's words with no brief: whole on its record line, beside their hash" 'grep -qxF "arg=prompt=$words" "$RAN" && grep -qx "arg=prompt_sha=$(sha "$words")" "$RAN" && ! grep -q "^arg=brief=." "$RAN"'
: >"$CALLS"
"$t/ab/bin/roster" fork orch --name orch-h "Read N-1446 p3041." >/dev/null
is "a fork's words naming a brief: the brief and the hash, not the words" 'grep -qx "arg=brief=N-1446.p3041" "$RAN" && grep -q "^arg=prompt_sha=" "$RAN" && ! grep -q "^arg=prompt=." "$RAN"'
# a start: its words whole on a fresh session; on a resume (coordd's restarts) the hash alone, since they are not the
# session's first user message (docket-core H-1639 p4)
mkdir -p "$t/w-s"; printf 'Hello there.' >"$t/fresh.md"; printf 'resume: 11111111-2222-3333-4444-555555555555\nGo on.' >"$t/again.md"
"$t/ab/bin/roster" add news claude-opus-5-5 "$t/fresh.md" "$t/w-s" >/dev/null
is "a fresh start's words with no brief: whole, beside their hash" 'grep -qx "arg=prompt=Hello there." "$RAN" && grep -qx "arg=prompt_sha=$(sha "Hello there.")" "$RAN"'
"$t/ab/bin/roster" add newr claude-opus-5-5 "$t/again.md" "$t/w-s" >/dev/null
is "a resume start: the hash, not the words" 'grep -qx -- "arg=--resume" "$RAN" && grep -qx "arg=prompt_sha=$(sha "Go on.")" "$RAN" && ! grep -q "^arg=prompt=." "$RAN"'
mkdir -p "$t/coord/prompts/orch-g.system"; : >"$CALLS"
err=$("$t/ab/bin/roster" fork orch --name orch-g 2>&1 >/dev/null) && rc=0 || rc=$?
is "a fork's name with versions of its own: refused, nothing made" '[ "$rc" != 0 ] && [[ $err == *"has system prompts of its own"* ]] && ! grep -qx add "$CALLS" && ! grep -qx run "$CALLS"'

echo "roster-cut-test: $fails failed"
[ "$fails" = 0 ]
