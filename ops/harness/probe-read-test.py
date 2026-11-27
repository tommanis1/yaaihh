#!/usr/bin/env python3
"""probe-read-test.py: probe-read.py's checks on made-up captures; exits 0 when each says what it should."""
import json
import os
import subprocess
import sys
import tempfile

here = os.path.dirname(os.path.abspath(__file__))
tmp = tempfile.mkdtemp()
fails = []


def put(name, text):
    path = os.path.join(tmp, name)
    with open(path, "w") as f:
        f.write(text)
    return path


def capture(name, body):
    return put(name, json.dumps({"path": "/v1/messages", "body": json.dumps(body)}) + "\n")


def run(*args):
    p = subprocess.run([sys.executable, "-I", os.path.join(here, "probe-read.py"), *args], capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def expect(what, want, got):
    code, out = got
    if code != want:
        fails.append(f"{what}: exit {code}, wanted {want}: {out.strip()}")
    if "Traceback" in out:
        fails.append(f"{what}: a traceback, not a finding: {out.strip()}")


prompt = "\x1b[1mQuick safety check: Is this a project you created or one you trust?\x1b[0m\n> 1. Yes, I trust this folder\n"
fresh, plain = put("fresh.out", prompt), put("accepted.out", "> hi\n")
crashed = put("crashed.out", "error: could not trust the store at /home/x/.trust\n")
expect("trust: both ask fresh, neither accepted", 0, run("trust", fresh, plain, fresh, plain))
expect("trust: no prompt at all on either side", 1, run("trust", plain, plain, plain, plain))
expect("trust: the word trust in an error is no prompt", 1, run("trust", crashed, plain, crashed, plain))

low = {"model": "m", "messages": [], "tools": [{}], "output_config": {"effort": "low"}, "metadata": {"user_id": "a"}}
high = {"model": "m", "messages": [], "tools": [{}], "output_config": {"effort": "high"}, "metadata": {"user_id": "b"}}
pl, ph = capture("p-low.jsonl", low), capture("p-high.jsonl", high)
expect("flags: low and high as pinned", 0, run("flags", "m", pl, ph, pl, ph))
expect("flags: the candidate swaps low and high", 1, run("flags", "m", pl, ph, ph, pl))
bl = capture("b-low.jsonl", dict(low, thinking={"budget_tokens": 4000}))
bh = capture("b-high.jsonl", dict(high, thinking={"budget_tokens": 1000}))
expect("flags: a budget that falls as effort rises", 1, run("flags", "m", bl, bh, bl, bh))
empty = put("empty.jsonl", "")
expect("flags: no request captured", 1, run("flags", "m", empty, ph, pl, ph))
expect("system: no request captured", 1, run("system", empty, empty, "mark"))

print("\n".join(fails) or "probe-read-test: every check says what it should")
sys.exit(1 if fails else 0)
