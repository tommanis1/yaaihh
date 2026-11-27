#!/usr/bin/env python3
"""Conformance of eflint-rs against the reference interpreter.

  run.py --expect REF_BIN   run the reference on every case and store its output in expected/
                            (an output over 64 KiB as its SHA-256 alone)
  run.py OUR_BIN [--guided] run this implementation (with the guided derivation if asked), compare, print how
                            many files answer the same and, for each file that differs, where it first differs

A case is one .eflint file run from its own folder with standard input closed, twice:
with --test-mode (errors, failed queries and instance-query answers only) and without
(every transition, violation and change of fact). A benchmark case is its specification file
with the statements of the scenario typed on standard input, the state displayed (`:d`) before
and after each, which is how the benchmark's authors drive the reference.
"""
import os, subprocess, sys, json, hashlib

HERE = os.path.dirname(os.path.abspath(__file__))
TIMEOUT = 120
# an output longer than this is kept as its SHA-256 alone, so the reference's answers stay small
LARGE = 64 * 1024

def sha256(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()

def same(exp, got):
    """A run answers as the reference's did: the same exit status, and the same output or, where only its SHA-256 is
    kept, the same SHA-256."""
    if "stdout_sha256" in exp:
        return exp["exit"] == got["exit"] and exp["stdout_sha256"] == sha256(got["stdout"])
    return exp == got

def cases():
    out = []
    ref = os.path.join(HERE, "reference", "tests")
    for d, _, fs in sorted(os.walk(ref)):
        for f in sorted(fs):
            if f.endswith(".eflint"):
                p = os.path.join(d, f)
                out.append(("reference/" + os.path.relpath(p, ref), p, None))
    bench = os.path.join(HERE, "benchmark", "tests")
    for d, _, fs in sorted(os.walk(bench)):
        for f in sorted(fs):
            parts = f.split(".")
            if not f.endswith(".eflint") or parts[-2] == "spec":
                continue
            spec = os.path.join(d, parts[0] + ".spec.eflint")
            rel = os.path.relpath(d, bench)
            stmts = statements(open(os.path.join(d, f)).read())
            feed = "".join(":d\n" + s + "\n" for s in stmts) + ":d\n:q\n"
            out.append(("benchmark/" + rel + "/" + f, spec, feed))
    return out

def statements(text):
    """The statements of a scenario, one per line: text up to a full stop outside string literals."""
    out, cur, i, in_str = [], "", 0, False
    while i < len(text):
        c = text[i]
        if in_str:
            cur += c
            if c == "\\" and i + 1 < len(text):
                cur += text[i + 1]; i += 1
            elif c == '"':
                in_str = False
        elif text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                i += 1
            continue
        elif c == '"':
            in_str = True; cur += c
        elif c == "." and not text.startswith("..", i) and (i == 0 or text[i - 1] != "."):
            nxt = text[i + 1] if i + 1 < len(text) else "\n"
            cur += c
            if nxt.isspace():
                out.append(" ".join(cur.split())); cur = ""
        else:
            cur += c
        i += 1
    if cur.strip():
        out.append(" ".join(cur.split()))
    return out

def run(binary, path, test_mode, feed=None, flags=()):
    args = [binary, os.path.basename(path)] + (["--test-mode"] if test_mode else []) + list(flags)
    try:
        r = subprocess.run(args, cwd=os.path.dirname(path), input=(feed or "").encode(),
                           capture_output=True, timeout=TIMEOUT)
        return {"exit": r.returncode, "stdout": r.stdout.decode("utf-8", "replace")}
    except subprocess.TimeoutExpired:
        return {"exit": "timeout", "stdout": ""}

def main():
    exp_dir = os.path.join(HERE, "expected")
    if len(sys.argv) > 2 and sys.argv[1] == "--expect":
        os.makedirs(exp_dir, exist_ok=True)
        for name, path, feed in cases():
            res = {"test": run(sys.argv[2], path, True), "full": run(sys.argv[2], path, False, feed)}
            for mode in res.values():
                if len(mode["stdout"].encode("utf-8")) > LARGE:
                    mode["stdout_sha256"] = sha256(mode.pop("stdout"))
            out = os.path.join(exp_dir, name + ".json")
            os.makedirs(os.path.dirname(out), exist_ok=True)
            json.dump(res, open(out, "w"), indent=1)
        print("expected output written for", len(cases()), "cases")
        return
    if len(sys.argv) < 2 or any(a != "--guided" for a in sys.argv[2:]):
        sys.exit(__doc__)
    ours = os.path.abspath(sys.argv[1])
    flags = sys.argv[2:]
    rows, timed_out = [], []
    for name, path, feed in cases():
        exp = json.load(open(os.path.join(exp_dir, name + ".json")))
        got = {"test": run(ours, path, True, None, flags), "full": run(ours, path, False, feed, flags)}
        ref_pass = exp["test"]["exit"] == 0 and exp["test"].get("stdout") == ""
        if "timeout" in (exp["test"]["exit"], exp["full"]["exit"]):
            timed_out.append((name, got["full"]["exit"]))
            continue
        rows.append((name, ref_pass, same(exp["test"], got["test"]), same(exp["full"], got["full"]), exp, got))
    sets = [
        ("Reference suite, files the reference passes", [r for r in rows if r[0].startswith("reference/") and r[1]]),
        ("Reference suite, files the reference itself fails", [r for r in rows if r[0].startswith("reference/") and not r[1]]),
        ("Benchmark", [r for r in rows if r[0].startswith("benchmark/")]),
    ]
    # "same": the standard output and the exit status are byte for byte the reference's on the same file
    for title, sel in sets:
        t = sum(1 for r in sel if r[2]); f = sum(1 for r in sel if r[3])
        print("%s: %d files; test mode same %d, full output same %d" % (title, len(sel), t, f))
    for name, ours_exit in timed_out:
        note = "ours finished" if ours_exit == 0 else "ours did not finish either"
        print("NO REFERENCE ANSWER", name, "(" + note + ")")
    for name, _, t, f, exp, got in rows:
        if not (t and f):
            print("DIFF", name, "test" if not t else "", "full" if not f else "")
            for mode in ("test", "full"):
                if not same(exp[mode], got[mode]):
                    print("   [%s] exit ref=%s ours=%s" % (mode, exp[mode]["exit"], got[mode]["exit"]))
                    if "stdout" not in exp[mode]:
                        print("   the reference's output is kept as its SHA-256 alone")
                        continue
                    e, g = exp[mode]["stdout"].split("\n"), got[mode]["stdout"].split("\n")
                    for i in range(max(len(e), len(g))):
                        a = e[i] if i < len(e) else "<end>"; b = g[i] if i < len(g) else "<end>"
                        if a != b:
                            print("   line %d\n     ref: %s\n     our: %s" % (i + 1, a[:200], b[:200])); break
    sys.exit(0 if all(r[2] and r[3] for r in rows) else 1)

main()
