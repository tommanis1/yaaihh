#!/usr/bin/env python3
"""wire-diff.py A.jsonl B.jsonl [--expect-gone TEXT]... [--expect-tool-gone NAME]... [--nothing-else]: compares the main messages
request of two captures (harness-wire-check.sh), the one with the most tools, and says what changed on the wire:
each request's size in bytes and its sha256, the system blocks' sizes, which paragraphs of the system text are in one
and not the other, and which tools. Exits 1 when an --expect-gone TEXT is still in B's system text or was never in
A's, or an --expect-tool-gone NAME is still among B's tools or was never among A's; exits 2 when the two requests are
the same bytes. With --nothing-else, any other paragraph or tool that differs is a failure too.
"""
import hashlib
import json
import sys


def main_request(path):
    best = None
    for line in open(path):
        rec = json.loads(line)
        if not rec["path"].split("?")[0].endswith("/v1/messages"):
            continue
        body = rec["body"]
        req = json.loads(body)
        if best is None or len(req.get("tools") or []) > len(best[1].get("tools") or []):
            best = (body, req)
    if best is None:
        sys.exit(f"{path}: no messages request")
    return best


def system_text(req):
    s = req.get("system")
    return "\n\n".join(b.get("text", "") for b in s) if isinstance(s, list) else (s or "")


def paras(text):
    return [p.strip() for p in text.split("\n\n") if p.strip()]


def short(p):
    p = " ".join(p.split())
    return p if len(p) <= 100 else p[:97] + "..."


args = sys.argv[1:]
a_path, b_path, rest = args[0], args[1], args[2:]
gone = [rest[i + 1] for i in range(len(rest) - 1) if rest[i] == "--expect-gone"]
tools_gone = [rest[i + 1] for i in range(len(rest) - 1) if rest[i] == "--expect-tool-gone"]
(a_body, a), (b_body, b) = main_request(a_path), main_request(b_path)
for name, body, req in (("A", a_body, a), ("B", b_body, b)):
    s = req.get("system")
    sizes = [len(x.get("text", "")) for x in s] if isinstance(s, list) else [len(s or "")]
    print(f"{name}: {len(body.encode())} bytes, sha256 {hashlib.sha256(body.encode()).hexdigest()[:16]}, "
          f"system blocks {sizes}, {len(req.get('tools') or [])} tools")
pa, pb = paras(system_text(a)), paras(system_text(b))
for p in pa:
    if p not in pb:
        print(f"  system, only in A: {short(p)}")
for p in pb:
    if p not in pa:
        print(f"  system, only in B: {short(p)}")
ta, tb = [t["name"] for t in a.get("tools") or []], [t["name"] for t in b.get("tools") or []]
for t in ta:
    if t not in tb:
        print(f"  tool, only in A: {t}")
for t in tb:
    if t not in ta:
        print(f"  tool, only in B: {t}")
bad = 0
for g in gone:
    if g not in system_text(a):
        print(f"FAIL: never in A's system text: {short(g)}"); bad = 1
    elif g in system_text(b):
        print(f"FAIL: still in B's system text: {short(g)}"); bad = 1
    else:
        print(f"ok: in A's system text, gone from B's: {short(g)}")
for t in tools_gone:
    if t not in ta:
        print(f"FAIL: never among A's tools: {t}"); bad = 1
    elif t in tb:
        print(f"FAIL: still among B's tools: {t}"); bad = 1
    else:
        print(f"ok: among A's tools, gone from B's: {t}")
if "--nothing-else" in rest:
    others = [p for p in pa if p not in pb and p not in gone] + [p for p in pb if p not in pa]
    others += [t for t in ta if t not in tb and t not in tools_gone] + [t for t in tb if t not in ta]
    if others:
        print(f"FAIL: {len(others)} other changes, listed above"); bad = 1
    else:
        print("ok: nothing else in the system text or the tools changed")
if a_body == b_body:
    print("the two requests are the same bytes"); sys.exit(2)
sys.exit(bad)
