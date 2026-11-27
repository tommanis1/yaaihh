#!/usr/bin/env python3
"""check.py's record check, on made-up v2 records: `python3 -I ops/release/v1import/test_check.py`."""

import json
import os
import subprocess
import sys
import tempfile
import unittest

CHECK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "check.py")
PLAN = [{"do": "act", "name": "yaaihh.v1.Thread-Was", "args": ["N-1", "N-1", "thread", "ann", "The map", 1]},
        {"do": "act", "name": "yaaihh.v1.Passage-Was", "args": ["N-1.p1", "ann", "N-1"]},
        {"do": "act", "name": "yaaihh.v1.Take-Words", "args": ["imp", "N-1"]},
        {"do": "act", "name": "yaaihh.v1.Import-Closed", "args": []}]


def performed(name, run=None):
    return {"performed": {"act": {"inst": name, "args": []}, "party": "imp", "run": run}}


GOOD = [{"loaded": "ab"}, performed("yaaihh.v1.Thread-Was"), performed("yaaihh.v1.Passage-Was"),
        {"began": {"run": "r3", "by": "imp"}}, performed("yaaihh.v1.Take-Words", "r3"),
        {"effect": {"op": "std.comms.TextOf", "args": ["N-1.p1"], "run": "r3"}}, {"answered": {"line": 5, "value": "x"}},
        performed("yaaihh.v1.Words-Were-Taken", "r3"), {"ended": {"run": "r3", "value": None}},
        performed("yaaihh.v1.Import-Closed")]


class Record(unittest.TestCase):
    def check(self, lines):
        with tempfile.TemporaryDirectory() as d:
            os.mkdir(os.path.join(d, "plan"))
            with open(os.path.join(d, "plan", "plan.jsonl"), "w") as f:
                f.writelines(json.dumps(l) + "\n" for l in PLAN)
            with open(os.path.join(d, "plan", "words.jsonl"), "w") as f:
                f.write(json.dumps({"node": "N-1.p1", "sha256": "", "words": "x"}) + "\n")
            with open(os.path.join(d, "record.jsonl"), "w") as f:
                f.writelines(json.dumps(l) + "\n" for l in lines)
            return subprocess.run([sys.executable, "-I", CHECK, "record", "--record", os.path.join(d, "record.jsonl"),
                                   "--plan", os.path.join(d, "plan")], capture_output=True, text=True)

    def test_a_record_as_the_plan_says_holds(self):
        p = self.check(GOOD)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("record: 10 lines", p.stdout)
        self.assertNotIn("MISS", p.stdout)

    def test_a_refused_line_misses(self):
        p = self.check(GOOD[:2] + [{"refused": {"act": {"inst": "yaaihh.v1.Passage-Was", "args": []}, "why": {"policy": "no"}}}] + GOOD[3:])
        self.assertEqual(p.returncode, 1)
        self.assertIn("MISS line 2: refused", p.stdout)
        self.assertIn("0 performed lines of yaaihh.v1.Passage-Was for 1", p.stdout)

    def test_a_run_that_never_ended_misses(self):
        p = self.check(GOOD[:-2] + GOOD[-1:])
        self.assertEqual(p.returncode, 1)
        self.assertIn("1 runs began and did not end: ['r3']", p.stdout)

    def test_a_run_no_take_words_began_misses(self):
        """a reaction an imported Event began: on the machine's start it would be a real model's or session's"""
        p = self.check(GOOD[:3] + [{"began": {"run": "r2", "by": "imp"}}, {"ended": {"run": "r2", "value": None}}] + GOOD[3:])
        self.assertEqual(p.returncode, 1)
        self.assertIn("MISS 2 runs for 1 Take-Words", p.stdout)

    def test_words_not_answered_miss(self):
        p = self.check([l for l in GOOD if "answered" not in l])
        self.assertEqual(p.returncode, 1)
        self.assertIn("0 answered lines for 1 passages", p.stdout)


if __name__ == "__main__":
    unittest.main()
