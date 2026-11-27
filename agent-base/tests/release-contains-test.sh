#!/usr/bin/env bash
# release-contains-test.sh DIR: the list each release folder carries of the releases it contains (contains.txt; I-1457,
# proto-lead N-1446 p509), as release.sh's `contains` step and `live`'s copy-in write it. A copy of this checkout's
# release.sh runs in DIR/tree, a git repository of its own with made-up commits, and made-up release folders named
# for them; no program is built, and --no-app --no-window --no-exe with YAAIHH_WIN_C in DIR keep the usage recording,
# C:\yaaihh and the Windows window out of it.
#   A          the first commit
#   B          on A
#   C          on A, cut after B and not containing it (consult-3's 17 Nov case: ebf3912 after 2dc2691)
#   B-dirty    built on B with changes no commit holds
#   E          on B, a train's folder cut in a tree of its own, copied in by `live DIR/E`
#   W1         a window-only edge, TIME-edge-B-on-C (its window head B, the engine C's), put by `exe FILE NAME`; its
#              list is its head's alone: coordd puts the engine it runs on its line itself (proto-lead N-1446 p610)
#   W0         an edge named the older way, TIME-edge-E (its window head E, no engine named)
#   F          on E, cut after both edges
#   X          a folder whose commit this repository does not have
set -uo pipefail
D=${1:?release-contains-test.sh DIR}
HERE=$(cd "$(dirname "$0")/../.." && pwd)
T=$D/tree
rm -rf "$T" "$D/train" "$D/c"
mkdir -p "$T/agent-base/bin" "$T/releases" "$D/train"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/release.sh" "$T/ops/release/release.sh"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/bundle-v2.sh" "$HERE/ops/release/put.sh" "$T/ops/release/"
export YAAIHH_WIN_C=$D/c YAAIHH_PROXY_AT=$D/proxy-at
g() { git -C "$T" -c core.hooksPath=/dev/null -c user.name=test -c user.email=test@example.invalid "$@"; }
commit() { echo "$1" >"$T/f" && g add f && g commit -qm "$1" && g rev-parse --short=8 HEAD; }
g init -q -b main
a=$(commit A); b=$(commit B)
g checkout -q -b side "$a"; c=$(commit C)
g checkout -q main; e=$(commit E); f=$(commit F)
A=20261117-100000-$a B=20261117-110000-$b C=20261117-120000-$c BD=20261117-130000-$b-dirty E=20261117-140000-$e
W1=20261117-150000-edge-$b-on-$c W0=20261117-160000-edge-$e F=20261117-170000-$f X=20261117-180000-deadbee
for r in "$A" "$B" "$C" "$BD"; do mkdir -p "$T/releases/$r"; done
mkdir -p "$D/train/$E" && echo 20261116-000000-0000000 >"$D/train/$E/contains.txt"  # the train tree's own list
ln -s "$C" "$T/releases/current"

fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $(echo "$2" | paste -sd' ')"; else no "$1: '$2', wanted '$3'"; fi; }
run() { out=$("$T/ops/release/release.sh" "$@" 2>&1) || no "release.sh $*: exit $?: $out"; }
says() { if [[ $out == *"$2"* ]]; then ok "$1 says: $2"; else no "$1 does not say '$2': $out"; fi; }
list() { cat "$T/releases/$1/contains.txt" 2>/dev/null || echo "(no list)"; }

run contains
says "contains" "contains: 4 given their list; the rest had one"
is "A lists" "$(list "$A")" ""
is "B lists" "$(list "$B")" "$A"
is "C, cut after B, lists A and not B" "$(list "$C")" "$A"
is "B-dirty lists what B's commit contains, B too" "$(list "$BD")" "$A
$B"
[ -e "$T/releases/current/contains.txt" ] && ok "current is no folder of its own: C's list read through it" || no "C's list not under current"

run contains
says "contains again" "contains: 0 given their list; the rest had one"
is "B's list as it was" "$(list "$B")" "$A"

# A deleted by a tidy: B's list still names it, so E, copied in, contains both
rm -rf "${T:?}/releases/$A"
run live --no-app --no-window --no-exe "$D/train/$E"
is "live E: current" "$(readlink "$T/releases/current")" "$E"
is "E, copied in, lists what this tree's releases say, A through B's list, not its own tree's" "$(list "$E")" "$A
$B"
grep -qx "$BD" "$T/releases/$E/contains.txt" && no "E lists B-dirty" || ok "no release lists B-dirty, E neither"
grep -qx "$E" "$T/releases/$C/contains.txt" && no "C lists E" || ok "C (on another line) does not list E"

# edges: each one's list where `exe FILE NAME` puts its exe (no C:\yaaihh here: put_exe leaves it)
edge() { echo "$1" >"$D/$1.exe"; run exe "$D/$1.exe" "$1"; cat "$T/releases/edges/$1.txt" 2>/dev/null || echo "(no list)"; }
is "W1 (window B, engine C) lists what B contains, not C, which coordd's own half names" "$(edge "$W1")" "$A
$B"
is "W0, named the older way (its last part its window head E), lists what E contains and W1, whose head E carries" "$(edge "$W0")" "$A
$B
$E
$W1"
mkdir "$T/releases/$F" "$T/releases/$X"
run contains
says "contains over F and X" "is no commit of this repository"
says "contains over F and X" "contains: 2 given their list; the rest had one"
is "F lists both edges, whose heads it carries, and not C" "$(list "$F")" "$A
$B
$E
$W1
$W0"
is "X lists nothing" "$(list "$X")" ""
run list
[[ $out != *edges* ]] && ok "list names no edges folder" || no "list names edges: $out"
run tidy --dry
[[ $out != *edges* ]] && ok "tidy leaves the edges folder alone" || no "tidy would touch edges: $out"
# his window runs an edge put before edges had lists: the contains step gives it its list
W2=20261117-190000-edge-$e-on-$b
mkdir -p "$D/c/yaaihh" && echo "$W2" >"$D/c/yaaihh/release.txt"
run contains
says "contains with his window on an edge" "contains: edge $W2 (his window's) lists"
is "W2 (window E, engine B) lists what E contains, both edges among it, not B-dirty" "$(cat "$T/releases/edges/$W2.txt")" "$A
$B
$E
$W1
$W0"

echo "$fails failed"
[ "$fails" = 0 ]
