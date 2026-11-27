#!/usr/bin/env python3
"""The v1 importer: convert.py's plan sent to coordd-v2 over its socket, each Take-Words run's TextOf answered from
the words file (N-1832).

    python3 -I importer.py --socket PATH --plan FILE --words FILE [--cgroup-out FILE] [--wait SECONDS]

With --cgroup-out it first writes its own cgroup there (`0::` of /proc/self/cgroup), which the parties table must
vouch for as the importer with its `--by-socket` driver, and waits up to --wait seconds for the socket. Then, one
request at a time, in the plan's order: each Event must come back `performed`; each Take-Words must start a run, and
every `effect` line of that run or of its `par` branches (runs RUN.K, which write no `began` or `ended` line of their
own) is a TextOf of one of the thread's passages, answered with its words once their sha256 is checked against the
file's, until the run's `ended` line. It stops at the first request refused, Event
refused, run raised or cancelled, or word that is not as the file has it, saying which, and exits 1. It reads only
PLAN and WORDS, and the socket.
"""

import argparse
import hashlib
import json
import os
import queue
import socket
import sys
import threading
import time

TEXT_OF = "std.comms.TextOf"
TAKE_WORDS = "yaaihh.v1.Take-Words"
# the only lines a Take-Words run is followed by; the watch keeps no other
FOLLOWED = ("effect", "ended", "raised", "cancelled")


class Stop(Exception):
    """The import stops here, saying why."""


class Wire:
    """One connection: requests out, and what comes back sorted into replies (`done`, `no`) and lines by number,
    of these only FOLLOWED's (the watch sends every line in order, so a line kept means every line before it came)."""

    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.connect(path)
        self.replies, self.lines, self.cv = queue.Queue(), {}, threading.Condition()
        self.closed = False
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        buf = b""
        while True:
            b = self.s.recv(1 << 16)
            if not b:
                break
            buf += b
            *whole, buf = buf.split(b"\n")
            for raw in whole:
                if not raw:
                    continue
                m = json.loads(raw)
                if m.get("is") == "line":
                    if next(iter(m["line"]), None) in FOLLOWED:
                        with self.cv:
                            self.lines[m["at"]] = m["line"]
                            self.cv.notify_all()
                else:
                    self.replies.put(m)
        with self.cv:
            self.closed = True
            self.cv.notify_all()
        self.replies.put({"is": "no", "why": "the engine closed the connection"})

    def ask(self, req):
        """REQ sent; its reply's value, or Stop with the engine's why."""
        self.s.sendall(json.dumps(req, ensure_ascii=False).encode() + b"\n")
        r = self.replies.get(timeout=600)
        if r.get("is") != "done":
            raise Stop(f"{req.get('do')} {req.get('name', req.get('line', ''))} {json.dumps(req.get('args', []), ensure_ascii=False)[:200]}: {r.get('why')}")
        return r.get("value")

    def next_line(self, at, timeout=600):
        """The first kept line from AT on, and its number, once the watch has it; those before AT are let go."""
        end = time.monotonic() + timeout
        with self.cv:
            while True:
                for k in [k for k in self.lines if k < at]:
                    del self.lines[k]
                if self.lines:
                    k = min(self.lines)
                    return k, self.lines.pop(k)
                left = end - time.monotonic()
                if self.closed or left <= 0:
                    raise Stop(f"no line from {at} on came")
                self.cv.wait(left)


def of_run(name, run):
    """NAME is RUN or one of its `par` branches (RUN.K, RUN.K.J, ...)."""
    return isinstance(name, str) and (name == run or name.startswith(run + "."))


def run_lines(wire, run, start):
    """The lines of RUN and its branches from START on, in order, each as it comes (another run's are passed over)."""
    at = start
    while True:
        k, line = wire.next_line(at)
        at = k + 1
        (kind, body), = line.items()
        if isinstance(body, dict) and of_run(body.get("run"), run):
            yield k, kind, body


def take_words(wire, words, req):
    """REQ, a Take-Words: its run's TextOf effects answered from WORDS, each once, until it ends; the count."""
    thread = req["args"][1]
    v = wire.ask(req)
    run, state = v.get("run"), v.get("state")
    if not run:
        raise Stop(f"Take-Words {thread}: no run began: {v}")
    if state == "done":
        return 0
    # resting on its first TextOf, or waiting on the `par` whose branches ask them: then from the first line not
    # yet taken, as every earlier run's lines were taken up to its end
    first = state.get("resting") if isinstance(state, dict) else (0 if state == "waiting" else None)
    if first is None:
        raise Stop(f"Take-Words {thread}: its run {run} is {state}")
    answered = 0
    for at, kind, body in run_lines(wire, run, first):
        if kind == "effect":
            if body.get("op") != TEXT_OF:
                raise Stop(f"Take-Words {thread}: line {at} asks {body.get('op')}, not TextOf")
            node = body["args"][0]
            w = words.get(node)
            if w is None:
                raise Stop(f"Take-Words {thread}: line {at} asks the words of {node}, which the words file lacks")
            if hashlib.sha256(w["words"].encode()).hexdigest() != w["sha256"]:
                raise Stop(f"Take-Words {thread}: the words of {node} are not those whose sha256 the file gives")
            wire.ask({"do": "answer", "line": at, "answer": {"value": w["words"], "costs": []}})
            answered += 1
        elif kind == "ended" and body.get("run") == run:
            return answered
        elif kind in ("raised", "cancelled"):
            raise Stop(f"Take-Words {thread}: its run {run} {kind} at line {at}: {json.dumps(body, ensure_ascii=False)[:300]}")


def main(argv):
    a = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    a.add_argument("--socket", required=True)
    a.add_argument("--plan", required=True)
    a.add_argument("--words", required=True)
    a.add_argument("--cgroup-out")
    a.add_argument("--wait", type=float, default=600)
    o = a.parse_args(argv)
    if o.cgroup_out:
        with open("/proc/self/cgroup") as f:
            cg = next(l[3:].strip() for l in f if l.startswith("0::"))
        with open(o.cgroup_out + ".part", "w") as f:
            f.write(cg + "\n")
        os.replace(o.cgroup_out + ".part", o.cgroup_out)
    with open(o.words, encoding="utf-8") as f:
        words = {w["node"]: w for w in map(json.loads, f) if w}
    with open(o.plan, encoding="utf-8") as f:
        plan = [json.loads(l) for l in f if l.strip()]
    end = time.monotonic() + o.wait
    while not os.path.exists(o.socket):
        if time.monotonic() > end:
            sys.exit(f"importer: no socket at {o.socket} after {o.wait} s")
        time.sleep(0.2)
    wire = Wire(o.socket)
    t0 = time.monotonic()
    events = taken = answered = 0
    try:
        wire.ask({"do": "watch", "from": 0})
        for k, req in enumerate(plan):
            if req["name"] == TAKE_WORDS:
                answered += take_words(wire, words, req)
                taken += 1
                continue
            v = wire.ask(req)
            if not isinstance(v, dict) or v.get("event") != "performed":
                raise Stop(f"request {k + 1}, {req['name']} {json.dumps(req['args'], ensure_ascii=False)[:200]}: {v}")
            events += 1
    except Stop as e:
        print(f"importer: stopped: {e}; {events} Events performed, {taken} threads' words taken, {answered} answers", flush=True)
        sys.exit(1)
    print(f"importer: done in {time.monotonic() - t0:.1f} s: {events} Events performed, {taken} threads' words taken, "
          f"{answered} TextOf answered", flush=True)


if __name__ == "__main__":
    main(sys.argv[1:])
