#!/usr/bin/env python3
"""tonight.jsonl: research's record lines for the 0.3 acceptance test (proto-reader, N-1446 p11).

The lines are copied byte for byte, never edited. The first line of the output names the record, each act's line
range with the sha256 of its raw bytes, and the numbers of the lines kept, so anyone with the record can check the copy:

    python3 -I tonight.py ~/agent-coord/research/tickets/log.jsonl > ../src/tests/tonight.jsonl

Each act keeps only the lines about its tickets and its agents:
- act i-kept, N-294 (16 Nov 15:43:51Z), a thread his message opened, closed Kept 72 s later: a thread of his that
  ended, for S6 insight (3)'s Ended and its median (proto-lead N-1446 p473, 2); with its two handler runs (his post's,
  and the watcher's Nothing on paper-research's p2), as act 1 keeps its own;
- act 0, N-1104's opening (19 Nov 14:12:11Z), so the thread is opened as it was;
- act 0e, the record's era line (20 Nov 12:34:07Z), so names like consult-5 are agents, as they were tonight;
- act i-moved, N-1432 (20 Nov 12:37:04Z), his message's thread moved into N-1104 3 s later: not counted by insight
  (3), his words went onto another thread (p473, 2);
- act 1, his p586 and p587 on N-1104 (20 Nov 18:22:55Z-18:24:22Z): N-1104's lines, the two owner_post runs, and
  paper-research's free / delivery / given / turn lines;
- act i-agents, N-1443 (20 Nov 18:40:14Z), an agents' thread paper-research opened and sent (closed Between at once):
  insight (3)'s row of agents' threads, not his completion (p473, 2);
- act 1b, N-1445 opened to paper-research-brain and his p3 there routed to paper-research (record line 103418).
The acts are in the record's order, so the lines are too.
"""
import hashlib
import json
import sys

ACTS = [
    {
        # N-294, 16 Nov: his message's thread, closed Kept at 15:45:03Z; with its owner_post run (…964) and the run on
        # paper-research's p2 (…965, the watcher's Nothing at 14510), so a driver plays them as written (accounts-2
        # N-1446 p580)
        "act": "i-kept",
        "lines": [14478, 14520],
        "tickets": [294, 1099511627964, 1099511627965],
        "agents": [],
    },
    {
        # N-1104 opened, 19 Nov: his first words to paper-research
        "act": "0",
        "lines": [75089, 75092],
        "tickets": [1104],
        "agents": [],
    },
    {
        # the record's era 2 (machine.rs:646 ERA_NAMES), begun at 12:34:07Z: consult-5, here-1 and consult-6 are agents
        "act": "0e",
        "lines": [96826, 96826],
        "tickets": [],
        "agents": [],
    },
    {
        # N-1432, 20 Nov: his message's thread, closed Moved to N-1104 at 12:37:07Z
        "act": "i-moved",
        "lines": [97530, 97540],
        "tickets": [1432],
        "agents": [],
    },
    {
        "act": "1",
        "lines": [101193, 101270],
        "tickets": [1104, 1099511630338, 1099511630339],
        "agents": ["paper-research"],
    },
    {
        # N-1443, 20 Nov: paper-research's agents' thread, sent (closed Between) at 18:40:14Z
        "act": "i-agents",
        "lines": [101718, 101719],
        "tickets": [1443],
        "agents": [],
    },
    {
        "act": "1b",
        "lines": [103218, 103598],
        "tickets": [1445, 1099511630386],
        "agents": ["paper-research-brain"],
    },
]
# an agent's own lines worth keeping beside its tickets' (what it was given and when its turn ended)
AGENT_LINES = {"turn_ended", "given"}
AGENT_EVENTS = {"free", "delivery_acted", "delivered"}


def keeps(d, act):
    if d.get("t") in act["tickets"] or d.get("parent") in act["tickets"]:
        # a delivery ticket's lines and other projects' givens on these tickets are kept too
        return True
    ev = d.get("ev")
    if ev == "event" and d.get("name") == "era":
        return True
    if ev == "event" and any(f.get("t") in act["tickets"] for f in d.get("fired") or []):
        # the event that opened one of them (N-1104's owner_message)
        return True
    if ev in AGENT_LINES and (d.get("agent") in act["agents"] or d.get("to") in act["agents"]):
        return d.get("t") is None or d.get("t") in act["tickets"]
    if ev == "event" and d.get("name") in AGENT_EVENTS:
        v = d.get("value") or {}
        return v.get("agent") in act["agents"] or v.get("to") in act["agents"]
    return False


def main(path):
    with open(path, "rb") as f:
        raw = f.read().split(b"\n")
    head = {"tonight": "research tickets/log.jsonl", "acts": []}
    body = []
    for act in ACTS:
        a, b = act["lines"]
        chunk = raw[a - 1 : b]
        kept = [i for i, l in enumerate(chunk, a) if keeps(json.loads(l), act)]
        head["acts"].append({"act": act["act"], "lines": [a, b], "sha256": hashlib.sha256(b"\n".join(chunk)).hexdigest(), "kept": kept})
        body.extend(raw[i - 1] for i in kept)
    out = sys.stdout.buffer
    out.write(json.dumps(head, separators=(",", ":")).encode() + b"\n")
    out.write(b"\n".join(body) + b"\n")


if __name__ == "__main__":
    main(sys.argv[1])
