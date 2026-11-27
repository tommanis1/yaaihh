#!/usr/bin/env python3
"""channel.py: the coord channel of one Claude Code agent.

An MCP server over stdio that declares Claude Code's `claude/channel` capability. bin/roster starts it for every agent
(`--mcp-config`, `--dangerously-load-development-channels server:coord`). It listens on $COORD/agents/$AGENT_NAME/
channel.sock; coordd connects, writes one JSON line {events: [{content, meta, keys?}, ...]} (one event per mail) and
reads `ok` once they are handed to Claude Code, which shows it to the model as <channel source="coord" from=... id=...>body</channel>: it starts a turn
when the session is idle (coordd holds mail while a turn runs). Nothing else is sent to the session; what the mail
is and what to do with it is the standing text (claude/standing.md, roster passes it with --append-system-prompt).

Plain JSON-RPC 2.0, newline-delimited, no SDK: the server answers initialize, ping and three tools, `ask`, `reply` and
`pass`, and sends notifications. `pass` (consult-lead N-412 post 1900) passes the owner's @all handed to the agent on
to another, as `coord pass` does: one request to coordd, which checks and posts it. `reply` (paper-research N-101.p1285) posts an answer to one message, by its id, as `coord post`
would, recorded as answering it; the single word Nothing posts nothing (N-544), and is no answer to the owner's post
that the turn must answer (`his`, below; paper-research N-412.p2202, 1). `ask` (docket-edge's 8, paper-research m1197) is coordd's `ask_wait`: the call returns with the answer,
or after coordd's wait (10 minutes) with "the answer will come later", never an error. It is always loaded (_meta
anthropic/alwaysLoad), so the model needs no tool search first; roster's settings keep Claude Code from moving it to
the background (CLAUDE_CODE_MCP_AUTO_BACKGROUND_MS=0) or cutting it (MCP_TOOL_TIMEOUT above coordd's wait). A
cancelled call hangs up on coordd, which takes that as the same cut.
"""
import json, os, re, socket, sys, threading

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
# The ids of the messages handed that hold the owner's posts the turn must answer, each with those posts' ids, and each
# such post's own id with itself (coordd's `his`, beside the meta, by delivery: his post put into a running turn, or
# handed again for being left unanswered): the reply tool's Nothing is no answer to them, and says so, naming his posts
# alone, never another post that ends such a message (paper-research N-412.p2202, 1; consult-3 p2210). The latest 2000
# ids are kept.
his, his_order = {}, []


# What the reply tool says when coordd took its typed post as no request: a coordd from before the kind (coord-types
# `KIND_UNTAKEN`, the same words, which its test holds equal; paper-research N-1481 p48)
KIND_UNTAKEN = "this coordd takes no kind: name whom you ask with @NAME"

