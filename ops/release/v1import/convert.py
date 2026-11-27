#!/usr/bin/env python3
"""v1's tickets, as v1's own machine views them, turned into the import's requests for coordd-v2 (N-1832).

    python3 -I convert.py --views FILE --record FILE --importer NAME --driver NAME --owner NAME --out DIR

VIEWS is what views_out.rs (branch v1-views, on production's commit) writes from a replay of the record: a first line
with the record's lines and the byte they end at, then one ticket a line, each post with the words v1 shows for it.
RECORD is the kept copy of v1's record, that many bytes; its sha256 goes into the import's first Event. IMPORTER is
the party the parties table vouches for the importer's connection, DRIVER its `--by-socket` driver's name, OWNER the
party v1's "owner" is in v2.

DIR is made new and gets:
  plan.jsonl    one wire request a line, in the order they are sent: yaaihh.v1's Events in v1's order, then one
                Take-Words for each thread, then Import-Closed
  words.jsonl   for each passage, what the importer answers its TextOf with, and that text's sha256
  hand.txt      the `--by-socket` file: DRIVER's Events and the Effect it answers; never Words-Were-Taken, which
                Take-Words performs itself after its last answer (paper-research N-1817 p187). With --carries, a copy's:
                DRIVER also carries those Effects, which a start with no session drivers has nobody else for (the load
                check, spec 14 check 4); the import must make no run perform one, which check.py checks
  world.docket  the parties the import names, and the owner's role, for a copy to start with
  queries.jsonl what a restarted engine must hold: for each thread, passage and party, as the gate asks (`Holds(..)`)
  report.json   the counts: v1's lines by kind, tickets by kind, passages by how they came in, and every ticket or
                passage that came in short of what v1 shows, by id
Nothing is read but VIEWS and RECORD.
"""

import argparse
import collections
import datetime
import hashlib
import json
import os
import sys

V1, DESK, COMMS = "yaaihh.v1.", "yaaihh.desk.", "std.comms."

# The Events the importer performs, and the Effect it answers; the plan uses no other name over the socket.
EVENTS = [V1 + e for e in ("Record-Taken", "Thread-Was", "Retitled-Was", "Under-Was", "Passage-Was", "Said-To-Him-Was",
                            "His-Post-Was", "His-Words-To-Were", "Seen-Was", "Ended-Was", "Work-Was", "Asked-Him-Was",
                            "Import-Closed")] + [DESK + "Written"]
EFFECTS = [COMMS + "TextOf"]
TAKE_WORDS = V1 + "Take-Words"
# performed by Take-Words's own body; a driver that names it could mark a thread's words taken with none taken
NEVER_HANDED = V1 + "Words-Were-Taken"

# A query is read in the top module, yaaihh.v1 (`--top`): its own names bare, an imported one by the alias v1.docket
# imports it as (as coordd-v2's gate writes a query, tests/gate.rs `seen`)
SEEN = {V1: "", DESK: "Desk.", COMMS: "C.", "std.rails.": "R.", "std.hooks.": "H."}


def seen(name):
    """Qualified NAME as yaaihh.v1 sees it."""
    module, last = name.rsplit(".", 1)
    return SEEN[module + "."] + last


# How v1 ended a ticket (`coord end`) as the desk words it; a ticket closed with no end is done.
HOW = {"done": "done", "dropped": "dropped", "kept": "kept"}


def seconds(at):
    """V1's time AT (an RFC 3339 text, or seconds already) in seconds since 1970."""
    if isinstance(at, (int, float)):
        return int(at)
    if isinstance(at, str):
        return int(datetime.datetime.fromisoformat(at.replace("Z", "+00:00")).timestamp())
    raise ValueError(f"no time: {at!r}")


def sha256(b):
    return hashlib.sha256(b).hexdigest()


