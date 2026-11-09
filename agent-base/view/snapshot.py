#!/usr/bin/env python3
"""Write the live view's snapshot (design, owner decision 9).

Usage: snapshot.py [--plan PLAN.md] [--rulings DIR] [--every SECONDS] [--out DIR]

Reads, for every box under ~/agent-coord:
  - the agents' state written by claude/hook and coord (session.json, beat,
    now/, ruling) and feed.jsonl;
  - the box's status and memory from `sbx ls --json`;
  - commits fetched from the box (refs/sandboxes/BOX/* in the box's repo, which
    sbx keeps after the box is removed), with their
    Agent/Session/Ruling trailers. It fetches at most once a minute.
Also ~/agent-coord/.classifier.json (tools/classifier-watch.py): sessions whose
auto-mode checks run on the client fallback become host alarms. Also the plan's headings (an id such as T3 or L0 before a colon becomes the
item id) and the newest ruling file.

Writes OUT/snap.js as `window.SNAP = {...}`, which view/index.html reloads,
so the page works from file:// with no server. With --every it loops.
"""
import argparse, glob, json, os, re, subprocess, time
from datetime import datetime

COORD = os.path.expanduser("~/agent-coord")
HERE = os.path.dirname(os.path.abspath(__file__))
TOOL_ALARM, BEAT_ALARM = 900, 600


def read(p, default=None):
    try:
        with open(p) as f:
            return f.read().strip()
    except OSError:
        return default


def jread(p):
    try:
        with open(p) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def plan_tree(path):
    """Headings of PLAN as nested items: level, id, title."""
    items = []
    for line in (read(path, "") or "").splitlines():
        m = re.match(r"(#{2,4})\s+(.*)", line)
        if m:
            title = m[2].strip()
            idm = re.match(r"([A-Z]+\d*(?:\.\d+)*[a-z]?):\s*(.*)", title)
            items.append({"level": len(m[1]) - 1, "id": idm[1] if idm else "",
                          "title": idm[2] if idm else title})
    return items


def latest_ruling(d):
    files = sorted(glob.glob(os.path.join(d, "*.md")))
    if not files:
        return None
    f = files[-1]
    first = next((l.lstrip("# ").strip() for l in (read(f, "") or "").splitlines() if l.strip()), "")
    return {"id": os.path.splitext(os.path.basename(f))[0], "title": first, "path": f}


def commits(box, repo, last_fetch):
    if time.time() - last_fetch.get(box, 0) > 60:
        # a stopped or busy box times the fetch out; the view then shows the commits it already has
        try:
            subprocess.run(["git", "-C", repo, "fetch", "-q", f"sandbox-{box}"], capture_output=True, timeout=30)
        except subprocess.TimeoutExpired:
            pass
        last_fetch[box] = time.time()
    fmt = "%H%x1f%h%x1f%ct%x1f%s%x1f%(trailers:key=Agent,valueonly,separator=)%x1f%(trailers:key=Session,valueonly,separator=)%x1f%(trailers:key=Ruling,valueonly,separator=)"
    r = subprocess.run(["git", "-C", repo, "log", "-n", "30", f"--format={fmt}", f"--glob=refs/sandboxes/{box}/*"],
                       capture_output=True, text=True)
    out = []
    for line in r.stdout.splitlines():
        h, short, ct, subj, agent, session, ruling = line.split("\x1f")
        out.append({"kind": "commit", "t": int(ct), "hash": short, "full": h, "text": subj,
                    "from": agent or "?", "session": session, "ruling": ruling, "repo": repo})
    return out


