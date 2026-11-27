#!/usr/bin/env bash
# opencode-pin-test: the harness (bin/opencode-rpc), the sign-in (coordd's accounts/opencode.rs), roster's opencode
# agent and bin/opencode-pin name one opencode: the same version and the same sha256 (docket-core3, N-946 post 12). A
# version bump that changes only one of them fails here instead of at run time. Before accounts/opencode.rs is on the
# branch, it says so and checks nothing more. Then what opencode-pin does with a coord it is not to touch, or with a
# machine's copy that is not the pinned one, on made-up folders (no fetch: nothing here reaches the network).
set -eu
here=$(cd "$(dirname "$0")/../.." && pwd)
rpc=$here/agent-base/bin/opencode-rpc rs=$here/crates/coordd/src/accounts/opencode.rs
pin=$here/agent-base/bin/opencode-pin roster=$here/agent-base/bin/roster
h_rpc=$(sed -n "s/^OPENCODE_SHA256 = '\([0-9a-f]\{64\}\)'$/\1/p" "$rpc")
v_rpc=$(sed -n "s/.*'opencode', '\([0-9.]*\)', 'bin', 'opencode'.*/\1/p" "$rpc")
[ -n "$h_rpc" ] && [ -n "$v_rpc" ] || { echo "FAIL: no OPENCODE_SHA256 or version path in $rpc"; exit 1; }
[ -f "$rs" ] || { echo "ok: $rs not on this branch; opencode-rpc pins $v_rpc $h_rpc"; exit 0; }
h_rs=$(sed -n 's/^pub const SHA256: &str = "\([0-9a-f]\{64\}\)";$/\1/p' "$rs")
v_rs=$(sed -n 's/^pub const VERSION: &str = "\([0-9.]*\)";$/\1/p' "$rs")
h_pin=$(sed -n 's/^SHA256=\([0-9a-f]\{64\}\)$/\1/p' "$pin")
v_pin=$(sed -n 's/^VERSION=\([0-9.]*\)$/\1/p' "$pin")
v_roster=$(sed -n 's|.*bin=\$COORD/opt/opencode/\([0-9.]*\)/bin/opencode$|\1|p' "$roster")
fail=0
[ "$h_rpc" = "$h_rs" ] || { echo "FAIL: sha256 differs: opencode-rpc $h_rpc, accounts/opencode.rs ${h_rs:-none}"; fail=1; }
[ "$h_rpc" = "$h_pin" ] || { echo "FAIL: sha256 differs: opencode-rpc $h_rpc, opencode-pin ${h_pin:-none}"; fail=1; }
for v in "accounts/opencode.rs ${v_rs:-none}" "opencode-pin ${v_pin:-none}" "roster ${v_roster:-none}"; do
  [ "$v_rpc" = "${v##* }" ] || { echo "FAIL: version differs: opencode-rpc $v_rpc, $v"; fail=1; }
done
[ $fail = 1 ] || echo "ok: opencode-rpc, accounts/opencode.rs, opencode-pin and roster pin opencode $v_rpc, $h_rpc"

t=$(mktemp -d)
trap 'chmod -R u+w "$t"; rm -rf "$t"' EXIT
ok() { echo "ok: $*"; }
no() { echo "FAIL: $*"; fail=1; }
run() { OPENCODE_PIN_HOME=$t/machine bash "$pin" "$@" >"$t/out" 2>&1; }
set +e
run; [ $? = 2 ] && ok "no coord: its usage" || no "no coord: $(cat "$t/out")"
run "$t/none"; [ $? = 1 ] && grep -q 'no coord folder' "$t/out" && ok "a coord folder that is not there is refused" || no "missing coord: $(cat "$t/out")"
# a coord with its own copy (research's, fetched on 19 Nov) is left as it is: nothing read, nothing linked
mkdir -p "$t/own/opt/opencode/$v_pin/bin" && echo own >"$t/own/opt/opencode/$v_pin/bin/opencode"
run "$t/own"; [ $? = 0 ] && grep -q 'there already' "$t/out" && [ "$(cat "$t/own/opt/opencode/$v_pin/bin/opencode")" = own ] && [ ! -e "$t/machine" ] \
  && ok "a coord with its own copy is left as it is" || no "own copy: $(cat "$t/out")"
# a place there without the program is not written into
mkdir -p "$t/half/opt/opencode/$v_pin"
run "$t/half"; [ $? = 1 ] && grep -q 'left as it is' "$t/out" && [ -z "$(ls -A "$t/half/opt/opencode/$v_pin")" ] && ok "a place without the program is left" || no "half: $(cat "$t/out")"
# the machine's copy is not the pinned one: refused, and the coord not linked to it
mkdir -p "$t/machine/opencode/$v_pin/bin" "$t/c" && echo other >"$t/machine/opencode/$v_pin/bin/opencode"
run "$t/c"; [ $? = 1 ] && grep -q 'is not the pinned opencode' "$t/out" && [ ! -e "$t/c/opt/opencode/$v_pin" ] && [ ! -L "$t/c/opt/opencode/$v_pin" ] \
  && ok "a machine's copy that is not the pinned one links nothing" || no "other copy: $(cat "$t/out")"
exit $fail
