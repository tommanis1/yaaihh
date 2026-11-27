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
#   box.sh coordd-stop BOX     stop BOX's coordd (SIGTERM) and wait until it has ended; its agents go on running
#                              (release.sh coord-switch and coord-back, docket-core4 N-182)
#   box.sh fetch BOX           fetch BOX's commits into REPO as sandbox-BOX/*
#   box.sh down BOX            stop BOX (sbx rm BOX deletes it and its clone)
#   box.sh hold BOX [WHY]      nothing goes into BOX until `box.sh release BOX`: sh, coordd and start refuse, so no caller
#                              (yaaihh's link retrying every 3 s, a host `coord`) starts it again (`sbx exec` starts a stopped
#                              box); for repairs on a stopped box (8 Nov: e2fsck of its disk after the host disk filled)
#   box.sh release BOX         end the hold
#
# A box DIR with box.conf instead of sbxenv.yaml is a plain Docker container (owner 10 Nov: a thinner box; agda-dev is
# the first): `up DIR` builds DIR/Dockerfile and runs it with box.conf's name, memory, cpus, mounts and env, then runs
# DIR/setup.sh once in it; sh, start, restart, coordd and down work the same on it. It has no network: the model route
# reaches it through $COORD/proxy.sock, a host-side socat (a transient user unit box-proxy-BOX, from up and start)
# to the route's endpoint, which the container's entry listens for on the same 127.0.0.1 port. Its memory is docker's
# --memory, which the kernel takes back when unused (an sbx VM keeps what it once touched).
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
#   - coordd, the box's one watcher (vault designs/yaaihh-2026-11-07.md §7), runs from up and each start (bin/coordd, a
#     link into releases/current, release.sh), logging to $COORD/coordd.log: in an sbx box as root, so an agent cannot
#     leave its cgroup or speak for another (design §6); in a docker box as the agent user, with the agents, the runner
#     and the repositories (coordd_up_agent, N-101 post 145), which one uid cannot keep apart, as on the host. The agent
#     user has no sudo and is not in the docker group.
set -euo pipefail
AB=$(cd "$(dirname "$0")/.." && pwd)
cmd=${1:-}; BOX=${2:-}
# `up DIR`: the box is declared by DIR/sbxenv.yaml (owner 29 Oct: declarative layers); its name comes from the file.
ENVDIR=; if { [ "$cmd" = up ] || [ "$cmd" = start ]; } && [ -f "$BOX/sbxenv.yaml" ]; then ENVDIR=$(realpath "$BOX"); BOX=$(sed -n 's/^name: *//p' "$ENVDIR/sbxenv.yaml")
elif { [ "$cmd" = up ] || [ "$cmd" = start ]; } && [ -f "$BOX/box.conf" ]; then ENVDIR=$(realpath "$BOX"); BOX=$(sed -n 's/^name=//p' "$ENVDIR/box.conf"); fi
[ -n "$BOX" ] || { sed -n '4,20p' "$0" | sed 's/^# //'; exit 2; }
COORD=$HOME/agent-coord/$BOX
# a declared box remembers its DIR, so yaaihh starts it by name (`start BOX`, `restart BOX`)
if [ -n "$ENVDIR" ]; then mkdir -p "$COORD" && echo "$ENVDIR" >"$COORD/envdir"
elif { [ "$cmd" = start ] || [ "$cmd" = restart ]; } && [ -s "$COORD/envdir" ]; then ENVDIR=$(cat "$COORD/envdir"); fi
. "$AB/lib/route.sh"
# the backend: docker for a box declared by box.conf (found again through $COORD/envdir), else sbx
CONF=
if [ -n "$ENVDIR" ]; then CONF=$ENVDIR/box.conf; elif [ -s "$COORD/envdir" ]; then CONF=$(cat "$COORD/envdir")/box.conf; fi
[ -f "$CONF" ] || CONF=
# COORD_IN_BOX: what runs there is in a box (roster's session_place, lib/route.sh); on every exec, so coordd and roster
# started through here have it, and coordd hands it down to what it starts
EXEC=(sbx exec); ENVV=(-e "COORD=$COORD" -e "AGENT_BASE=$AB" -e COORD_IN_BOX=1)
# The replay lock (I-1509; paper-research N-1446 p1644: one machine, one lock): the host's, which `build-runner replay`
# and the scripts' `flock LOCK` take ($BUILD_RUNNER_REPLAY_LOCK, else ~/agent-coord/docket-try-saved/replay.lock),
# hard-linked into $COORD, which the box mounts: the same file there, so the saver of the box's coordd (`coordd
# save-replay`) waits behind the host's replays and they behind it. Linked again at each coordd start (replay_lock).
# A box that cannot open it is said in coordd's journal by the saver, which then saves without it.
HOST_REPLAY_LOCK=${BUILD_RUNNER_REPLAY_LOCK:-$HOME/agent-coord/docket-try-saved/replay.lock}
ENVV+=(-e "BUILD_RUNNER_REPLAY_LOCK=$COORD/replay.lock")
replay_lock() {
  { mkdir -p "$(dirname "$HOST_REPLAY_LOCK")" && touch "$HOST_REPLAY_LOCK" && ln -f "$HOST_REPLAY_LOCK" "$COORD/replay.lock"; } 2>/dev/null ||
    echo "box.sh: the replay lock $HOST_REPLAY_LOCK is not linked into $COORD: the box's saver saves without it" >&2
}
anthropic_url=$(route_get ANTHROPIC_BASE_URL)
if [ -n "$CONF" ]; then
  . "$CONF"; EXEC=(docker exec -u agent)  # sbx exec runs as the agent user; docker exec as root unless told
  # box.conf's env on every exec as on the container; the route as on the host (entry.sh listens on its port)
  for e in "${env[@]}"; do ENVV+=(-e "$e"); done
  [ -z "$anthropic_url" ] || ENVV+=(-e "ANTHROPIC_BASE_URL=$anthropic_url")
