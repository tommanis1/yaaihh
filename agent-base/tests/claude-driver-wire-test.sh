#!/usr/bin/env bash
# claude-driver-wire-test.sh: the real Claude Code's hooks reach Docket v2's Claude driver through the relay, and the
# driver's answers change what Claude Code does (std.hooks; crates/drivers). Each case runs `claude -p` once
# against a stand-in model API on loopback, in a network namespace with nothing else (harness-wire/
# harness-wire-check.sh), with its hooks set to run docket-hook-relay. In place of an engine the example driver
# answer-by-rule answers each moment by a fixed rule; the stand-in model calls Bash once. Checks:
#   - Inject at turn_begins: its text is in the request Claude Code sends;
#   - Deny at tool_called: the command does not run, and the model is given the driver's reason;
#   - Modify at tool_called: the command the driver gave runs in place of the model's;
#   - Answer allow at permission_asked, outside bypass permissions: the command runs with nobody asked; with no
#     answer it does not run.
# Needs the real claude (CLAUDE, or claude on PATH), unshare, and the two programs built (RELAY, DRIVER; default the
# workspace's target/debug); without them it says so and exits 77. The requests are kept in OUT when it is given;
# otherwise in a folder of its own, removed at the end.
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
root=$(cd "$here/.." && pwd)
w=$here/tests/harness-wire
claude=${CLAUDE:-$(command -v claude || true)}
relay=${RELAY:-$root/target/debug/docket-hook-relay}
driver=${DRIVER:-$root/target/debug/answer-by-rule}
for x in "$claude" "$relay" "$driver"; do [ -n "$x" ] && [ -x "$(readlink -f "$x")" ] || { echo "claude-driver-wire-test: no $x to run: skipped"; exit 77; }; done
unshare -rn true 2>/dev/null || { echo "claude-driver-wire-test: no network namespace: skipped"; exit 77; }
export CLAUDE=$claude
# a socket's path is at most 107 bytes: its folder is a short one of its own
sock=$(mktemp -d /tmp/dkh.XXXXXX)
if [ -n "${OUT:-}" ]; then out=$OUT; trap 'rm -rf "$sock"' EXIT; else out=$(mktemp -d /tmp/claude-driver.XXXXXX); trap 'rm -rf "$sock" "$out"' EXIT; fi
mkdir -p "$out"; [ -n "${OUT:-}" ] && echo "claude-driver-wire-test: requests kept in $out"
"$driver" settings "$relay" >"$out/settings.json"
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
# run CASE COMMAND MODE [RULE...]: the stand-in model asks Bash to run COMMAND; MODE bypass or ask
run() {
  local case=$1 cmd=$2 mode=$3; shift 3
  rm -f "$sock/stop"
  # the driver's own files in a folder of their own: harness-wire-check.sh clears OUT/CASE.* before a case
  mkdir -p "$out/driver"
  "$driver" "$sock" zed "$out/driver/$case.moments" "$@" >"$out/driver/$case.sockpath" 2>"$out/driver/$case.err" &
  local pid=$! k
  for k in $(seq 100); do [ -s "$out/driver/$case.sockpath" ] && break; sleep 0.05; done
  local -a flags=(--settings "$out/settings.json")
  [ "$mode" = bypass ] && flags=(--dangerously-skip-permissions "${flags[@]}")
  STANDIN_TOOL_COMMAND=$cmd bash "$w/harness-wire-check.sh" "$case" "$out" DOCKET_HOOK_SOCKET="$(cat "$out/driver/$case.sockpath")" -- "${flags[@]}" >"$out/$case.check" 2>&1
  touch "$sock/stop"; wait "$pid"
}
results() { python3 -I -c '
import json, sys
for l in open(sys.argv[1]):
    for m in json.loads(json.loads(l)["body"])["messages"]:
        for c in m["content"] if isinstance(m["content"], list) else []:
            if c.get("type") == "tool_result":
                x = c.get("content"); print(x if isinstance(x, str) else json.dumps(x))' "$1"; }
moments() { jq -r .moment "$1" | tr '\n' ' '; }

run denied 'echo original' bypass 'tool_called=Deny:the driver refused this command' 'turn_begins=Inject:INJECTED-MARK-42'
python3 -I -c 'import json,sys; b=json.loads(json.loads(open(sys.argv[1]).readline())["body"]); sys.exit("INJECTED-MARK-42" not in json.dumps(b["messages"]))' "$out/denied.jsonl" &&
  ok "Inject at turn_begins: its text is in the request" || bad "Inject: not in the request"
r=$(results "$out/denied.jsonl")
[[ $r == *"the driver refused this command"* && $r != *original* ]] && [[ $(moments "$out/driver/denied.moments") != *tool_result* ]] &&
  ok "Deny at tool_called: not run, the model given the driver's reason" || bad "Deny: $r / $(moments "$out/driver/denied.moments")"

run modified 'echo original' bypass 'tool_called=Modify:{"command":"echo changed-by-driver"}'
r=$(results "$out/modified.jsonl")
[[ $r == *changed-by-driver* && $r != *original* ]] && ok "Modify at tool_called: the driver's command ran in place of the model's" || bad "Modify: $r"

# a command that writes a file: Claude Code asks leave for it outside bypass permissions (an echo alone it runs)
asks='echo via-permission >made.txt && cat made.txt'
run permitted "$asks" ask 'permission_asked=Answer:allow'
r=$(results "$out/permitted.jsonl")
[[ $r == *via-permission* ]] && [[ $(moments "$out/driver/permitted.moments") == *permission_asked* ]] &&
  ok "Answer allow at permission_asked: run with nobody asked" || bad "permission answered: $r / $(moments "$out/driver/permitted.moments")"
run unanswered "$asks" ask
r=$(results "$out/unanswered.jsonl")
[[ -n $r && $r != *via-permission* ]] && ok "with no answer at permission_asked: not run" || bad "permission unanswered ran: $r"

echo "claude-driver-wire-test: $fails failed"
exit $((fails > 0))
