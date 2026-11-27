#!/usr/bin/env bash
# roster-tab-test.sh: `roster tab NAME DIR CMD...` keeps CMD word for word in agents/NAME/tab.json, an `sh -c` and its
# script with a newline included, and `roster restart NAME` (and so `roster resume` after a reboot) runs the same words
# again (jq 1.7 took the -c for its own option and the restart ran `sh "…"`: Tovrin-dev's runner, yaaihh N-101 p1665).
# Stand-ins for coordc and coord-pty record how they were called; nothing is started.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/ab/bin" "$t/ab/lib" "$t/coord" "$t/w" "$t/ran"
cp "$here/bin/roster" "$t/ab/bin/"; cp "$here/lib/"*.sh "$t/ab/lib/"
cat >"$t/ab/bin/coordc" <<'X'
#!/bin/sh
case $1 in ls | hangup | add) exit 0 ;; *) exit 3 ;; esac
X
# coord-pty's words, each NUL-ended, a file for each call
cat >"$t/ab/bin/coord-pty" <<'X'
#!/bin/sh
printf '%s\0' "$@" >"$RAN/$(ls "$RAN" | wc -l)"
X
chmod +x "$t/ab/bin/coordc" "$t/ab/bin/coord-pty"
export AGENT_BASE=$t/ab COORD=$t/coord RAN=$t/ran
fails=0
ok() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got [$2], want [$3]"; fails=$((fails + 1)); fi; }
# cmd N: the command call N ran under `coordc run runner --`, its words one a line (a newline within a word as \n)
cmd() {
  local -a w; mapfile -d '' -t w <"$RAN/$1"
  local i; for ((i = 0; i + 2 < ${#w[@]}; i++)); do
    if [ "${w[i]}" = run ] && [ "${w[i + 1]}" = runner ] && [ "${w[i + 2]}" = -- ]; then
      printf '%s\0' "${w[@]:i+3}" | jq -Rs 'split("\u0000")[:-1]' -c; return
    fi
  done
}
script=$'exec env A=1 prog --flag\nsecond line'
want='["sh","-c","exec env A=1 prog --flag\nsecond line","--","x"]'
"$t/ab/bin/roster" tab runner "$t/w" sh -c "$script" -- x >/dev/null
ok "tab.json keeps the command's words, -c and -- among them" "$(jq -c .cmd "$t/coord/agents/runner/tab.json")" "$want"
ok "the tab runs them" "$(cmd 0)" "$want"
"$t/ab/bin/roster" restart runner >/dev/null
ok "the restart runs the same words again" "$(cmd 1)" "$want"
ok "and keeps them for the next" "$(jq -c .cmd "$t/coord/agents/runner/tab.json")" "$want"
[ "$fails" = 0 ]
