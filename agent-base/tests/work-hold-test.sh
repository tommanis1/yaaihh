#!/usr/bin/env bash
# work-hold-test.sh DIR: agent-base/bin/work-hold's one-holder rule, with a lock, log and name of its own in DIR (no
# unit started, the machine's holder untouched): the first holds and stays, a second exits 0 at once saying a holder
# runs, and once the first is ended a third holds again. Then the units it starts with none named (WORK_HOLD_UNITS
# unset): the gateway's and the usage recording's, asked of a stand-in systemctl first on the PATH, which records each
# call and starts nothing. The test's holders are its own processes, ended by pid.
set -uo pipefail
D=${1:?work-hold-test.sh DIR}
WH=$(cd "$(dirname "$0")/.." && pwd)/bin/work-hold
mkdir -p "$D"
export WORK_HOLD_LOCK=$D/lock WORK_HOLD_LOG=$D/log WORK_HOLD_UNITS="" WORK_HOLD_NAME=work-hold-test-$$
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
holder() { pgrep -f "^$WORK_HOLD_NAME" | head -1; }
wait_holder() { for _ in $(seq 1 100); do [ -n "$(holder)" ] && return 0; sleep 0.1; done; return 1; }

: >"$D/log"
"$WH" >/dev/null 2>&1 &
if wait_holder; then ok "the first holds as $WORK_HOLD_NAME (pid $(holder))"; else no "the first does not hold"; fi
first=$(holder)

out=$("$WH" 2>&1); rc=$?
[ $rc = 0 ] && [[ $out == *"a holder runs already"* ]] && ok "a second exits 0: $out" || no "a second: rc $rc, $out"
[ "$(pgrep -fc "^$WORK_HOLD_NAME")" = 1 ] && ok "one holder" || no "holders: $(pgrep -fc "^$WORK_HOLD_NAME")"

[ -n "$first" ] && kill "$first"
for _ in $(seq 1 50); do [ -z "$(holder)" ] && break; sleep 0.1; done
"$WH" >/dev/null 2>&1 &
if wait_holder && [ "$(holder)" != "$first" ]; then ok "after its end a new one holds (pid $(holder))"; else no "no new holder after the first ended"; fi
grep -q "holding as $WORK_HOLD_NAME" "$D/log" && ok "the log says it holds" || no "log: $(cat "$D/log")"

h=$(holder); [ -n "$h" ] && kill "$h"
for _ in $(seq 1 50); do [ -z "$(holder)" ] && break; sleep 0.1; done

# the units by default: cpa-core's and cpa-usage's (docket-edge7, N-1396 p202), through the stand-in systemctl
mkdir -p "$D/bin"
printf '#!/bin/sh\necho "$*" >>"%s/systemctl.calls"\n' "$D" >"$D/bin/systemctl" && chmod +x "$D/bin/systemctl"
: >"$D/systemctl.calls"
(unset WORK_HOLD_UNITS; PATH="$D/bin:$PATH" exec "$WH" >/dev/null 2>&1) &
if wait_holder; then ok "with no units named it holds (pid $(holder))"; else no "with no units named it does not hold"; fi
for u in cpa-core.service cpa-usage.service; do
  grep -qx -- "--user start $u" "$D/systemctl.calls" && ok "it starts $u" || no "it does not start $u: $(tr '\n' ';' <"$D/systemctl.calls")"
done
[ "$(grep -c -- '--user start' "$D/systemctl.calls")" = 2 ] && ok "and no other unit" || no "starts: $(grep -- '--user start' "$D/systemctl.calls" | tr '\n' ';')"
h=$(holder); [ -n "$h" ] && kill "$h"
[ $fails = 0 ] && echo "work-hold-test: all ok" || echo "work-hold-test: $fails failed"
exit $fails
