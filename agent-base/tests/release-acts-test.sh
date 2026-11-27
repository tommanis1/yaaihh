#!/usr/bin/env bash
# release-acts-test.sh [DIR]: the record's line of each cut and install release.sh makes, with its way back (paper-
# research N-1446 p1491, lack 7: "release.sh writes its cut and its install as acts, with the way back"): `coordc act
# release TEXT FIELD=VALUE...`, one line per step, saying what is live after it (releases/'s current and previous, his
# window's release and the one before it, each with its whole commit) and the command that undoes it. A copy of this
# checkout's release.sh runs in DIR/tree, a git repository of its own with made-up commits and made-up releases. The
# stand-ins, all in DIR: AGENT_BASE's coordc writes down its arguments; host-coordd, systemctl and logger (first on
# PATH) write down their calls and start, stop and log nothing; C: is DIR/c (YAAIHH_WIN_C) with a powershell.exe that
# shows no window open; HOME is DIR/home, so the coord the switch moves (`acts`) and the MCP folder are DIR's. No coordd,
# no unit, no journal, no Windows and no record of anyone's are reached; --no-app keeps the usage recording out of it.
# coordc runs handed off (proto-window p1542), so a line lands after its step has ended: each check waits for the lines
# it wants, up to 10 s, and one that wants none waits 1 s. Without DIR, a mktemp folder, removed after.
set -uo pipefail
HERE=$(cd "$(dirname "$0")/../.." && pwd)
if [ -n "${1:-}" ]; then D=$1; rm -rf "$D"; mkdir -p "$D"; else D=$(mktemp -d); trap 'rm -rf "$D"' EXIT; fi
T=$D/tree C=$D/c
mkdir -p "$T/agent-base/bin" "$T/yaaihh" "$T/releases" "$C/yaaihh" "$C/windows/System32/WindowsPowerShell/v1.0" \
  "$D/bin" "$D/ab/bin" "$D/ab-fail/bin" "$D/ab-slow/bin" "$D/from" "$D/home/agent-coord/acts/tickets" \
  "$D/home/agent-coord/acts/mcp" "$D/run"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/release.sh" "$T/ops/release/release.sh"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/bundle-v2.sh" "$HERE/ops/release/put.sh" "$T/ops/release/"
