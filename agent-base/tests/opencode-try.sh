#!/usr/bin/env bash
# opencode-try.sh COORD WORKTREE MACHINE: an opencode account and an agent on it, tried on the machine with the pinned
# opencode itself (the owner, N-1104 p780: "also stuff opencode we really need that asap"), a made-up key and folders
# of the try's own: no account of his, no request to a provider, no login of his read. Go's API is a stand-in on
# loopback (go-api.py, the try's own), opencode's Go pointed there by OPENCODE_CONFIG_CONTENT (host-coordd passes it on),
# and a turn is sent only once the agent's opencode says it is pointed there. COORD names a scratch coord,
# ~/agent-coord/COORD, made here (a folder already there is refused unless an earlier run of this try made it); its
# coordd must not run. WORKTREE is a checkout with coordd, coordc and coord-pty built (target/debug, by the
# build runner). MACHINE is where the machine's copy of the pinned opencode is kept for the try (OPENCODE_PIN_HOME;
# fetched there by opencode-pin when it is not there yet), so ~/agent-coord/opt is not touched.
#
# Under $COORD/opencode-try/ it lays out an agent-base of its own (WORKTREE's, with those binaries), keeps the
# accounts (COORD_ACCOUNTS, so the owner's ~/agent-coord/accounts is not touched) and runs coordd under host-coordd.
# Every request goes to coordd's socket as the window sends it (accounts-try/ask).
#
#   1  the coord's pinned opencode is a link to MACHINE's copy (opencode-pin), and host-coordd's own run of it then
#      finds it there
#   2  an opencode account of OpenCode Go is made (the Add account sheet's `opencode` tile): not signed in
#   3  its made-up key from the masked field goes into opencode's own `auth login`: signed in, and opencode's own
#      `models opencode-go` are its models; the key is in opencode's file under the account's home/, and kept by
#      coordd for Go's usage alone (the owner, N-1104 p808) as one header in a file of its own, mode 600, which curl
#      is given by name; in no other file of the coord's or the try's. Go's usage is read at once: the card has its
#      5-hour, weekly and monthly windows. Go's page is a stand-in curl's (made-up meters), so nothing leaves the
#      machine; every other call of curl's goes to the system's
#   4  New agent's list (Request::Models) holds Go's models, once coordd's next read of the places has come
#   5  New agent on opencode-go/kimi-k3 with no first prompt: the agent runs as an opencode agent, bound to the Go
#      account; the card (Request::Accounts) names it
#   6  a turn (his message to it) on the stand-in of Go's API: the request carries the key opencode keeps and not the
#      plugin's mark; each request is kept whole in the agent's raw/ (the raw pane's calls, opencode/plugin/coord.js 3)
#      with no header, and coordd's page of them (Request::RawPage) has the request and its answer
#   7  the agent is let go (roster down): its opencode server ends and coordd has it stopped
#   8  his `Check limits` on the card (Request::AccountCheck) reads Go's page again: its weekly limit used up makes the
#      account out until the week's reset, and the next reading, the week begun again, takes it back
#
# At the end the agent and coordd are stopped; $COORD/opencode-try stays for reading.
set -euo pipefail
NAME=${1:?opencode-try.sh COORD WORKTREE MACHINE} WT=${2:?WORKTREE} MACHINE=${3:?MACHINE}
WT=$(cd "$WT" && pwd)
COORD=$HOME/agent-coord/$NAME
T=$COORD/opencode-try
UNIT=coordd-$NAME
A=oc1 MODEL=opencode-go/kimi-k3 KEY=sk-made-up-opencode-try-4711
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
has() { if grep -qF -- "$3" <<<"$2"; then ok "$1"; else no "$1: wanted '$3' in: $2"; fi; }
say() { echo; echo "== $*"; }

