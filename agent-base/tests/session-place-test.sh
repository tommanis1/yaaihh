#!/usr/bin/env bash
# session-place-test.sh: where roster takes an agent session to run (lib/route.sh session_place; paper-research N-101
# post 108). Four cases, and the safe side: a host project (its own socket, no box) keeps prompts and takes the route's
# endpoint; a box's own coordd (default socket) and a box's second coordd (its own socket, the trial's) bypass prompts
# and keep the endpoint their exec gave; an agent in the light sandbox on the host (ROSTER_BOX) bypasses prompts and
# takes the route's endpoint (the owner, N-216.p1); with nothing said, roster is on the host. And the owner's Yolo
# for a host agent (N-1400), which changes the flag and one settings key only.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
COORD=$(mktemp -d); trap 'rm -rf "$COORD"' EXIT
printf 'ANTHROPIC_BASE_URL=http://127.0.0.1:8317\n' >"$COORD/route"
. "$here/lib/route.sh"
fails=0
case_() {  # case_ NAME IN_BOX SOCK WANT_MODE WANT_URL [ROSTER_BOX]
  local name=$1 inbox=$2 sock=$3 want_mode=$4 want_url=$5 rbox=${6:-}
  local mode=(--dangerously-skip-permissions) filter=. url=()
  COORD_IN_BOX=$inbox COORDD_SOCK=$sock ROSTER_BOX=$rbox session_place
  local got_mode="${mode[*]:-}" got_url="${url[*]:-}"
  if [ "$got_mode" = "$want_mode" ] && [ "$got_url" = "$want_url" ]; then
    echo "ok   $name: mode=[$got_mode] url=[$got_url]"
  else
    echo "FAIL $name: mode=[$got_mode] want [$want_mode]; url=[$got_url] want [$want_url]"; fails=$((fails + 1))
  fi
  # a filter that keeps the bypass in the box, drops it on the host
  case $want_mode in
    '') [[ $filter == *defaultMode* ]] || { echo "FAIL $name: the host's filter keeps defaultMode"; fails=$((fails + 1)); } ;;
    *) [ "$filter" = . ] || { echo "FAIL $name: the box's filter changed ($filter)"; fails=$((fails + 1)); } ;;
  esac
}
case_ "host project (own socket, no box)" "" /run/user/1000/coordd/p.sock "" "ANTHROPIC_BASE_URL=http://127.0.0.1:8317"
case_ "box, its own coordd (default socket)" 1 "" "--dangerously-skip-permissions" ""
case_ "box, a second coordd (its own socket)" 1 /run/coordd-trial.sock "--dangerously-skip-permissions" ""
case_ "light sandbox on the host (ROSTER_BOX): bypass, the route's endpoint" "" /run/user/1000/coordd/p.sock "--dangerously-skip-permissions" "ANTHROPIC_BASE_URL=http://127.0.0.1:8317" 7
case_ "nothing said: the host, the safe side" "" "" "" "ANTHROPIC_BASE_URL=http://127.0.0.1:8317"
# The owner's Yolo (0.2 line 25, N-1400; the caller's YOLO=1 from session.json's `mode`): on the host the bypass flag and
# the one key that skips its confirmation, the route's endpoint, the defaultMode strip; in a box or the light sandbox
# nothing changes (they bypass already). Each filter run over agent-base's own settings, as roster runs it.
yolo_case() {  # yolo_case NAME IN_BOX ROSTER_BOX YOLO WANT_MODE WANT_ON WANT_SKIP
  local name=$1 inbox=$2 rbox=$3 yolo=$4 want_mode=$5 want_on=$6 want_skip=$7
  local mode=(--dangerously-skip-permissions) filter=. url=() YOLO_ON=
  COORD_IN_BOX=$inbox COORDD_SOCK=/run/user/1000/coordd/p.sock ROSTER_BOX=$rbox YOLO=$yolo session_place
  local got_mode="${mode[*]:-}" got
  got=$(jq -c "$filter | {skip: (.skipDangerousModePermissionPrompt // false), dm: (.permissions.defaultMode // null)}" "$here/claude/settings.json")
  local want_dm=null; [ -z "$inbox$rbox" ] || want_dm='"bypassPermissions"'
  if [ "$got_mode" = "$want_mode" ] && [ "$YOLO_ON" = "$want_on" ] && [ "$got" = "{\"skip\":$want_skip,\"dm\":$want_dm}" ]; then
    echo "ok   $name: mode=[$got_mode] on=[$YOLO_ON] settings $got"
  else
    echo "FAIL $name: mode=[$got_mode] want [$want_mode]; on=[$YOLO_ON] want [$want_on]; settings $got want skip $want_skip, defaultMode $want_dm"; fails=$((fails + 1))
  fi
}
yolo_case "host, Safe: no flag, the confirmation's key stripped" "" "" "" "" "" false
yolo_case "host, Yolo: the flag, the confirmation's key kept, defaultMode stripped" "" "" 1 "--dangerously-skip-permissions" 1 true
yolo_case "box, Yolo: the box's as ever" 1 "" 1 "--dangerously-skip-permissions" "" true
yolo_case "light sandbox, Yolo: the box's as ever" "" 7 1 "--dangerously-skip-permissions" "" true
# roster takes YOLO from session.json's `mode` (coordd's record), journals a start in Yolo, and a fork reads its own file
grep -q "jq -r '.mode // empty' \"\$COORD/agents/\$name/session.json\"" "$here/bin/roster" && grep -q 'logger -t roster "$name starts in Yolo' "$here/bin/roster" \
  && echo "ok   roster reads its own session.json's mode and journals a Yolo start" || { echo "FAIL roster does not take the mode from session.json"; fails=$((fails + 1)); }
# box.sh says it on every exec
grep -q 'ENVV=(.*-e COORD_IN_BOX=1)' "$here/box/box.sh" && echo "ok   box.sh sets COORD_IN_BOX on every exec" || { echo "FAIL box.sh does not set COORD_IN_BOX"; fails=$((fails + 1)); }
# roster decides by session_place alone, not by COORDD_SOCK
! grep -n 'if \[ -n "${COORDD_SOCK:-}" \]' "$here/bin/roster" && grep -q '^  session_place$' "$here/bin/roster" && echo "ok   roster decides by session_place" || { echo "FAIL roster still decides by COORDD_SOCK"; fails=$((fails + 1)); }
[ "$fails" = 0 ]
