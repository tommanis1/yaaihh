#!/usr/bin/env python3
"""importer.py's tests, against a stand-in engine on a unix socket that answers as coordd-v2's serve.rs does: a watch
is done at once and its lines come after; an act's reply goes before the lines it wrote reach the watch.
`python3 -I ops/release/v1import/test_importer.py`."""

import hashlib
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
IMPORTER = os.path.join(HERE, "importer.py")


class Engine(threading.Thread):
    """The stand-in: Events are performed unless named in REFUSE; a Take-Words of thread T asks TextOf of each of
    POSTED[T] in turn and ends; each answer is kept in ANSWERS. With PAR (as yaaihh.v1's `par ... up to 1` runs on
    coordd-v2) the run is waiting and each TextOf is its branch's, RUN.K, with no line of the branch's own beginning
    or end; without, the run rests on its first TextOf."""

    def __init__(self, path, posted, refuse=(), par=True):
        super().__init__(daemon=True)
        self.posted, self.refuse, self.answers, self.record, self.par = posted, set(refuse), [], [], par
        self.waiting = {}
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(path)
        self.srv.listen(1)

    def write(self, conn, line):
        self.record.append(line)
        self.pending.append((len(self.record) - 1, line))

    def flush(self, conn):
        for at, line in self.pending:
            conn.sendall(json.dumps({"is": "line", "at": at, "line": line}).encode() + b"\n")
        self.pending = []

    def run(self):
        conn, _ = self.srv.accept()
        try:
            self.serve(conn)
        finally:
            conn.close()
            self.srv.close()

    def serve(self, conn):
        self.pending, buf = [], b""
        while True:
            b = conn.recv(1 << 16)
            if not b:
                return
            buf += b
            *whole, buf = buf.split(b"\n")
            for raw in whole:
                req = json.loads(raw)
                reply = self.request(conn, req)
                conn.sendall(json.dumps(reply).encode() + b"\n")
                self.flush(conn)

    def request(self, conn, req):
        if req["do"] == "watch":
            return {"is": "done", "value": None}
        if req["do"] == "act" and req["name"].endswith("Take-Words"):
            run = f"r{len(self.record)}"
            self.write(conn, {"began": {"run": run, "by": req["args"][0]}})
            nodes = list(self.posted.get(req["args"][1], []))
            if not nodes:
                self.write(conn, {"ended": {"run": run, "value": None}})
                return {"is": "done", "value": {"run": run, "state": "done"}}
            self.write(conn, {"effect": {"op": "std.comms.TextOf", "args": [nodes[0]], "run": self.branch(run, 0)}})
            self.waiting[len(self.record) - 1] = (run, nodes[1:], 1)
            state = "waiting" if self.par else {"resting": len(self.record) - 1}
            return {"is": "done", "value": {"run": run, "state": state}}
        if req["do"] == "act":
            kind = "refused" if req["name"] in self.refuse else "performed"
            self.write(conn, {kind: {"act": {"inst": req["name"], "args": req["args"]}}})
            return {"is": "done", "value": {"event": kind}}
        if req["do"] == "answer":
            run, left, k = self.waiting.pop(req["line"])
            self.answers.append(req["answer"]["value"])
            if left:
                self.write(conn, {"effect": {"op": "std.comms.TextOf", "args": [left[0]], "run": self.branch(run, k)}})
                self.waiting[len(self.record) - 1] = (run, left[1:], k + 1)
            else:
                self.write(conn, {"ended": {"run": run, "value": None}})
            return {"is": "done", "value": None}
        return {"is": "no", "why": f"no such request {req['do']}"}

    def branch(self, run, k):
        return f"{run}.{k}" if self.par else run


def words(node, w):
    return {"node": node, "sha256": hashlib.sha256(w.encode()).hexdigest(), "words": w}


