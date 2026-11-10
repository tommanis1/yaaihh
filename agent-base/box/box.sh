#!/usr/bin/env bash
# box.sh: create and reach an agent box (a Docker Sandbox) from the host.
#
#   box.sh up DIR              create the box DIR/sbxenv.yaml declares (sbx env create)
#   box.sh key BOX             write ~/agent-coord/BOX/.proxy-key from its route (a box's initialize hook)
#   box.sh up BOX REPO [MEM]   create BOX on a private clone of REPO (default 16g);
#                              BOX_MOUNTS="PATH[:ro] ..." adds workspaces (a project layer's toolchain)
#   box.sh sh BOX [CMD...]     a shell (or CMD) inside BOX with the box environment (on a terminal when run from one)
#   box.sh start DIR|BOX       start the box declared by DIR again, with DIR/start.sh (BOX: the DIR it was last started
#                              from); then resume the agents that ran when its coordd last went down (roster resume)
#   box.sh restart BOX         stop BOX and start it again: the drill a reboot would be, after a change to start.sh
#   box.sh coordd BOX          start coordd in BOX unless it runs
#   box.sh fetch BOX           fetch BOX's commits into REPO as sandbox-BOX/*
#   box.sh down BOX            stop BOX (sbx rm BOX deletes it and its clone)
#   box.sh hold BOX [WHY]      nothing goes into BOX until `box.sh release BOX`: sh, coordd and start refuse, so no caller
#                              (yaaihh's link retrying every 3 s, a host `coord`) starts it again (`sbx exec` starts a stopped
#                              box); for repairs on a stopped box (8 Nov: e2fsck of its disk after the host disk filled)
#   box.sh release BOX         end the hold
#
# Inside the box (design §1-§5, decisions of 29 Oct):
#   - REPO is a private in-sandbox clone (`sbx --clone`): the host checkout is
#     mounted read-only and only the host fetches commits back. This replaces
#     the bare repos and git daemon of the 28 Oct draft; main and dev still move
#     only on the host. No GitHub or SSH credentials enter the box.
#   - this agent-base checkout is mounted read-only at its host path, so hooks
#     and tools change without recreating the box;
#   - $COORD (~/agent-coord/BOX) is mounted read-write: agent state, stops and
#     the feed, read on the host by the view;
#   - the model route ($COORD/route, lib/route.sh): a proxy reached from the box at host.docker.internal with the proxy's
#     client key, never a Kiro or Claude credential;
#   - kit/ adds tmux and socat;
#   - each agent runs on a terminal held by bin/coord-pty in the box; the owner opens one in a host terminal with
#     `box.sh sh BOX coord attach NAME` (Ctrl-\ detaches), e.g. every agent as a WezTerm tab:
#       for n in $(box.sh sh BOX coord ls | awk -F'\t' '$3 != "stopped" { print $1 }'); do
#         wezterm cli spawn -- box.sh sh BOX coord attach "$n"; done
#   - sbx's host SSH agent forwarding and host browser opening must be off;
#   - the box is detached (`sbx run -d`): it keeps running with no session
#     attached, until box.sh down;
#   - coordd, the box's one watcher (vault designs/yaaihh-2026-11-07.md §7), runs as root from up and each
#     start (bin/coordd, a link into releases/current, release.sh), logging to $COORD/coordd.log; the agent user has no sudo and
#     is not in the docker group, so an agent cannot leave its cgroup or speak for another (design §6).
set -euo pipefail
AB=$(cd "$(dirname "$0")/.." && pwd)
cmd=${1:-}; BOX=${2:-}
# `up DIR`: the box is declared by DIR/sbxenv.yaml (owner 29 Oct: declarative layers); its name comes from the file.
ENVDIR=; if { [ "$cmd" = up ] || [ "$cmd" = start ]; } && [ -f "$BOX/sbxenv.yaml" ]; then ENVDIR=$(realpath "$BOX"); BOX=$(sed -n 's/^name: *//p' "$ENVDIR/sbxenv.yaml"); fi
[ -n "$BOX" ] || { sed -n '4,20p' "$0" | sed 's/^# //'; exit 2; }
COORD=$HOME/agent-coord/$BOX
# a declared box remembers its DIR, so yaaihh starts it by name (`start BOX`, `restart BOX`)
if [ -n "$ENVDIR" ]; then mkdir -p "$COORD" && echo "$ENVDIR" >"$COORD/envdir"
elif { [ "$cmd" = start ] || [ "$cmd" = restart ]; } && [ -s "$COORD/envdir" ]; then ENVDIR=$(cat "$COORD/envdir"); fi
. "$AB/lib/route.sh"
ENVV=(-e "COORD=$COORD" -e "AGENT_BASE=$AB")
anthropic_url=$(route_get ANTHROPIC_BASE_URL)
[ -z "$anthropic_url" ] || ENVV+=(-e "ANTHROPIC_BASE_URL=$(route_in_box "$anthropic_url")")
# the key sbx's claude agent is created with: the route's, else the caller's ANTHROPIC_API_KEY (a box has no login)
box_key() { if [ -s "$COORD/.proxy-key" ]; then cat "$COORD/.proxy-key"; else printf '%s' "${ANTHROPIC_API_KEY:-}"; fi; }
# the route's port opened in the box's network policy
allow_route() {
  local hp; hp=$(printf '%s' "$anthropic_url" | sed -E 's#^[a-z]+://([^/]+).*#\1#')
  [ -z "$hp" ] || sbx policy allow network --sandbox "$BOX" "${hp/127.0.0.1/localhost}" >/dev/null
}
# The sandbox daemon runs as the memory-capped user unit sandboxd.service (3 Nov: a box VM with no cap froze the host).
# An sbx call with no daemon running starts one itself, uncapped and inside the caller's terminal, so start the unit first.
systemctl --user start sandboxd.service

