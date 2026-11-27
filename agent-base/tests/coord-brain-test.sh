#!/usr/bin/env bash
# `coord read-brain` and `coord report-work` (std/bridges/agy.docket's Pull-Brain, std/delivery.docket's Report-Work)
# go to the v2 daemon as one `coordd-v2 ask` each, as the session's own party, and print its answer as it comes; the
# MCP server coord-brain-mcp's tools read_brain and report_work are those two commands, for the session's own name
# alone (paper-research N-1978). bin/coord and bin/coord-brain-mcp run as copies in a release's layout beside a
# stand-in coordd-v2 that writes down its words and answers as told; no daemon is asked. Everything in a mktemp
# folder, removed after. The daemon's side of both (the briefing, the refusals) is crates/coordd-v2/tests/ask.rs's.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/rel/agent-base/bin" "$dir/research/v2"
cp "$here/bin/coord" "$here/bin/coord-brain-mcp" "$dir/rel/agent-base/bin/"
cat >"$dir/rel/coordd-v2" <<EOF
#!/usr/bin/env python3
import json, os, sys
open("$dir/calls.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
rc = int(os.environ.get("STANDIN_RC", "0"))
(sys.stdout if rc == 0 else sys.stderr).write(os.environ.get("STANDIN_OUT", "") + "\n")
sys.exit(rc)
EOF
# coordd's client, for the v1 side: its words written down, the same answer
cat >"$dir/rel/agent-base/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, os, sys
open("$dir/v1.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
print(os.environ.get("STANDIN_OUT", ""))
EOF
chmod +x "$dir/rel/coordd-v2" "$dir/rel/agent-base/bin/coord" "$dir/rel/agent-base/bin/coord-brain-mcp" "$dir/rel/agent-base/bin/coordc"
export COORD="$dir/research" COORDD_SOCK=/nonexistent AGENT_NAME=g1
coord="$dir/rel/agent-base/bin/coord" sock="$dir/research/v2/coordd.sock"
fails=0
ok() { echo "ok - $1"; }
bad() { echo "not ok - $1"; fails=$((fails + 1)); }
# run WORDS...: coord with them; its rc, stdout in $out, stderr in $dir/err, the coordd-v2 call it made in $call
run() { rm -f "$dir/calls.jsonl"; out=$("$coord" "$@" 2>"$dir/err" </dev/null); rc=$?; call=$(cat "$dir/calls.jsonl" 2>/dev/null); }
py() { python3 -I -c 'import json,sys; print(json.dumps(sys.argv[1:]))' "$@"; }
# a v2 daemon's socket in place, as a running coordd-v2's: what the v2 cases ask through
python3 -I -c 'import socket,sys; socket.socket(socket.AF_UNIX).bind(sys.argv[1])' "$sock"
B='{"mode":"work","priority_directive":"the spec first","new_directives":[],"unread_messages":["[k1 from o1] read the spec"],"assigned_items":["map-work"],"suggested_wake_mins":10,"wake_hint":"pull again in 10 min"}'

STANDIN_OUT=$B run read-brain reading the spec
[ "$rc" = 0 ] && [ "$call" = "$(py ask "$sock" std.bridges.agy.Pull-Brain g1 'reading the spec')" ] && [ "$out" = "$B" ] \
  && ok "read-brain: one ask of Pull-Brain as itself with its note, the briefing printed as it came" || bad "read-brain: rc $rc, $call, $out, $(cat "$dir/err")"
run read-brain
[ "$rc" = 0 ] && [ "$call" = "$(py ask "$sock" std.bridges.agy.Pull-Brain g1 '')" ] && ok "read-brain with no note: the empty note" || bad "no note: $call"
STANDIN_OUT=reported run report-work --refs abc123 --refs "N-1978.p2, the post" --item map-work the map is done
[ "$rc" = 0 ] && [ "$call" = "$(py ask "$sock" std.delivery.Report-Work g1 'the map is done' 'abc123, N-1978.p2, the post' map-work)" ] && [ "$out" = reported ] \
  && ok "report-work: summary, its refs as one text, the item" || bad "report-work: $call, $out"
run report-work -- --refs is words here
[ "$rc" = 0 ] && [ "$call" = "$(py ask "$sock" std.delivery.Report-Work g1 '--refs is words here' '' '')" ] && ok "report-work: after --, words only; no refs, no item" || bad "--: $call"
run report-work --refs abc
[ "$rc" = 2 ] && [ -z "$call" ] && grep -q 'say what was done' "$dir/err" && ok "report-work with no summary is refused, and nothing is asked" || bad "no summary: rc $rc, $call"
STANDIN_RC=3 STANDIN_OUT="coordd-v2 ask: std.bridges.agy.Pull-Brain: refused: policy: not enabled" run read-brain
[ "$rc" = 3 ] && grep -q 'refused: policy' "$dir/err" && [ -z "$out" ] && ok "a refused pull: its exit and its words, on stderr" || bad "refused: rc $rc, $out, $(cat "$dir/err")"
COORDD_V2_SOCK=/elsewhere.sock run read-brain
[ "$call" = "$(py ask /elsewhere.sock std.bridges.agy.Pull-Brain g1 '')" ] && ok "COORDD_V2_SOCK names another socket" || bad "sock: $call"
AGENT_NAME= run read-brain
[ "$rc" = 2 ] && [ -z "$call" ] && grep -q 'inside an agent session' "$dir/err" && ok "the owner's terminal pulls nothing" || bad "owner: rc $rc $call"
COORDD_V2=$dir/none run read-brain
[ "$rc" = 2 ] && grep -q 'no coordd-v2 at' "$dir/err" && ok "no coordd-v2: said, not run" || bad "no binary: rc $rc $(cat "$dir/err")"

# the MCP server: its two tools, each the command above, for this session's own name only
mcp() { printf '%s\n' "$@" | COORD_BRAIN_COORD="$coord" "$dir/rel/agent-base/bin/coord-brain-mcp" 2>"$dir/mcp-err"; }
rm -f "$dir/calls.jsonl"
got=$(STANDIN_OUT=$B mcp '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_brain","arguments":{"agent":"g1","status_note":"at the parser"}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"read_brain","arguments":{"agent":"w1"}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"report_work","arguments":{"agent":"g1","summary":"parser fixed","refs":["c1","c2"],"done_item":"map-work"}}}' \
  '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"report_work","arguments":{"agent":"g1","summary":" "}}}')
r() { jq -c --argjson i "$1" "select(.id == \$i) | $2" <<<"$got"; }
[ "$(r 1 .result.serverInfo.name)" = '"brain"' ] && [ "$(r 2 '[.result.tools[].name]')" = '["read_brain","report_work"]' ] \
  && ok "mcp: initialize and its two tools" || bad "mcp list: $got"
[ "$(r 3 .result.isError)" = false ] && [ "$(r 3 '.result.content[0].text')" = "$(jq -c -n --arg t "$B" '$t')" ] \
  && ok "mcp read_brain: the briefing as the command printed it" || bad "mcp read: $(r 3 .)"
[ "$(r 4 .result.isError)" = true ] && r 4 '.result.content[0].text' | grep -q 'not this session (g1)' && ok "mcp read_brain for another agent: refused here" || bad "mcp other: $(r 4 .)"
[ "$(r 6 .result.isError)" = true ] && ok "mcp report_work with no summary: refused here" || bad "mcp empty: $(r 6 .)"
calls=$(cat "$dir/calls.jsonl" 2>/dev/null)
[ "$calls" = "$(py ask "$sock" std.bridges.agy.Pull-Brain g1 'at the parser')
$(py ask "$sock" std.delivery.Report-Work g1 'parser fixed' 'c1, c2' map-work)" ] && ok "mcp: two asks made, its own and nothing else" || bad "mcp calls: $calls"

# no v2 socket: coordd's (coordc), the words as given, the MCP tools the same commands
rm -f "$sock" "$dir/v1.jsonl"
STANDIN_OUT=$B run read-brain reading the spec
v1=$(cat "$dir/v1.jsonl" 2>/dev/null)
[ "$rc" = 0 ] && [ -z "$call" ] && [ "$v1" = "$(py read-brain reading the spec)" ] && [ "$out" = "$B" ] \
  && ok "v1: read-brain is coordc's, nothing asked of coordd-v2" || bad "v1 read: rc $rc, $call, $v1, $(cat "$dir/err")"
rm -f "$dir/v1.jsonl"; run report-work --refs abc123 --item I-12 the map is done
v1=$(cat "$dir/v1.jsonl" 2>/dev/null)
[ "$rc" = 0 ] && [ -z "$call" ] && [ "$v1" = "$(py report-work --refs abc123 --item I-12 the map is done)" ] \
  && ok "v1: report-work is coordc's, its words as given" || bad "v1 report: rc $rc, $call, $v1"
rm -f "$dir/v1.jsonl"; run report-work --refs abc
[ "$rc" = 2 ] && [ ! -e "$dir/v1.jsonl" ] && grep -q 'say what was done' "$dir/err" && ok "v1: no summary refused before coordc" || bad "v1 empty: rc $rc"
rm -f "$dir/v1.jsonl"
got=$(STANDIN_OUT=$B mcp '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"report_work","arguments":{"agent":"g1","summary":"done","refs":["c1"],"done_item":"I-12"}}}')
v1=$(cat "$dir/v1.jsonl" 2>/dev/null)
[ "$(r 7 .result.isError)" = false ] && [ "$v1" = "$(py report-work --refs c1 --item I-12 -- done)" ] && ok "v1: the mcp's report_work through coordc" || bad "v1 mcp: $got, $v1"

# the agy pack (packs.toml gives it to agy/ and gemini- models) teaches the pull: both tools, each mode, the timer
pack=$here/prompts/packs/agy.md missing=
for w in read_brain report_work status_note suggested_wake_mins 'started your turn' '`work`' '`sleep`' '`stop`' 'set no timer' 'Never wait with a sleep' 'Never make up work'; do
  grep -qiF -- "$w" "$pack" || missing="$missing [$w]"
done
[ -z "$missing" ] && ok "the agy pack teaches the pull protocol" || bad "agy.md lacks:$missing"
grep -A1 '^\[\[pack\]\]' "$here/prompts/packs/packs.toml" | grep -q '"agy/"' && ok "packs.toml gives agy/ models the agy pack" || bad "packs.toml: no agy/ entry"

[ "$fails" = 0 ] && echo "all passed" || { echo "$fails failed"; exit 1; }
