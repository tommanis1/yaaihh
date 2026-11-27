#!/usr/bin/env bash
# roster-via-coordd-test.sh: in a boxed project (coordd's $COORD/boxed) the owner's `roster add` and `roster restart`
# are handed to coordd (`coordc start`), which starts the agent as its own user (N-101 post 406, the joint proposal's
# A); what coordd itself runs (ROSTER_VIA_COORDD) is not handed back; a project that is not boxed starts as before.
# A stand-in coordc records how it was called; nothing is started.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/coord/agents/orch"
cp "$here/bin/roster" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
echo "$*" >>"$CALLS"
case $1 in ls) ;; start) echo "$2: started by coordd" ;; add) exit 0 ;; *) exit 3 ;; esac
X
chmod +x "$t/ab/bin/coordc"
echo key >"$t/coord/.proxy-key"
echo '{"session_id":"s1","cwd":"/w"}' >"$t/coord/agents/orch/session.json"
echo hello >"$t/prompt.md"
export AGENT_BASE=$t/ab COORD=$t/coord CALLS=$t/calls
fails=0
check() {  # check NAME WANT: the calls coordc got, since the last check
  local got; got=$(cat "$CALLS" 2>/dev/null || true); : >"$CALLS"
  if [ "$got" = "$2" ]; then echo "ok   $1"; else echo "FAIL $1: coordc got [$got], want [$2]"; fails=$((fails + 1)); fi
}
touch "$t/coord/boxed"
"$t/ab/bin/roster" add orch2 claude-haiku-5-5 "$t/prompt.md" /w >/dev/null
check "boxed: the owner's add goes to coordd" "ls
start orch2 claude-haiku-5-5 $t/prompt.md /w"
"$t/ab/bin/roster" restart orch >/dev/null
check "boxed: the owner's restart goes to coordd" "start orch"
# coordd's own roster calls: not handed back (the stand-in refuses what a real start would call next)
ROSTER_VIA_COORDD=1 "$t/ab/bin/roster" add orch3 claude-haiku-5-5 "$t/prompt.md" /w >/dev/null 2>&1 || true
got=$(cat "$CALLS"); : >"$CALLS"
case $got in *start*) echo "FAIL coordd's own add was handed back: [$got]"; fails=$((fails + 1)) ;; *"add orch3"*) echo "ok   coordd's own add registers and starts it here" ;; *) echo "FAIL coordd's own add: [$got]"; fails=$((fails + 1)) ;; esac
rm "$t/coord/boxed"
"$t/ab/bin/roster" add orch4 claude-haiku-5-5 "$t/prompt.md" /w >/dev/null 2>&1 || true
got=$(cat "$CALLS"); : >"$CALLS"
case $got in *start*) echo "FAIL not boxed, handed to coordd: [$got]"; fails=$((fails + 1)) ;; *"add orch4"*) echo "ok   not boxed: roster starts it itself" ;; *) echo "FAIL not boxed: [$got]"; fails=$((fails + 1)) ;; esac
[ "$fails" = 0 ]