class Importer(unittest.TestCase):
    def go(self, plan, ws, posted, refuse=(), par=True):
        with tempfile.TemporaryDirectory() as d:
            sock = os.path.join(d, "s")
            e = Engine(sock, posted, refuse, par)
            e.start()
            for name, lines in (("plan.jsonl", plan), ("words.jsonl", ws)):
                with open(os.path.join(d, name), "w") as f:
                    f.writelines(json.dumps(l) + "\n" for l in lines)
            p = subprocess.run([sys.executable, "-I", IMPORTER, "--socket", sock, "--plan", os.path.join(d, "plan.jsonl"),
                                "--words", os.path.join(d, "words.jsonl"), "--cgroup-out", os.path.join(d, "cgroup"),
                                "--wait", "5"], capture_output=True, text=True, timeout=60)
            with open(os.path.join(d, "cgroup")) as f:
                cg = f.read()
            return p, e, cg

    PLAN = [{"do": "act", "name": "yaaihh.v1.Thread-Was", "args": ["N-1", "N-1", "thread", "ann", "The map", 1]},
            {"do": "act", "name": "yaaihh.v1.Passage-Was", "args": ["N-1.p1", "ann", "N-1"]},
            {"do": "act", "name": "yaaihh.v1.Passage-Was", "args": ["N-1.p2", "bea", "N-1"]},
            {"do": "act", "name": "yaaihh.v1.Thread-Was", "args": ["N-2", "N-2", "thread", "ann", "Empty", 2]},
            {"do": "act", "name": "yaaihh.v1.Take-Words", "args": ["imp", "N-1"]},
            {"do": "act", "name": "yaaihh.v1.Take-Words", "args": ["imp", "N-2"]},
            {"do": "act", "name": "yaaihh.v1.Import-Closed", "args": []}]
    WORDS = [words("N-1.p1", "I looked at the map code."), words("N-1.p2", "Two levels, then.")]

    def test_each_textof_is_answered_with_its_passages_words_once(self):
        p, e, cg = self.go(self.PLAN, self.WORDS, {"N-1": ["N-1.p2", "N-1.p1"]})
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("5 Events performed, 2 threads' words taken, 2 TextOf answered", p.stdout)
        self.assertEqual(e.answers, ["Two levels, then.", "I looked at the map code."])
        self.assertTrue(cg.startswith("/"), cg)

    def test_a_run_resting_on_its_own_textof_is_answered_too(self):
        p, e, _ = self.go(self.PLAN, self.WORDS, {"N-1": ["N-1.p1", "N-1.p2"]}, par=False)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(e.answers, ["I looked at the map code.", "Two levels, then."])

    def test_a_refused_event_stops_the_import_naming_it(self):
        p, e, _ = self.go(self.PLAN, self.WORDS, {"N-1": ["N-1.p1"]}, refuse={"yaaihh.v1.Passage-Was"})
        self.assertEqual(p.returncode, 1)
        self.assertIn("request 2, yaaihh.v1.Passage-Was", p.stdout)
        self.assertIn("N-1.p1", p.stdout)
        self.assertEqual(len(e.record), 2, "nothing after the refused line")

    def test_words_not_as_the_file_has_them_are_never_sent(self):
        bad = [dict(self.WORDS[0], words="I looked at something else."), self.WORDS[1]]
        p, e, _ = self.go(self.PLAN, bad, {"N-1": ["N-1.p1"]})
        self.assertEqual(p.returncode, 1)
        self.assertIn("the words of N-1.p1 are not those", p.stdout)
        self.assertEqual(e.answers, [])

    def test_a_passage_the_file_lacks_stops_the_import(self):
        p, e, _ = self.go(self.PLAN, self.WORDS[:1], {"N-1": ["N-1.p1", "N-1.p2"]})
        self.assertEqual(p.returncode, 1)
        self.assertIn("the words of N-1.p2, which the words file lacks", p.stdout)
        self.assertEqual(e.answers, ["I looked at the map code."])


if __name__ == "__main__":
    unittest.main()