ASK_TOOL = {
    "name": "ask",
    "description": "Ask a question when you cannot go on without its answer; write the whole of it once (what, why, "
                   "what you would do). It waits for the answer inside your turn, up to 10 minutes. If none comes by "
                   "then, it says the answer will come later, as a message.",
    "inputSchema": {"type": "object", "properties": {
        "question": {"type": "string", "description": "the question, as you would write it to a person"},
        "context": {"type": "string", "description": "what the one answering needs to know, if anything"}},
        "required": ["question"]},
    "_meta": {"anthropic/alwaysLoad": True},
}
REPLY_TOOL = {
    "name": "reply",
    "description": "Answer a message: give the message's id (as its channel tag's id shows it, e.g. N-349.p3) and "
                   "your answer. The answer is posted on that ticket, under that message. It reaches whoever wrote it "
                   "as an ask does if they asked you something, else at their next turn. One answer to several "
                   "messages: give all their ids, comma-separated, the first where it is posted; each is recorded as "
                   "answered. Use it for each message you answer when you were given several at once; a message "
                   "answered with it gets nothing more from your turn's end.",
    "inputSchema": {"type": "object", "properties": {
        "to": {"type": "string", "description": "the id of the message answered, e.g. N-349.p3; several: "
                                                 "N-355.p1, N-360.p1 (posted on the first), commas or spaces"},
        "text": {"type": "string", "description": "your answer, whole"},
        "star": {"type": "boolean", "description": "star your answer for the owner to look at (it shows in his "
                                                    "Starred list until he opens it)"},
        # what the answer is (I-1480), as coord post's KIND flag sends it: `{"is": "ask", "to": ["w2"]}`,
        # `{"is": "read", "to": [...], "head": ...}`, `{"is": "blocker", "to": [...]}`, `{"is": "hold", "on": "I-12"}`,
        # `{"is": "result"}`, `{"is": "fyi"}`; its words, one line a kind, are N-1481 p6/p11 as p23, p24, p73 and
        # p74 settled them
        "kind": {"type": "object",
                 "description": "What your answer is. This decides who gets it now and who reads it later. Put @ only "
                                "on whoever must act on it; type everything else result or fyi, and write other names "
                                "without @. Untyped, an @NAME is an ask to NAME, and a post that names nobody is an "
                                "fyi."
                                "\n- {\"is\": \"ask\", \"to\": [TO]}: TO must do or answer something. TO gets it at "
                                "the end of its turn, or at once if it is idle."
                                "\n- {\"is\": \"read\", \"to\": [TO], \"head\": HEAD}: TO must read HEAD and say "
                                "whether it is right. TO gets it as an ask."
                                "\n- {\"is\": \"blocker\", \"to\": [TO]}: something in your way that only TO can lift. "
                                "TO gets it as an ask."
                                "\n- {\"is\": \"hold\", \"on\": ON}: work on ON, an item (I-N), stops until this is "
                                "settled. Whoever has started ON gets it at the end of its turn, or at once if it is "
                                "idle."
                                "\n- {\"is\": \"result\"}: what you did or found. Posted on the thread or item where "
                                "someone asked you for it, it reaches them once, as an ask does. Anyone else on the "
                                "thread or item it is posted on sees its first line at its next turn's start."
                                "\n- {\"is\": \"fyi\"}: news that nobody has to act on. If it answers something "
                                "someone asked you, it reaches them as an ask does; otherwise it wakes nobody, and "
                                "whoever is on the thread or item it is posted on sees its first line at its next "
                                "turn's start.",
                 "properties": {"is": {"type": "string", "enum": ["ask", "read", "blocker", "hold", "result", "fyi"]},
                                "to": {"type": "array", "items": {"type": "string"}},
                                "head": {"type": "string"}, "on": {"type": "string"}},
                 "required": ["is"]}},
        "required": ["to", "text"]},
    "_meta": {"anthropic/alwaysLoad": True},
}
PASS_TOOL = {
    "name": "pass",
    "description": "Pass the owner's @all on to the agent whose work it is: give the message's id (e.g. N-349.p3), "
                   "that agent's name, and why. Only the agent it is handed to passes it, before anybody has answered "
                   "it; the agent you name then holds it, to answer or pass on, and you owe nothing on it. Any agent of "
                   "the project that runs now; not one that held it and passed it before.",
    "inputSchema": {"type": "object", "properties": {
        "message": {"type": "string", "description": "the id of the owner's @all, e.g. N-349.p3"},
        "agent": {"type": "string", "description": "the agent to pass it to"},
        "why": {"type": "string", "description": "why it is theirs, one line (shown on the thread)"}},
        "required": ["message", "agent"]},
    "_meta": {"anthropic/alwaysLoad": True},
}
# the calls under way, by JSON-RPC id: their coordd connections, closed when Claude Code cancels the call
calls, calls_lock = {}, threading.Lock()


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
    # a boxed agent (coordd's sandbox.rs) runs as its own uid and coordd as the owner's: the socket is open, its folder
    # is the owner's and the agent's group's only
    if os.environ.get("AGENT_BOX"):
        os.chmod(SOCK, 0o777)
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
                # a delivery without an id (the owner's words to an agent that holds only them) brings its dedupe key
                # outside what the model is shown: taken off the meta here
                key = ev.get("meta", {}).pop("key", None)
                mid = ev.get("meta", {}).get("id") or key
                # a delivery handed again because its first hand-over was never taken up goes in all the same
                if mid and ev.get("meta", {}).get("again"):
                    mid = f"{mid} again"
                # one message joining several deliveries (coordd's render::handover) brings each one's key: it is
                # dropped when every one of them went through already, as when coordd hands a batch again, or part of
                # it, after an answer it did not hear (it hands the batch's own deliveries first, core.rs `retry`).
                # The keys come beside the meta (render::event: the meta holds strings only, for Claude Code drops a
                # notification whose meta has a list, N-1396 p71). Build 27's coordd put them in the meta, so they
                # are taken off it all the same.
                in_meta = ev.get("meta", {}).pop("keys", None)
                keys = [str(k) for k in ev.get("keys") or in_meta or ([mid] if mid else [])]
                with out:
                    got = ev.get("his") or {}
                    # a list (this head's first form): each id stands for itself
                    pairs = got.items() if isinstance(got, dict) else ((h, [h]) for h in got)
                    for d, posts in pairs:
                        posts = [str(q) for q in posts or []]
                        for k, v in [(str(d), posts)] + [(q, [q]) for q in posts]:
                            if k not in his:
                                his_order.append(k)
                            his[k] = v
                    while len(his_order) > 2000:
                        his.pop(his_order.pop(0), None)
                    if keys and all(k in passed for k in keys):
                        continue
                    for k in keys:
                        if k not in passed:
                            passed.add(k)
                            passed_order.append(k)
                    while len(passed_order) > 2000:
                        passed.discard(passed_order.pop(0))
                write({"jsonrpc": "2.0", "method": "notifications/claude/channel",
                       "params": {"content": ev["content"], "meta": ev.get("meta", {})}})
            c.sendall(b"ok\n")
        except (ValueError, KeyError) as e:
            c.sendall(f"error {e}\n".encode())


