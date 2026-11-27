#!/usr/bin/env bash
# dev.sh: the dev yaaihh (ops/dev/README.md, the owner N-299.p6): its own releases, its own config, its own
# coordds and its own app, beside prod. Prod's release links, units, sockets and ~/.config/yaaihh are never touched.
#
#   dev.sh move [--no-app] [--boxed] [NAME]  move dev to release NAME of docket-core's releases (default: the newest).
#                        - NAME is copied into dev's releases.
#                        - Each dev coord's state is copied aside ($COORD/aside/TAG).
#                        - `current` points at NAME, and the release it pointed at before is kept as `previous`.
#                        - Each dev coordd that runs is restarted on NAME, and so is dev's app.
#   dev.sh back [--no-app] [--boxed]   the way back: `current` and `previous` change places, and dev's coordds and app restart
#   dev.sh restore [--boxed] COORD [TAG]   put coord COORD's state back as the last move copied it aside (or as TAG), and restart
#                        its coordd. Its state as it is now is copied aside first. What happened after the move is lost.
#   dev.sh aside COORD   copy coord COORD's state aside as a move does, and change nothing (before it goes to prod: README, D)
#   dev.sh app           (re)start dev's app on `current`
#   dev.sh list          dev's releases (current and previous marked), then dev's coords and the release each coordd
#                        runs, and the boxed agents running under it
#
# A coordd restart under a running boxed agent is refused (move, back, restore) while `current`'s box-enter binds
# coordd's socket into the box as a file, not its folder (`SOCK.d`, from docket-core5's 1c08efc): host-coordd's start makes a new socket, and the box keeps the old one, so
# coordd can hand that agent nothing (paper-research, N-101 p1235). --boxed restarts anyway; each agent named then
# needs `roster restart NAME` (as the owner) after it.
#
# Environment, with the defaults:
#   DEV=/home/t/Documents/papers/soft-swarm-dev             dev's folder; it holds releases/
#   YAAIHH_DEV_CONFIG=~/.config/yaaihh-devline/projects.toml   dev's app config; its projects with `boxed = false` are
#                                                           dev's coords
#   FROM=/home/t/Documents/papers/soft-swarm-docket-rel/releases   where docket-core builds the releases
#
# Each dev coordd's unit is coordd-ID, started by `host-coordd start ID` of dev's own `current` release. Its AGENT_BASE
# is $DEV/releases/current/agent-base, so a restart after a move runs the new release (host-coordd's UAB). A coord
# whose coordd runs another agent-base (prod's) is refused: dev moves only what dev runs.
set -euo pipefail
DEV=${DEV:-/home/t/Documents/papers/soft-swarm-dev}
CONF=${YAAIHH_DEV_CONFIG:-$HOME/.config/yaaihh-devline/projects.toml}
FROM=${FROM:-/home/t/Documents/papers/soft-swarm-docket-rel/releases}
rel=$DEV/releases
uab=$rel/current/agent-base
app=1
force_boxed=0