else
  [ -z "$anthropic_url" ] || ENVV+=(-e "ANTHROPIC_BASE_URL=$(route_in_box "$anthropic_url")")
fi
# the key sbx's claude agent is created with: the route's, else the caller's ANTHROPIC_API_KEY (a box has no login)
box_key() { if [ -s "$COORD/.proxy-key" ]; then cat "$COORD/.proxy-key"; else printf '%s' "${ANTHROPIC_API_KEY:-}"; fi; }
# the route's port opened in the box's network policy
allow_route() {
  local hp; hp=$(printf '%s' "$anthropic_url" | sed -E 's#^[a-z]+://([^/]+).*#\1#')
  [ -z "$hp" ] || sbx policy allow network --sandbox "$BOX" "${hp/127.0.0.1/localhost}" >/dev/null
}
# The sandbox daemon runs as the memory-capped user unit sandboxd.service (3 Nov: a box VM with no cap froze the host).
# An sbx call with no daemon running starts one itself, uncapped and inside the caller's terminal, so start the unit first.
[ -n "$CONF" ] || systemctl --user start sandboxd.service

# inbox [SBX-EXEC-FLAGS...] -- CMD...
held() {  # refuse while BOX is held (box.sh hold)
  [ -e "$COORD/held" ] || return 0
  echo "box.sh: $BOX is held ($(cat "$COORD/held")); box.sh release $BOX ends it" >&2; exit 1
}

# A docker box runs coordd, the agents, the runner and the repositories as one user, agent (paper-research N-101 post
# 145; yaaihh N-169 posts 116-144: coordd as root made the repositories "dubious" to git and would have left root's
# refs, objects and view folders where agent writes). Its cgroup tree ($CG, coordd's COORDD_CGROUP default) is agent's:
# coordd in the leaf $CG/coordd, and every hand start (a shell, `coordc run runner -- ...`, roster) from the leaf
# $CG/launch, which coordd takes as the owner (cgroup.rs identify). A process moves between cgroups only with write
# access to the common ancestor's cgroup.procs: from docker exec's /init that is the container's root, root's, so a
# hand start is moved into $CG/launch as root first and only then becomes agent; from there `coordc run` moves it on to
# $CG/agents/NAME/main, both under $CG. What one uid cannot keep apart, as on the host: an agent may write its own pid
# into $CG/launch and speak as the owner.
CG=/sys/fs/cgroup/swarm
# LAUNCH: as root, into $CG/launch, then exec "$@" as agent with agent's home. Not there (coordd never started in this
# box): said, and the command runs from where it is.
LAUNCH='g=$1; shift
if ! { mkdir -p "$g/launch" && chown agent:agent "$g/launch" "$g/launch/cgroup.procs" && echo $$ >"$g/launch/cgroup.procs"; } 2>/dev/null; then
  echo "box.sh: not in $g/launch (coordd not started in this box?): a coordc run from here cannot place its agent" >&2
fi
exec setpriv --reuid=agent --regid=agent --init-groups env HOME=/home/agent USER=agent LOGNAME=agent "$@"'

