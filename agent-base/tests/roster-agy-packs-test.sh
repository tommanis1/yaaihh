#!/usr/bin/env bash
# roster-agy-packs-test.sh: an agy agent's system text (roster's agy_system, run from roster's own text) has the pack of
# its model and those harness.toml adds by role; a bare agent gets none, and no diff of an earlier start stays beside it.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
mkdir -p "$t/coord/agents/ag/agy" "$t/flavor/projects/p"
: >"$t/flavor/projects/p/project.toml"; echo "$t/flavor/projects/p/project.toml" >"$t/coord/project"
defs=$(for f in standing system_prompt harness_switches agy_system; do awk -v f="$f() {" 'index($0, f) == 1 {on = 1} on {print} on && /^}$/ {exit}' "$here/bin/roster"; done)
run() { COORD=$t/coord AGENT_BASE=$here bash -c "$defs"$'\nsysflag=(); agy_system ag agy/gemini-4-pro '"$t/coord/agents/ag/agy"' && echo "$sys_how"'; }
sp=$t/coord/agents/ag/system-prompt.md
fails=0
is() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi; }
printf '[agy]\npacks = ["fable-owner"]\n' >"$t/coord/agents/ag/harness.toml"
how=$(run)
is "appended, with agy's pack and then the one by role" '[ "$how" = append ] && [ "$(jq -c .packs "$sp.diff.json")" = "[\"agy\",\"fable-owner\"]" ] && grep -q "^# Verification contract" "$sp" && grep -q "^# Ownership" "$sp"'
: >"$t/coord/agents/ag/agy/bare"
run >/dev/null
is "a bare agent: no pack, and no diff left from the start before" '! grep -q "^# Verification contract\|^# Ownership" "$sp" && [ ! -e "$sp.diff.json" ]'
rm "$t/coord/agents/ag/agy/bare"
printf '[agy]\npacks = ["../x"]\n' >"$t/coord/agents/ag/harness.toml"
is "a pack named as a path: refused" '! run >/dev/null 2>&1'
[ $fails = 0 ] && echo "roster-agy-packs-test: all ok" || { echo "roster-agy-packs-test: $fails failed"; exit 1; }