def box_snapshot(box, sbx, last_fetch, now):
    d = os.path.join(COORD, box)
    agents, alarms = [], []
    for a in sorted(glob.glob(os.path.join(d, "agents", "*"))):
        name = os.path.basename(a)
        s = jread(os.path.join(a, "session.json")) or {}
        try:
            beat = int(os.path.getmtime(os.path.join(a, "beat")))
        except OSError:
            beat = None
        doing = [x for x in (jread(p) for p in glob.glob(os.path.join(a, "now", "*"))) if x]
        doing.sort(key=lambda x: x.get("at", 0))
        agents.append({"name": name, "model": s.get("model", ""), "session": s.get("session_id", ""),
                       "started": s.get("started", ""), "beat": beat, "ruling": read(os.path.join(a, "ruling")),
                       "doing": doing})
        for x in doing:
            if now - x.get("at", now) > TOOL_ALARM:
                alarms.append(f"{box}/{name}: {x['tool']} running {int((now - x['at']) // 60)} min")
        if doing and beat and now - beat > BEAT_ALARM:
            alarms.append(f"{box}/{name}: no heartbeat for {int((now - beat) // 60)} min with a tool call open")
    feed = []
    for line in (read(os.path.join(d, "feed.jsonl"), "") or "").splitlines()[-100:]:
        try:
            e = json.loads(line)
        except ValueError:
            continue
        e["t"] = int(datetime.fromisoformat(e["at"]).timestamp())
        feed.append(e)
    repo = read(os.path.join(d, "repo"))
    if repo and os.path.isdir(repo):
        feed += commits(box, repo, last_fetch)
    info = sbx.get(box, {})
    if info.get("status") != "running" and any(a["doing"] for a in agents):
        alarms.append(f"{box}: box is {info.get('status', 'gone')} with tool calls open")
    return {"name": box, "status": info.get("status", "gone"), "memory_mib": info.get("memory_mib"),
            "agents": agents, "feed": feed, "alarms": alarms}


def classifier_alarms(now):
    """Sessions tools/classifier-watch.py found on the client fallback; an alarm too if it stopped writing."""
    try:
        with open(os.path.join(COORD, ".classifier.json")) as f:
            c = json.load(f)
    except (OSError, ValueError):
        return []
    if now - c["at"] > 900:
        return [f"classifier-watch: no report for {int((now - c['at']) // 60)} min"]
    return [f"auto mode on the client fallback: {s['title'] or s['session'][:8] or s['pid']} (pid {s['pid']},"
            f" {s['checks']} {c['model']} checks in {c['window'] // 60} min); restart: claude --resume {s['session']}"
            for s in c["sessions"]]


# Each `sbx ls` opens ~47 Secret Service sessions (sbx reads the Docker login from the keyring); every 3 s that churn
# crashed gnome-keyring 46, relocking it. So at most
# every SBX_EVERY seconds.
SBX_EVERY = 300
_sbx = {"at": 0.0, "boxes": {}}


def sbx_boxes(now):
    if now - _sbx["at"] >= SBX_EVERY:
        r = subprocess.run(["sbx", "ls", "--json"], capture_output=True, text=True)
        try:
            _sbx["boxes"] = {b["name"]: b for b in json.loads(r.stdout or "[]")}
        except ValueError:
            pass
        _sbx["at"] = now
    return _sbx["boxes"]


def snapshot(args, last_fetch):
    now = time.time()
    sbx = sbx_boxes(now)
    boxes = [box_snapshot(os.path.basename(b), sbx, last_fetch, now)
             for b in sorted(glob.glob(os.path.join(COORD, "*"))) if os.path.isdir(b)]
    return {"at": int(now), "boxes": boxes, "alarms": classifier_alarms(now),
            "plan": {"path": args.plan, "items": plan_tree(args.plan)} if args.plan else None,
            "ruling": latest_ruling(args.rulings) if args.rulings else None}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--plan"); p.add_argument("--rulings")
    p.add_argument("--every", type=float); p.add_argument("--out", default=os.path.join(HERE, "out"))
    args = p.parse_args()
    os.makedirs(args.out, exist_ok=True)
    last_fetch = {}
    while True:
        tmp = os.path.join(args.out, "snap.js.tmp")
        with open(tmp, "w") as f:
            f.write("window.SNAP = " + json.dumps(snapshot(args, last_fetch)) + ";\n")
        os.replace(tmp, os.path.join(args.out, "snap.js"))
        if not args.every:
            break
        time.sleep(args.every)


if __name__ == "__main__":
    main()