# inbox [SBX-EXEC-FLAGS...] -- CMD...
held() {  # refuse while BOX is held (box.sh hold)
  [ -e "$COORD/held" ] || return 0
  echo "box.sh: $BOX is held ($(cat "$COORD/held")); box.sh release $BOX ends it" >&2; exit 1
}

inbox() {
  held
  local flags=(); while [ "$1" != -- ]; do flags+=("$1"); shift; done; shift
  # LANG: the box has no locale; without UTF-8, tmux draws a client's lines and the hook's ● as _ (6 Nov)
  sbx exec "${flags[@]}" "${ENVV[@]}" -e LANG=C.UTF-8 -e "PATH=$AB/bin:/home/agent/.local/bin:/usr/local/bin:/usr/bin:/bin" "$BOX" "$@"
}

# coordd_up: close the agent user's routes to root, then start coordd unless it runs. sbx exec has no detach (-d), so coordd
# is started in its own session from a root exec, which it outlives.
coordd_up() {
  sbx exec -u root "${ENVV[@]}" "$BOX" sh -c '
    # sbx gives every exec the sudo and docker GIDs itself, so /etc/group cannot take them away: sudo loses its
    # passwordless rule (agent has no password) and the docker socket the docker group
    rm -f /etc/sudoers.d/agent /etc/sudoers.d/*agent* 2>/dev/null || true
    chgrp root /var/run/docker.sock 2>/dev/null || true
    pgrep -x coordd >/dev/null && exit 0
    setsid "$AGENT_BASE/bin/coordd" </dev/null >>"$COORD/coordd.log" 2>&1 &
    for _ in $(seq 50); do [ -S /run/coordd.sock ] && exit 0; sleep 0.1; done
    echo "coordd did not start; see $COORD/coordd.log" >&2; exit 1'
}

case $cmd in
up)
  [ -n "$ENVDIR" ] || { REPO=$(realpath "${3:?box.sh up BOX REPO [MEM] or box.sh up DIR}"); MEM=${4:-16g}; }
  # sbx defaults that reach past the box: the host SSH agent's keys, and URLs
  # opened in the host browser (around the network policy). Both must be off.
  for k in ssh.agentForwardingEnabled browser.enabled; do
    [ "$(sbx settings get "$k")" = false ] || { echo "refusing: sbx setting $k is not false (sbx settings set $k false; systemctl --user restart sandboxd)"; exit 1; }
  done
  if [ -n "$ENVDIR" ]; then
    sbx ls 2>/dev/null | grep -q "^$BOX " && { echo "$BOX exists; box.sh sh $BOX"; exit 1; }
    # REPO for box.sh fetch: kept when set (a box whose workspace is $COORD names its host repo there)
    mkdir -p "$COORD" && [ -s "$COORD/repo" ] || sed -n '/^workspace:/,/^[a-z]/s/^ *path: *//p' "$ENVDIR/sbxenv.yaml" | head -1 >"$COORD/repo"
    route_write_key
    ANTHROPIC_API_KEY=$(box_key) sbx env create --force "$ENVDIR"
    coordd_up
    echo "$BOX up: box.sh sh $BOX, then roster up FILE; box.sh sh $BOX coord attach NAME to watch one"; exit 0
  fi
  mkdir -p "$COORD" && echo "$REPO" >"$COORD/repo"
  route_write_key
  sbx ls 2>/dev/null | grep -q "^$BOX " && { echo "$BOX exists; box.sh sh $BOX"; exit 1; }
  ANTHROPIC_API_KEY=$(box_key) sbx create --clone --name "$BOX" --memory "$MEM" --kit "$AB/kit" \
    -e ANTHROPIC_API_KEY "${ENVV[@]}" claude "$REPO" "$AB:ro" "$COORD" ${BOX_MOUNTS:-}
  allow_route
  # keep running with no session attached (else sbx stops it, and the agents, 30 s after the last exec ends)
  sbx run -d --name "$BOX" claude >/dev/null
  coordd_up
  echo "$BOX up: box.sh sh $BOX, then roster up FILE; box.sh sh $BOX coord attach NAME to watch one"
  ;;
# `start DIR`: start a declared box again (after sbx stop or a host restart) and run DIR/start.sh in it, the box's
# per-start steps, as root (sbx has no post-start hook; a box's bind-mounts of its working data from the host, say)
start)
  held
  [ -n "$ENVDIR" ] || { echo "box.sh start DIR (DIR holds sbxenv.yaml)"; exit 2; }
  was_up=; inbox -- true 2>/dev/null && was_up=1   # yaaihh runs start at each of its own starts: the box may be up
  sbx run -d --name "$BOX" claude >/dev/null
  # as root (the agent user has no sudo), and before coordd: the project's sources read what start.sh mounts
  [ ! -f "$ENVDIR/start.sh" ] || inbox -u root -i -- bash -s <"$ENVDIR/start.sh"
  route_write_key
  # the agents come back idle and get only the owner's mail until he removes $COORD/hold (owner m312, 10 Nov: a box
  # started again, by hand or by a bridge's sbx exec, waits for him). Only a box that was down: a start of a running
  # box (yaaihh restarting, 10 Nov) set it again and held the agents' mail for an hour
  [ -n "$was_up" ] || touch "$COORD/hold"
  coordd_up
  inbox -- roster resume
  ;;
