#!/usr/bin/env python3
"""probe-read.py CHECK ARGS...: one check of bump-check.sh on the pinned (P) and candidate (C) captures; prints what
it found, exits 0 when the candidate behaves as the pinned version does.

    moments P-FILE C-FILE                    the hook names each run's hooks wrote: the same, and not none
    system P.jsonl C.jsonl MARK              MARK in C's system text; system blocks and their cache marks shaped as P's
    flags MODEL P-LOW P-HIGH C-LOW C-HIGH    MODEL sent by all four; the fields effort changes are P's, and some,
                                             each rising with effort (a level word or a number)
    trust P-FRESH P-ACCEPTED C-FRESH C-ACCEPTED   the trust dialog on both fresh screens, on neither accepted one
A capture with no request is a failure said in one line, never a traceback.
"""
import json
import re
import sys


def requests(path):
    for line in open(path):
        rec = json.loads(line)
        if rec["path"].split("?")[0].endswith("/v1/messages") and rec["body"]:
            yield json.loads(rec["body"])


class NoRequest(Exception):
    pass


def main_request(path):
    # the turn's own request: the one offering the most tools, as wire-diff.py takes it
    found = list(requests(path))
    if not found:
        raise NoRequest(f"no request to /v1/messages captured in {path}")
    return max(found, key=lambda r: len(r.get("tools") or []))


def shape(req):
    s = req.get("system")
    return [(b.get("type"), b.get("cache_control")) for b in s] if isinstance(s, list) else [("text", None)]


def moments(p, c):
    got = [sorted(set(open(f).read().split())) for f in (p, c)]
    print(f"pinned {got[0]}, candidate {got[1]}")
    return got[0] == got[1] and got[0] != []


def system(p, c, mark):
    a, b = main_request(p), main_request(c)
    text = "\n\n".join(x.get("text", "") for x in b["system"]) if isinstance(b.get("system"), list) else b.get("system", "")
    print(f"pinned blocks {shape(a)}, candidate blocks {shape(b)}, mark {'in' if mark in text else 'missing'}")
    return mark in text and shape(a) == shape(b)


LEVELS = ["low", "medium", "high", "xhigh", "max"]


def leaves(x, at=""):
    if isinstance(x, dict):
        for k, v in x.items():
            yield from leaves(v, f"{at}.{k}" if at else k)
    else:
        yield at, x


def effort_fields(low, high):
    # each leaf the two requests differ in that is a level word or a number, and whether it rises from low to high;
    # other differing leaves (a session's id) are not effort's
    a, b = (dict(leaves({k: v for k, v in main_request(f).items() if k not in ("messages", "system")})) for f in (low, high))
    out = {}
    for k in sorted(set(a) | set(b)):
        x, y = a.get(k), b.get(k)
        if x == y:
            continue
        if x in LEVELS and y in LEVELS:
            out[k] = LEVELS.index(x) < LEVELS.index(y)
        elif isinstance(x, (int, float)) and isinstance(y, (int, float)) and not isinstance(x, bool) and not isinstance(y, bool):
            out[k] = x < y
    return out


def flags(model, p_low, p_high, c_low, c_high):
    sent = {f: main_request(f).get("model") for f in (p_low, p_high, c_low, c_high)}
    pf, cf = effort_fields(p_low, p_high), effort_fields(c_low, c_high)
    print(f"models {sorted(set(sent.values()))}, effort fields (rising) pinned {pf}, candidate {cf}")
    return set(sent.values()) == {model} and pf != {} and pf == cf and all(pf.values())


# the dialog's own words, as the pinned 2.1.332 binary holds them, read with every space and mark dropped, since a
# terminal screen moves the cursor between words
DIALOG = ["isthisaprojectyoucreatedoroneyoutrust", "yesitrustthisfolder"]


def asks_trust(path):
    screen = re.sub(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[()][0-9A-B]|\x1b[=>]", "", open(path, errors="replace").read())
    letters = re.sub(r"[^a-z]", "", screen.lower())
    return all(w in letters for w in DIALOG)


def trust(p_fresh, p_accepted, c_fresh, c_accepted):
    got = [asks_trust(f) for f in (p_fresh, p_accepted, c_fresh, c_accepted)]
    print(f"asked: pinned fresh {got[0]}, accepted {got[1]}; candidate fresh {got[2]}, accepted {got[3]}")
    return got == [True, False, True, False]


CHECKS = {"moments": moments, "system": system, "flags": flags, "trust": trust}
try:
    sys.exit(0 if CHECKS[sys.argv[1]](*sys.argv[2:]) else 1)
except NoRequest as e:
    print(e)
    sys.exit(1)
