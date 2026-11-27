#!/usr/bin/env bash
# host-coordd-start-test.sh [DIR]: `host-coordd start NAME` one at a time per project (paper-research N-1446 p2119). His
# window's start runs it for every project (projects.rs bring_up) and WSL's may too: two at once both found the unit
# inactive, and the second's `rm -f "$COORDD_SOCK"` removed the socket the first's new coordd had just made, so that
# coordd ran unreachable and the second waited for its socket for ever.
#
# The script runs as it is, in DIR/ab/bin, against stand-ins first on its PATH: systemctl and systemd-run play the user
# unit in a state file (`is-active -q` reads the state, then takes 1 s, the window where two starts met), and the unit's
# "coordd" is a process holding a unix socket at COORDD_SOCK. The one change: `command -p systemd-run` (the system's,
# past PATH) is run as `systemd-run`, the stand-in. HOME and XDG_RUNTIME_DIR are DIR's; no real unit is touched.
#
# Two legs:
# 1. two starts at once, the second 0.3 s after the first: both exit 0; coordd is started once, by the first; the
#    second found it active and only resumed; the socket is there at the end; neither the pins nor roster resume held
#    the lock (fd 9).
# 2. a start waiting on the lock while release.sh's coord-switch begins (its mark SOCK.d/switching, a live pid and its
#    start time): it lets the lock go and waits for the switch, whose own start (YAAIHH_SWITCH_OWNER) takes the lock and
#    starts coordd again; the waiting start goes on only once the mark is gone, and finds the unit active.
# 3. a start in roster resume holds no lock (paper-research N-1446 p2135: what roster resume waits for may wait for
#    coordd, and a switch's start on the lock for it): with the first blocked in roster resume, a second start goes
#    through.
#
# HC=FILE runs another host-coordd (an older one, for its red). DIR is kept when given, else a temporary one is removed.
set -euo pipefail
HERE=$(cd "$(dirname "$0")/../.." && pwd)
HC=${HC:-$HERE/agent-base/bin/host-coordd}
if [ -n "${1:-}" ]; then mkdir -p "$1"; D=$(cd "$1" && pwd -P); else D=$(cd "$(mktemp -d)" && pwd -P); fi
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }

AB=$D/ab NAME=startlock
export HOME=$D/home XDG_RUNTIME_DIR=$D/run
run=$XDG_RUNTIME_DIR/coordd sock=$XDG_RUNTIME_DIR/coordd/$NAME.sock state=$D/state calls=$D/calls pids=$D/pids
rm -rf "$AB" "$HOME" "$XDG_RUNTIME_DIR" "$state" "$calls" "$pids" "$D/roster-block" "$D/roster-go"
mkdir -p "$AB/bin" "$AB/lib" "$HOME" "$XDG_RUNTIME_DIR"
: >"$calls"; : >"$pids"
# coordd_stop: the stand-in coordds (their pids in $pids) stopped; sw is the switch's stand-in process (leg 2)
sw=
coordd_stop() { while read -r p; do kill "$p" 2>/dev/null || true; done <"$pids"; : >"$pids"; }
done_all() { coordd_stop; [ -z "$sw" ] || kill "$sw" 2>/dev/null || true; }
if [ -z "${1:-}" ]; then trap 'done_all; rm -rf "$D"' EXIT; else trap done_all EXIT; fi

[ "$(grep -c 'command -p systemd-run' "$HC")" = 1 ] || { echo "host-coordd-start-test: $HC has no one \`command -p systemd-run\`"; exit 2; }
sed 's/command -p systemd-run/systemd-run/' "$HC" >"$AB/bin/host-coordd"
cp "$HERE/agent-base/lib/route.sh" "$AB/lib/route.sh"