class Plan:
    """The requests in order, the words by passage, and what the report counts."""

    def __init__(self, importer, owner):
        self.importer, self.owner = importer, owner
        self.lines, self.words, self.parties = [], [], {importer, owner}
        self.count = collections.Counter()
        self.queries = []
        self.listed = collections.defaultdict(list)

    def party(self, name):
        """V1's NAME as a v2 party: the owner by OWNER, any other name as it is."""
        p = self.owner if name == "owner" else name
        self.parties.add(p)
        return p

    def holds(self, what, name, *args):
        """A query the restarted engine must answer true: NAME(ARGS) holds."""
        self.queries.append({"what": what, "query": f"Holds({seen(name)}({', '.join(json.dumps(a, ensure_ascii=False) for a in args)}))"})

    def event(self, name, *args):
        assert name in EVENTS, name
        self.lines.append({"do": "act", "name": name, "args": list(args)})
        self.count["event " + name.rsplit(".", 1)[1]] += 1

    def act(self, name, *args):
        assert name == TAKE_WORDS, name
        self.lines.append({"do": "act", "name": name, "args": list(args)})
        self.count["act " + name.rsplit(".", 1)[1]] += 1


def to_him(post):
    """Post POST is said to the owner: typed to him, told him, or a reply answering him."""
    return any("owner" in (post.get(k) or []) for k in ("names", "told", "answering"))


def heard_by(post):
    """Whom v1 gave his post POST to: those it was typed to, those its handler told, those it informed; each once."""
    return [a for a in dict.fromkeys((post.get("names") or []) + (post.get("told") or []) + (post.get("inform") or []))
            if a != "owner"]


def passage(plan, thread, post, asking=False):
    """POST of THREAD as the desk's passage, its time after it, and its words for Take-Words; none when v1 does not
    draw it (taken back before anyone had it) or it only stands where it moved from. ASKING: it asks him what its open
    question ticket still waits on, so it stays with him however its own marks stand."""
    node = f"{thread}.p{post['n']}"
    if post.get("taken_back"):
        plan.count["passage not drawn by v1 (taken back before anyone had it)"] += 1
        return
    if post.get("moved"):
        plan.count["passage moved away (its words come in where it moved to)"] += 1
        return
    by = post.get("by") or "coord"
    if by == "owner":
        for a in heard_by(post):
            plan.event(V1 + "His-Words-To-Were", node, plan.party(a))
        plan.event(V1 + "His-Post-Was", node, thread)
        plan.holds("passage", DESK + "His", node)
        plan.count["passage his"] += 1
    elif to_him(post) or asking:
        open_ask = asking or post.get("asks_open")
        if open_ask:
            plan.event(V1 + "Asked-Him-Was", node)
        plan.event(V1 + "Said-To-Him-Was", node, plan.party(by), thread)
        plan.holds("passage", COMMS + "Spoken", node, plan.party(by))
        if not open_ask:
            plan.event(V1 + "Seen-Was", node)
        plan.count["passage said to him"] += 1
    else:
        plan.event(V1 + "Passage-Was", node, plan.party(by), thread)
        plan.holds("passage", COMMS + "Spoken", node, plan.party(by))
        plan.count["passage among agents"] += 1
    plan.event(DESK + "Written", node, seconds(post["at"]))
    plan.holds("passage", DESK + "Posted", node, thread)
    words = post.get("words")
    if words is None:
        words = post["text"] if isinstance(post.get("text"), str) else ""
    if not words and post.get("head") is not None:
        # a step's answer with no words, only its head (v1 draws it as a card): the head, as JSON, is what it said
        words = json.dumps(post["head"], ensure_ascii=False, sort_keys=True)
        plan.listed["passage words from its head (JSON)"].append(node)
    elif not words:
        # written with no words (`"text": ""`), or his bare `posted` at a thread's open: v1 shows it blank too
        plan.listed["passage blank in v1's record too"].append(node)
    for flag in ("gone", "withheld"):
        if post.get(flag):
            plan.listed["passage words " + flag].append(node)
    if post.get("dropped"):
        plan.listed["passage dropped by its handler (v1 shows it struck through)"].append(node)
    plan.words.append({"node": node, "sha256": sha256(words.encode()), "words": words})