[[ $NAME =~ ^[A-Za-z0-9_-]+$ ]] || { echo "not a coord name: $NAME" >&2; exit 2; }
[ ! -e "$COORD" ] || [ -f "$COORD/opencode-try-coord" ] || { echo "$COORD is there and is not this try's (no opencode-try-coord mark)" >&2; exit 2; }
! systemctl --user is-active -q "$UNIT" || { echo "$UNIT runs: not a scratch coord now" >&2; exit 2; }
for b in coordd coordc coord-pty; do [ -x "$WT/target/debug/$b" ] || { echo "no $WT/target/debug/$b (build-runner cargo build -p coordd -p coord-pty)" >&2; exit 2; }; done

# ---- the coord, its agent-base, the accounts' folder ------------------------------------------------------------
[ ! -e "$COORD" ] || { chmod -R u+w "$COORD"; rm -rf -- "$COORD"; }
mkdir -p "$T/work" "$T/accounts" "$T/projects"
: >"$COORD/opencode-try-coord"
AB=$T/base
cp -r "$WT/agent-base" "$AB"
for b in coordd coordc coord-pty; do cp "$WT/target/debug/$b" "$AB/bin/$b"; done
# Go's usage page, answered by a stand-in on coordd's PATH (agent-base's bin is first there): its arguments kept, the
# answer the try's own (go-usage-answer: the body, then the status on a line of its own); any other call is curl's
GO_URL=https://opencode.ai/inference/go/v1/usage
cat >"$AB/bin/curl" <<EOF
#!/bin/sh
for last; do :; done
[ "\$last" = $GO_URL ] || exec $(command -v curl) "\$@"
printf '%s\n' "\$@" >"$T/curl-args"
cat "$T/go-usage-answer"
EOF
chmod +x "$AB/bin/curl"
# made-up meters, each reset from now on: a 5-hour window, a week and a month; STATUS of the week
go_page() {
  local at='+%Y-%m-%dT%H:%M:%SZ'
  printf '{"usage":{"rolling":{"status":"ok","percent":30,"resetsAt":"%s"},"weekly":{"status":"%s","percent":%s,"resetsAt":"%s"},"monthly":{"status":"ok","percent":12,"resetsAt":"%s"}}}\n200' \
    "$(date -u -d '+2 hours' "$at")" "$1" "$2" "$(date -u -d '+3 days' "$at")" "$(date -u -d '+20 days' "$at")" >"$T/go-usage-answer"
}
go_page ok 45
# Go's API, a stand-in on loopback: what each request was (its path, model, header names, whether it carried the key,
# whether it held the turn's words), and an answer of made-up words, streamed when asked to be (an OpenAI chat
# completion, as Go's is)
cat >"$T/go-api.py" <<'PY'
import http.server, json, os, sys
T, key = sys.argv[1], os.environ["GO_API_KEY"]
class Go(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0)).decode()
        try:
            req = json.loads(body)
        except ValueError:
            req = {}
        with open(f"{T}/go-api.jsonl", "a") as f:
            f.write(json.dumps({"path": self.path, "model": req.get("model"), "headers": sorted(k.lower() for k in self.headers.keys()),
                                "key": self.headers.get("Authorization") == f"Bearer {key}", "asked": "Say done." in body}) + "\n")
        said = {"index": 0, "message": {"role": "assistant", "content": "Done here."}, "finish_reason": "stop"}
        usage = {"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12}
        self.send_response(200)
        if not req.get("stream"):
            out = json.dumps({"id": "try", "object": "chat.completion", "created": 0, "model": req.get("model"), "choices": [said], "usage": usage}).encode()
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(out)))
            self.end_headers()
            self.wfile.write(out)
            return
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        chunk = lambda c: b"data: " + json.dumps({"id": "try", "object": "chat.completion.chunk", "created": 0, "model": req.get("model"), **c}).encode() + b"\n\n"
        self.wfile.write(chunk({"choices": [{"index": 0, "delta": {"role": "assistant", "content": "Done here."}, "finish_reason": None}]}))
        self.wfile.write(chunk({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}))
        self.wfile.write(chunk({"choices": [], "usage": usage}))
        self.wfile.write(b"data: [DONE]\n\n")
s = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Go)
open(f"{T}/go-api.port.part", "w").write(str(s.server_address[1]))
os.rename(f"{T}/go-api.port.part", f"{T}/go-api.port")
s.serve_forever()
PY
GO_API_KEY=$KEY python3 -I "$T/go-api.py" "$T" &
GO_API_PID=$!
until [ -s "$T/go-api.port" ]; do kill -0 "$GO_API_PID" || exit 2; sleep 0.1; done
GO_API=http://127.0.0.1:$(cat "$T/go-api.port")/v1
export OPENCODE_CONFIG_CONTENT="{\"provider\":{\"opencode-go\":{\"options\":{\"baseURL\":\"$GO_API\"}}}}"
# a route with an endpoint nobody listens on and a made-up key, as accounts-try's: no agent here goes through it
printf '%s\n' 'ANTHROPIC_BASE_URL=http://127.0.0.1:9/opencode-try-route' 'KEY_COMMAND=echo opencode-try-not-a-key' >"$COORD/route"
: >"$T/empty-prompt"
export COORD_ACCOUNTS=$T/accounts COORD_CONVERSATIONS=$T/projects
# the CODEX_HOME of the agent that runs the try is its own coord's; roster here (host-coordd's resume, roster down) is
# this coord's and keeps an opencode agent's server pid under $COORD/codex-home/app
unset CODEX_HOME
HC=$AB/bin/host-coordd
# as the owner: coordd knows its caller by its cgroup, and an agent that runs the try is none of this coordd's
co() { env -u AGENT_NAME -u AGENT_MODEL "$HC" sh "$NAME" "$@"; }
ask() { co python3 -I "$AB/tests/accounts-try/ask" "$1"; }
end() { co roster down "$A" >/dev/null 2>&1 || true; "$HC" stop "$NAME" >/dev/null 2>&1 || true; kill "$GO_API_PID" 2>/dev/null || true; }
trap end EXIT
acct=$T/accounts/opencode/go

say "1. the pinned opencode, linked"
OPENCODE_PIN_HOME=$MACHINE bash "$AB/bin/opencode-pin" "$COORD"
is "the coord's place is a link to the machine's copy" "$(readlink "$COORD/opt/opencode/1.18.34")" "$MACHINE/opencode/1.18.34"
"$HC" start "$NAME"
# host-coordd's start runs opencode-pin in the background, before coordd's unit (host-coordd `start`), and it writes
# the log: waited for while it runs, so one that ends without a word is said by the check below, not waited on
until [ -s "$COORD/opencode-pin.log" ] || ! pgrep -f -- "opencode-pin $COORD\$" >/dev/null; do sleep 0.2; done
has "host-coordd's own run finds it there" "$(cat "$COORD/opencode-pin.log")" "is there already"

say "2. an opencode account of OpenCode Go"
r=$(ask '{"op":"accountnew","id":"go","label":"Go","tier":10,"harness":"opencode","provider":"opencode-go"}')
has "made, not signed in" "$r" "made, not signed in yet"
is "signed in" "$(jq -r .signed_in "$acct/state.json" 2>/dev/null || echo false)" false

