#!/usr/bin/env bash
# route-key-test.sh: lib/route.sh route_write_key with writers at once (paper-research N-1396 p454: release.sh's
# coord-switch start and a hook's start both wrote one fixed `.proxy-key.new`, one `mv` failed and the switch stopped).
# Eight writers at once each exit 0 and leave the whole key, mode 600, and no file of their own; a KEY_COMMAND that
# fails keeps the key as it was; a route with no KEY_COMMAND removes it. The key is a made-up one.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
COORD=$(mktemp -d); trap 'rm -rf "$COORD"' EXIT
. "$here/lib/route.sh"
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
left() { find "$COORD" -maxdepth 1 -name '.proxy-key.new*' | wc -l; }

# the key printed in two parts with a pause between, so the writers overlap
printf 'KEY_COMMAND=printf made-up-; sleep 0.3; printf test-key\n' >"$COORD/route"
pids=()
for _ in 1 2 3 4 5 6 7 8; do route_write_key & pids+=($!); done
rcs=()
for p in "${pids[@]}"; do if wait "$p"; then rcs+=(0); else rcs+=($?); fi; done
[ "${rcs[*]}" = "0 0 0 0 0 0 0 0" ] && ok "eight writers at once all exit 0" || bad "eight writers at once: exit codes ${rcs[*]}"
[ "$(cat "$COORD/.proxy-key")" = made-up-test-key ] && ok "the key is whole" || bad "the key is not whole: $(wc -c <"$COORD/.proxy-key") bytes"
[ "$(stat -c %a "$COORD/.proxy-key")" = 600 ] && ok "the key is mode 600" || bad "the key is mode $(stat -c %a "$COORD/.proxy-key")"
[ "$(left)" = 0 ] && ok "no writer's file is left" || bad "$(left) writer's files left"

# a KEY_COMMAND that fails: the key stays, nothing is left, the call says so
printf 'KEY_COMMAND=printf half; exit 3\n' >"$COORD/route"
if route_write_key; then bad "a failing KEY_COMMAND returned 0"; else ok "a failing KEY_COMMAND returns non-zero"; fi
[ "$(cat "$COORD/.proxy-key")" = made-up-test-key ] && ok "the key stays as it was" || bad "the key changed: $(wc -c <"$COORD/.proxy-key") bytes"
[ "$(left)" = 0 ] && ok "a failing writer leaves nothing" || bad "a failing writer left $(left) files"

# no KEY_COMMAND: agents fall back to their own login
printf 'ANTHROPIC_BASE_URL=http://127.0.0.1:8317\n' >"$COORD/route"
route_write_key && [ ! -e "$COORD/.proxy-key" ] && ok "no KEY_COMMAND removes the key" || bad "no KEY_COMMAND kept the key"

[ "$fails" = 0 ] && echo "route-key-test: all ok" || { echo "route-key-test: $fails failed"; exit 1; }