def tool_result(rid, text, error=False):
    write({"jsonrpc": "2.0", "id": rid, "result": {"content": [{"type": "text", "text": text}], "isError": error}})


def ask(rid, args):
    """The ask tool's call RID: coordd's ask_wait, its reply the tool's result. coordd knows the asker by this process
    (it runs in the agent's cgroup)."""
    question = str(args.get("question") or "").strip()
    if not question:
        return tool_result(rid, "ask: the question is empty.", True)
    req = {"op": "ask_wait", "text": question, "context": (str(args.get("context") or "").strip() or None)}
    try:
        c = socket.socket(socket.AF_UNIX)
        c.connect(os.environ["COORDD_SOCK"])
    except (OSError, KeyError) as e:
        return tool_result(rid, f"ask: coordd cannot be reached ({e}); ask with coord ask instead.", True)
    with calls_lock:
        calls[rid] = c
    buf = b""
    try:
        c.sendall((json.dumps(req) + "\n").encode())
        while b"\n" not in buf:
            chunk = c.recv(65536)
            if not chunk:
                break
            buf += chunk
    except OSError:
        pass
    finally:
        with calls_lock:
            cancelled = calls.pop(rid, None) is None
        c.close()
    if cancelled:
        return  # Claude Code gave the call up: it reads no answer
    try:
        reply = json.loads(buf.split(b"\n", 1)[0])
    except ValueError:
        return tool_result(rid, "No answer yet. The answer will come later, as a message to you; your step stays yours "
                                "and waits for it. Go on with any work that does not depend on it; end your turn if "
                                "nothing else is left.")
    tool_result(rid, reply.get("out", ""), not reply.get("ok", False))


