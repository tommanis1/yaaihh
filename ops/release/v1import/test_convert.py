#!/usr/bin/env python3
"""convert.py's tests, on made-up records: `python3 -I ops/release/v1import/test_convert.py`."""

import calendar
import json
import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import convert  # noqa: E402

AT = "2026-11-20T10:00:00Z"
T0 = calendar.timegm((2026, 11, 20, 10, 0, 0))  # AT in seconds


def record(n):
    """A made-up v1 record of N lines."""
    return b"".join(json.dumps({"ev": "post" if i % 2 else "given", "t": 1}).encode() + b"\n" for i in range(n))


def views(rec, *tickets):
    return [{"record": {"lines": rec.count(b"\n"), "bytes": len(rec)}, "problem_lines": 0}, *tickets]


def post(n, by, words, at=AT, **more):
    return {"n": n, "at": at, "by": by, "names": [], "told": [], "text": words, "words": words, **more}


THREAD = {"id": "N-1", "kind": "thread", "title": "The map", "opener": "ann", "opened_at": AT,
          "posts": [post(1, "owner", "Keep it small.", names=["ann"]),
                    post(2, "ann", "Done: two levels.", "2026-11-20T10:01:00Z", names=["owner"]),
                    post(3, "ann", "@bea the map code is yours.", "2026-11-20T10:02:00Z", names=["bea"])],
          "closed_at": "2026-11-20T11:00:00Z", "ended": {"how": "done", "at": "2026-11-20T11:00:00Z"}}
ITEM = {"id": "I-2", "kind": "item", "title": "Draw the map", "opener": "ann", "opened_at": AT, "parent": "N-1",
        "posts": [post(1, "ann", "Draw it.")], "started": ["bea"]}


