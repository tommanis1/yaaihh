#!/usr/bin/env bash
# The reply tool and the word Nothing (paper-research N-544): channel.py given `reply` with the single word Nothing
# posts nothing (no request reaches coordd) and says the messages count as read at the turn's end, but for the owner's
# post the turn must answer, to which Nothing is no answer (N-412.p2202, 1); any other words are posted as before. A
# typed reply goes as `typedpost` with its kind, and a coordd from before the kind, which refuses that op as no request,
# is said in KIND_UNTAKEN's words (I-1480; paper-research N-1481 p56). Runs channel.py against a stand-in coordd socket
# in a folder of its own, removed after.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
python3 -I - "$here/claude/channel.py" "$dir" <<'EOF'
import json, os, socket, subprocess, sys, threading, time
channel, d = sys.argv[1], sys.argv[2]
os.makedirs(f"{d}/coord/agents/w", exist_ok=True)
sock = f"{d}/coordd.sock"
got = []
before = []  # once set, the stand-in answers as a coordd from before the kind: `typedpost` is no request to it
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
            if before and req["op"] == "typedpost":
                out = {"ok": False, "out": "coordd: not a request: unknown variant `typedpost`, expected one of `msg`, `post` at line 1 column 19"}
            else:
                out = {"ok": True, "out": f"Posted on {req['t']} as post 9, under post 3."}
            c.sendall((json.dumps(out) + "\n").encode())
threading.Thread(target=serve, daemon=True).start()
env = dict(os.environ, AGENT_NAME="w", COORD=f"{d}/coord", COORDD_SOCK=sock, CLAUDE_CODE_SESSION_ID="s1")
p = subprocess.Popen([sys.executable, "-I", channel], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True)
def call(i, args):
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": i, "method": "tools/call", "params": {"name": "reply", "arguments": args}}) + "\n")
    p.stdin.flush()
    while True:
        r = json.loads(p.stdout.readline())
        if r.get("id") == i:
            return r["result"]
fails = 0
def check(what, ok):
    global fails
    print(("ok   " if ok else "FAIL ") + what)
    fails += 0 if ok else 1
for i, word in enumerate(["Nothing", "  nothing.\n", "NOTHING"], 1):
    r = call(i, {"to": "N-3.p2", "text": word})
    text = r["content"][0]["text"]
    check(f"{word!r}: nothing sent to coordd", got == [])
    check(f"{word!r}: not an error, and says so: {text}", not r.get("isError") and text.startswith("Nothing posted, nobody woken: N-3.p2 counts as read"))
r = call(4, {"to": "N-3.p2, N-4.p1", "text": "Nothing", "star": True})
check("several ids: nothing sent, both named", got == [] and "N-3.p2, N-4.p1 count as read" in r["content"][0]["text"])
r = call(5, {"to": "N-3.px", "text": "Nothing"})
check("a bad id is refused as before", r.get("isError") is True and got == [])
for i, word in [(6, "Nothing more from me."), (7, "Nothing!")]:
    r = call(i, {"to": "N-3.p2", "text": word})
    check(f"{word!r} is posted", got and got[-1] == {"op": "post", "t": "N-3", "text": word, "answers": 2})
# the tag's ids as an agent may copy them, space-separated (one message joining several, 0.2 line 211): taken as the
# comma-separated form is; a step's id among them is refused, named, and nothing is posted
r = call(8, {"to": "N-412.p851 N-591.p40", "text": "both are in"})
check("ids with spaces: posted on the first, the second answered too", not r.get("isError") and got[-1] == {"op": "post", "t": "N-412", "text": "both are in", "answers": 851, "also": ["N-591.p40"]})
n = len(got)
r = call(9, {"to": "N-412.p851 N-591.p40 C-12.s3", "text": "done"})
text = r["content"][0]["text"]
check(f"a step's id among them: refused, named, nothing posted: {text}", r.get("isError") is True and len(got) == n and "C-12.s3" in text and "a step" in text)
r = call(10, {"to": "N-5.closed", "text": "seen"})
check("a notice's id: refused, nothing posted", r.get("isError") is True and len(got) == n and "N-5.closed" in r["content"][0]["text"])
# the owner's post the turn must answer (coordd's `his` beside the meta: put into a running turn, or handed again for
# being left unanswered): Nothing is no answer to it, and the words say it comes again once, then stays named at the
# foot; an agent's post given the same way still counts as read (paper-research N-412.p2202, 1). The words name his
# posts alone: N-7.p3 is a message his p2 and w3's p3 make, and Nothing to it names p2 (consult-3 p2210)
p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
p.stdin.flush()
chan = f"{d}/coord/agents/w/channel-s1.sock"
for _ in range(100):
    if os.path.exists(chan):
        break
    time.sleep(0.05)