def ticket(plan, v):
    """Ticket V: the thread, its passages, its end; an open item with a holder is still owed."""
    thread, kind = v["id"], v["kind"]
    title = v.get("title") or ""
    if not title:
        first = next((p.get("words") or "" for p in v.get("posts") or [] if p.get("words")), "")
        title = first.strip().split("\n", 1)[0][:120]
        plan.listed["ticket titled from its first passage (v1's title is a place)"].append(thread)
    plan.event(V1 + "Thread-Was", thread, thread, kind, plan.party(v.get("opener") or "coord"), title,
               seconds(v["opened_at"]))
    plan.count["ticket " + kind] += 1
    for q in ((V1 + "V1-Id", thread, thread), (V1 + "Kind", thread, kind), (V1 + "Words-Taken", thread)):
        plan.holds("thread", *q)
    closed = bool(v.get("ended") or v.get("closed_at"))
    for k, p in enumerate(v.get("posts") or []):
        # an open question's first passage is its asking one
        passage(plan, thread, p, asking=(kind == "question" and not closed and k == 0 and p.get("by") != "owner"))
    for x in v.get("struck") or []:
        plan.listed["passage taken back after an agent had it (v1 draws it struck through)"].append(
            f"{thread}.p{x['post']['n']}")
    ended = v.get("ended")
    if closed:
        # ended by `coord end`: done, dropped or kept; else closed as its program left it: done, or failed
        how = HOW.get(ended.get("how"), ended.get("how") or "done") if ended else ((v.get("status") or {}).get("is") or "done")
        if how not in HOW:
            plan.listed[f"ticket ended {how} (as v1's program left it)"].append(thread)
        plan.event(V1 + "Ended-Was", thread, how, seconds((ended or {}).get("at") or v["closed_at"]))
        plan.holds("thread", DESK + "Ended", thread, how)
    else:
        plan.listed["open " + kind].append(thread)
        # its holder: who last said they work on it, else the one it waits on; its giver: its opener, else his ask
        # when a binding opened it on one (`on asked(e) where e.by == "owner"`), else the program
        waits = [w["agent"] for w in v.get("waits_on") or [] if w.get("agent")]
        holder = (v.get("started") or [None])[-1] or (waits[0] if waits else None)
        giver = v.get("opener") or ("owner" if 'e.by == "owner"' in (v.get("binding") or "") else "coord")
        if kind == "item" and holder:
            holder, giver = plan.party(holder), plan.party(giver)
            if holder != giver:
                plan.event(V1 + "Work-Was", thread, holder, giver)
                plan.holds("thread", "std.rails.Assigned", holder, thread)
            else:
                plan.listed["open item its own opener holds (no Work-Was: holder and giver are one)"].append(thread)


def convert(views, record, importer, owner):
    """The plan for VIEWS (its lines, parsed) and RECORD (its bytes)."""
    head, tickets = views[0], views[1:]
    want = head.get("record") or {}
    if want.get("bytes") is not None and want["bytes"] != len(record):
        raise SystemExit(f"the record is {len(record)} bytes and the views are of {want['bytes']}")
    lines = record.count(b"\n")
    if want.get("lines") is not None and want["lines"] != lines:
        raise SystemExit(f"the record has {lines} lines and the views are of {want['lines']}")
    plan = Plan(importer, owner)
    plan.event(V1 + "Record-Taken", sha256(record), lines, importer)
    for v in tickets:
        ticket(plan, v)
    ids = {v["id"] for v in tickets}
    for v in tickets:
        up = v.get("under") or v.get("parent")
        if up and up in ids:
            plan.event(V1 + "Under-Was", v["id"], up)
        elif up:
            plan.listed["ticket under one v1 does not show"].append(v["id"])
    for v in tickets:
        plan.act(TAKE_WORDS, importer, v["id"])
    plan.event(V1 + "Import-Closed")
    plan.holds("import", V1 + "Closed")
    for p in sorted(plan.parties):
        plan.holds("party", "std.hooks.Party", p)
    kinds = collections.Counter()
    for raw in record.split(b"\n"):
        if raw:
            try:
                kinds[json.loads(raw).get("ev", "(no ev)")] += 1
            except ValueError:
                kinds["(not JSON)"] += 1
    assert sum(kinds.values()) == lines
    report = {"record": {"sha256": sha256(record), "lines": lines, "bytes": len(record), "by_kind": dict(kinds.most_common())},
              "replay_problem_lines": head.get("problem_lines"),
              "tickets": len(tickets), "requests": len(plan.lines), "passages": len(plan.words),
              "queries": len(plan.queries), "parties": len(plan.parties),
              "counts": dict(sorted(plan.count.items())),
              "listed": {k: v for k, v in sorted(plan.listed.items())}}
    return plan, report