die() { echo "dev.sh: $*" >&2; exit 1; }
usage() { sed -n '5,25p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
name_of() { readlink "$rel/$1" 2>/dev/null || true; }
# set_link LINK NAME: releases/LINK points at NAME, in one rename (release.sh's)
set_link() { ln -sfn "$2" "$rel/$1.new" && mv -T "$rel/$1.new" "$rel/$1"; }
stamp() { date -u +%Y%m%dT%H%M%SZ; }

# config KEY: from dev's config, `agent_base` or `coords` (the ids of its host projects, one per line); a boxed project
# in it is refused, since dev runs host coords only
config() {
  [ -f "$CONF" ] || die "no dev config $CONF (README, a)"
  python3 -I - "$CONF" "$1" <<'EOF'
import sys, tomllib
c = tomllib.load(open(sys.argv[1], "rb"))
if sys.argv[2] == "agent_base":
    print(c.get("agent_base", ""))
else:
    for p in c.get("project", []):
        i = p.get("id") or p.get("box")
        if p.get("boxed", True):
            sys.exit(f"dev.sh: {sys.argv[1]}: project {i} is boxed; dev runs host coords only (boxed = false)")
        print(i)
EOF
}

# runs ID: the AGENT_BASE coordd-ID's unit has (empty when the unit is not loaded)
runs() {
  [ "$(systemctl --user show "coordd-$1" -p LoadState --value)" = loaded ] || return 0
  systemctl --user show "coordd-$1" -p Environment --value | tr ' ' '\n' | sed -n 's/^AGENT_BASE=//p' | tail -1
}

# ours ID: refused unless coordd-ID is not loaded or was started by dev (its AGENT_BASE is dev's)
ours() {
  local a; a=$(runs "$1")
  [ -z "$a" ] || [ "$a" = "$uab" ] || case $a in "$rel"/*) ;; *) die "coordd-$1 runs AGENT_BASE=$a, not dev's ($uab): dev.sh moves only coords dev runs (README, c: C1 starts Tovrin's on dev)" ;; esac
}

# the coord's files a move copies aside, relative to $COORD, and the project's (release.sh's STATE and aside, the same
# copy, so `release.sh`'s and dev's aside folders read alike: ops/release/way-back.md)
STATE=(tickets registry.jsonl reconcile.jsonl running.json resume.json layers/layers.json project)
aside() {  # aside COORD TAG
  local d=$1/aside/$2 f pf pd
  mkdir -p "$d/coord" "$d/project"
  for f in "${STATE[@]}"; do
    [ -e "$1/$f" ] || continue
    mkdir -p "$d/coord/$(dirname "$f")"
    cp -a "$1/$f" "$d/coord/$f"
  done
  if [ -s "$1/project" ]; then
    pf=$(cat "$1/project") pd=$(dirname "$pf")
    echo "$pf" >"$d/project-file"
    cp -a "$pf" "$d/project/"
    for f in kinds calls policy; do [ -d "$pd/$f" ] && cp -a "$pd/$f" "$d/project/"; done
  fi
  echo "aside: $d ($(du -sh "$d" | cut -f1))"
}

restore_from() {  # restore_from COORD TAG (release.sh's restore)
  local d=$1/aside/$2 f pf pd
  [ -d "$d/coord/tickets" ] || die "$d holds no record"
  rm -rf "$1/tickets"; cp -a "$d/coord/tickets" "$1/tickets"
  for f in running.json resume.json reconcile.jsonl; do
    rm -f "${1:?}/$f"; [ ! -e "$d/coord/$f" ] || cp -a "$d/coord/$f" "$1/$f"
  done
  if [ -s "$d/project-file" ]; then
    pf=$(cat "$d/project-file") pd=$(dirname "$pf")
    cp -a "$d/project/$(basename "$pf")" "$pf"
    echo "$pf" >"$1/project"
    for f in kinds calls policy; do
      [ -d "$d/project/$f" ] || continue
      rm -rf "${pd:?}/$f"; cp -a "$d/project/$f" "$pd/$f"
    done
  fi
  echo "restored from $d"
}

# boxed ID: the boxed agents running under coordd-ID, one per line: an agents/NAME cgroup of its unit with a process
# whose uid is a sub-uid (a box's, >= 100000)
boxed() {
  local cg f n p u
  cg=$(systemctl --user show "coordd-$1" -p ControlGroup --value)
  [ -n "$cg" ] && [ -d "/sys/fs/cgroup$cg/agents" ] || return 0
  find "/sys/fs/cgroup$cg/agents" -name cgroup.procs | while read -r f; do
    n=${f#"/sys/fs/cgroup$cg/agents/"}; n=${n%%/*}
    while read -r p; do
      u=$(awk '/^Uid:/{print $2; exit}' "/proc/$p/status" 2>/dev/null) || continue
      [ -n "$u" ] && [ "$u" -ge 100000 ] && { echo "$n"; break; }
    done <"$f"
  done | sort -u
}

# file_bind: `current`'s box-enter binds coordd's socket as a file (the boxes running now were made by it): it lacks
# the bind of the socket's folder (docket-core5 1c08efc, N-101 p1281: `SOCK.d`, which a release's coordd always makes;
# the file bind stays there only as the fallback for a coordd that made none)
file_bind() { ! grep -qF 'mount --bind "$sock.d" "$d/tmp/.coordd"' "$uab/bin/box-enter" 2>/dev/null; }

# no_boxed ID...: refused when a coordd among them that runs has a boxed agent running and the socket is bound as a file
no_boxed() {
  [ "$force_boxed" = 0 ] && file_bind || return 0
  local id b bad=
  for id in "$@"; do
    systemctl --user is-active -q "coordd-$id" || continue
    b=$(boxed "$id" | tr '\n' ' ')
    [ -z "$b" ] || bad="$bad coordd-$id: ${b% };"
  done
  [ -z "$bad" ] || die "boxed agents run:$bad a coordd restart leaves their boxes on the old socket (box-enter binds it as a file), so coordd could hand them nothing. Stop them first, or give --boxed and roster restart each after"
}

coordd_stop() {  # coordd_stop ID: coordd stopped and waited for; its agents and holders go on (KillMode=process)
  systemctl --user stop "coordd-$1"
  while systemctl --user is-active -q "coordd-$1"; do sleep 0.2; done
}

coordd_start() {  # coordd_start ID: started by dev's current host-coordd, the lines its replay refused shown
  local c=$HOME/agent-coord/$1 since
  since=$(wc -c <"$c/coordd.log" 2>/dev/null || echo 0)
  "$uab/bin/host-coordd" start "$1"
  echo "coordd-$1 runs $(name_of current); what its load said about the record:"
  tail -c +"$((since + 1))" "$c/coordd.log" | grep -E 'coordd: tickets' || echo "  (nothing: the record replays)"
}

# restart TAG: each dev coord checked first (all or none), then each whose coordd runs: copied aside under TAG (when
# TAG is given), stopped and started on `current`; one not running is left so, and said
# (the coords: $ids, read by the caller, so a refused config stops the script: a failure inside `for x in $(...)` would not)
restart() {
  local id
  for id in $ids; do ours "$id"; done
  for id in $ids; do
    if ! systemctl --user is-active -q "coordd-$id"; then echo "coordd-$id is not running: left so"; continue; fi
    [ -z "${1:-}" ] || aside "$HOME/agent-coord/$id" "$1"
    coordd_stop "$id"
    coordd_start "$id"
  done
}

start_app() {  # dev's app on `current`, as its own user unit (never in the caller's cgroup, as release.sh's run_yaaihh);
  # --dev: its own tray icon, never the proxy's usage recording; --replace: the dev app running on this config quits
  # first (its socket is named after the config's folder, tray.rs, so prod's app is not this one)
  local want; want=$(config agent_base)
  [ "$want" = "$uab" ] || die "$CONF's agent_base is '$want', not $uab: the app would start coordds of another release"
  /usr/bin/systemd-run --user --collect --quiet --unit="yaaihh-devline-$(date +%s%N)" -p WorkingDirectory="$HOME" \
    --setenv=YAAIHH_CONFIG="$CONF" "$rel/current/yaaihh" --dev --replace
  echo "dev's app started on $(name_of current)"
}

newest() { ls "$FROM" | grep -E '^[0-9]{8}-[0-9]{6}-[0-9a-f]+$' | sort | tail -1; }

case ${1:-} in
  move)
    shift; [ "${1:-}" = --no-app ] && { app=0; shift; }
    [ "${1:-}" = --boxed ] && { force_boxed=1; shift; }
    r=${1:-$(newest)}
    [ -n "$r" ] || die "no release in $FROM"
    [[ $r =~ ^[0-9]{8}-[0-9]{6}-[0-9a-f]+$ ]] || die "$r is not a release of a committed tree (DATE-TIME-SHA)"
    [ -x "$FROM/$r/coordd" ] && [ -x "$FROM/$r/yaaihh" ] && [ -d "$FROM/$r/agent-base/kinds" ] || [ -d "$rel/$r" ] || die "$FROM/$r is no whole release"
    old=$(name_of current)
    [ "$old" != "$r" ] || { echo "dev is on $r already"; exit 0; }
    ids=$(config coords)
    for id in $ids; do ours "$id"; done
    no_boxed $ids
    mkdir -p "$rel"
    if [ ! -d "$rel/$r" ]; then
      rm -rf "${rel:?}/$r.part"
      cp -a "$FROM/$r" "$rel/$r.part"
      mv -T "$rel/$r.part" "$rel/$r"
    fi
    set_link current "$r"
    [ -z "$old" ] || set_link previous "$old"
    restart "$(stamp)-dev-move-${old:-none}-to-$r"
    [ "$app" = 0 ] || start_app
    echo "dev: $r (previous: ${old:-none}; dev.sh back is the way back)"
    ;;
  back)
    shift; [ "${1:-}" = --no-app ] && { app=0; shift; }
    [ "${1:-}" = --boxed ] && force_boxed=1
    cur=$(name_of current) prev=$(name_of previous)
    [ -n "$prev" ] && [ -d "$rel/$prev" ] || die "no previous release to go back to"
    ids=$(config coords)
    for id in $ids; do ours "$id"; done
    no_boxed $ids
    set_link current "$prev"
    set_link previous "$cur"
    restart ""
    [ "$app" = 0 ] || start_app
    echo "dev: back on $prev (previous: $cur). If a coordd above refused lines of its record, dev.sh restore ID puts back the copy of its last move"
    ;;
  restore)
    shift; [ "${1:-}" = --boxed ] && { force_boxed=1; shift; }
    set -- restore "$@"
    id=${2:-}; [ -n "$id" ] || usage
    [[ $id =~ ^[A-Za-z0-9_-]+$ ]] || die "not a coord name: $id"
    ids=$(config coords)
    printf '%s\n' $ids | grep -qxF "$id" || die "$id is not one of dev's coords ($CONF)"
    ours "$id"
    no_boxed "$id"
    c=$HOME/agent-coord/$id
    tag=${3:-$(ls "$c/aside" 2>/dev/null | grep -- '-dev-move-' | sort | tail -1)}
    [ -n "$tag" ] && [ -d "$c/aside/$tag" ] || die "no move of $id aside in $c/aside"
    coordd_stop "$id"
    aside "$c" "$(stamp)-dev-restore-from-$(name_of current)"
    restore_from "$c" "$tag"
    coordd_start "$id"
    ;;
  aside)
    id=${2:-}; [ -n "$id" ] || usage
    [[ $id =~ ^[A-Za-z0-9_-]+$ ]] || die "not a coord name: $id"
    [ -d "$HOME/agent-coord/$id" ] || die "no coord $HOME/agent-coord/$id"
    aside "$HOME/agent-coord/$id" "$(stamp)-dev-aside-$(name_of current)"
    ;;
  app) start_app ;;
  list)
    cur=$(name_of current) prev=$(name_of previous)
    for r in $(ls "$rel" 2>/dev/null | grep -Ev '^(current|previous)$|\.part$' | sort); do
      m=; [ "$r" = "$cur" ] && m=" (current)"; [ "$r" = "$prev" ] && m=" (previous)"; echo "$r$m  $(du -sh "$rel/$r" | cut -f1)"
    done
    ids=$(config coords)
    for id in $ids; do
      a=$(runs "$id"); echo "coordd-$id: $(systemctl --user is-active "coordd-$id" || true), AGENT_BASE ${a:-none}, boxed agents running: $(boxed "$id" | tr '\n' ' ')"
    done
    ;;
  *) usage ;;
esac