say "3. its key, into opencode's own sign-in"
r=$(ask "{\"op\":\"accountkeystore\",\"id\":\"go\",\"key\":\"$KEY\"}") || true
echo "   coordd: $r"
has "opencode keeps it" "$r" "key stored"
is "signed in" "$(jq -r .signed_in "$acct/state.json")" true
n=$(jq '.models | length' "$acct/state.json")
[ "$n" -gt 0 ] && ok "its models, as opencode names them: $n, e.g. $(jq -r '.models[0:3] | join(", ")' "$acct/state.json")" || no "no models"
is "$MODEL among them" "$(jq -r --arg m "$MODEL" '.models | index($m) != null' "$acct/state.json")" true
# every file under the coord but opencode's own under the account's home/, the one kept for Go's usage, and this
# script's copy, which names it
held=$(grep -rlF -- "$KEY" "$COORD" 2>/dev/null | grep -v -e "^$acct/home/" -e "^$acct/usage-key$" -e "^$AB/tests/opencode-try.sh$" || true)
is "the key in no file of ours but the one for Go's usage" "$held" ""
[ -n "$(grep -rlF -- "$KEY" "$acct/home/data/opencode" 2>/dev/null)" ] && ok "opencode keeps it under the account's home/" || no "opencode does not keep it under $acct/home/data/opencode"
is "kept for Go's usage as one header" "$(cat "$acct/usage-key")" "Authorization: Bearer $KEY"
is "the owner's alone" "$(stat -c %a "$acct/usage-key")" 600
has "Go's usage read at once" "$r" "Go's usage: 5-hour 30% until"
given=$(cat "$T/curl-args")
has "curl is given the file by name" "$given" "@$acct/usage-key"
is "Go's usage page, last" "$(tail -n 1 <<<"$given")" "$GO_URL"
is "the key in no argument of curl's" "$(grep -cF -- "$KEY" <<<"$given" || true)" 0
is "the windows, as Go's page named them" "$(jq -c '[(.five_hour, .seven_day, .month | .used_percent | floor), .limits_from]' "$acct/state.json")" '[30,45,12,"go usage"]'
r=$(ask '{"op":"accounts","json":true}')
is "the card has them, and the key kept" "$(jq -c '.accounts[] | select(.id == "go") | [.usage_key, (.month.used_percent | floor)]' <<<"$r")" '[true,12]'

say "4. New agent's list"
for _ in $(seq 150); do
  m=$(ask '{"op":"models"}')
  grep -qxF "$MODEL" <<<"$m" && break
  sleep 1
done
has "it holds $MODEL" "$m" "$MODEL"
echo "   $(grep -c / <<<"$m") opencode models of $(wc -l <<<"$m")"

say "5. New agent on $MODEL, no first prompt"
r=$(ask "{\"op\":\"addagent\",\"name\":\"$A\",\"model\":\"$MODEL\",\"text\":\"\",\"dir\":\"$T/work\"}") || true
echo "   coordd: $r"
has "started" "$r" "$A: started ($MODEL)"
st=$(co coordc ls | awk -F'\t' -v n="$A" '$1 == n { print $3 }')
[ -n "$st" ] && [ "$st" != stopped ] && ok "coordd has it running: $st" || no "coordd has it: '$st'"
has "registered as opencode" "$(cat "$COORD/registry.jsonl")" "\"kind\":\"opencode\""
b=$(find "$acct/bound" -mindepth 1 2>/dev/null || true)
has "bound to the Go account" "$b" "$A"
r=$(ask '{"op":"accounts","json":true}')
is "the card names it" "$(jq -r --arg a "$A" '[.accounts[] | select(.id == "go") | .agents[].agent] | index($a) != null' <<<"$r")" true
[ -s "$COORD/agents/$A/opencode/coord/session" ] && ok "its session is made: $(cat "$COORD/agents/$A/opencode/coord/session")" || no "no session"

say "6. a turn, on the stand-in of Go's API"
c=$COORD/agents/$A/opencode/coord
# the agent's opencode server's own config, asked with its password given on curl's stdin (never on a command line)
url=$(printf 'user = "opencode:%s"\n' "$(cat "$c/password")" | curl -s -K - "$(cat "$c/url")/config" | jq -r '.provider["opencode-go"].options.baseURL // empty')
if [ "$url" != "$GO_API" ]; then
  no "opencode's Go is not the stand-in ('$url'): no turn is sent"