acts=$D/acts.jsonl calls=$D/calls; : >"$acts"; : >"$calls"
cat >"$D/ab/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$acts", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
printf '#!/bin/sh\necho "coordc: no coordd answers" >&2\nexit 1\n' >"$D/ab-fail/bin/coordc"
printf '#!/bin/sh\nsleep 3\nexec "%s" "$@"\n' "$D/ab/bin/coordc" >"$D/ab-slow/bin/coordc"  # a slow coordc: its line lands 3 s on
# host-coordd: start says the record loaded (coordd_start reads coordd.log), sh answers `coord ls` with no agent
cat >"$T/agent-base/bin/host-coordd" <<EOF
#!/bin/sh
echo "host-coordd \$*" >>"$calls"
case \$1 in start) echo "coordd: tickets: 0 problem lines" >>"$D/home/agent-coord/\$2/coordd.log" ;; esac
exit 0
EOF
cat >"$D/bin/systemctl" <<EOF
#!/bin/sh
echo "systemctl \$*" >>"$calls"
case " \$* " in *" is-active "*) exit 3 ;; esac
exit 0
EOF
# logger: its words from its arguments, else each line of stdin, as logger takes them (act_journal: a line's coordc
# that failed); an empty stdin logs nothing
cat >"$D/bin/logger" <<EOF
#!/bin/sh
if [ \$# -gt 2 ]; then echo "logger \$*" >>"$calls"; else while IFS= read -r l; do echo "logger \$* \$l" >>"$calls"; done; fi
EOF
printf '#!/bin/sh\nexit 0\n' >"$C/windows/System32/WindowsPowerShell/v1.0/powershell.exe"  # no window open
chmod +x "$D/ab/bin/coordc" "$D/ab-fail/bin/coordc" "$D/ab-slow/bin/coordc" "$T/agent-base/bin/host-coordd" \
  "$D/bin/systemctl" "$D/bin/logger" "$C/windows/System32/WindowsPowerShell/v1.0/powershell.exe"
export HOME=$D/home XDG_RUNTIME_DIR=$D/run YAAIHH_WIN_C=$C PATH=$D/bin:$PATH GIT_CONFIG_NOSYSTEM=1 AGENT_BASE=$D/ab
unset YAAIHH_WIN_DIR RELEASE_ACTS
g() { git -C "$T" -c core.hooksPath=/dev/null -c user.name=test -c user.email=test@example.invalid "$@"; }
g init -q -b main
echo A >"$T/f" && g add f agent-base && g commit -qm A && a=$(g rev-parse --short HEAD) A=$(g rev-parse HEAD)
echo B >"$T/f" && g add f && g commit -qm B && b=$(g rev-parse --short HEAD) B=$(g rev-parse HEAD)
OLD=20261122-100000-$a NEW=20261122-110000-$b W1=20261122-120000-edge-$b-on-$a
mkdir -p "$T/releases/$OLD" && echo "made-up exe OLD" >"$T/releases/$OLD/yaaihh.exe"
ln -s "$OLD" "$T/releases/current"
echo "$OLD" >"$C/yaaihh/release.txt" && cp "$T/releases/$OLD/yaaihh.exe" "$C/yaaihh/yaaihh.exe"
for p in coordd coordc coord-pty docket-ls; do echo "made-up $p" >"$D/from/$p"; done
# the coordd a switch runs as `coordd save-replay COORD` (I-1509): one from before it, which refuses any argument as
# a real one does (main.rs: usage, exit 2), so the switch says the replay was not saved and goes on
printf '#!/bin/sh\necho "usage: coordd (no arguments; started by host-coordd or box.sh with COORD, COORDD_SOCK and AGENT_BASE set)" >&2\nexit 2\n' >"$D/from/coordd"
chmod +x "$D/from/coordd"
echo "made-up exe NEW ggml $NEW" >"$D/from/yaaihh.exe"
echo "made-up exe W1 $W1" >"$D/w1.exe"

fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
seen=0
run() { out=$("$T/ops/release/release.sh" "$@" 2>&1); rc=$?; }  # run ARGS: release.sh ARGS in the copy; its output in $out, rc in $rc
await() {  # await WANT: the act lines since the last await, once WANT of them have landed (up to 10 s; WANT 0: 1 s), in
  # $nnew, the last of them in $line
  local i n
  for i in $(seq 50); do
    n=$(wc -l <"$acts")
    if [ "$1" = 0 ]; then [ "$i" -gt 5 ] && break; elif [ $((n - seen)) -ge "$1" ]; then sleep 0.2; break; fi
    sleep 0.2
  done
  n=$(wc -l <"$acts"); nnew=$((n - seen)); line=$(tail -n +$((seen + 1)) "$acts" | tail -1); seen=$n
}
fld() {  # fld KEY: the last line's field KEY (TEXT: its words; IS: its act), or (none)
  python3 -I -c 'import json, sys
a = json.loads(sys.argv[1]) if sys.argv[1] else ["", "", ""]
d = dict(x.split("=", 1) for x in a[3:])
k = sys.argv[2]
print(a[2] if k == "TEXT" else " ".join(a[:2]) if k == "IS" else d.get(k, "(none)"))' "$line" "$1"
}
line_is() {  # line_is WHAT KEY=WANT...: one new line, `act release`, its fields KEY as wanted
  local what=$1 kv bad=; shift
  await 1
  [ "$nnew" = 1 ] || { no "$what: $nnew lines, wanted 1 (rc $rc): $out"; return; }
  [ "$(fld IS)" = "act release" ] || bad="$bad IS=$(fld IS)"
  for kv in "$@"; do [ "$(fld "${kv%%=*}")" = "${kv#*=}" ] || bad="$bad ${kv%%=*}='$(fld "${kv%%=*}")' (wanted '${kv#*=}')"; done
  [ -z "$bad" ] && ok "$what: $(fld TEXT)" || no "$what:$bad"
}
says() { if [[ $out == *"$2"* ]]; then ok "$1 says: $2"; else no "$1 does not say '$2': $out"; fi; }

export RELEASE_ACTS=1  # this tree is no checkout of the swarm's repository: the lines are asked for

# the cut: build --from, the programs made up
run build --from "$D/from" "$NEW"
is "build --from rc" "$rc" 0
line_is "the cut's line" phase=cut kind=release release="$NEW" commit="$B" tree="$(g rev-parse 'HEAD^{tree}')" from="$D/from" exe=1 \
  folder="$T/releases/$NEW" checkout="$T" dirty="(none)" usage="(none)" way_back="(none)"

# with proxy/ checked out, its cpa-usage and the helpers his window runs (put.sh proxy_put) are packed, each one the
# proxy has a source of; one FROM lacks is said and left out, the rest made (a tower build without it still makes a
# release). No line asked for, and the release taken away after each
P2=20261122-110500-$b
mkdir -p "$T/proxy/crates/cpa/src/bin"; : >"$T/proxy/Cargo.toml"; : >"$T/proxy/crates/cpa/src/bin/yaaihh-proxy-serve.rs"
: >"$T/proxy/crates/cpa/src/bin/yaaihh-proxy-accounts.rs"; echo "$P2" >>"$D/from/yaaihh.exe"
for p in cpa-usage yaaihh-proxy-serve; do echo "made-up $p" >"$D/from/$p"; done
RELEASE_ACTS=0 run build --from "$D/from" "$P2"
[ "$rc" = 0 ] && [[ $out == *"no plain $D/from/yaaihh-proxy-accounts: the release has none"* ]] &&
  cmp -s "$D/from/yaaihh-proxy-serve" "$T/releases/$P2/yaaihh-proxy-serve" && [ ! -e "$T/releases/$P2/yaaihh-proxy-accounts" ] &&
  ! grep -q " yaaihh-proxy-accounts$" "$T/releases/$P2/built-from.sha256" &&
  ok "build --from with proxy/: a helper FROM lacks said and left out, the other packed" || no "build --from without a helper: rc $rc: $(ls "$T/releases/$P2" 2>&1): $out"
rm -rf "$T/releases/$P2"
echo "made-up yaaihh-proxy-accounts" >"$D/from/yaaihh-proxy-accounts"
RELEASE_ACTS=0 run build --from "$D/from" "$P2"
[ "$rc" = 0 ] && cmp -s "$D/from/yaaihh-proxy-serve" "$T/releases/$P2/yaaihh-proxy-serve" && cmp -s "$D/from/yaaihh-proxy-accounts" "$T/releases/$P2/yaaihh-proxy-accounts" &&
  grep -q " yaaihh-proxy-accounts$" "$T/releases/$P2/built-from.sha256" && ok "build --from with proxy/: both helpers packed, in built-from.sha256" ||
  no "build --from with proxy/: rc $rc: $(ls "$T/releases/$P2" 2>&1): $out"
rm -rf "$T/proxy" "$T/releases/$P2" "$D/from/cpa-usage" "$D/from/yaaihh-proxy-serve" "$D/from/yaaihh-proxy-accounts"

# live: NEW current, OLD the way back; NEW's exe put at C:\yaaihh (--no-window: no window followed)
run live --no-app --no-window "$NEW"
is "live rc" "$rc" 0
line_is "live's line" phase=live release="$NEW" commit="$B" current="$NEW" current_commit="$B" previous="$OLD" \
  previous_commit="$A" window="$NEW" window_commit="$B" window_previous="$OLD" window_previous_commit="$A" exe=put app=0 \
  way_back="release.sh back" copied_from="(none)" window_kept="(none)" checkout="$T"
[[ $(fld TEXT) == "release live: $NEW (previous: $OLD; the way back: release.sh back)" ]] && ok "live's words name the way back" || no "live's words: $(fld TEXT)"
run live --no-app --no-window "$NEW"; await 0
is "live of the release current already: nothing changed, no line" "$nnew" 0

# coord-switch: coord acts (DIR's) moves to NEW, its state aside; its way back is back, then coord-back with the tag
echo '{"ev":"a"}' >"$HOME/agent-coord/acts/tickets/log.jsonl"
run coord-switch acts
is "coord-switch rc" "$rc" 0
[[ $out == *"release.sh: replay of acts not saved, its start replays the whole record"*"usage: coordd (no arguments"* ]] &&
  ok "a coordd from before I-1509 refuses save-replay: said, and the switch goes on" || no "the switch's save-replay: $out"
tag=$(ls "$HOME/agent-coord/acts/aside" | grep -- "-switch-$OLD-to-$NEW\$" | tail -1)
line_is "coord-switch's line" phase=switch coord=acts coordd="$NEW" coordd_commit="$B" from="$OLD" aside="$tag" \
  way_back="release.sh back; release.sh coord-back acts $tag" current="$NEW" previous="$OLD"
[ -n "$tag" ] && grep -q "host-coordd start acts" "$calls" && ok "the switch's stand-ins ran (aside $tag)" || no "no switch aside, or no start: $(cat "$calls")"

# back: OLD current again, its exe put back; the way forward again is back
run back --no-app --no-window
is "back rc" "$rc" 0
line_is "back's line" phase=back release="$OLD" commit="$A" current="$OLD" previous="$NEW" window="$OLD" window_previous="$NEW" \
  exe=put app=0 way_back="release.sh back"

# coord-back: acts returns to the switch's copy; the state it had is kept aside, the way forward again
run coord-back acts
is "coord-back rc" "$rc" 0
ret=$(ls "$HOME/agent-coord/acts/aside" | grep -- "-return-from-$NEW\$" | tail -1)
line_is "coord-back's line" phase=coord_back coord=acts coordd="$OLD" coordd_commit="$A" restored="$tag" aside="$ret" \
  stopped="(none)" way_back="release.sh back; release.sh coord-back acts $ret"

# exe FILE NAME: a window-only edge, its commit the window's head
run exe "$D/w1.exe" "$W1"
is "exe FILE NAME rc" "$rc" 0
line_is "exe's line" phase=exe release="$W1" commit="$B" file="$D/w1.exe" window="$W1" window_commit="$B" \
  window_previous="$OLD" current="$OLD" way_back="release.sh exe --back"
run exe "$D/w1.exe" "$W1"; await 0
is "exe of the edge there already: nothing changed, no line" "$nnew" 0

# exe --back: OLD's exe again; forward again is the edge's exe, kept aside, put as exe FILE NAME
run exe --back
is "exe --back rc" "$rc" 0
kept=$(ls "$C/yaaihh"/yaaihh-old-*.exe | tail -1)
line_is "exe --back's line" phase=exe_back release="$OLD" commit="$A" from="$W1" kept="$kept" window="$OLD" \
  window_previous="(none)" way_back="release.sh exe $kept $W1"

# window: no window open, none started; the line says what his window is on
run window
is "window rc" "$rc" 0
line_is "window's line" phase=window window="$OLD" window_kept="(none)"

# install-mcp: DIR's coord's MCP folder; the first has no way back, the second has one
echo "tool one" >"$D/tool1" && echo "tool two" >"$D/tool2"
run install-mcp acts "$D/tool1" tool
is "install-mcp rc" "$rc" 0
line_is "install-mcp's line" phase=mcp coord=acts name=tool file="$D/tool1" sha256="$(sha256sum <"$D/tool1" | cut -d' ' -f1)" way_back="(none)"
run install-mcp acts "$D/tool2" tool
line_is "install-mcp again: its way back" phase=mcp file="$D/tool2" way_back="release.sh install-mcp --back acts tool"
run install-mcp --back acts tool
line_is "install-mcp --back's line" phase=mcp_back coord=acts name=tool sha256="$(sha256sum <"$D/tool1" | cut -d' ' -f1)" \
  way_back="release.sh install-mcp --back acts tool"

# nothing waits for a line, and no limit is ours (p1542): a slow coordc's line lands after its step has ended; a coordc
# that fails says why in the journal, not to the step; none at all is said at once
t0=$(date +%s%N); AGENT_BASE=$D/ab-slow run window; t=$(( ($(date +%s%N) - t0) / 1000000 ))
[ "$rc" = 0 ] && [ "$t" -lt 2000 ] && [ "$(wc -l <"$acts")" = "$seen" ] && ok "a slow coordc: the step ends at once (rc 0 in $t ms), its line not there yet" ||
  no "a slow coordc: rc $rc in $t ms, $(( $(wc -l <"$acts") - seen )) lines already"
line_is "a slow coordc's line, landed after its step" phase=window window="$OLD"
[[ $out == *"coordc"* ]] && no "a slow coordc: the step says something of it: $out" || ok "a slow coordc: the step says nothing of it"
AGENT_BASE=$D/ab-fail run window
is "a coordc that fails: rc" "$rc" 0
[[ $out == *"coordc"* ]] && no "a coordc that fails: the step says something of it: $out" || ok "a coordc that fails: the step says nothing of it"
for i in $(seq 50); do grep -q 'logger -t release-act release.sh' "$calls" && break; sleep 0.2; done
grep -qx "logger -t release-act coordc: no coordd answers" "$calls" &&
  grep -qx "logger -t release-act release.sh: the record's line of this window not recorded: coordc act release failed (rc 1)" "$calls" &&
  [ "$(grep -c 'logger -t release-act' "$calls")" = 2 ] &&
  ok "a coordc that fails: its words and the line it lost in the journal (logger -t release-act), nothing else there" ||
  no "a coordc that fails: the journal has $(grep release-act "$calls")"
await 0
AGENT_BASE= run window
is "no coordc: rc" "$rc" 0
says "no coordc" "no coordc at \$AGENT_BASE/bin (not an agent's run)"

# no line from a checkout that is not the swarm's (this one; the release tests' copies), unless asked; none with 0
unset RELEASE_ACTS
run window; await 0
is "a tree of its own, RELEASE_ACTS unset: rc" "$rc" 0
is "a tree of its own, RELEASE_ACTS unset: no line" "$nnew" 0
[[ $out == *"coordc"* ]] && no "a tree of its own says something of coordc: $out" || ok "a tree of its own says nothing of the line"
RELEASE_ACTS=0 run back --no-app --no-window; await 0
is "RELEASE_ACTS=0: back, no line" "$nnew" 0
# a copy of release.sh in a folder of no repository of its own, inside this checkout: the checkout's repository has
# SWARM_ROOT, but the copy is not at its top
I=$HERE/.release-acts-test-$$
mkdir "$I" && cp -a "$T/." "$I/" && rm -rf "$I/.git" && out=$("$I/ops/release/release.sh" window 2>&1); await 0
is "a copy in a folder inside this checkout, no repository of its own: no line" "$nnew" 0
rm -rf "$I"
git -C "$HERE" cat-file -e "$(sed -n 's/^SWARM_ROOT=//p' "$HERE/ops/release/release.sh")^{commit}" 2>/dev/null &&
  ok "SWARM_ROOT is a commit of this checkout's repository" || no "SWARM_ROOT is no commit of $HERE's repository"

echo "$fails failed"
[ "$fails" = 0 ]