def hand(driver, carries=()):
    """The `--by-socket` file: DRIVER performs EVENTS and answers EFFECTS, and on a copy CARRIES too."""
    out = ["# the v1 importer's driver (ops/release/v1import/convert.py): its Events and the Effect it answers"]
    out += [f"{driver} event {e}" for e in EVENTS] + [f"{driver} effect {e}" for e in EFFECTS]
    extra = [e for e in dict.fromkeys(carries) if e not in EFFECTS]
    if extra:
        out += ["# a copy's start has no session drivers: these are carried here so the texts load, and no run of the",
                "# import may perform one"]
        out += [f"{driver} effect {e}" for e in extra]
    return "\n".join(out) + "\n"


def world(plan):
    """The parties the import names and the owner's role, as a scenario's world states them."""
    out = ["module migrate.world", "", "import std.hooks as H", "import std.comms as C", "",
           "// the parties v1's record names, and the importer (ops/release/v1import/convert.py)"]
    out += [f"+H.Party({json.dumps(p)})." for p in sorted(plan.parties)]
    out += [f"+C.Role({json.dumps(plan.owner)}, \"owner\")."]
    return "\n".join(out) + "\n"


def main(argv):
    a = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    for k in ("views", "record", "importer", "driver", "owner", "out"):
        a.add_argument("--" + k, required=True)
    a.add_argument("--carries", default="", help="a copy's: Effects the driver carries too, comma-separated")
    o = a.parse_args(argv)
    with open(o.views, encoding="utf-8") as f:
        views = [json.loads(l) for l in f if l.strip()]
    with open(o.record, "rb") as f:
        record = f.read()
    plan, report = convert(views, record, o.importer, o.owner)
    h = hand(o.driver, [e for e in o.carries.split(",") if e])
    assert NEVER_HANDED not in h and all(NEVER_HANDED != l["name"] for l in plan.lines)
    os.mkdir(o.out)
    with open(os.path.join(o.out, "plan.jsonl"), "w", encoding="utf-8") as f:
        f.writelines(json.dumps(l, ensure_ascii=False) + "\n" for l in plan.lines)
    with open(os.path.join(o.out, "words.jsonl"), "w", encoding="utf-8") as f:
        f.writelines(json.dumps(w, ensure_ascii=False) + "\n" for w in plan.words)
    with open(os.path.join(o.out, "hand.txt"), "w", encoding="utf-8") as f:
        f.write(h)
    with open(os.path.join(o.out, "world.docket"), "w", encoding="utf-8") as f:
        f.write(world(plan))
    with open(os.path.join(o.out, "queries.jsonl"), "w", encoding="utf-8") as f:
        f.writelines(json.dumps(q, ensure_ascii=False) + "\n" for q in plan.queries)
    with open(os.path.join(o.out, "report.json"), "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1, ensure_ascii=False)
    print(f"{report['tickets']} tickets, {report['passages']} passages, {report['requests']} requests; "
          f"record {report['record']['lines']} lines, sha256 {report['record']['sha256'][:12]}")


if __name__ == "__main__":
    main(sys.argv[1:])