restart)
  [ -n "$ENVDIR" ] || { echo "box.sh restart BOX: $BOX has no envdir (box.sh start DIR once)"; exit 2; }
  sbx stop "$BOX" && "$0" start "$BOX"
  ;;
coordd) coordd_up ;;  # start coordd in a running box (after an update of bin/coordd: stop it first)
key) route_write_key ;;
# TOOL_PATH (a box file's env: a layer's toolchain) goes before PATH, for the shell and every agent started from it.
# On a terminal (stdin and stdout), CMD gets one in the box too (a shell, `coord attach`); piped (yaaihh's bridge), not.
sh) shift 2; [ $# -gt 0 ] || set -- bash; flag=; [ -t 0 ] && [ -t 1 ] && flag=-it
    inbox $flag -- /bin/sh -c 'PATH=${TOOL_PATH:+$TOOL_PATH:}$PATH exec /usr/bin/env "$@"' sh "$@" ;;
fetch) git -C "$(cat "$COORD/repo")" fetch "sandbox-$BOX" ;;
down) sbx stop "$BOX" ;;
hold) shift 2; echo "${*:-held by box.sh hold at $(date +%H:%M)}" >"$COORD/held"; echo "$BOX held" ;;
release) rm -f "$COORD/held"; echo "$BOX released" ;;
*) sed -n '4,17p' "$0" | sed 's/^# //'; exit 2 ;;
esac
