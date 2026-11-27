#!/usr/bin/env bash
# agy-pin-test: agy-rpc (bin/agy-rpc), coordd's agy accounts (accounts/agy.rs), roster's agy agent and bin/agy-pin name
# one t3code and one agy: the same versions and the same sha256s, so a bump that changes only one of them fails here
# instead of at run time (as opencode-pin-test). Then what agy-pin does with a coord it is not to touch, with a coord's
# copy checked again at each run, with a machine's copy that is not the pinned one, and with a fetch ended by TERM, on
# made-up folders and a copy of agy-pin whose two pins are made-up programs' (no fetch: a stand-in curl, nothing here
# reaches the network).
set -eu
here=$(cd "$(dirname "$0")/../.." && pwd)
rpc=$here/agent-base/bin/agy-rpc rs=$here/crates/coordd/src/accounts/agy.rs
pin=$here/agent-base/bin/agy-pin roster=$here/agent-base/bin/roster
fail=0
ok() { echo "ok: $*"; }
no() { echo "FAIL: $*"; fail=1; }
for name in T3_VERSION T3_SHA256 AGY_VERSION AGY_SHA256; do
  a=$(sed -n "s/^$name = '\([0-9a-f.]*\)'$/\1/p" "$rpc")
  b=$(sed -n "s/^pub const $name: &str = \"\([0-9a-f.]*\)\";$/\1/p" "$rs")
  c=$(sed -n "s/^$name=\([0-9a-f.]*\)$/\1/p" "$pin")
  [ -n "$a" ] && [ "$a" = "$b" ] && [ "$a" = "$c" ] && ok "$name is $a in agy-rpc, accounts/agy.rs and agy-pin" \
    || no "$name differs: agy-rpc ${a:-none}, accounts/agy.rs ${b:-none}, agy-pin ${c:-none}"
done
v=$(sed -n "s/^AGY_VERSION=\([0-9.]*\)$/\1/p" "$pin") v3=$(sed -n "s/^T3_VERSION=\([0-9.]*\)$/\1/p" "$pin")
v_roster=$(sed -n 's|.*AGY_BIN="\$COORD/opt/agy/\([0-9.]*\)/bin/antigravity".*|\1|p' "$roster" | sort -u)
[ "$v_roster" = "$v" ] && ok "roster's key agents run agy $v" || no "roster names agy ${v_roster:-none}, agy-pin $v"

t=$(mktemp -d "${AGY_PIN_TEST_PARENT:-$HOME/agent-coord/docket-try-saved/accounts-lead}/agy-pin-test.XXXXXX")
trap 'chmod -R u+w "$t"; rm -rf "$t"' EXIT
run() { AGY_PIN_HOME=$t/machine bash "$pin" "$@" >"$t/out" 2>&1; }
set +e
run; [ $? = 2 ] && ok "no coord: its usage" || no "no coord: $(cat "$t/out")"
run "$t/none"; [ $? = 1 ] && grep -q 'no coord folder' "$t/out" && ok "a coord folder that is not there is refused" || no "missing coord: $(cat "$t/out")"
# a coord's own copies that are not the pinned programs are refused, left as they are: nothing linked, nothing fetched
mkdir -p "$t/own/opt/t3/$v3" "$t/own/opt/agy/$v/bin" && echo own >"$t/own/opt/t3/$v3/t3" && echo own >"$t/own/opt/agy/$v/bin/antigravity"
run "$t/own"; [ $? = 1 ] && grep -q "t3 is not the pinned t3" "$t/out" && [ "$(cat "$t/own/opt/t3/$v3/t3")" = own ] && [ ! -e "$t/machine" ] \
  && ok "a coord's own copy that is not the pinned one is refused and left" || no "own copies: $(cat "$t/out")"
