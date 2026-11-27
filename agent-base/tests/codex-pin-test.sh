#!/usr/bin/env bash
# codex-pin-test: a coord made new gets the pinned Codex (the owner, N-2002: new projects listed no gpt- models).
# host-coordd's start runs codex-pin; codex-pin names the version the codex driver states; on a made-up fresh coord
# it links opt/codex to the machine's copy, and bin/codex then answers with Codex's catalog. Then what codex-pin does
# with a coord it is not to touch, or a machine's copy that is not the pinned one. No fetch: nothing here reaches the
# network. The catalog part needs a copy of the pinned Codex on this machine (CODEX_PIN_COPY, else the one under
# ~/agent-coord/opt or tovrin-dev's), read and never written; without one it says so and checks the rest.
set -u
here=$(cd "$(dirname "$0")/../.." && pwd)
pin=$here/agent-base/bin/codex-pin
fail=0
ok() { echo "ok: $*"; }
no() { echo "FAIL: $*"; fail=1; }
[ -x "$pin" ] || no "no $pin"
grep -q '^  setsid -f "$AB/bin/codex-pin" "$COORD"' "$here/agent-base/bin/host-coordd" \
  && ok "host-coordd's start runs codex-pin for its coord" || no "host-coordd's start does not run codex-pin"
v_pin=$(sed -n 's/^VERSION=\([0-9.]*\)$/\1/p' "$pin")
v_drv=$(sed -n 's/^pub const VERSION: &str = "\([0-9.]*\)";$/\1/p' "$here/crates/drivers/codex/src/lib.rs")
[ -n "$v_pin" ] && [ "$v_pin" = "$v_drv" ] && ok "codex-pin and the codex driver pin $v_pin" || no "version differs: codex-pin ${v_pin:-none}, driver ${v_drv:-none}"
h_pin=$(sed -n 's/^SHA256=\([0-9a-f]\{64\}\)$/\1/p' "$pin")

t=$(mktemp -d)
trap 'chmod -R u+w "$t" 2>/dev/null; rm -rf "$t"' EXIT
run() { CODEX_PIN_HOME=$t/machine bash "$pin" "$@" >"$t/out" 2>&1; }
run; [ $? = 2 ] && ok "no coord: its usage" || no "no coord: $(cat "$t/out")"
run "$t/none"; [ $? = 1 ] && grep -q 'no coord folder' "$t/out" && ok "a coord folder that is not there is refused" || no "missing coord: $(cat "$t/out")"
# a coord with its place there (the links older projects have) is left as it is: nothing read, nothing linked
mkdir -p "$t/own/opt/codex/bin" && echo own >"$t/own/opt/codex/bin/codex"
run "$t/own"; [ $? = 0 ] && grep -q 'there already' "$t/out" && [ "$(cat "$t/own/opt/codex/bin/codex")" = own ] && [ ! -e "$t/machine" ] \
  && ok "a coord with its own Codex is left as it is" || no "own copy: $(cat "$t/out")"
mkdir -p "$t/half/opt/codex"
run "$t/half"; [ $? = 1 ] && grep -q 'left as it is' "$t/out" && [ -z "$(ls -A "$t/half/opt/codex")" ] && ok "a place without the program is left" || no "half: $(cat "$t/out")"
mkdir -p "$t/machine/codex/$v_pin/bin" "$t/c" && echo other >"$t/machine/codex/$v_pin/bin/codex"
run "$t/c"; [ $? = 1 ] && grep -q 'is not the pinned codex' "$t/out" && [ ! -e "$t/c/opt/codex" ] && [ ! -L "$t/c/opt/codex" ] \
  && ok "a machine's copy that is not the pinned one links nothing" || no "other copy: $(cat "$t/out")"
rm -rf "$t/machine"
# on a machine that is not x86_64 nothing is fetched or linked (uname here a stand-in)
mkdir -p "$t/arm/bin" "$t/a" && printf '#!/bin/sh\necho aarch64\n' >"$t/arm/bin/uname" && chmod +x "$t/arm/bin/uname"
PATH=$t/arm/bin:$PATH run "$t/a"; [ $? = 1 ] && grep -q 'is aarch64, and the pinned Codex is for x86_64 only' "$t/out" && [ ! -e "$t/machine" ] && [ ! -L "$t/a/opt/codex" ] \
  && ok "on an aarch64 machine nothing is fetched or linked" || no "arch: $(cat "$t/out")"

# a fresh coord, as new-project leaves it and host-coordd's pins find it: before codex-pin its codex says what is
# missing; after, it is the pinned Codex and lists its catalog's models
copy=${CODEX_PIN_COPY:-}
for c in "$HOME/agent-coord/opt/codex/$v_pin" "$HOME/agent-coord/tovrin-dev/opt/codex-$v_pin"; do
  [ -n "$copy" ] || { [ "$(sha256sum "$c/bin/codex" 2>/dev/null | cut -d' ' -f1)" = "$h_pin" ] && copy=$c; }
done
mkdir -p "$t/fresh/codex-home"
COORD=$t/fresh bash "$here/agent-base/bin/codex" --version >"$t/out" 2>&1; rc=$?
[ $rc = 127 ] && grep -q "no Codex at $t/fresh/opt/codex/bin/codex" "$t/out" && ok "before the pin, bin/codex says what is missing" || no "no pin: exit $rc, $(cat "$t/out")"
if [ -z "$copy" ]; then
  echo "codex-pin-test: no copy of Codex $v_pin on this machine (CODEX_PIN_COPY): its catalog not checked"
else
  mkdir -p "$t/machine/codex" && ln -s "$copy" "$t/machine/codex/$v_pin"
  run "$t/fresh"; [ $? = 0 ] && [ "$(readlink "$t/fresh/opt/codex")" = "$t/machine/codex/$v_pin" ] && ok "a fresh coord's opt/codex is the machine's copy" || no "fresh: $(cat "$t/out")"
  COORD=$t/fresh CODEX_HOME=$t/fresh/codex-home timeout 60 bash "$here/agent-base/bin/codex" debug models 2>/dev/null >"$t/models"
  n=$(python3 -I -c 'import json,sys; print(sum(m.get("visibility") == "list" and m["slug"].startswith("gpt-") for m in json.load(open(sys.argv[1]))["models"]))' "$t/models" 2>/dev/null || echo 0)
  [ "$n" -gt 0 ] && ok "its codex lists $n gpt- models from the catalog" || no "the fresh coord's codex lists no gpt- model"
fi
[ $fail = 0 ] && echo "codex-pin-test: every case as it should be"
exit $fail
