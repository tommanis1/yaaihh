#!/usr/bin/env bash
# An agent's start in its coord's record (paper-research N-1446 p1388): `roster said FIELD=VALUE... -- CMD...`, which
# agent() runs as the program `coordc run NAME` starts, says the start as the agent's own act `began` through
# $AGENT_BASE/bin/coordc, then execs CMD with its words and its environment as given (the same process; roster's own
# set-up below `said`, its PATH and CODEX_HOME, not reached); a field with no value is left out; the start waits for
# nothing (paper-research N-1446 p1398): the program runs at once while a slow coordc's line still lands after it, the
# act is no child of the program, and what a failing coordc says goes to the journal (a stand-in logger), not to the
# terminal. The test's own waits for a line are bounded; roster's are none. brief_of finds the first post a prompt names, said_of the
# session a start resumes, a fork's parent, or a new session's id for --session-id. A stand-in coordc writes down its
# words; no coordd is asked and no agent is started. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
roster=$here/bin/roster
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/ab/bin" "$dir/slow/bin" "$dir/bad/bin" "$dir/coord"
cat >"$dir/ab/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$dir/acts.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
printf '#!/bin/sh\nsleep 3\nexec "%s/ab/bin/coordc" "$@"\n' "$dir" >"$dir/slow/bin/coordc"
printf '#!/bin/sh\necho "coord act: refused, a stand-in" >&2; exit 1\n' >"$dir/bad/bin/coordc"
mkdir -p "$dir/fakebin"; printf '#!/bin/sh\n{ echo "args: $*"; cat; } >>"%s/journal"\n' "$dir" >"$dir/fakebin/logger"; chmod +x "$dir/fakebin/logger"
cat >"$dir/prog" <<EOF
#!/usr/bin/env python3
import json, os, sys, time
kids = open(f"/proc/self/task/{os.getpid()}/children").read().split()
json.dump({"argv": sys.argv[1:], "pid": os.getpid(), "path": os.environ.get("PATH"), "codex_home": os.environ.get("CODEX_HOME"),
           "at": time.time(), "children": kids}, open("$dir/prog.json", "w"))
EOF
chmod +x "$dir/ab/bin/coordc" "$dir/slow/bin/coordc" "$dir/bad/bin/coordc" "$dir/prog"
echo /not-the-agents >"$dir/coord/path"  # roster's own set-up would put it first on PATH
export COORD="$dir/coord" AGENT_NAME=w1
fails=0
ok() { echo "ok - $1"; }
bad() { echo "not ok - $1"; fails=$((fails + 1)); }
j() { python3 -I -c "import json,sys; v=json.load(open(sys.argv[1])); print(eval(sys.argv[2]))" "$@"; }
# landed FILE N: FILE has N lines, waited for up to 30 s (the test's bound; roster waits for nothing)
landed() { local i; for ((i = 0; i < 300; i++)); do [ "$(wc -l <"$1" 2>/dev/null || echo 0)" -ge "$2" ] && return 0; sleep 0.1; done; return 1; }

# the act and the program
words=(--resume s-0 --name w1 $'a prompt\nof two lines' '' -- --dangerously-skip-permissions)
bash -c 'echo $$ >"$1/pid"; shift; exec "$@"' sh "$dir" env -u CODEX_HOME PATH=/usr/bin:/bin AGENT_BASE="$dir/ab" "$roster" said phase=start kind=claude model=claude-opus-5-5 \
  effort=high dir=/w/x session=s-1 from= brief=N-1446.p1388 prompt_sha=abc starter=paper-research -- "$dir/prog" "${words[@]}"
rc=$?
[ "$rc" = 0 ] && ok "said runs the program" || bad "said: rc $rc"
landed "$dir/acts.jsonl" 1; a=$(head -n 1 "$dir/acts.jsonl" 2>/dev/null)
[ "$(wc -l <"$dir/acts.jsonl" 2>/dev/null)" = 1 ] && ok "one act" || bad "acts: $(cat "$dir/acts.jsonl" 2>/dev/null)"
[ "$(python3 -I -c 'import json,sys; print(json.loads(sys.argv[1])[:3])' "$a")" = "['act', 'began', 'start: claude claude-opus-5-5 effort high in /w/x, session s-1, briefed by N-1446.p1388, by paper-research']" ] \
  && ok "it is \`act began\` with its words" || bad "act words: $a"
[ "$(python3 -I -c 'import json,sys; print(json.loads(sys.argv[1])[3:])' "$a")" = "['phase=start', 'kind=claude', 'model=claude-opus-5-5', 'effort=high', 'dir=/w/x', 'session=s-1', 'brief=N-1446.p1388', 'prompt_sha=abc', 'starter=paper-research']" ] \
  && ok "its fields, the empty one left out" || bad "act fields: $a"
[ "$(j "$dir/prog.json" 'v["argv"]')" = "$(python3 -I -c 'import sys; print(sys.argv[1:])' "${words[@]}")" ] \
  && ok "the program gets its words as given (a newline, an empty word, a --)" || bad "argv: $(j "$dir/prog.json" 'v["argv"]')"
