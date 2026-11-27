#!/usr/bin/env bash
# A job ends when its caller is gone (docket-edge5 H-1651 p1, p3; paper-research N-1446 p3164): a caller stopped by
# SIGTERM says its own end (`stop_ends`: rc minus the signal, `gone`); one killed outright (SIGKILL) has its end said by
# the next job of its coord (`JobFile.end_gone`: `gone`, no rc, `job_agent` when another agent's), once; a dead
# caller's job of another coord is left to that coord, and a job that ended is not ended again. JobFile is loaded from
# build-runner as a module, with a stand-in coordc that writes down its arguments; no service, no coordd. Everything
# in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/ab/bin" "$dir/wt"
cat >"$dir/ab/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$dir/acts.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
chmod +x "$dir/ab/bin/coordc"
g() { git -C "$dir/wt" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@t.invalid "$@"; }
g init -q -b work && echo a >"$dir/wt/a" && g add a && g commit -q -m 'the work'
export BUILD_RUNNER_HOME="$dir/home" GIT_CONFIG_NOSYSTEM=1 AGENT_BASE="$dir/ab"
cat >"$dir/caller.py" <<'EOF'
# a caller: loads build-runner, starts a job of AGENT in COORD, then dies as HOW says (kill: SIGKILL; term: SIGTERM
# with stop_ends, as a cargo client has it)
import importlib.machinery, importlib.util, os, signal, sys, time
path, wt, agent, coord, how = sys.argv[1:6]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
job = br.JobFile({"args": ["test", "-p", "coordd"]}, wt, {"branch": "work", "commit": "c0ffee00"}, agent, coord)
for p in br.ACT_SENT:
    p.wait(timeout=30)
print(job.path.stem, flush=True)
if how == "term":
    br.stop_ends(job)
os.kill(os.getpid(), signal.SIGKILL if how == "kill" else signal.SIGTERM)
time.sleep(5)
EOF
python3 -I - "$here/build-runner" "$dir" <<'EOF'
import importlib.machinery, importlib.util, json, os, subprocess, sys, time
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
def acts():
    for p in br.ACT_SENT:
        p.wait(timeout=30)
    time.sleep(0.3)  # a dead caller's own children
    try:
        return [json.loads(l) for l in open(f"{d}/acts.jsonl")]
    except FileNotFoundError:
        return []
def fields(a):
    return dict(kv.split("=", 1) for kv in a[3:])
def ends_of(i):
    return [fields(a) for a in acts() if fields(a).get("phase") == "end" and fields(a).get("id") == i]
wt = f"{d}/wt"
def caller(agent, coord, how):
    p = subprocess.run([sys.executable, "-I", f"{d}/caller.py", path, wt, agent, coord, how], capture_output=True, text=True, timeout=60)
    return p.stdout.strip(), p.returncode
# 1. SIGTERM: the caller says its own end
i, rc = caller("w1", "research", "term")
check("a caller stopped by SIGTERM dies by it", rc == -15, rc)
e = ends_of(i)
check("its job has one end line", len(e) == 1, e)
if e:
    check("rc minus the signal, gone with its name, no counts", e[0].get("rc") == "-15" and "SIGTERM" in e[0].get("gone", "") and "passed" not in e[0], e[0])
    t = [a for a in acts() if fields(a).get("id") == i and fields(a).get("phase") == "end"][0][2]
    check("its words say so", t.startswith("job end: test -p coordd on work c0ffee00: its caller was stopped (SIGTERM), "), t)
# 2. SIGKILL: no end until the coord's next job
k, rc = caller("w2", "research", "kill")
check("a caller killed outright dies", rc == -9, rc)
check("its job has no end of its own", ends_of(k) == [], ends_of(k))
o, rc = caller("w3", "other", "kill")
time.sleep(1.1)
job = br.JobFile({"args": ["build", "-p", "coordd"]}, wt, {"branch": "work", "commit": "c0ffee00"}, "w1", "research")
e = ends_of(k)
check("the coord's next job ends it, once", len(e) == 1, e)
if e:
    check("gone, with no rc and no counts", "is gone with no end of its own" in e[0].get("gone", "") and "rc" not in e[0] and "passed" not in e[0], e[0])
    check("the agent whose job it was, another's", e[0].get("job_agent") == "w2", e[0])
    check("its seconds are those to its file's last change", int(e[0].get("secs", "99")) <= 1, e[0].get("secs"))
    check("under its own id, with its start's commit", e[0].get("commit") == "c0ffee00" and e[0].get("verb") == "test", e[0])
check("its file says ended", json.load(open(f"{d}/home/jobs/{k}.json")).get("step") == "ended")
check("a dead caller's job of another coord is left", ends_of(o) == [], ends_of(o))
check("the SIGTERM job, ended, is not ended again", len(ends_of(i)) == 1, ends_of(i))
br.JobFile({"args": ["build", "-p", "coordd"]}, wt, {"branch": "work", "commit": "c0ffee00"}, "w1", "research")
check("a later job does not end it again", len(ends_of(k)) == 1, ends_of(k))
br.JobFile({"args": ["build", "-p", "coordd"]}, wt, {"branch": "work", "commit": "c0ffee00"}, "w3", "other")
e = ends_of(o)
check("its own coord's next job ends that one, as its own agent's (no job_agent)", len(e) == 1 and "job_agent" not in e[0], e)
check("a live job is not ended", ends_of(job.path.stem) == [], ends_of(job.path.stem))
# 3. the day-old sweep keeps a dead job that never ended (another coord's) until GONE_KEPT
z = br.JOBS_DIR / "999999-1.json"
z.write_text(json.dumps({"pid": 999999, "agent": "w9", "coord": "nobody", "step": "running"}))
os.utime(z, (time.time() - 3 * 86400,) * 2)
br.JobFile({"args": ["build"]}, wt, {"branch": "work", "commit": "c0ffee00"}, "w1", "research")
check("a dead unended job three days old is kept", z.exists())
os.utime(z, (time.time() - 8 * 86400,) * 2)
br.JobFile({"args": ["build"]}, wt, {"branch": "work", "commit": "c0ffee00"}, "w1", "research")
check("past GONE_KEPT it goes", not z.exists())
print(f"{fails} failed")
sys.exit(1 if fails else 0)
EOF