def reply(rid, args):
    """The reply tool's call RID (paper-research N-101.p1285, 1): a post on the message's ticket that answers it
    (`answers`), as `coord post` makes it. coordd knows the writer by this process."""
    raw = args.get("to") or ""
    ids = [str(i).strip() for i in raw] if isinstance(raw, list) else [i.strip() for i in re.split(r"[,\s]+", str(raw))]
    ids = [i for i in ids if i]
    text = str(args.get("text") or "").strip()
    if not text:
        return tool_result(rid, "reply: the answer is empty.", True)
    if not ids:
        return tool_result(rid, "reply: no message id given, like N-349.p3.", True)
    # the tag of a message joining several deliveries gives every id, space-separated, as an agent may copy it
    # (N-412.p851 N-591.p40 C-12.s3): the messages' ids are taken as written; a step's or a notice's is not answered
    # here, so they are named and nothing is posted
    others = [i for i in ids if "." in i and not re.fullmatch(r"[A-Za-z]+-\d+\.p\d+", i)]
    if others:
        steps = [i for i in others if re.fullmatch(r"[A-Za-z]+-\d+\.s\d+(\.[a-z]+)?", i)]
        why = f" {', '.join(steps)}: a step, which your turn's last message answers." if steps else ""
        return tool_result(rid, f"reply: {', '.join(others)} {'is' if len(others) == 1 else 'are'} not a message id like "
                                f"N-349.p3; nothing was posted.{why} Give the reply tool the ids of the messages you "
                                f"answer.", True)
    # the first is where it is posted; the rest are answered too (paper-research N-101 p1361)
    to, also = ids[0], ids[1:]
    t, _, n = to.partition(".p")
    if not t or (n and not n.isdigit()):
        return tool_result(rid, f"reply: {to!r} is not a message id like N-349.p3.", True)
    bad = [a for a in also if not re.fullmatch(r"[A-Za-z]+-\d+\.p\d+", a)]
    if bad:
        return tool_result(rid, f"reply: {', '.join(bad)} is not a message id like N-349.p3.", True)
    # the single word Nothing posts nothing and wakes nobody, as at a turn's end (paper-research N-544): what the turn
    # was given counts as read when it ends (its `turn_ended` line), so nothing is sent to coordd now. Nor for the
    # owner's post the turn must answer (`his`), which is not read unanswered: it comes again once, then stays owed
    # (coordd's `his_at_end`; paper-research N-412.p2202, 1), and the words say so
    if says_nothing(text):
        owner = his_of(ids)
        rest = [i for i in ids if not his_has(i)]
        said = []
        if rest:
            said.append(f"{', '.join(rest)} {'counts' if len(rest) == 1 else 'count'} as read when your turn ends, as a "
                        f"turn ending with the word Nothing does.")
        if owner:
            said.append(f"Nothing is no answer to the owner's post: {', '.join(owner)}. It is not read until you answer "
                        f"it: left unanswered when your turn ends, it is handed to you again if it has not been "
                        f"already; after that, the foot of each message you are given names it until you answer it. "
                        f"Answer it with this tool, to its id, or with your turn's last message on its thread.")
        return tool_result(rid, "Nothing posted, nobody woken: " + " ".join(said))
    try:
        req = {"op": "post", "t": t, "text": text, "answers": int(n) if n else None}
        if also:
            req["also"] = also
        # its kind as given, a word alone read as `{"is": WORD}`; coordd checks it (`kind_taken`) and says what is wrong.
        # A typed post goes as `typedpost`, which a coordd from before the kind refuses as no request, where it would
        # take `post` and drop the kind (coord-types `Request::line`; paper-research N-1481 p56)
        kind = args.get("kind")
        if isinstance(kind, str) and kind.strip():
            kind = {"is": kind.strip()}
        if isinstance(kind, dict) and kind:
            req["kind"] = kind
            req["op"] = "typedpost"
        r = coordd(req)
    except (OSError, KeyError, ValueError) as e:
        return tool_result(rid, f"reply: coordd cannot be reached ({e}); answer with coord post {t} TEXT instead.", True)
    out = r.get("out", "")
    if req["op"] == "typedpost" and not r.get("ok") and "unknown variant `typedpost`" in out:
        return tool_result(rid, KIND_UNTAKEN, True)
    # star: the new post, by the number coordd's answer gives ("Posted on N-349 as post 7, under post 3.")
    if r.get("ok") and args.get("star"):
        m = re.search(r" as post (\d+)", out)
        try:
            s = coordd({"op": "star", "t": f"{t}.p{m.group(1)}", "on": True}) if m else {"out": "no post number to star"}
            out += " " + s.get("out", "")
        except (OSError, KeyError, ValueError) as e:
            out += f" Not starred: coordd cannot be reached ({e}); coord star {t}.pN stars it."
    tool_result(rid, out, not r.get("ok", False))