class Convert(unittest.TestCase):
    def plan(self, *tickets, rec=None):
        rec = rec if rec is not None else record(4)
        return convert.convert(views(rec, *tickets), rec, "imp", "o1")

    def names(self, plan):
        return [l["name"].rsplit(".", 1)[1] for l in plan.lines]

    def test_the_driver_file_never_names_words_were_taken(self):
        """paper-research N-1817 p187: Take-Words performs it after its last answer; a driver that could would mark a
        thread's words taken with none taken."""
        h = convert.hand("v1-import")
        self.assertNotIn("Words-Were-Taken", h)
        self.assertIn("v1-import effect std.comms.TextOf", h)
        plan, _ = self.plan(THREAD, ITEM)
        self.assertNotIn("Words-Were-Taken", json.dumps(plan.lines))
        for l in plan.lines:
            self.assertTrue(l["name"] in convert.EVENTS or l["name"] == convert.TAKE_WORDS, l)
            self.assertTrue(l["name"] == convert.TAKE_WORDS or f"v1-import event {l['name']}" in h, l)

    def test_a_copys_driver_carries_its_effects_once_and_never_words_were_taken(self):
        h = convert.hand("v1-import", ["std.comms.TextOf", "std.comms.Model", "std.comms.Model", "std.control.Sleep"])
        lines = [l for l in h.splitlines() if not l.startswith("#")]
        self.assertEqual([l for l in lines if " effect " in l],
                         ["v1-import effect std.comms.TextOf", "v1-import effect std.comms.Model",
                          "v1-import effect std.control.Sleep"])
        self.assertNotIn("Words-Were-Taken", h)
        self.assertEqual(convert.hand("v1-import", []), convert.hand("v1-import"))

    def test_a_thread_comes_in_in_v1s_order_with_its_times_and_words(self):
        plan, report = self.plan(THREAD, ITEM)
        self.assertEqual(plan.lines[0]["args"], [convert.sha256(record(4)), 4, "imp"])
        self.assertEqual(self.names(plan), [
            "Record-Taken",
            "Thread-Was", "His-Words-To-Were", "His-Post-Was", "Written", "Said-To-Him-Was", "Seen-Was", "Written",
            "Passage-Was", "Written", "Ended-Was",
            "Thread-Was", "Passage-Was", "Written", "Work-Was",
            "Under-Was", "Take-Words", "Take-Words", "Import-Closed"])
        self.assertEqual(plan.lines[1]["args"], ["N-1", "N-1", "thread", "ann", "The map", T0])
        self.assertEqual(plan.lines[2]["args"], ["N-1.p1", "ann"])
        self.assertEqual(plan.lines[4]["args"], ["N-1.p1", T0])
        self.assertEqual(plan.lines[5]["args"], ["N-1.p2", "ann", "N-1"])
        self.assertEqual(plan.lines[10]["args"], ["N-1", "done", T0 + 3600])
        self.assertEqual(plan.lines[14]["args"], ["I-2", "bea", "ann"])
        self.assertEqual(plan.lines[15]["args"], ["I-2", "N-1"])
        self.assertEqual(plan.lines[16]["args"], ["imp", "N-1"])
        self.assertEqual([w["node"] for w in plan.words], ["N-1.p1", "N-1.p2", "N-1.p3", "I-2.p1"])
        self.assertEqual(plan.words[1]["sha256"], convert.sha256("Done: two levels.".encode()))
        self.assertEqual(report["listed"], {"open item": ["I-2"]})
        self.assertEqual(sorted(plan.parties), ["ann", "bea", "imp", "o1"])
        qs = [q["query"] for q in plan.queries]
        # as yaaihh.v1, where queries are read, sees each name
        for q in ['Holds(V1-Id("N-1", "N-1"))', 'Holds(Desk.His("N-1.p1"))',
                  'Holds(C.Spoken("N-1.p2", "ann"))', 'Holds(Desk.Posted("N-1.p3", "N-1"))',
                  'Holds(Desk.Ended("N-1", "done"))', 'Holds(R.Assigned("bea", "I-2"))',
                  'Holds(Closed())', 'Holds(H.Party("o1"))']:
            self.assertIn(q, qs)
        self.assertEqual(len(qs), len(set(qs)))

    def test_his_post_is_his_whoever_it_was_to(self):
        """V1's "owner" is OWNER in v2; his words to nobody are still his post, with no one they were to."""
        t = dict(THREAD, posts=[post(1, "owner", "A note to myself.")])
        plan, _ = self.plan(t)
        self.assertEqual(self.names(plan)[1:4], ["Thread-Was", "His-Post-Was", "Written"])

    def test_his_words_are_heard_by_each_he_was_given_to_once(self):
        t = dict(THREAD, posts=[post(1, "owner", "All of you.", names=["ann"], told=["ann", "bea"], inform=["cid"])])
        plan, _ = self.plan(t)
        self.assertEqual([l["args"] for l in plan.lines if l["name"].endswith("His-Words-To-Were")],
                         [["N-1.p1", "ann"], ["N-1.p1", "bea"], ["N-1.p1", "cid"]])

    def test_a_reply_answering_him_is_said_to_him(self):
        t = dict(THREAD, posts=[post(1, "ann", "Done.", answering=["owner"], auto=True)])
        plan, _ = self.plan(t)
        self.assertEqual(self.names(plan)[2:4], ["Said-To-Him-Was", "Seen-Was"])

    def test_a_step_answer_with_only_a_head_says_its_head(self):
        t = dict(THREAD, posts=[post(1, "owner", "", head={"head": "Granted", "n": 3}, step=1)])
        plan, report = self.plan(t)
        self.assertEqual(plan.words[0]["words"], '{"head": "Granted", "n": 3}')
        self.assertEqual(report["listed"]["passage words from its head (JSON)"], ["N-1.p1"])

    def test_a_question_still_open_to_him_stays_with_him(self):
        t = dict(THREAD, posts=[post(1, "ann", "Ship tonight?", names=["owner"], asks_open=True)])
        del t["ended"], t["closed_at"]
        plan, report = self.plan(t)
        self.assertEqual(self.names(plan)[1:5], ["Thread-Was", "Asked-Him-Was", "Said-To-Him-Was", "Written"])
        self.assertNotIn("Seen-Was", self.names(plan))
        self.assertEqual(report["listed"], {"open thread": ["N-1"]})

    def test_an_item_his_ask_opened_is_owed_by_the_one_it_waits_on(self):
        i = {"id": "I-5", "kind": "item", "title": "Do it", "opener": None, "opened_at": AT, "posts": [],
             "binding": 'on asked(e) where e.by == "owner" open item(e.title, none, e.post)',
             "waits_on": [{"agent": "cid", "call": "goal", "ticket": "I-5"}]}
        mine = dict(i, id="I-6", opener="cid", binding=None)
        plan, report = self.plan(i, mine)
        self.assertEqual([l["args"] for l in plan.lines if l["name"].endswith("Work-Was")], [["I-5", "cid", "o1"]])
        self.assertEqual(report["listed"]["open item its own opener holds (no Work-Was: holder and giver are one)"], ["I-6"])

    def test_an_open_questions_asking_passage_stays_with_him(self):
        q = {"id": "Q-3", "kind": "question", "title": "Ship tonight?", "opener": "ann", "opened_at": AT,
             "status": {"is": "awaits"}, "posts": [post(1, "ann", "Ship tonight?")]}
        plan, report = self.plan(q)
        self.assertEqual(self.names(plan)[1:5], ["Thread-Was", "Asked-Him-Was", "Said-To-Him-Was", "Written"])
        self.assertNotIn("Seen-Was", self.names(plan))
        self.assertEqual(report["listed"], {"open question": ["Q-3"]})

    def test_a_passage_blank_in_v1_is_listed_blank(self):
        t = dict(THREAD, id="N-7", posts=[post(1, "ann", "")])
        plan, report = self.plan(t)
        self.assertEqual(plan.words[0]["words"], "")
        self.assertEqual(report["listed"]["passage blank in v1's record too"], ["N-7.p1"])

    def test_a_ticket_its_program_failed_ends_failed(self):
        h = {"id": "H-4", "kind": "head", "title": "A head", "opener": "ann", "opened_at": AT, "closed_at": AT,
             "status": {"is": "failed", "raised": {"exc": "Cancelled"}}, "posts": []}
        plan, report = self.plan(h)
        self.assertEqual([l["args"] for l in plan.lines if l["name"].endswith("Ended-Was")], [["H-4", "failed", T0]])
        self.assertEqual(report["listed"], {"ticket ended failed (as v1's program left it)": ["H-4"]})

    def test_what_comes_in_short_is_listed_by_id(self):
        t = dict(THREAD, posts=[post(1, "ann", "", gone=True), post(2, "bea", "", withheld=True),
                                post(3, "ann", "Gone again.", taken_back=True)],
                 struck=[{"post": post(4, "ann", "Struck.")}])
        plan, report = self.plan(t)
        self.assertEqual(report["listed"]["passage words gone"], ["N-1.p1"])
        self.assertEqual(report["listed"]["passage words withheld"], ["N-1.p2"])
        self.assertEqual(report["listed"]["passage taken back after an agent had it (v1 draws it struck through)"], ["N-1.p4"])
        self.assertEqual([w["node"] for w in plan.words], ["N-1.p1", "N-1.p2"])

    def test_a_record_not_the_one_the_views_are_of_is_refused(self):
        rec = record(4)
        with self.assertRaises(SystemExit):
            convert.convert(views(rec, THREAD), rec[:-1], "imp", "o1")
        with self.assertRaises(SystemExit):
            convert.convert(views(rec, THREAD), record(5), "imp", "o1")

    def test_the_files_written_hold_no_place_a_record_names(self):
        """A place's path is never carried into v2: the plan carries ids and words, never where v1 read them."""
        rec = record(4)
        loc = {"loc": {"file": "/x/projects/p/s.jsonl", "offset": 7, "len": 9}}
        t = dict(THREAD, posts=[post(1, "ann", "Read from its place.", text=loc)])
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "views.jsonl"), "w") as f:
                f.writelines(json.dumps(l) + "\n" for l in views(rec, t))
            with open(os.path.join(d, "record.jsonl"), "wb") as f:
                f.write(rec)
            out = os.path.join(d, "out")
            subprocess.run([sys.executable, "-I", convert.__file__, "--views", os.path.join(d, "views.jsonl"),
                            "--record", os.path.join(d, "record.jsonl"), "--importer", "imp", "--driver", "v1-import",
                            "--owner", "o1", "--out", out], check=True, capture_output=True)
            self.assertEqual(sorted(os.listdir(out)), ["hand.txt", "plan.jsonl", "queries.jsonl", "report.json", "words.jsonl", "world.docket"])
            for name in os.listdir(out):
                with open(os.path.join(out, name)) as f:
                    self.assertNotIn("/x/projects", f.read(), name)
            with open(os.path.join(out, "world.docket")) as f:
                self.assertIn('+C.Role("o1", "owner").', f.read())


if __name__ == "__main__":
    unittest.main()