inbox() {
  held
  local flags=() root=; while [ "$1" != -- ]; do [ "$1" != "-u" ] || root=1; flags+=("$1"); shift; done; shift
  # LANG: the box has no locale; without UTF-8, tmux draws a client's lines and the hook's ● as _ (6 Nov)
  local env=("${ENVV[@]}" -e LANG=C.UTF-8 -e "PATH=$AB/bin:/home/agent/.local/bin:/usr/local/bin:/usr/bin:/bin")
  if [ -n "$CONF" ] && [ -z "$root" ]; then
    docker exec "${flags[@]}" "${env[@]}" "$BOX" /bin/sh -c "$LAUNCH" box-launch "$CG" "$@"
  else
    "${EXEC[@]}" "${flags[@]}" "${env[@]}" "$BOX" "$@"
  fi
}

# coordd_up: close the agent user's routes to root, then start coordd unless it runs. sbx exec has no detach (-d), so coordd
# is started in its own session from a root exec, which it outlives.
coordd_up() {
  replay_lock
  [ -z "$CONF" ] || { coordd_up_agent; return; }
  "${EXEC[@]}" -u root "${ENVV[@]}" "$BOX" sh -c '
    # sbx gives every exec the sudo and docker GIDs itself, so /etc/group cannot take them away: sudo loses its
    # passwordless rule (agent has no password) and the docker socket the docker group
    rm -f /etc/sudoers.d/agent /etc/sudoers.d/*agent* 2>/dev/null || true
    chgrp root /var/run/docker.sock 2>/dev/null || true
    pgrep -x coordd >/dev/null && exit 0
    # /run is on the container layer, so a docker box keeps the last coordd socket across a stop: without the rm the
    # wait below passed at once and roster resume met "Connection refused" (13 Nov 17:23, after a host boot)
    rm -f /run/coordd.sock
    setsid "$AGENT_BASE/bin/coordd" </dev/null >>"$COORD/coordd.log" 2>&1 &
    p=$!
    # its socket is waited for while coordd runs, with no time limit (the owner, N-68); one that ended will not make it
    until [ -S /run/coordd.sock ]; do
      if ! kill -0 "$p" 2>/dev/null && ! pgrep -x coordd >/dev/null; then
        tail -n 10 "$COORD/coordd.log" >&2 2>/dev/null
        echo "coordd did not start; see $COORD/coordd.log" >&2; exit 1
      fi
      sleep 0.1
    done'
}

# coordd_up_agent: a docker box's coordd, as agent (see LAUNCH above). As root: the agent user's routes to root closed
# (as coordd_up), $CG/coordd and $CG/launch made and $CG handed to agent (cgroup v2 delegation, as host-coordd's unit
# has from systemd's Delegate=yes and DelegateSubgroup=coordd), /run/coordd made agent's for the socket and the live
# journal (/run/coordd/live.json, coordd's default; /run/coordd.sock, the socket's default, a link to
# /run/coordd/coordd.sock, so every caller that names none still finds it), the starting shell moved into $CG/coordd;
# then coordd started as agent. A box whose coordd ran as root before: what that one left in $CG, /run/coordd and
# $COORD (coord_acl) is agent's from here.
coordd_up_agent() {
  # the owner's sub-uid and sub-gid ranges, agent's in the box too: a boxed layer (`box = true`) maps them (sandbox.rs),
  # and what its agents write in $COORD and the repositories is then the same host uids it would be on the host
  local su sg; su=$(awk -F: -v u="$(id -un)" -v i="$(id -u)" '$1 == u || $1 == i { print $2 ":" $3; exit }' /etc/subuid 2>/dev/null)
  sg=$(awk -F: -v u="$(id -un)" -v i="$(id -u)" '$1 == u || $1 == i { print $2 ":" $3; exit }' /etc/subgid 2>/dev/null)
  "${EXEC[@]}" -u root "${ENVV[@]}" -e "CG=$CG" -e "SUBUID=$su" -e "SUBGID=$sg" "$BOX" sh -c '
    for f in subuid subgid; do
      r=$SUBUID; [ $f = subuid ] || r=$SUBGID
      [ -n "$r" ] || continue
      { grep -v "^agent:" /etc/$f 2>/dev/null; echo "agent:$r"; } >/etc/$f.new && mv /etc/$f.new /etc/$f
    done
    rm -f /etc/sudoers.d/agent /etc/sudoers.d/*agent* 2>/dev/null || true
    chgrp root /var/run/docker.sock 2>/dev/null || true
    pgrep -x coordd >/dev/null && exit 0
    { mkdir -p "$CG/coordd" "$CG/launch" /run/coordd && chown -R agent:agent "$CG" /run/coordd &&
      touch "$COORD/coordd.log" && chown agent:agent "$COORD/coordd.log" &&
      rm -f /run/coordd.sock /run/coordd/coordd.sock && ln -s coordd/coordd.sock /run/coordd.sock &&
      echo $$ >"$CG/coordd/cgroup.procs"; } || { echo "coordd not started: $CG or /run/coordd could not be made agent'"'"'s" >&2; exit 1; }
    find "$COORD" -xdev -user 0 -exec chown -h agent:agent {} + 2>/dev/null || true
    HOME=/home/agent USER=agent LOGNAME=agent COORDD_SOCK=/run/coordd/coordd.sock \
      setsid setpriv --reuid=agent --regid=agent --init-groups "$AGENT_BASE/bin/coordd" </dev/null >>"$COORD/coordd.log" 2>&1 &
    p=$!
    # its socket is waited for while coordd runs, with no time limit (the owner, N-68); one that ended will not make it
    until [ -S /run/coordd.sock ]; do
      if ! kill -0 "$p" 2>/dev/null && ! pgrep -x coordd >/dev/null; then
        tail -n 10 "$COORD/coordd.log" >&2 2>/dev/null
        echo "coordd did not start; see $COORD/coordd.log" >&2; exit 1
      fi
      sleep 0.1
    done'
}

# forward_up: the docker box's way to the model route: $COORD/proxy.sock, forwarded by a host socat (a transient user
# unit, so it is not in any caller's terminal and up or start brings it back after a host restart) to the route's host
# and port; the container's entry.sh listens on the same port inside
# coord_acl: what coordd (root in a docker box) creates in $COORD stays writable by the agent user, uid 1000 as the host
# user: a default ACL, inherited by every new file and folder (an sbx box's file sharing maps root's writes to the host
# user itself; docker does not, and CONSULT's mail socket could not be made, 10 Nov)
coord_acl() { setfacl -R -m "u:$(id -u):rwX" -m "d:u:$(id -u):rwX" "$COORD" 2>/dev/null || true; }

# claude_flags: the account's cached feature flags, from the host's ~/.claude.json into the box agent's. Claude Code
# fetches them from Anthropic, which a docker box (no network) cannot reach; without them its channel flag is off and
# every coord mail is dropped on arrival (10 Nov). Only the flag cache is copied, no account data.
claude_flags() {
  local h=$HOME/.claude.json b=$COORD/home/.claude.json
  [ -s "$h" ] || return 0; [ -s "$b" ] || echo '{}' >"$b"
  jq --slurpfile h "$h" '. + ($h[0] | {cachedGrowthBookFeatures, cachedGrowthBookFeaturesAt})' "$b" >"$b.new" &&
    chmod 600 "$b.new" && mv "$b.new" "$b"
}

forward_up() {
  local hp; hp=$(printf '%s' "$anthropic_url" | sed -E 's#^[a-z]+://([^/]+).*#\1#')
  [ -n "$hp" ] || return 0
  systemctl --user is-active -q "box-proxy-$BOX" && return 0
  systemctl --user reset-failed "box-proxy-$BOX" 2>/dev/null || true
  /usr/bin/systemd-run --user -q --unit "box-proxy-$BOX" -p MemoryMax=64M \
    socat UNIX-LISTEN:"$COORD/proxy.sock",unlink-early,mode=600,fork TCP:"$hp"
}

# docker_up: build DIR/Dockerfile and run the container box.conf describes (`up DIR` for a box.conf box)
docker_up() {
  docker container inspect "$BOX" >/dev/null 2>&1 && { echo "$BOX exists; box.sh sh $BOX"; exit 1; }
  local port; port=$(printf '%s' "$anthropic_url" | sed -nE 's#^[a-z]+://[^/:]+:([0-9]+).*#\1#p')
  mkdir -p "$COORD/home"; coord_acl; claude_flags
  route_write_key; forward_up
  docker build -q -t "box-$BOX" "$ENVDIR" >/dev/null
  # the machine's sub-uid registry (coordd sandbox.rs `.subids`, beside the coords), the host's own file: a boxed
  # layer's agent in a box and one in a host project never get the same uid
  local subids; subids=$(dirname "$COORD")/.subids; [ -e "$subids" ] || echo '{}' >"$subids"
  local v=(-v "$COORD:$COORD" -v "$COORD/home:/home/agent" -v "$subids:$subids") m
  for m in "${mounts[@]}"; do case $m in *:ro) v+=(-v "${m%:ro}:${m%:ro}:ro") ;; *) v+=(-v "$m:$m") ;; esac; done
  # SYS_ADMIN, for root only, with apparmor off (docker-default refuses the mount): entry.sh remounts the cgroup tree
  # writable for coordd (owner 10 Nov); the agent user gets no capabilities. PERFMON: docker's seccomp profile passes
  # perf_event_open only with it; the agent user stays bound by the host's perf_event_paranoid (its own processes)
  docker run -d -q --name "$BOX" --hostname "$BOX" --init --restart unless-stopped --network none \
    --memory "$memory" --memory-swap "$memory" --cpus "$cpus" --cap-add PERFMON --cap-add SYS_ADMIN --security-opt apparmor=unconfined \
    "${v[@]}" "${ENVV[@]}" -e "PROXY_PORT=${port:-8317}" "box-$BOX" >/dev/null
  coordd_up
  [ ! -f "$ENVDIR/setup.sh" ] || inbox -i -- bash -s <"$ENVDIR/setup.sh"
  echo "$BOX up: box.sh sh $BOX, then roster up FILE; box.sh sh $BOX coord attach NAME to watch one"
}

case $cmd in
up)
  [ -z "$CONF" ] || { docker_up; exit 0; }
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
  # one start at a time: at a host boot the owner's app and box-autostart@BOX both run it within seconds (13 Nov 17:22);
  # the second waits and then finds coordd up and the agents resumed
  exec 8>"$COORD/box-start.lock"; flock 8
  [ -n "$ENVDIR" ] || { echo "box.sh start DIR (DIR holds sbxenv.yaml)"; exit 2; }
  if [ -n "$CONF" ]; then forward_up; coord_acl; claude_flags; docker start "$BOX" >/dev/null; else sbx run -d --name "$BOX" claude >/dev/null; fi
  # as root (the agent user has no sudo), and before coordd: the project's sources read what start.sh mounts
  [ ! -f "$ENVDIR/start.sh" ] || inbox -u root -i -- bash -s <"$ENVDIR/start.sh"
  route_write_key
  # no $COORD/hold here (owner 14 Nov): start used to write it, from yaaihh misreading m312, and every box start then held
  # all agent mail until someone removed it
  coordd_up
  inbox -- roster resume
  ;;
restart)
  [ -n "$ENVDIR" ] || { echo "box.sh restart BOX: $BOX has no envdir (box.sh start DIR once)"; exit 2; }
  if [ -n "$CONF" ]; then docker stop -t 30 "$BOX" >/dev/null; else sbx stop "$BOX"; fi && "$0" start "$BOX"
  ;;
coordd-stop) held
  "${EXEC[@]}" -u root "$BOX" sh -c 'pkill -x coordd || exit 0; while pgrep -x coordd >/dev/null; do sleep 0.1; done' ;;
coordd) coordd_up ;;  # start coordd in a running box (after an update of bin/coordd: stop it first)
key) route_write_key ;;
# TOOL_PATH (a box file's env: a layer's toolchain) goes before PATH, for the shell and every agent started from it.
# On a terminal (stdin and stdout), CMD gets one in the box too (a shell, `coord attach`); piped (yaaihh's bridge), not.
# docker exec forwards stdin only with -i (sbx exec always): a docker box gets it piped too, for the bridge's commands.
sh) shift 2; [ $# -gt 0 ] || set -- bash; flag=; [ -z "$CONF" ] || flag=-i; [ -t 0 ] && [ -t 1 ] && flag=-it
    inbox $flag -- /bin/sh -c 'PATH=${TOOL_PATH:+$TOOL_PATH:}$PATH exec /usr/bin/env "$@"' sh "$@" ;;
fetch) git -C "$(cat "$COORD/repo")" fetch "sandbox-$BOX" ;;
down) if [ -n "$CONF" ]; then docker stop -t 30 "$BOX"; else sbx stop "$BOX"; fi ;;
hold) shift 2; echo "${*:-held by box.sh hold at $(date +%H:%M)}" >"$COORD/held"; echo "$BOX held" ;;
release) rm -f "$COORD/held"; echo "$BOX released" ;;
*) sed -n '4,20p' "$0" | sed 's/^# //'; exit 2 ;;
esac
