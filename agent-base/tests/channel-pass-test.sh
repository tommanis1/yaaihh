#!/usr/bin/env bash
# The pass tool (consult-lead N-412 post 1900): channel.py lists `pass` beside `reply`, and a call sends coordd one
# request, the one `coord pass T.pN NAME WHY` sends ({"op": "pass", "t", "n", "to", "why"}; coordd's Request::Pass, whose
# reading of this very line claims_tests.rs checks); coordd's answer is the tool's result, its refusal an error with
# coordd's words. A bad id or no agent sends nothing. Runs channel.py against a stand-in coordd socket in a folder of
# its own, removed after.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
python3 -I - "$here/claude/channel.py" "$dir" <<'EOF'
import json, os, socket, subprocess, sys, threading
channel, d = sys.argv[1], sys.argv[2]
os.makedirs(f"{d}/coord/agents/w", exist_ok=True)
sock = f"{d}/coordd.sock"
got = []
srv = socket.socket(socket.AF_UNIX); srv.bind(sock); srv.listen(5)
def serve():
    while True:
        c, _ = srv.accept()
        with c:
            buf = b""
            while b"\n" not in buf:
                buf += c.recv(65536)
            req = json.loads(buf.split(b"\n", 1)[0])
            got.append(req)
            # a5 runs; a6 has stopped, as coordd would refuse it
            if req.get("to") == "a6":
                out = {"ok": False, "out": "coord pass: a6 cannot be woken now (not running): N-3.p2, the owner's @all, stays with you."}
            else:
                out = {"ok": True, "out": "Posted on N-3 as post 9. a5 holds N-3.p2 now, to answer or pass on; you owe nothing on it."}
            c.sendall((json.dumps(out) + "\n").encode())
threading.Thread(target=serve, daemon=True).start()
env = dict(os.environ, AGENT_NAME="w", COORD=f"{d}/coord", COORDD_SOCK=sock, CLAUDE_CODE_SESSION_ID="s1")
p = subprocess.Popen([sys.executable, "-I", channel], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True)
def rpc(i, method, params):
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": i, "method": method, "params": params}) + "\n")
    p.stdin.flush()
    while True:
        r = json.loads(p.stdout.readline())
        if r.get("id") == i:
            return r["result"]
def call(i, args):
    return rpc(i, "tools/call", {"name": "pass", "arguments": args})
fails = 0
def check(what, ok):
    global fails
    print(("ok   " if ok else "FAIL ") + what)
    fails += 0 if ok else 1
tools = [t["name"] for t in rpc(1, "tools/list", {})["tools"]]
check(f"pass is listed beside reply: {tools}", tools == ["ask", "reply", "pass"])
r = call(2, {"message": "N-3.p2", "agent": "@a5", "why": "the diff is a5's runner"})
check("one request, as coord pass sends it", got == [{"op": "pass", "t": "N-3", "n": 2, "to": "a5", "why": "the diff is a5's runner"}])
check(f"coordd's answer is the result: {r['content'][0]['text']}", not r.get("isError") and "a5 holds N-3.p2 now" in r["content"][0]["text"])
r = call(3, {"message": "N-3.p2", "agent": "a6"})
check("no why: none sent", got[-1] == {"op": "pass", "t": "N-3", "n": 2, "to": "a6"})
check(f"coordd's refusal is an error in its words: {r['content'][0]['text']}", r.get("isError") is True and "a6 cannot be woken" in r["content"][0]["text"])
before = len(got)
for i, args in [(4, {"message": "N-3", "agent": "a5"}), (5, {"message": "N-3.px", "agent": "a5"}), (6, {"message": "N-3.p2", "agent": " "})]:
    r = call(i, args)
    check(f"{args}: refused, nothing sent", r.get("isError") is True and len(got) == before)
p.stdin.close(); p.wait(timeout=10)
sys.exit(1 if fails else 0)
EOF
