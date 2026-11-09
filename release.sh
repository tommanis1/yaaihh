#!/usr/bin/env bash
# release.sh: releases of the swarm's own programs (yaaihh, coordd, coordc, coord-pty), so a change goes live without
# replacing what runs, and comes back out the same way.
#
#   release.sh build          build this checkout into releases/NAME (NAME = date-time and commit); nothing changes.
#                             With a flavour checked out at flavor/ (its app/), yaaihh is the flavour's: the core with
#                             its extensions (the proxy's pages, from proxy/), and the plugin page window beside it
#   release.sh dev [NAME]     run NAME's yaaihh beside prod (--dev: own tray icon, never the proxy's usage recording)
#   release.sh live [NAME]    make NAME (default: the newest) prod: `current` points at it, `previous` at the old one,
#                             and yaaihh restarts from it (the old one quits, the new one takes the usage recording)
#   release.sh rollback       current and previous swap, and yaaihh restarts from the one now current
#   release.sh list           the releases, current and previous marked
#
# What the links change: agent-base/bin/{coordd,coordc,coord-pty} point into releases/current, so a coordd, holder or
# coordc started after `live` is the new one; one already running keeps running from its own folder. coordd itself
# is restarted by hand when wanted (its agents and holders keep running; it finds them again):
#   host project:  kill the project's coordd (by pid), then agent-base/bin/host-coordd start NAME
#   box:           sbx exec -u root BOX kill PID, then agent-base/box/box.sh coordd BOX
# Releases live under soft-swarm (not ~/.local) because boxes see soft-swarm read-only and nothing else of the host's.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
rel=$here/releases bin=$here/agent-base/bin progs=(yaaihh coordd coordc coord-pty)
mkdir -p "$rel"

name_of() { readlink "$rel/$1" 2>/dev/null || true; }

# yaaihh from release $1 as its own user unit, never inside the caller's cgroup (an agent's cgroup makes coordd take
# it for that agent: 7 Nov, its own chat could not be read); extra arguments go to yaaihh
run_yaaihh() {
  local r=$1; shift
  /usr/bin/systemd-run --user --collect --quiet --unit="yaaihh-$(date +%s%N)" -p WorkingDirectory="$HOME" \
    "$rel/$r/yaaihh" "$@"
}

# point bin's coord programs into releases/current (once; relative, so the box's read-only mount reads the same)
link_bin() {
  for p in coordd coordc coord-pty; do
    [ "$(readlink "$bin/$p" 2>/dev/null)" = "../../releases/current/$p" ] || ln -sfn "../../releases/current/$p" "$bin/$p"
  done
}

go_live() {  # go_live NAME: current = NAME, previous = what was current
  local r=$1 old; old=$(name_of current)
  [ -d "$rel/$r" ] || { echo "release.sh: no release $r" >&2; exit 1; }
  [ "$old" = "$r" ] && { echo "release.sh: $r is current already"; return; }
  [ -n "$old" ] && ln -sfn "$old" "$rel/previous.new" && mv -T "$rel/previous.new" "$rel/previous"
  ln -sfn "$r" "$rel/current.new" && mv -T "$rel/current.new" "$rel/current"
  link_bin
  run_yaaihh "$r" --replace
  echo "live: $r (previous: ${old:-none}); yaaihh restarted from it"
}

case ${1:-} in
  build)
    cd "$here/yaaihh"  # yaaihh/.cargo/config.toml (local patches) applies to every build below
    if [ -f "$here/flavor/app/Cargo.toml" ]; then
      cargo build --release -p coordd -p coord-pty
      # the flavour's own workspace, into yaaihh's target (one build cache); its binary is also called yaaihh
      cargo build --release --manifest-path "$here/flavor/app/Cargo.toml" --target-dir target
      if [ -f "$here/proxy/Cargo.toml" ]; then
        cargo build --release --manifest-path "$here/proxy/Cargo.toml" -p yaaihh-webview --target-dir target
        progs+=(yaaihh-webview)
      fi
    else
      cargo build --release -p yaaihh -p coordd -p coord-pty
    fi
    sha=$(git -C "$here" rev-parse --short HEAD)
    git -C "$here" diff --quiet HEAD -- yaaihh || sha=$sha-dirty
    for repo in flavor proxy; do
      if [ -d "$here/$repo/.git" ] && ! git -C "$here/$repo" diff --quiet HEAD; then sha=${sha%-dirty}-dirty; fi
    done
    r=$(date +%Y%m%d-%H%M%S)-$sha
    mkdir "$rel/$r.part"
    for p in "${progs[@]}"; do install -m 755 "$here/yaaihh/target/release/$p" "$rel/$r.part/$p"; done
    mv "$rel/$r.part" "$rel/$r"
    echo "$r"
    ;;
  dev)
    r=${2:-$(ls "$rel" | grep -v -e current -e previous -e '\.part$' | sort | tail -1)}
    run_yaaihh "$r" --dev
    echo "dev: $r beside prod (Quit it from its tray icon, \"yaaihh dev\")"
    ;;
  live)
    go_live "${2:-$(ls "$rel" | grep -v -e current -e previous -e '\.part$' | sort | tail -1)}"
    ;;
  rollback)
    prev=$(name_of previous)
    [ -n "$prev" ] || { echo "release.sh: no previous release" >&2; exit 1; }
    go_live "$prev"
    ;;
  list)
    cur=$(name_of current) prev=$(name_of previous)
    for r in $(ls "$rel" | grep -v -e current -e previous -e '\.part$' | sort); do
      mark=; [ "$r" = "$cur" ] && mark=" (current)"; [ "$r" = "$prev" ] && mark=" (previous)"; echo "$r$mark"
    done
    ;;
  *) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