else
  ok "opencode's Go is the stand-in: $url"
  r=$(ask "{\"op\":\"msg\",\"to\":[\"$A\"],\"body\":\"Say done.\"}") || true
  echo "   coordd: $r"
  pid=$(cat "$COORD/codex-home/app/$A.pid")
  until [ -s "$T/go-api.jsonl" ] || ! kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
  CODEX_SOCK=$c "$AB/bin/opencode-rpc" wait-idle "$(cat "$c/session")"
  # a request after the turn (its title) is kept once its answer has ended: each the stand-in answered has a file
  while [ "$(find "$c/raw" -name 'call-*.log' | wc -l)" -lt "$(wc -l <"$T/go-api.jsonl")" ] && kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
  sent=$(grep -F '"asked": true' "$T/go-api.jsonl" | head -n 1)
  is "the turn's request went to the stand-in's chat completions" "$(jq -r .path <<<"$sent")" /v1/chat/completions
  has "for $MODEL" "$(jq -r .model <<<"$sent")" "${MODEL#*/}"
  is "with the key opencode keeps, and not the plugin's mark" "$(jq -c '[.key, (.headers | index("x-coord-session"))]' <<<"$sent")" '[true,null]'
  is "a file for each request" "$(find "$c/raw" -name 'call-*.log' | wc -l)" "$(wc -l <"$T/go-api.jsonl")"
  turn=$(grep -lF 'Say done.' "$c"/raw/call-*.log | head -n 1)
  has "the turn's file, under its session" "$(cat "$turn")" "Session: $(cat "$c/session")"
  has "with the answer as it came" "$(sed -n '/^=== API RESPONSE 1 ===$/,$p' "$turn")" 'Done here.'
  is "the owner's alone" "$(stat -c %a "$turn")" 600
  held=$(grep -rlF -- "$KEY" "$COORD" 2>/dev/null | grep -v -e "^$acct/home/" -e "^$acct/usage-key$" -e "^$AB/tests/opencode-try.sh$" || true)
  is "the key in none of them, nor in any other file of ours" "$held" ""
  r=$(ask "{\"op\":\"rawpage\",\"name\":\"$A\"}") || true
  lines=$(jq -r '.lines | join("\n")' <<<"$r" 2>/dev/null || echo "$r")
  has "the raw pane's page has the request" "$lines" "${MODEL#*/} · status 200 ══"
  has "and its answer" "$lines" $'[reply text]\nDone here.'
fi

say "7. the agent let go"
pid=$(cat "$COORD/codex-home/app/$A.pid")
has "its server is opencode-rpc serve $A" "$(tr '\0' ' ' <"/proc/$pid/cmdline")" "opencode-rpc serve $A "
co roster down "$A"
while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
ok "its server ($pid) ended"
[ ! -e "$COORD/codex-home/app/$A.pid" ] && ok "its pid file is gone" || no "its pid file is left"
for _ in $(seq 30); do
  st=$(co coordc ls | awk -F'\t' -v n="$A" '$1 == n { print $3 }')
  [ "$st" = stopped ] && break
  sleep 1
done
is "coordd has it" "$st" stopped

say "8. his Check limits, Go's page read again"
go_page rate-limited 100
r=$(ask '{"op":"accountcheck","id":"go"}') || true
echo "   coordd: $r"
has "read" "$r" "weekly 100% until"
week=$(jq -r '.seven_day.resets_at' "$acct/state.json")
is "out until the week's reset" "$(jq -r '.out_until' "$acct/state.json")" "$week"
is "for its limit, in Go's words" "$(jq -c '[.out_for, .out_words]' "$acct/state.json")" '["rate_limit","Go: its weekly limit is used up"]'
go_page ok 1
r=$(ask '{"op":"accountcheck","id":"go"}') || true
has "read again" "$r" "weekly 1% until"
is "the week begun again: it takes requests" "$(jq -r '.out_until' "$acct/state.json")" null

echo
[ $fails = 0 ] && echo "opencode-try: all ok" || echo "opencode-try: $fails failed"
exit $fails
