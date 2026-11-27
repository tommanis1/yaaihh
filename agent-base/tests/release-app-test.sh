#!/usr/bin/env bash
# release-app-test.sh DIR: what release.sh's `live` and `back` do for the app now that the WSL window is gone (go_app;
# docket-edge7, N-1396 p202, consult-3 p204): they put the release's cpa-usage, the usage recording, when it differs
# from the one installed; a release without one leaves the installed one running; `back` to a release from before the
# window went starts its WSL window, which then only reads the records, cpa-usage holding the lock; --no-app does
# neither. A copy of this checkout's release.sh runs in DIR with made-up releases; usage-live.sh, the installed cpa-usage
# and systemd-run are stand-ins (USAGE_LIVE, CPA_USAGE_AT, SYSTEMD_RUN) that record each call: no unit is put or started,
# and --no-window --no-exe keep C:\yaaihh and the Windows window out of it.
#   OLD    a release from before the window went: a yaaihh, no cpa-usage
#   NEW    a cpa-usage (made-up bytes), no yaaihh
#   NEWER  another cpa-usage
set -uo pipefail
D=${1:?release-app-test.sh DIR}
HERE=$(cd "$(dirname "$0")/../.." && pwd)
T=$D/tree
rm -rf "$T" "$D/installed" "$D/calls"
mkdir -p "$T/agent-base/bin" "$T/releases/OLD" "$T/releases/NEW" "$T/releases/NEWER" "$D/installed"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/release.sh" "$T/ops/release/release.sh"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/bundle-v2.sh" "$HERE/ops/release/put.sh" "$T/ops/release/"
printf '#!/bin/sh\necho made-up old window\n' >"$T/releases/OLD/yaaihh" && chmod +x "$T/releases/OLD/yaaihh"
echo "made-up cpa-usage new" >"$T/releases/NEW/cpa-usage"
echo "made-up cpa-usage newer" >"$T/releases/NEWER/cpa-usage"
chmod +x "$T/releases/NEW/cpa-usage" "$T/releases/NEWER/cpa-usage"
ln -s OLD "$T/releases/current"
# the stand-ins: usage-live.sh puts the binary where the installed one is, as the real one does; systemd-run starts nothing
printf '#!/bin/sh\necho "usage-live $*" >>"%s/calls"\ncp "$1" "%s/installed/cpa-usage"\necho "cpa-usage runs (stand-in)"\n' "$D" "$D" >"$D/usage-live"
printf '#!/bin/sh\necho "systemd-run $*" >>"%s/calls"\n' "$D" >"$D/systemd-run"
chmod +x "$D/usage-live" "$D/systemd-run"
export YAAIHH_PROXY_AT=$D/proxy-at USAGE_LIVE=$D/usage-live CPA_USAGE_AT=$D/installed/cpa-usage SYSTEMD_RUN=$D/systemd-run

fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
run() {  # run ARGS: release.sh ARGS in the copy; its calls in $calls, its output in $out
  : >"$D/calls"
  out=$("$T/ops/release/release.sh" "$@" 2>&1) || no "release.sh $*: exit $?: $out"
  calls=$(cat "$D/calls")
}
says() { if [[ $out == *"$2"* ]]; then ok "$1 says: $2"; else no "$1 does not say '$2': $out"; fi; }

run live --no-window --no-exe NEW
is "live NEW: current" "$(readlink "$T/releases/current")" NEW
is "live NEW: calls" "$calls" "usage-live $T/releases/NEW/cpa-usage"
cmp -s "$T/releases/NEW/cpa-usage" "$D/installed/cpa-usage" && ok "live NEW: NEW's cpa-usage installed" || no "live NEW: installed is not NEW's"

run back --no-window --no-exe
is "back to OLD: current" "$(readlink "$T/releases/current")" OLD
[[ $calls == "systemd-run --user "* && $calls == *" $T/releases/OLD/yaaihh --replace" && $calls != *$'\n'* ]] &&
  ok "back to OLD: its WSL window started, and nothing else: $calls" || no "back to OLD: calls '$calls'"
says "back to OLD" "OLD has no cpa-usage; the one installed keeps running"
says "back to OLD" "OLD has a WSL window (a release from before it went): started"
cmp -s "$T/releases/NEW/cpa-usage" "$D/installed/cpa-usage" && ok "back to OLD: NEW's cpa-usage still installed" || no "back to OLD: installed changed"

run back --no-window --no-exe
is "forward to NEW: current" "$(readlink "$T/releases/current")" NEW
is "forward to NEW: calls" "$calls" ""
says "forward to NEW" "NEW's cpa-usage is the one installed; not put again"

run live --no-window --no-exe NEWER
is "live NEWER: calls" "$calls" "usage-live $T/releases/NEWER/cpa-usage"

run back --no-window --no-exe
is "back to NEW: current" "$(readlink "$T/releases/current")" NEW
is "back to NEW: calls" "$calls" "usage-live $T/releases/NEW/cpa-usage"

run live --no-app --no-window --no-exe NEWER
is "live --no-app NEWER: current" "$(readlink "$T/releases/current")" NEWER
is "live --no-app NEWER: calls" "$calls" ""
run back --no-app --no-window --no-exe
is "back --no-app: calls" "$calls" ""

grep -q '^  dev)' "$T/ops/release/release.sh" && no "release.sh still has dev" || ok "release.sh has no dev"
[ $fails = 0 ] && echo "release-app-test: all ok" || echo "release-app-test: $fails failed"
exit $fails
