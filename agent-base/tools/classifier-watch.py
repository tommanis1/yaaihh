#!/usr/bin/env python3
"""Name the Claude Code sessions whose auto-mode checks run on the client fallback.

Usage: classifier-watch.py [--every SECONDS] [--window SECONDS] [--min N] [--no-notify]

Behind the proxy a session gets the server-side auto-mode review ("Auto mode
server: Enabled" in /status) until it skips one; from then on the process
checks tool calls itself with CLAUDE_CODE_AUTO_MODE_MODEL (kiro-haiku-5.5,
about 95-110k uncached tokens per check) for as long as it runs. Restarting
the session (claude --resume ID) brings the server review back (29 Oct).

The proxy records no session id, and the checks of a lead's in-process
teammates do not line up with any transcript's tool calls (measured 29 Oct
12:1x), so a check is attributed by socket: every 0.5 s this samples
`ss -tnpi` for the bytes each process has sent to the proxy, and a check
(usage.db start = timestamp_ms - latency_ms) goes to the process whose send
jumped nearest its start. A process with at least --min (10) checks in the last
--window seconds is on the fallback (a latched lead made 30 in 8 min; a session
on the server review makes an occasional one); ~/.claude/sessions/PID.json names its
session.

Writes ~/agent-coord/.classifier.json (view/snapshot.py turns it into alarms)
every --every seconds and, unless --no-notify, sends a desktop notification
when a session first appears there. Without --every it samples one window
(--window, default 60 s here) and prints the result.
"""
import argparse, glob, json, os, re, sqlite3, subprocess, time

HOME = os.path.expanduser("~")
DB = f"{HOME}/.local/opt/EasyCLIProxyAPI/usage-records/usage.db"
OUT = f"{HOME}/agent-coord/.classifier.json"
PROXY = "127.0.0.1:8317"
SAMPLE, JUMP = 0.5, 100_000   # a request body above JUMP bytes is a model call (a check is ~350 kB)
BEFORE, AFTER = 1, 3          # the jump is seen up to AFTER s after the check's start


def check_model():
    try:
        with open(f"{HOME}/.claude/settings.json") as f:
            env = json.load(f).get("env", {})
    except (OSError, ValueError):
        env = {}
    return os.environ.get("CLAUDE_CODE_AUTO_MODE_MODEL") or env.get("CLAUDE_CODE_AUTO_MODE_MODEL") or "kiro-haiku-5.5"


def sent():
    """{(pid, local port): bytes sent} for every claude process's connection to the proxy (not the proxy's own)."""
    out = subprocess.run(["ss", "-tnpiH", "dst", PROXY], capture_output=True, text=True).stdout.splitlines()
    socks = {}
    for line, info in zip(out, out[1:] + [""]):
        m = re.search(r":(\d+)\s+" + re.escape(PROXY) + r'.*\(\("claude",pid=(\d+)', line)
        b = re.search(r"bytes_sent:(\d+)", info)
        if m and b:
            socks[(int(m.group(2)), m.group(1))] = int(b.group(1))
    return socks


def session(pid):
    """The session a claude process runs, with its title from the transcript."""
    try:
        with open(f"{HOME}/.claude/sessions/{pid}.json") as f:
            s = json.load(f)
    except (OSError, ValueError):
        try:
            cmd = open(f"/proc/{pid}/cmdline").read().replace("\0", " ").strip()
        except OSError:
            cmd = ""
        return {"session": "", "cwd": "", "title": cmd[:80]}
    sid, name = s.get("sessionId", ""), s.get("name") or ""
    for f in glob.glob(f"{HOME}/.claude/projects/*/{sid}.jsonl")[:1]:
        with open(f, errors="replace") as fh:
            for line in fh:
                if '"customTitle"' in line or '"aiTitle"' in line:
                    try:
                        d = json.loads(line)
                    except ValueError:
                        continue
                    name = d.get("customTitle") or name or d.get("aiTitle") or ""
    return {"session": sid, "cwd": s.get("cwd", ""), "title": name}


def notify(text):
    subprocess.run(["notify-send", "-a", "classifier-watch", "Auto mode on the client fallback", text],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--every", type=float); p.add_argument("--window", type=float)
    p.add_argument("--min", type=int, default=10); p.add_argument("--no-notify", action="store_true")
    args = p.parse_args()
    window = args.window or (600 if args.every else 60)
    model = check_model()
    db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    try:
        with open(OUT) as f:
            flagged = {s["pid"] for s in json.load(f).get("sessions", [])}
    except (OSError, ValueError, KeyError):
        flagged = set()
    started, last_out = time.time(), 0
    prev, jumps = sent(), []   # jumps: (time, pid)
    while True:
        time.sleep(SAMPLE)
        now, cur = time.time(), sent()
        jumps += [(now, pid) for (pid, port), b in cur.items() if b - prev.get((pid, port), 0) > JUMP]
        prev = cur
        if args.every and now - last_out < args.every or not args.every and now - started < window:
            continue
        time.sleep(AFTER)      # usage.db rows are written when a call ends; checks run ~5 s
        since = max(started, time.time() - window)
        jumps = [j for j in jumps if j[0] >= since - BEFORE]
        per, unmatched, n = {}, 0, 0
        for ms, lat in db.execute("select timestamp_ms, coalesce(latency_ms, 0) from usage_events "
                                  "where alias = ? and timestamp_ms > ?", (model, int(since * 1000))):
            s = (ms - lat) / 1000
            if s < since:
                continue
            n += 1
            near = min(((abs(t - s), pid) for t, pid in jumps if -BEFORE <= t - s <= AFTER), default=None)
            if near is None:
                unmatched += 1
            else:
                per[near[1]] = per.get(near[1], 0) + 1
        sessions = [{"pid": pid, "checks": k, **session(pid)}
                    for pid, k in sorted(per.items(), key=lambda x: -x[1]) if k >= args.min]
        for s in sessions:
            if s["pid"] not in flagged and not args.no_notify:
                notify(f"{s['title'] or s['session'][:8] or s['pid']}: {s['checks']} {model} checks in"
                       f" {int(window // 60) or 1} min. Restart it: claude --resume {s['session']}")
        flagged = {s["pid"] for s in sessions}
        os.makedirs(os.path.dirname(OUT), exist_ok=True)
        with open(OUT + ".tmp", "w") as fh:
            json.dump({"at": int(time.time()), "model": model, "window": int(min(window, time.time() - started)),
                       "checks": n, "unmatched": unmatched, "sessions": sessions}, fh)
        os.replace(OUT + ".tmp", OUT)
        if not args.every:
            for s in sessions:
                print(f"{s['checks']:4d}  pid {s['pid']}  {s['session'][:8]}  {s['title'][:50]}")
            print(f"{n} {model} checks in {int(window)} s, {unmatched} not matched to a process")
            break
        last_out = time.time()


if __name__ == "__main__":
    main()