c = socket.socket(socket.AF_UNIX)
c.connect(chan)
with c:
    c.sendall((json.dumps({"events": [
        {"content": "owner wrote: stop and tell me", "meta": {"from": "owner", "kind": "owner", "id": "N-7.p3"}, "keys": ["N-7.p3"], "his": {"N-7.p3": ["N-7.p2"]}},
        {"content": "lead wrote: the fixture is in", "meta": {"from": "lead", "kind": "thread", "id": "N-8.p1"}, "keys": ["N-8.p1"]},
    ]}) + "\n").encode())
    check("the channel takes both", c.recv(16) == b"ok\n")
n = len(got)
r = call(11, {"to": "N-7.p3", "text": "Nothing"})
text = r["content"][0]["text"]
check(f"his post's message: nothing sent, Nothing is no answer, it comes again if it has not, then is named at the foot: {text}",
      len(got) == n and not r.get("isError") and text.startswith("Nothing posted, nobody woken: Nothing is no answer to the owner's post: N-7.p2.")
      and "handed to you again if it has not been already" in text and "names it until you answer it" in text and "counts as read" not in text)
r = call(12, {"to": "N-7.p2", "text": "nothing"})
check("his post by its own id: the same", len(got) == n and "no answer to the owner's post: N-7.p2." in r["content"][0]["text"])
r = call(13, {"to": "N-8.p1", "text": "Nothing"})
text = r["content"][0]["text"]
check(f"an agent's post given beside it: read at the turn's end, as before: {text}", len(got) == n and text == "Nothing posted, nobody woken: N-8.p1 counts as read when your turn ends, as a turn ending with the word Nothing does.")
r = call(14, {"to": "N-8.p1, N-7.p3", "text": "Nothing"})
text = r["content"][0]["text"]
check(f"both at once: each said its own way: {text}", len(got) == n and text.startswith("Nothing posted, nobody woken: N-8.p1 counts as read when your turn ends") and "no answer to the owner's post: N-7.p2." in text and "N-7.p3" not in text)
# a typed reply (I-1480): its kind as given, a word alone as {"is": WORD}, sent as `typedpost`; to a coordd from before
# the kind, which refuses that op, the tool says so in KIND_UNTAKEN's words, and an untyped reply goes as `post`
r = call(15, {"to": "N-3.p2", "text": "check the lock", "kind": {"is": "ask", "to": ["w2"]}})
check("a typed reply goes as typedpost with its kind", not r.get("isError") and got[-1] == {"op": "typedpost", "t": "N-3", "text": "check the lock", "answers": 2, "kind": {"is": "ask", "to": ["w2"]}})
r = call(16, {"to": "N-3.p2", "text": "the lock passed", "kind": "fyi"})
check("a kind's word alone is its `is`", not r.get("isError") and got[-1].get("op") == "typedpost" and got[-1].get("kind") == {"is": "fyi"})
before.append(True)
r = call(17, {"to": "N-3.p2", "text": "check the lock", "kind": {"is": "ask", "to": ["w2"]}})
text = r["content"][0]["text"]
check(f"to a coordd from before, refused as no request: said in the words: {text}", r.get("isError") is True and text == "this coordd takes no kind: name whom you ask with @NAME" and got[-1]["op"] == "typedpost")
r = call(18, {"to": "N-3.p2", "text": "the lock passed"})
check("untyped, to it: posted as before", not r.get("isError") and got[-1] == {"op": "post", "t": "N-3", "text": "the lock passed", "answers": 2})
p.stdin.close(); p.wait(timeout=10)
sys.exit(1 if fails else 0)
EOF
