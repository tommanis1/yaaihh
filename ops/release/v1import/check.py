#!/usr/bin/env python3
"""The import's checks, on a copy (N-1832).

    python3 -I check.py record --record FILE --plan DIR
        coordd-v2's record after the import, against the plan: no `refused` line; `performed` lines, by name, one
        for each request of the plan and one Words-Were-Taken for each Take-Words (its body performs it); every run
        that began has ended, none raised or was cancelled; one `answered` line for each passage's words. Prints the counts and the record's sha256; exit 1 on any miss.

    python3 -I check.py ask --socket PATH --queries FILE [--cgroup-out FILE] [--wait SECONDS]
        A restarted engine asked each query of FILE (one {"query", "what"} a line, as convert.py writes them); each
        must come back true. With --cgroup-out it first writes its own cgroup there, for the parties table, and waits
        for the socket. Prints the counts and every query that did not hold; exit 1 if one did not.
"""

import argparse
import collections
import hashlib
import json
import os
import socket
import sys
import time


def record(o):
    with open(o.record, "rb") as f:
        raw = f.read()
    with open(os.path.join(o.plan, "plan.jsonl"), encoding="utf-8") as f:
        plan = [json.loads(l) for l in f if l.strip()]
    with open(os.path.join(o.plan, "words.jsonl"), encoding="utf-8") as f:
        passages = sum(1 for l in f if l.strip())
    kinds, began, ended, bad, done = collections.Counter(), set(), set(), [], collections.Counter()
    for k, l in enumerate(raw.split(b"\n")):
        if not l:
            continue
        (kind, body), = json.loads(l).items()
        kinds[kind] += 1
        if kind == "performed":
            done[body["act"]["inst"]] += 1
        elif kind == "began":
            began.add(body["run"])
        elif kind == "ended":
            ended.add(body["run"])
        elif kind in ("refused", "raised", "cancelled"):
            bad.append(f"line {k}: {kind} {json.dumps(body, ensure_ascii=False)[:300]}")
    want = collections.Counter(r["name"] for r in plan)
    takes = sum(n for k, n in want.items() if k.endswith(".Take-Words"))
    want["yaaihh.v1.Words-Were-Taken"] += takes
    misses = list(bad)
    for name in sorted(set(want) | set(done)):
        if want[name] != done[name]:
            misses.append(f"{done[name]} performed lines of {name} for {want[name]}")
    if began - ended:
        misses.append(f"{len(began - ended)} runs began and did not end: {sorted(began - ended)[:10]}")
    # the import's only runs are its Take-Words: a run an imported Event began would be a reaction, which on the
    # machine's start would be a real model's or session's
    if len(began) != takes:
        misses.append(f"{len(began)} runs for {takes} Take-Words")
    if kinds["answered"] != passages:
        misses.append(f"{kinds['answered']} answered lines for {passages} passages")
    print(f"record: {sum(kinds.values())} lines, sha256 {hashlib.sha256(raw).hexdigest()}; " +
          ", ".join(f"{k} {n}" for k, n in kinds.most_common()))
    for m in misses:
        print("MISS " + m)
    return 1 if misses else 0


def ask(o):
    if o.cgroup_out:
        with open("/proc/self/cgroup") as f:
            cg = next(l[3:].strip() for l in f if l.startswith("0::"))
        with open(o.cgroup_out + ".part", "w") as f:
            f.write(cg + "\n")
        os.replace(o.cgroup_out + ".part", o.cgroup_out)
    with open(o.queries, encoding="utf-8") as f:
        qs = [json.loads(l) for l in f if l.strip()]
    end = time.monotonic() + o.wait
    while not os.path.exists(o.socket):
        if time.monotonic() > end:
            sys.exit(f"check: no socket at {o.socket} after {o.wait} s")
        time.sleep(0.2)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(o.socket)
    f = s.makefile("rwb")
    held, failed, by = 0, [], collections.Counter()
    t0 = time.monotonic()
    for q in qs:
        f.write(json.dumps({"do": "query", "query": q["query"]}, ensure_ascii=False).encode() + b"\n")
        f.flush()
        r = json.loads(f.readline())
        if r.get("is") == "done" and r.get("value") is True:
            held += 1
            by[q["what"]] += 1
        else:
            failed.append(f"{q['what']}: {q['query']}: {r.get('value', r.get('why'))}")
    print(f"queries: {held} of {len(qs)} held in {time.monotonic() - t0:.1f} s; " + ", ".join(f"{k} {n}" for k, n in sorted(by.items())))
    for m in failed[:50]:
        print("MISS " + m)
    if len(failed) > 50:
        print(f"MISS and {len(failed) - 50} more")
    return 1 if failed else 0


def main(argv):
    a = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = a.add_subparsers(dest="how", required=True)
    r = sub.add_parser("record")
    r.add_argument("--record", required=True)
    r.add_argument("--plan", required=True)
    q = sub.add_parser("ask")
    q.add_argument("--socket", required=True)
    q.add_argument("--queries", required=True)
    q.add_argument("--cgroup-out")
    q.add_argument("--wait", type=float, default=600)
    o = a.parse_args(argv)
    sys.exit(record(o) if o.how == "record" else ask(o))


if __name__ == "__main__":
    main(sys.argv[1:])
