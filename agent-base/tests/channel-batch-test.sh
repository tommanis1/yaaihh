#!/usr/bin/env bash
# One message joining several deliveries (coordd's render::handover, 0.2 line 211) through channel.py: it reaches the
# session once. coordd hands a batch again when it heard no `ok`, its own deliveries first (core.rs `retry`, consult-3
# N-807.p112, 1); channel.py drops a message every part of which went through already (the event's `keys`, beside the
# meta), so the batch, or part of it, sent again after a lost answer is not given twice, and what came since goes
# after it. A delivery handed again because it was not taken up still goes in. Every meta the session is shown holds
# strings only (Claude Code drops a notification whose meta has a list, N-1396 p71); build 27's coordd put the keys in
# the meta, and they are taken off it the same. Runs channel.py with its socket in a folder of its own, removed after.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
python3 -I - "$here/claude/channel.py" "$dir" <<'EOF'
import json, os, socket, subprocess, sys, threading, time
channel, d = sys.argv[1], sys.argv[2]
os.makedirs(f"{d}/coord/agents/w", exist_ok=True)
env = dict(os.environ, AGENT_NAME="w", COORD=f"{d}/coord", COORDD_SOCK=f"{d}/none.sock", CLAUDE_CODE_SESSION_ID="s1")
p = subprocess.Popen([sys.executable, "-I", channel], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True)
seen = []
def read():
    for line in p.stdout:
        m = json.loads(line)
        if m.get("method") == "notifications/claude/channel":
            seen.append(m["params"])
threading.Thread(target=read, daemon=True).start()
p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
p.stdin.flush()
sock = f"{d}/coord/agents/w/channel-s1.sock"
for _ in range(100):
    if os.path.exists(sock):
        break
    time.sleep(0.05)
def send(*events):
    c = socket.socket(socket.AF_UNIX)
    c.connect(sock)
    with c:
        c.sendall((json.dumps({"events": list(events)}) + "\n").encode())
        answer = c.recv(16)
    time.sleep(0.2)  # the notifications are written before the answer; the reader thread takes them
    return answer
def batch(content, ids, again=(), in_meta=False):
    # the event as render::event makes it: the keys beside the meta; IN_META, as build 27's coordd sent it
    meta = {"from": "coord", "kind": "handover", "id": " ".join(ids)}
    keys = [f"{i} again" if i in again else i for i in ids]
    if again:
        meta["again"] = "second hand-over"
    if in_meta:
        meta["keys"] = keys
        return {"content": content, "meta": meta}
    return {"content": content, "meta": meta, "keys": keys}
fails = 0
def check(what, ok):
    global fails
    print(("ok   " if ok else "FAIL ") + what)
    fails += 0 if ok else 1
check("a batch passes, answered ok", send(batch("A and B", ["N-1.p1", "N-2.p4"])) == b"ok\n" and [s["content"] for s in seen] == ["A and B"])
check("the session is not shown the keys", "keys" not in seen[0]["meta"] and seen[0]["meta"]["id"] == "N-1.p1 N-2.p4")
check("the same batch again (its ok lost): dropped, answered ok", send(batch("A and B", ["N-1.p1", "N-2.p4"])) == b"ok\n" and len(seen) == 1)
check("part of it again (the rest no longer due): dropped", send({"content": "B", "meta": {"id": "N-2.p4", "from": "w3"}}) == b"ok\n" and len(seen) == 1)
check("what came since: passes", send({"content": "C", "meta": {"id": "N-3.p2", "from": "lead"}}) == b"ok\n" and [s["content"] for s in seen] == ["A and B", "C"])
check("a batch with a part handed again as not taken up: passes", send(batch("A again, D", ["N-1.p1", "N-4.p1"], again={"N-1.p1"})) == b"ok\n" and len(seen) == 3)
check("one delivery as before: once", send({"content": "E", "meta": {"id": "N-5.p1"}}) == b"ok\n" and send({"content": "E", "meta": {"id": "N-5.p1"}}) == b"ok\n" and [s["content"] for s in seen][-1] == "E" and len(seen) == 4)
check("build 27's batch, its keys in the meta: passes, the keys not shown", send(batch("F and G", ["N-6.p1", "N-7.p2"], in_meta=True)) == b"ok\n" and len(seen) == 5 and "keys" not in seen[-1]["meta"])
check("part of build 27's batch again: dropped", send({"content": "G", "meta": {"id": "N-7.p2", "from": "w3"}}) == b"ok\n" and len(seen) == 5)
check("every meta the session was shown holds strings only", all(isinstance(v, str) for s in seen for v in s["meta"].values()))
p.stdin.close(); p.wait(timeout=10)
sys.exit(1 if fails else 0)
EOF