def pass_on(rid, args):
    """The pass tool's call RID (consult-lead N-412 post 1900): coordd's `pass`, as `coord pass T.pN NAME WHY` sends it;
    coordd knows the passer by this process, and its answer (done, or why not) is the tool's result."""
    mid = str(args.get("message") or "").strip()
    agent = str(args.get("agent") or "").strip().lstrip("@")
    m = re.fullmatch(r"([A-Za-z]+-\d+)\.p(\d+)", mid)
    if not m:
        return tool_result(rid, f"pass: {mid!r} is not a message id like N-349.p3.", True)
    if not agent:
        return tool_result(rid, "pass: no agent named to pass it to.", True)
    req = {"op": "pass", "t": m.group(1), "n": int(m.group(2)), "to": agent}
    why = str(args.get("why") or "").strip()
    if why:
        req["why"] = why
    try:
        r = coordd(req)
    except (OSError, KeyError, ValueError) as e:
        return tool_result(rid, f"pass: coordd cannot be reached ({e}); pass it with coord pass {mid} {agent} instead.", True)
    tool_result(rid, r.get("out", ""), not r.get("ok", False))


def his_of(ids):
    """The owner's posts the turn must answer that IDS are, or that the messages IDS held (`his`), in order, once."""
    with out:
        named = []
        for i in ids:
            for q in his.get(i, []):
                if q not in named:
                    named.append(q)
        return named


def his_has(i):
    """Whether I is one of the owner's posts the turn must answer, or a message that held one (`his`)."""
    with out:
        return i in his


def says_nothing(text):
    """Whether TEXT is the single word Nothing, as coordd reads a turn's last message (ticket/mod.rs `says_nothing`):
    trimmed, one full stop allowed, any case."""
    t = text.strip()
    return (t[:-1] if t.endswith(".") else t).lower() == "nothing"


def coordd(req):
    """One request to coordd, its reply as a dict."""
    c = socket.socket(socket.AF_UNIX)
    c.connect(os.environ["COORDD_SOCK"])
    with c:
        c.sendall((json.dumps(req) + "\n").encode())
        buf = b""
        while b"\n" not in buf:
            chunk = c.recv(65536)
            if not chunk:
                break
            buf += chunk
    return json.loads(buf.split(b"\n", 1)[0])


def cancel(rid):
    with calls_lock:
        c = calls.pop(rid, None)
    if c is not None:
        try:
            c.shutdown(socket.SHUT_RDWR)  # coordd sees the hang-up and takes it as the wait cut
        except OSError:
            pass


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
            elif req.get("method") == "notifications/cancelled":
                cancel(req.get("params", {}).get("requestId"))
            continue
        method = req.get("method")
        if method == "initialize":
            result = {"protocolVersion": req.get("params", {}).get("protocolVersion", "2025-06-18"),
                      "capabilities": {"experimental": {"claude/channel": {}}, "tools": {}},
                      "serverInfo": {"name": "coord", "version": "1"}}
            write({"jsonrpc": "2.0", "id": req["id"], "result": result})
        elif method == "ping":
            write({"jsonrpc": "2.0", "id": req["id"], "result": {}})
        elif method == "tools/list":
            write({"jsonrpc": "2.0", "id": req["id"], "result": {"tools": [ASK_TOOL, REPLY_TOOL, PASS_TOOL]}})
        elif method == "tools/call" and req.get("params", {}).get("name") == "ask":
            # off the reading loop: a wait of minutes must not hold up pings, cancels or the next call
            threading.Thread(target=ask, args=(req["id"], req["params"].get("arguments") or {}), daemon=True).start()
        elif method == "tools/call" and req.get("params", {}).get("name") == "reply":
            threading.Thread(target=reply, args=(req["id"], req["params"].get("arguments") or {}), daemon=True).start()
        elif method == "tools/call" and req.get("params", {}).get("name") == "pass":
            threading.Thread(target=pass_on, args=(req["id"], req["params"].get("arguments") or {}), daemon=True).start()
        else:
            write({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32601, "message": f"no method {method}"}})
    try:
        os.unlink(SOCK)  # the session ended: coord sees the agent stopped
    except OSError:
        pass


if __name__ == "__main__":
    main()