# agy-pin with made-up programs' hashes as its pins: a coord's copies of them are taken, each checked again at this run
printf 'made-up t3\n' >"$t/t3prog"; printf 'made-up agy\n' >"$t/agyprog"
h3=$(sha256sum "$t/t3prog" | cut -d' ' -f1) ha=$(sha256sum "$t/agyprog" | cut -d' ' -f1)
sed -e "s/^T3_SHA256=.*/T3_SHA256=$h3/" -e "s/^AGY_SHA256=.*/AGY_SHA256=$ha/" "$pin" >"$t/agy-pin"
runc() { AGY_PIN_HOME=$t/machine bash "$t/agy-pin" "$@" >"$t/out" 2>&1; }
mkdir -p "$t/kept/opt/t3/$v3" "$t/kept/opt/agy/$v/bin" && cp "$t/t3prog" "$t/kept/opt/t3/$v3/t3" && cp "$t/agyprog" "$t/kept/opt/agy/$v/bin/antigravity"
runc "$t/kept"; [ $? = 0 ] && [ "$(grep -c 'there already, its .* the pinned one' "$t/out")" = 2 ] && [ ! -e "$t/machine" ] \
  && ok "a coord's copies of the pinned programs are taken, checked again" || no "kept copies: $(cat "$t/out")"
# the same copy changed after its pin is refused at the next run, and left as it is
echo changed >>"$t/kept/opt/agy/$v/bin/antigravity"
runc "$t/kept"; [ $? = 1 ] && grep -q "antigravity is not the pinned agy" "$t/out" && grep -q changed "$t/kept/opt/agy/$v/bin/antigravity" \
  && ok "a copy changed after its pin is refused at the next run" || no "changed copy: $(cat "$t/out")"
# a fetch ended by TERM keeps nothing: its folder is removed (a stand-in curl that writes its output, then waits)
mkdir -p "$t/fakebin" "$t/e/opt/t3/$v3" && cp "$t/t3prog" "$t/e/opt/t3/$v3/t3"
printf '#!/bin/sh\nwhile [ $# -gt 0 ]; do [ "$1" = --output ] && out=$2; shift; done\n: >"$out"; exec sleep 30\n' >"$t/fakebin/curl"
chmod +x "$t/fakebin/curl"
PATH=$t/fakebin:$PATH AGY_PIN_HOME=$t/machine setsid bash "$t/agy-pin" "$t/e" >"$t/out" 2>&1 &
p=$!
for _ in $(seq 100); do compgen -G "$t/machine/agy/.fetch-$v.*/cli_linux_x64.tar.gz" >/dev/null && break; sleep 0.1; done
compgen -G "$t/machine/agy/.fetch-$v.*/cli_linux_x64.tar.gz" >/dev/null && fetching=1 || fetching=
kill -TERM -- -"$p"; wait "$p"; code=$?
[ -n "$fetching" ] && [ "$code" != 0 ] && ! compgen -G "$t/machine/agy/.fetch-*" >/dev/null && [ ! -e "$t/machine/agy/$v" ] && [ ! -e "$t/e/opt/agy/$v" ] \
  && ok "a fetch ended by TERM keeps nothing" || no "TERM: fetching ${fetching:-no}, exit $code, left: $(ls -A "$t/machine/agy" 2>&1) $(cat "$t/out")"
# a place there without the program is not written into, and nothing after it is done
mkdir -p "$t/half/opt/t3/$v3"
run "$t/half"; [ $? = 1 ] && grep -q 'left as it is' "$t/out" && [ -z "$(ls -A "$t/half/opt/t3/$v3")" ] && [ ! -e "$t/half/opt/agy" ] \
  && ok "a place without the program is left, and agy's is not made" || no "half: $(cat "$t/out")"
# the machine's copy is not the pinned one: refused, and the coord not linked to it
mkdir -p "$t/machine/t3/$v3" "$t/c" && echo other >"$t/machine/t3/$v3/t3"
run "$t/c"; [ $? = 1 ] && grep -q 'is not the pinned t3' "$t/out" && [ ! -e "$t/c/opt/t3/$v3" ] && [ ! -L "$t/c/opt/t3/$v3" ] \
  && ok "a machine's t3code that is not the pinned one links nothing" || no "other t3: $(cat "$t/out")"
mkdir -p "$t/d/opt/t3/$v3" "$t/machine/agy/$v/bin" && cp "$t/t3prog" "$t/d/opt/t3/$v3/t3" && echo other >"$t/machine/agy/$v/bin/antigravity"
runc "$t/d"; [ $? = 1 ] && grep -q 'is not the pinned agy' "$t/out" && [ ! -e "$t/d/opt/agy/$v" ] && [ ! -L "$t/d/opt/agy/$v" ] \
  && ok "a machine's agy that is not the pinned one links nothing" || no "other agy: $(cat "$t/out")"
exit $fail