# systemctl --user VERB [-q] [UNIT] [-p PROP --value]: the unit's state is $state (active, or no file)
cat >"$AB/bin/systemctl" <<EOF
#!/usr/bin/env bash
[ "\$1" = --user ] && shift
verb=\$1; shift
q=; [ "\${1:-}" = -q ] && { q=1; shift; }
st=inactive; [ -f "$state" ] && st=\$(cat "$state")
case \$verb in
is-active)
  if [ -n "\$q" ]; then echo "\${TAG:-?} is-active-q \$st" >>"$calls"; sleep 1; [ "\$st" = active ]
  else echo "\$st"; [ "\$st" = active ]; fi ;;
show)
  case "\$*" in
  *LoadState*) [ "\$st" = active ] && echo loaded || echo not-found ;;
  *Environment*) echo "AGENT_BASE=$AB PATH=$AB/bin" ;;
  esac ;;
start | reset-failed | daemon-reload) echo "\${TAG:-?} systemctl \$verb" >>"$calls" ;;
*) echo "systemctl stand-in: no \$verb" >&2; exit 1 ;;
esac
EOF
# systemd-run --user ... --unit UNIT ... --setenv=COORDD_SOCK=S ... CMD: the unit's coordd is a process with a socket at S
# (its fds past stderr closed, as a unit's process has none of its starter's)
cat >"$AB/bin/systemd-run" <<EOF
#!/usr/bin/env bash
s=
for a in "\$@"; do case \$a in --setenv=COORDD_SOCK=*) s=\${a#--setenv=COORDD_SOCK=} ;; esac; done
if [ -f "$state" ]; then echo "\${TAG:-?} systemd-run refused: unit active" >>"$calls"; echo "Failed to start transient service unit: Unit already exists." >&2; exit 1; fi
echo active >"$state"
echo "\${TAG:-?} systemd-run" >>"$calls"
python3 -I -c 'import socket, sys, time
s = socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(); time.sleep(60)' "\$s" </dev/null >/dev/null 2>&1 9>&- &
echo \$! >>"$pids"
EOF
# roster resume and the pins: what they were run with, and whether the start's lock (fd 9) reached them; roster resume
# of the start named in $D/roster-block waits until $D/roster-go is there
cat >"$AB/bin/roster" <<EOF
#!/usr/bin/env bash
echo "\${TAG:-?} roster \$1 fd9=\$([ -e /proc/self/fd/9 ] && echo open || echo closed)" >>"$calls"
if [ "\${TAG:-}" = "\$(cat "$D/roster-block" 2>/dev/null)" ]; then
  echo "\$TAG roster blocked" >>"$calls"
  while [ ! -e "$D/roster-go" ]; do sleep 0.1; done
fi
EOF
for f in opencode-pin agy-pin; do
  cat >"$AB/bin/$f" <<EOF
#!/usr/bin/env bash
echo "\${TAG:-?} $f \$1 fd9=\$([ -e /proc/self/fd/9 ] && echo open || echo closed)" >>"$calls"
EOF
done
printf '#!/bin/sh\nexit 1\n' >"$AB/bin/coordd"
chmod +x "$AB/bin/"*

start() { TAG=$1 timeout 10 "$AB/bin/host-coordd" start "$NAME" >"$D/out-$1" 2>&1; }
has() { grep -qxF "$1" "$calls"; }
count() { grep -cxF "$1" "$calls" || true; }

# leg 1: two starts at once
start A & a=$!
sleep 0.3
start B & b=$!
ra=0; wait $a || ra=$?
rb=0; wait $b || rb=$?
[ "$ra $rb" = "0 0" ] && ok "two starts at once both exit 0" || bad "two starts at once exit $ra and $rb (124: timed out waiting for the socket)"
[ "$(count 'A systemd-run')$(count 'B systemd-run')" = 10 ] && ! grep -q 'refused' "$calls" \
  && ok "coordd is started once, by the first" || bad "coordd's starts: $(grep systemd-run "$calls" | tr '\n' ';')"
has 'B is-active-q active' && ok "the second found the unit active" || bad "the second found it $(grep '^B is-active-q' "$calls" | tr '\n' ';')"
[ -S "$sock" ] && ok "the socket is there at the end" || bad "the socket is gone: coordd runs unreachable"
[ "$(count 'A roster resume fd9=closed')$(count 'B roster resume fd9=closed')" = 11 ] && ok "both resumed, without the lock" \
  || bad "resumes: $(grep roster "$calls" | tr '\n' ';')"
[ "$(grep -c 'pin .* fd9=closed' "$calls")" = 4 ] && ! grep -q 'fd9=open' "$calls" && ok "the pins ran without the lock" \
  || bad "pins: $(grep pin "$calls" | tr '\n' ';')"
[ -f "$run/$NAME.start.lock" ] && ok "the lock is RUN/NAME.start.lock" || bad "no $run/$NAME.start.lock"

# leg 2: a switch begins while a start waits on the lock
coordd_stop; rm -f "$state" "$sock"; : >"$calls"
sleep 60 & sw=$!
st=$(cat /proc/$sw/stat); st=${st##*) }; read -ra st <<<"$st"
start A & a=$!
sleep 0.3
start B & b=$!
sleep 0.3
mkdir -p "$sock.d"; echo "$sw ${st[19]}" >"$sock.d/switching"
ra=0; wait $a || ra=$?
sleep 0.6
# the switch: its coordd stopped, then its own start
coordd_stop; rm -f "$state" "$sock"
rc=0; TAG=C YAAIHH_SWITCH_OWNER=$sw timeout 10 "$AB/bin/host-coordd" start "$NAME" >"$D/out-C" 2>&1 || rc=$?
echo "switch done" >>"$calls"; rm -f "$sock.d/switching"
rb=0; wait $b || rb=$?
[ "$ra $rb $rc" = "0 0 0" ] && ok "the first, the waiting start and the switch's start exit 0" || bad "exits: first $ra, waiting $rb, switch's $rc"
has 'C systemd-run' && ok "the switch's own start took the lock and started coordd" || bad "the switch's start: $(grep '^C ' "$calls" | tr '\n' ';')"
b1=$(grep -n '^B is-active-q' "$calls" | head -1 | cut -d: -f1 || true); d1=$(grep -n '^switch done$' "$calls" | cut -d: -f1 || true)
[ -n "$b1" ] && [ "$b1" -gt "$d1" ] && has 'B is-active-q active' && ! grep -q '^B systemd-run' "$calls" \
  && ok "the waiting start went on only after the switch, and found the unit active" \
  || bad "the waiting start went on before the switch was done: $(tr '\n' ';' <"$calls")"
grep -q 'being switched' "$D/out-B" && ok "it said it waited for the switch" || bad "it did not say it waited: $(cat "$D/out-B")"
[ -S "$sock" ] && ok "the socket is there at the end" || bad "the socket is gone"

# leg 3: a start in roster resume holds no lock
coordd_stop; rm -f "$sock" "$D/roster-go"; : >"$calls"; echo active >"$state"; echo A >"$D/roster-block"
start A & a=$!
for _ in $(seq 50); do has 'A roster blocked' && break; sleep 0.1; done
has 'A roster blocked' && ok "the first start is in roster resume, held there" || bad "the first start never reached roster resume: $(tr '\n' ';' <"$calls")"
start B & b=$!
for _ in $(seq 50); do kill -0 $b 2>/dev/null || break; sleep 0.1; done
if kill -0 $b 2>/dev/null; then bad "the second start still waits 5 s on, the lock held by the first's roster resume"
else rb=0; wait $b || rb=$?; [ "$rb" = 0 ] && has 'B is-active-q active' && has 'B roster resume fd9=closed' \
  && ok "a second start goes through while the first is in roster resume" || bad "the second start: rc $rb, $(grep '^B ' "$calls" | tr '\n' ';')"; fi
touch "$D/roster-go"
ra=0; wait $a || ra=$?; wait $b 2>/dev/null || true
[ "$ra" = 0 ] && ok "the first ends once roster resume does" || bad "the first exits $ra"

[ "$fails" = 0 ] && echo "host-coordd-start-test: all ok" || { echo "host-coordd-start-test: $fails failed"; exit 1; }
