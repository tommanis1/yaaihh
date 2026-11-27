#!/usr/bin/env bash
# harness-wire-test.sh: the owner's switches change what the real Claude Code sends (walk items 2, 3, 17 and 30). Each
# case is one `claude -p` against a stand-in model API on loopback, in a network namespace with nothing else
# (harness-wire/harness-wire-check.sh: nothing can leave the machine, the owner's home is read-only for it, and the
# session's home, config and key are made up); its switches come from harness.toml files through bin/harness-settings,
# the same way roster gives them to an agent. Checks:
#   - a part switched off (Pronouns, read from the request itself, not from a list): its paragraph is gone from the
#     system text sent, and nothing else of it changed;
#   - a tool switched off (ScheduleWakeup): gone from the request's tools, and nothing else changed;
#   - the system prompt from a file (--append-system-prompt-file, as roster gives it): its text is in the request, and
#     one of 200020 bytes, past Linux's 128 KiB on one argument, whole;
#   - the safety check: on (unboxed default), Claude Code refuses `rm -rf ~` even with --dangerously-skip-permissions;
#     off (an agent's harness.toml), the same command runs, and what it removes is the run's own made-up home. Both run
#     out of the box, as roster runs an agent; boxed on WSL the check stays on, since it reads Windows' settings.
# Needs the real claude (CLAUDE, or claude on PATH) and unshare; without them it says so and exits 77 (skipped). The
# requests are kept in OUT (default: a new folder under /tmp, printed), for the proof.
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
w=$here/tests/harness-wire
claude=${CLAUDE:-$(command -v claude || true)}
[ -n "$claude" ] && [ -x "$(readlink -f "$claude")" ] || { echo "harness-wire-test: no claude to run: skipped"; exit 77; }
unshare -rn true 2>/dev/null || { echo "harness-wire-test: no network namespace (unshare -rn): skipped"; exit 77; }
export CLAUDE=$claude
out=${OUT:-$(mktemp -d /tmp/harness-wire.XXXXXX)}
mkdir -p "$out"; echo "harness-wire-test: requests kept in $out"
t=$out/coord-tree; rm -rf "$t"; mkdir -p "$t/coord/agents/zed" "$t/flavor/projects/p"
echo "$t/flavor/projects/p/project.toml" >"$t/coord/project"; : >"$t/flavor/projects/p/project.toml"
printf 'The standing text, as roster appends it from its file.\n' >"$t/system-prompt.md"
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
# run CASE [NAME=VALUE...]: one capture with the switches harness-settings gives zed now, as roster gives them
run() {
  local name=$1 got; shift
  got=$(COORD=$t/coord "$here/bin/harness-settings" claude zed) || { bad "$name: harness-settings refused"; return 1; }
  local -a flags env
  mapfile -d '' -t flags < <(jq -j '.flags[] | (., "\u0000")' <<<"$got")
  mapfile -d '' -t env < <(jq -j '.env | to_entries[] | ("\(.key)=\(.value)", "\u0000")' <<<"$got")
  # the file copied into the run folder, the one the harness's box shows the session
  RUN_FILE=$t/system-prompt.md bash "$w/harness-wire-check.sh" "$name" "$out" "$@" "${env[@]}" -- --dangerously-skip-permissions \
    --append-system-prompt-file "$out/run/system-prompt.md" "${flags[@]}"
}
result() { python3 -I -c '
import json,sys
for l in open(sys.argv[1]):
    for m in json.loads(json.loads(l)["body"]).get("messages", []):
        for c in m["content"] if isinstance(m["content"], list) else []:
            if c.get("type") == "tool_result":
                x = c.get("content"); print(x if isinstance(x, str) else json.dumps(x))' "$1"; }

run base || exit 1
pronouns=$(python3 -I -c '
import json,sys
req = json.loads(json.loads(open(sys.argv[1]).readline())["body"])
text = "\n\n".join(b["text"] for b in req["system"])
print(next(p.strip() for p in text.split("\n\n") if p.strip().startswith("When you use a pronoun")))' "$out/base.jsonl" 2>/dev/null)
[ -n "$pronouns" ] && ok "the base request holds the Pronouns part" || bad "no Pronouns part in the base request"
grep -q "The standing text, as roster appends it from its file." "$out/base.jsonl" &&
  ok "the system prompt from a file: its text is in the request" || bad "the appended file's text is not in the request"

jq -n --arg p "$pronouns" '"[claude]\nparts_off = [\($p | tojson)]\n"' -r >"$t/flavor/projects/p/harness.toml"
run pronouns-off
python3 -I "$w/wire-diff.py" "$out/base.jsonl" "$out/pronouns-off.jsonl" --expect-gone "$pronouns" --nothing-else &&
  ok "Pronouns off: its paragraph gone from the request, nothing else changed" || bad "Pronouns off (above)"

printf '[claude]\ntools_off = ["ScheduleWakeup"]\n' >"$t/flavor/projects/p/harness.toml"
run tool-off
python3 -I "$w/wire-diff.py" "$out/base.jsonl" "$out/tool-off.jsonl" --expect-tool-gone ScheduleWakeup --nothing-else &&
  ok "ScheduleWakeup off: gone from the request's tools, nothing else changed" || bad "tool off (above)"
rm "$t/flavor/projects/p/harness.toml"

# the two safety cases run as roster runs the pinned claude, out of the box (harness-wire-check.sh's UNBOXED)
STANDIN_TOOL_COMMAND='rm -rf ~' UNBOXED=1 run safety-on
[[ "$(result "$out/safety-on.jsonl")" == *"Dangerous rm operation detected"* ]] && [ "$(cat "$out/safety-on.sentinel")" = kept ] &&
  ok "the check on: rm -rf ~ refused under --dangerously-skip-permissions, the home kept" || bad "check on: $(result "$out/safety-on.jsonl" | head -c 200)"
printf '[claude]\nsafety_check = false\n' >"$t/coord/agents/zed/harness.toml"
STANDIN_TOOL_COMMAND='rm -rf ~' UNBOXED=1 run safety-off
[ "$(cat "$out/safety-off.sentinel")" = gone ] && [[ "$(result "$out/safety-off.jsonl")" != *"Dangerous rm"* ]] &&
  ok "the check off (the agent's harness.toml): the same command ran, removing the run's made-up home" || bad "check off: $(result "$out/safety-off.jsonl" | head -c 200)"
# on WSL, in the box a candidate is probed in, Windows' managed settings cannot be read, and the check stays on
if [ -e /proc/sys/fs/binfmt_misc/WSLInterop ] || [ -n "${WSL_INTEROP:-}" ]; then
  STANDIN_TOOL_COMMAND='rm -rf ~' run safety-off-boxed
  [[ "$(result "$out/safety-off-boxed.jsonl")" == *"Dangerous rm operation detected"* ]] && [ "$(cat "$out/safety-off-boxed.sentinel")" = kept ] &&
    ok "the check off, boxed on WSL: refused all the same, the home kept" || bad "check off boxed: $(result "$out/safety-off-boxed.jsonl" | head -c 200)"
fi

rm "$t/coord/agents/zed/harness.toml"

# a system prompt past Linux's 128 KiB on one argument (roster's old cap was 100000 bytes): from its file, whole
{ printf 'Long standing text. '; head -c 200000 /dev/zero | tr '\0' x; } >"$t/system-prompt.md"
run long-prompt
n=$(python3 -I -c '
import json,sys
req = json.loads(json.loads(open(sys.argv[1]).readline())["body"])
text = "\n\n".join(b["text"] for b in req["system"])
i = text.find("Long standing text. ")
print(len(text[i:].split("\n\n")[0]) if i >= 0 else 0)' "$out/long-prompt.jsonl")
[ "$n" = 200020 ] && ok "a 200020-byte system prompt from its file: in the request whole" || bad "long prompt: $n bytes of it in the request"

echo "harness-wire-test: $fails failed"
exit $((fails > 0))