[ "$(j "$dir/prog.json" 'v["pid"]')" = "$(cat "$dir/pid")" ] && ok "the program is the same process (exec)" || bad "pid: $(j "$dir/prog.json" 'v["pid"]') vs $(cat "$dir/pid")"
[ "$(j "$dir/prog.json" 'v["path"]')" = /usr/bin:/bin ] && [ "$(j "$dir/prog.json" 'v["codex_home"]')" = None ] \
  && ok "its environment as given: roster's PATH and CODEX_HOME not set" || bad "env: $(cat "$dir/prog.json")"
[ "$(j "$dir/prog.json" 'v["children"]')" = "[]" ] && ok "the act is no child of the program (no zombie under claude)" || bad "children: $(j "$dir/prog.json" 'v["children"]')"

# coordc not there, refusing, not answering
rm -f "$dir/prog.json"
AGENT_BASE="$dir/none" "$roster" said phase=restart -- "$dir/prog" x && [ -f "$dir/prog.json" ] && ok "no coordc: the program runs" || bad "no coordc"
rm -f "$dir/prog.json"
PATH="$dir/fakebin:/usr/bin:/bin" AGENT_BASE="$dir/bad" "$roster" said phase=restart -- "$dir/prog" x >"$dir/out" 2>"$dir/err" && [ -f "$dir/prog.json" ] \
  && [ ! -s "$dir/err" ] && [ ! -s "$dir/out" ] && ok "a coordc that refuses: the program runs, nothing on its terminal" || bad "bad coordc: $(cat "$dir/out" "$dir/err")"
landed "$dir/journal" 3
grep -qx 'args: -t roster-said' "$dir/journal" && grep -qx 'coord act: refused, a stand-in' "$dir/journal" \
  && grep -qx "roster said: w1's start line not recorded: coordc act began failed (rc 1)" "$dir/journal" \
  && ok "what the refusing coordc says goes to the journal, with roster's line" || bad "journal: $(cat "$dir/journal" 2>/dev/null)"
rm -f "$dir/prog.json"; n=$(wc -l <"$dir/acts.jsonl"); t=$(date +%s.%N)
AGENT_BASE="$dir/slow" "$roster" said phase=restart -- "$dir/prog" x && [ -f "$dir/prog.json" ] \
  && python3 -I -c 'import sys; sys.exit(0 if float(sys.argv[1]) - float(sys.argv[2]) < 1.5 else 1)' "$(j "$dir/prog.json" 'v["at"]')" "$t" \
  && ok "a coordc that is slow (3 s): the program starts at once" || bad "slow coordc: the program waited, or did not run"
[ "$(wc -l <"$dir/acts.jsonl")" = "$n" ] && ok "  its line not yet there when the program runs" || bad "the slow line came first"
landed "$dir/acts.jsonl" $((n + 1)) && tail -n 1 "$dir/acts.jsonl" | grep -q '"phase=restart"' && ok "  and it still lands, after" || bad "the slow coordc's line never landed"
AGENT_BASE="$dir/ab" "$roster" said phase=start 2>"$dir/err"; [ $? = 2 ] && grep -q usage "$dir/err" && ok "no program: usage" || bad "no program: $(cat "$dir/err")"

# brief_of and said_of, as roster defines them
eval "$(sed -n '/^brief_of() {/p' "$roster")"
eval "$(sed -n '/^said_of() {/,/^}/p' "$roster")"
[ "$(brief_of $'Read N-1446 p1388, then N-1104.p764')" = N-1446.p1388 ] && ok "brief_of: N-1446 p1388" || bad "brief_of p: $(brief_of 'Read N-1446 p1388')"
[ "$(brief_of 'as in N-1104.p764')" = N-1104.p764 ] && ok "brief_of: N-1104.p764" || bad "brief_of .p"
[ "$(brief_of 'N-101 post 27, B')" = N-101.p27 ] && ok "brief_of: N-101 post 27" || bad "brief_of post"
[ -z "$(brief_of 'work on I-1480 under N-1446')" ] && ok "brief_of: no post named, none" || bad "brief_of none: $(brief_of 'work on I-1480 under N-1446')"
said_of --resume 0b8c-1 'go on'
[ "$said_session" = 0b8c-1 ] && [ -z "$said_from" ] && [ ${#said_newid[@]} = 0 ] && ok "said_of: a restart's session, no new id" || bad "said_of restart: $said_session/$said_from/${said_newid[*]}"
said_of --resume 0b8c-1 --fork-session 'you are the fork'
[ -z "$said_session" ] && [ "$said_from" = 0b8c-1 ] && [ ${#said_newid[@]} = 0 ] && ok "said_of: a fork's parent, no session of its own named" || bad "said_of fork: $said_session/$said_from"
said_of 'a first prompt'
[[ $said_session =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] && [ "${said_newid[*]}" = "--session-id $said_session" ] \
  && ok "said_of: a new session's id, given as --session-id" || bad "said_of new: $said_session/${said_newid[*]}"

# agent(): the program coordc run starts is roster said, then claude, its flags after coordc's first -- as coordd's
# started_yolo reads them
grep -q '"$AGENT_BASE/bin/coordc" run "$name" -- "$AGENT_BASE/bin/roster" said "${said\[@\]}" -- \\$' "$roster" \
  && grep -A1 'roster" said "${said\[@\]}" -- \\$' "$roster" | grep -q '^    claude "$@" "${said_newid\[@\]}" --name' \
  && ok "agent(): coordc run starts roster said, then claude" || bad "agent()'s command"
echo "$fails failed"
[ "$fails" = 0 ]
