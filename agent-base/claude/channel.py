#!/usr/bin/env python3
"""channel.py: the coord channel of one Claude Code agent.

An MCP server over stdio that declares Claude Code's `claude/channel` capability. bin/roster starts it for every agent
(`--mcp-config`, `--dangerously-load-development-channels server:coord`). It listens on $COORD/agents/$AGENT_NAME/
channel.sock; coordd connects, writes one JSON line {events: [{content, meta}, ...]} (one event per mail) and
reads `ok` once they are handed to Claude Code, which shows it to the model as <channel source="coord" from=... id=...>body</channel>: it starts a turn
when the session is idle (coordd holds mail while a turn runs). Nothing else is sent to the session; what the mail
is and what to do with it is the standing text (claude/team-mail.md, roster passes it with --append-system-prompt).

Plain JSON-RPC 2.0, newline-delimited, no SDK: the server only answers initialize and ping and sends notifications.
"""
import json, os, socket, sys, threading

NAME, COORD = os.environ["AGENT_NAME"], os.environ["COORD"]
# One socket per session: two sessions with one agent's name (8 Nov: a resumed background job came with an empty
# second session) each bound channel.sock, the last took it, and mail to the agent went to the empty one. coordd
# connects to the socket of the session its hooks named (core.rs push); channel.sock is the fallback with no id.
SID = os.environ.get("CLAUDE_CODE_SESSION_ID", "")
SOCK = f"{COORD}/agents/{NAME}/channel-{SID}.sock" if SID else f"{COORD}/agents/{NAME}/channel.sock"
out = threading.Lock()
# The mail ids already handed to Claude Code: coordd hands a mail again only when it had no `ok`, and one that went
# through all the same is not passed on twice (design coordd-core-2026-11-10 §4). The latest 2000 are kept.
passed, passed_order = set(), []


def write(msg):
    with out:
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()


def serve():
    os.makedirs(os.path.dirname(SOCK), exist_ok=True)  # a new agent's folder may not exist yet
    try:
        os.unlink(SOCK)
    except OSError:
        pass
    srv = socket.socket(socket.AF_UNIX)
    srv.bind(SOCK)
    srv.listen(16)
    while True:
        c, _ = srv.accept()
        threading.Thread(target=client, args=(c,), daemon=True).start()


def client(c):
    with c:
        buf = b""
        while b"\n" not in buf:
            chunk = c.recv(65536)
            if not chunk:
                return  # a liveness probe: connect and close
            buf += chunk
        try:
            for ev in json.loads(buf.split(b"\n", 1)[0])["events"]:
                mid = ev.get("meta", {}).get("id")
                with out:
                    if mid in passed:
                        continue
                    if mid:
                        passed.add(mid)
                        passed_order.append(mid)
                        if len(passed_order) > 2000:
                            passed.discard(passed_order.pop(0))
                write({"jsonrpc": "2.0", "method": "notifications/claude/channel",
                       "params": {"content": ev["content"], "meta": ev.get("meta", {})}})
            c.sendall(b"ok\n")
        except (ValueError, KeyError) as e:
            c.sendall(f"error {e}\n".encode())


def main():
    for line in sys.stdin:
        try:
            req = json.loads(line)
        except ValueError:
            continue
        if "id" not in req:
            if req.get("method") == "notifications/initialized" and not any(
                    t.name == "serve" for t in threading.enumerate()):
                threading.Thread(target=serve, name="serve", daemon=True).start()
            continue
        method = req.get("method")
        if method == "initialize":
            result = {"protocolVersion": req.get("params", {}).get("protocolVersion", "2025-06-18"),
                      "capabilities": {"experimental": {"claude/channel": {}}},
                      "serverInfo": {"name": "coord", "version": "1"}}
            write({"jsonrpc": "2.0", "id": req["id"], "result": result})
        elif method == "ping":
            write({"jsonrpc": "2.0", "id": req["id"], "result": {}})
        else:
            write({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32601, "message": f"no method {method}"}})
    try:
        os.unlink(SOCK)  # the session ended: coord sees the agent stopped
    except OSError:
        pass


if __name__ == "__main__":
    main()
