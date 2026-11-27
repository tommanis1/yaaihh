#!/usr/bin/env bash
# A runner job in its coord's record (paper-research N-1446 p1364, p1386; the owner's N-1104 p764): the caller's side
# (build-runner's JobFile) says the job to coordd as the act `job`, through the agent's own coordc
# ($AGENT_BASE/bin/coordc act job TEXT FIELD=VALUE...), once as it starts (worktree, branch, commit, the commit's
# Item: trailers, the tree it sent and how many files that tree holds the commit does not, verb, packages) and once as
# it ends (the same, with its rc, tests passed and failed, seconds, the files its output went to, each a field). Not for a caller in no agent, nor from the runner's
# own tests (BUILD_RUNNER_UNIT build-runner-test, ...); a coordc that is not
# there, or does not answer in 5 s, fails nothing and the job's file is written as before. A red (--red-of REV,
# proto-mind N-1446 p1502, head 3) says REV's commit and tree and the files it takes out, and one that takes nothing
# out, or names no commit, is refused (rc 3) before the service (whose connect is a stand-in here). JobFile is loaded from
# build-runner as a module, with a stand-in coordc that writes down its arguments; no service is started and no coordd
# is asked. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/ab/bin" "$dir/slow/bin" "$dir/wt"
cat >"$dir/ab/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$dir/acts.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
# a slow coordc: a start line lands 2 s after it was sent (p1542's leg, for a job's two lines: p1565)
cat >"$dir/slow/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys, time
if "phase=start" in sys.argv:
    time.sleep(2)
open("$dir/acts.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
# a coordc that coordd refuses, and a journal that writes down each line it gets, with its tag
mkdir -p "$dir/fail/bin" "$dir/logbin"
printf '#!/bin/sh\necho "coordc act: coordd refused: no such agent" >&2\nexit 1\n' >"$dir/fail/bin/coordc"
cat >"$dir/logbin/logger" <<EOF
#!/bin/sh
[ "\$1" = -t ] && t=\$2
while IFS= read -r l; do printf '%s %s\n' "\$t" "\$l" >>"$dir/journal.txt"; done
EOF
# a coordc whose coordd is down while $dir/down-SOCK stands, SOCK the name of the socket it goes to (COORDD_SOCK): it
# says so as coordc does when it cannot connect (rc 3, client.rs `connect`), and else writes down the socket's name and
# its arguments (paper-research N-1446 p1872)
mkdir -p "$dir/down/bin"
cat >"$dir/down/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, os, sys
sock = os.environ.get("COORDD_SOCK", "/run/coordd.sock")
if os.path.exists("$dir/down-" + os.path.basename(sock)):
    sys.stderr.write(f"coord: coordd is not running ({sock}: Connection refused)\n")
    sys.exit(3)
open("$dir/down-acts.jsonl", "a").write(json.dumps({"sock": os.path.basename(sock), "argv": sys.argv[1:]}) + "\n")
EOF
chmod +x "$dir/ab/bin/coordc" "$dir/slow/bin/coordc" "$dir/fail/bin/coordc" "$dir/down/bin/coordc" "$dir/logbin/logger"
g() { git -C "$dir/wt" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@t.invalid "$@"; }
g init -q -b work && echo a >"$dir/wt/a" && echo b >"$dir/wt/b" && g add a b && g commit -q -m $'the work\n\nItem: I-7\nItem: N-1104.p678\nCo-Authored-By: x <x@x.invalid>'
echo changed >>"$dir/wt/a"
export BUILD_RUNNER_HOME="$dir/home" GIT_CONFIG_NOSYSTEM=1
AGENT_BASE="$dir/ab" python3 -I - "$here/build-runner" "$dir" >"$dir/out.log" 2>"$dir/err.log" <<'EOF'
import importlib.machinery, importlib.util, json, os, re, sys, time
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
def acts_now():
    try:
        return [json.loads(l) for l in open(f"{d}/acts.jsonl")]
    except FileNotFoundError:
        return []
def acts():  # once the lines sent so far have landed: each job's go out from children of its own (`JobFile.send`)
    for p in getattr(br, "ACT_SENT", []):
        p.wait(timeout=30)
    return acts_now()
def fields(a):
    out = {}
    for kv in a[3:]:
        k, v = kv.split("=", 1)
        out[k] = json.loads(v) if v[:1] in "[{" else v
    return out
wt = f"{d}/wt"
commit = os.popen(f"git -C {wt} rev-parse --short=8 HEAD").read().strip()
req = {"args": ["test", "--release", "-p", "coordd", "-p", "docket-machine", "--", "--exact", "x"], "train": True}
job = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w1", "research")
a = acts()
check("one act as the job starts", len(a) == 1, a)
if a:
    s, f = a[0], fields(a[0])
    check("it is `act job` with its words", s[:3] == ["act", "job", f"job start: test -p coordd -p docket-machine on work {commit} + 1 changed, for I-7, N-1104.p678"], s[:3])
    check("its phase, verb, packages", f.get("phase") == "start" and f.get("verb") == "test" and f.get("packages") == ["coordd", "docket-machine"], f)
    check("its worktree, branch and commit", f.get("wt") == wt and f.get("branch") == "work" and f.get("commit") == commit, f)
    check("the commit's Item: trailers", f.get("items") == ["I-7", "N-1104.p678"], f.get("items"))
    check("the tracked files the commit alone does not hold", f.get("changed") == "1", f.get("changed"))
    check("the tree it sent: sent_tree's, made again", f.get("tree") == br.sent_tree(wt), f.get("tree"))
    check("the tree it sent holds the change", f.get("tree") not in (None, os.popen(f"git -C {wt} rev-parse 'HEAD^{{tree}}'").read().strip()), f.get("tree"))
    check("a train's job says so", f.get("train") == "1", f.get("train"))
    check("its id is its job file's name, with the file's since", f.get("id") == job.path.stem and f.get("since") == job.d.get("since"), (f.get("id"), job.path.stem))
    check("no command line goes", "--exact" not in json.dumps(s), s)
time.sleep(1)
job.end({"passed": 3, "failed": 1}, 101)
a = acts()
check("a second act as it ends", len(a) == 2, a)
if len(a) == 2:
    e, f = a[1], fields(a[1])
    check("its words say the rc and the tests", e[2].startswith(f"job end: test -p coordd -p docket-machine on work {commit} + 1 changed, for I-7, N-1104.p678: rc 101, 3 passed, 1 failed, "), e[2])
    check("its rc, passed, failed and seconds, each a field", f.get("rc") == "101" and f.get("passed") == "3" and f.get("failed") == "1" and int(f.get("secs", "0")) >= 1 and "result" not in f, f)
    check("the file its output went to", f.get("logs") == [f"{d}/out.log", f"{d}/err.log"], f.get("logs"))
    check("the end says the commit, items and tree again, under the same id", f.get("phase") == "end" and f.get("id") == job.path.stem and f.get("commit") == commit and f.get("items") == ["I-7", "N-1104.p678"]
          and f.get("tree") == fields(a[0]).get("tree"), f)
jf = json.load(open(job.path))
check("the job's file as before: ended, rc, counts", jf.get("step") == "ended" and jf.get("rc") == 101 and jf.get("passed") == 3 and jf.get("failed") == 1, jf)
check("the job's file carries the same work", jf.get("tree") == fields(a[0]).get("tree") and jf.get("items") == ["I-7", "N-1104.p678"] and jf.get("changed") == 1, jf)
job.step("ended", rc=1)
check("the end is said once", len(acts()) == 2, len(acts()))
# ---- out of the queue (the scheduling of the owner's N-1104 p1156; paper-research N-1446 p3044): a job's first step
# that runs (compiling, testing, replaying) says `runs`, once, between its start and its end; waiting says nothing
k = len(acts()); jq = br.JobFile({"args": ["test", "-p", "coordd"]}, wt, {"branch": "work", "commit": commit}, "w1", "research")
jq.step("queued", ahead=2); jq.step("queued", ahead=1)
check("waiting in the queue says nothing more than the start", [fields(x).get("phase") for x in acts()[k:]] == ["start"], acts()[k:])
jq.step("compiling", done=1, of=9); jq.step("testing"); jq.step("compiling", done=2, of=9)
a = acts()[k:]
check("its first running step says `runs`, once", [fields(x).get("phase") for x in a] == ["start", "runs"], a)
if len(a) == 2:
    check("`runs` has its words and the same id", a[1][2] == f"job runs: test -p coordd on work {commit} + 1 changed, for I-7, N-1104.p678" and fields(a[1]).get("id") == jq.path.stem, a[1])
jq.end({"passed": 1, "failed": 0}, 0)
check("then its end", [fields(x).get("phase") for x in acts()[k:]] == ["start", "runs", "end"], acts()[k:])
k = len(acts()); jr0 = br.JobFile({"args": ["test", "-p", "coordd"]}, wt, {"branch": "work", "commit": commit}, "w1", "research")
jr0.end({"refused": "no such package"}, 3)
jr0.step("compiling")
check("a job that ended without running says no `runs`, not after its end either", [fields(x).get("phase") for x in acts()[k:]] == ["start", "end"], acts()[k:])
# ---- the job line's state (proto-mind N-1446 p1502, head 2): which tests it named, or whole; refused with its words;
# no tests run; the failing tests by name, from the service's lines (Steps) through its last line to the act
facts = {"branch": "work", "commit": commit}
f0 = fields(acts()[0])
check("the start says the tests it named, --exact, and not whole", f0.get("tests") == ["x"] and f0.get("exact") == "1" and "whole" not in f0 and "targets" not in f0, f0)
def job_lines(r, m, rc):
    k = len(acts()); j = br.JobFile(r, wt, facts, "w1", "research"); j.end(m, rc); a = acts()[k:]
    return j, a, [fields(x) for x in a]
jw, aw, fw = job_lines({"args": ["test", "--release", "--no-fail-fast", "-p", "coordd", "-p", "yaaihh"], "train": True}, {"passed": 9, "failed": 0}, 0)
check("a test job naming no test says whole at both ends, with no names", len(fw) == 2 and all(x.get("whole") == "1" and x.get("tests") == [] for x in fw), fw)
check("whole is in its file too", json.load(open(jw.path)).get("whole") is True and json.load(open(jw.path)).get("tests") == [], json.load(open(jw.path)))
tn = br.tests_named
got = tn(["test", "-p", "coordd", "--bin", "coordd", "--", "graph", "label::", "--test-threads", "1", "--skip", "slow", "--exact"])
check("the names after --, an option's value no name, --skip apart, the target picked", got == {"tests": ["graph", "label::"], "targets": ["--bin coordd"], "skip": ["slow"], "exact": True, "ignored": "", "no_run": False, "whole": False}, got)
check("cargo's TESTNAME is a name; -p's value is not", tn(["test", "-p", "x", "a_name"])["tests"] == ["a_name"], tn(["test", "-p", "x", "a_name"]))
check("--ignored alone is not whole; --include-ignored is", tn(["test", "-p", "x", "--", "--ignored"])["whole"] is False and tn(["test", "-p", "x", "--", "--include-ignored"])["whole"] is True
      and tn(["test", "-p", "x", "--", "--include-ignored"])["ignored"] == "too", (tn(["test", "-p", "x", "--", "--ignored"]), tn(["test", "-p", "x", "--", "--include-ignored"])))
check("--lib and --test=T pick targets: not whole", tn(["test", "-p", "y", "--lib", "--test=t"])["targets"] == ["--lib", "--test t"] and not tn(["test", "-p", "y", "--lib"])["whole"], tn(["test", "-p", "y", "--lib", "--test=t"]))
check("--no-run runs nothing: not whole", tn(["test", "--no-run", "--profile", "replay", "-p", "coordd"]) == {"tests": [], "targets": [], "skip": [], "exact": False, "ignored": "", "no_run": True, "whole": False}, tn(["test", "--no-run", "-p", "coordd"]))
check("another verb names no tests", tn(["build", "-p", "x"]) == {} and tn([]) == {}, tn(["build", "-p", "x"]))
_, ab, fb = job_lines({"args": ["build", "--release", "-p", "coordd"]}, {}, 0)
check("a build's lines say nothing of tests", all(not set(x) & {"tests", "whole", "passed", "failed"} for x in fb), fb)
why = "a piece's test job names its package and its tests: build-runner cargo --folder engine test --profile quick -p PACKAGE -- TEST_NAME... (a whole crate's tests are a train's)"
jr, ar, fr = job_lines({"args": ["test", "-p", "coord-types"]}, {"exit": 3, "refused": why}, 3)
check("a refused job's end: rc 3, the runner's words, no counts", len(fr) == 2 and fr[1].get("rc") == "3" and fr[1].get("refused") == why and "passed" not in fr[1] and "failed" not in fr[1], fr[-1:])
check("its words say refused and why", f": rc 3, refused: {why}, " in ar[1][2], ar[1][2])
check("its file too", json.load(open(jr.path)).get("refused") == why and json.load(open(jr.path)).get("rc") == 3, json.load(open(jr.path)))
_, an, fn = job_lines(req, {"exit": 101}, 101)
check("a test job that ran none says 0 and 0, in words no tests run", fn[1].get("passed") == "0" and fn[1].get("failed") == "0" and ": rc 101, no tests run, " in an[1][2], (fn[1], an[1][2]))
_, a2, f2 = job_lines(req, {"passed": 3, "failed": 2, "failing": ["a::y", "b::z"]}, 101)
check("the failing tests by name, in the fields and the words", f2[1].get("failing") == ["a::y", "b::z"] and "failing_more" not in f2[1] and ": rc 101, 3 passed, 2 failed (a::y, b::z), " in a2[1][2], (f2[1], a2[1][2]))
many = [f"t::n{i}" for i in range(250)]
_, ac, fc = job_lines(req, {"passed": 0, "failed": 250, "failing": many}, 101)
check("past FAILING_MAX names, the rest counted; the words name three", fc[1].get("failing") == many[:br.FAILING_MAX] and fc[1].get("failing_more") == str(250 - br.FAILING_MAX) and fc[1].get("failed") == "250"
      and "(t::n0, t::n1, t::n2, …)" in ac[1][2], (len(fc[1].get("failing") or []), fc[1].get("failing_more")))
_, an2, fn2 = job_lines({"args": ["test", "--no-run", "-p", "coordd"]}, {"exit": 0}, 0)
check("a --no-run job says no_run, and no counts", fn2[1].get("no_run") == "1" and "passed" not in fn2[1] and "whole" not in fn2[1], fn2[1])
# p1564 point 3: the binaries a job brought back, each with the md5 a replay's binary_md5 gives; one not there is not named
import hashlib
open(f"{d}/coordd-0123456789abcdef", "wb").write(b"a test binary\n" * 100000)
md5b = hashlib.md5(open(f"{d}/coordd-0123456789abcdef", "rb").read()).hexdigest()[:8]
jb3, ab3, fb3 = job_lines({"args": ["test", "--no-run", "-p", "coordd"]}, {"exit": 0, "produced": [f"{d}/coordd-0123456789abcdef", f"{d}/not-brought-back"]}, 0)
check("a --no-run job's end names each binary it brought back with its md5, as replay's binary_md5", fb3[1].get("binaries") == [{"path": f"{d}/coordd-0123456789abcdef", "md5": md5b}]
      and "binaries" not in fb3[0], fb3[1].get("binaries"))
check("its file too", json.load(open(jb3.path)).get("binaries") == [{"path": f"{d}/coordd-0123456789abcdef", "md5": md5b}], json.load(open(jb3.path)).get("binaries"))
class W:
    def __init__(s): s.b = b""
    def write(s, x): s.b += x
def steps_of(lines, on=True):
    st = br.Steps(W(), on)
    for l in lines:
        st.test(l + b"\n")
    return st.ended()
got = steps_of([b"     Running unittests src/main.rs (target/quick/deps/coordd-0123456789abcdef)", b"", b"running 3 tests", b"test a::x ... ok",
                b"test a::y ... FAILED", b"test a::w ... ignored", b"", b"failures:", b"", b"---- a::y stdout ----", b"thread 'a::y' panicked at src/a.rs:1:1:",
                b"", b"failures:", b"    a::y", b"", b"test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s"])
check("Steps: the failing test's name from its own line, once", got == {"passed": 1, "failed": 1, "failing": ["a::y"]}, got)
got = steps_of([b"running 5 tests", b"..F.F", b"", b"failures:", b"", b"---- b::z stdout ----", b"left: 1", b"---- src/lib.rs - f (line 3) stdout ----",
                b"", b"failures:", b"    b::z", b"    src/lib.rs - f (line 3)", b"", b"test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"])
check("Steps under -q: the names from the failures' output, a doc-test's with its spaces", got == {"passed": 3, "failed": 2, "failing": ["b::z", "src/lib.rs - f (line 3)"]}, got)
check("Steps not asked for says nothing", steps_of([b"running 1 test", b"test a::y ... FAILED", b"test result: FAILED. 0 passed; 1 failed; 0 ignored"], on=False) == {})
# ---- p1563 (proto-window p1562): --show-output's passing outputs are no failures; a binary that died says so
show = [b"", b"successes:", b"", b"---- a::x stdout ----", b"said by a passing test", b"", b"successes:", b"    a::x", b""]
got = steps_of([b"running 2 tests", b"test a::x ... ok", b"test a::y ... FAILED", *show, b"failures:", b"", b"---- a::y stdout ----",
                b"thread 'a::y' panicked at src/a.rs:1:1:", b"", b"failures:", b"    a::y", b"", b"test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"])
check("Steps with --show-output: a passing test's output under successes: is no failure", got == {"passed": 1, "failed": 1, "failing": ["a::y"]}, got)
got = steps_of([b"running 2 tests", b".F", *show, b"failures:", b"", b"---- a::y stdout ----", b"", b"failures:", b"    a::y", b"",
                b"test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"])
check("the same under -q", got == {"passed": 1, "failed": 1, "failing": ["a::y"]}, got)
got = steps_of([b"running 1 test", b"test a::x ... ok", *show, b"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"])
check("all passing with --show-output: nothing failing", got == {"passed": 1, "failed": 0}, got)
got = steps_of([b"running 1 test", b"test a::q ... FAILED", b"", b"failures:", b"", b"---- a::q stdout ----", b"", b"failures:", b"    a::q", b"",
                b"test result: FAILED. 0 passed; 1 failed; 0 ignored", b"running 1 test", b"test a::x ... ok", *show, b"test result: ok. 1 passed; 0 failed; 0 ignored"])
check("a binary's failures: ends at its result: the next binary's passing output is no failure", got == {"passed": 1, "failed": 1, "failing": ["a::q"]}, got)
died = [b"     Running unittests src/main.rs (target/quick/deps/coordd-0123456789abcdef)", b"", b"running 3 tests", b"test a::x ... ok", b"",
        b"thread 'a::deep' has overflowed its stack", b"fatal runtime error: stack overflow", b"error: test failed, to rerun pass `-p coordd --bin coordd`", b"",
        b"Caused by:", b"  process didn't exit successfully: `/w/yaaihh/target/quick/deps/coordd-0123456789abcdef graph --exact` (signal: 6, SIGABRT: process abort signal)",
        b"     Running unittests src/lib.rs (target/quick/deps/docket_machine-0123456789abcdef)", b"", b"running 1 test", b"test g ... ok", b"",
        b"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"]
got = steps_of(died)
check("Steps: a binary that died is named with its signal, and the next binary's counts still come", got == {"passed": 1, "failed": 0, "died": [{"binary": "coordd", "signal": 6, "name": "SIGABRT"}]}, got)
got = steps_of([b"running 5 tests", b"..", b"error: test failed, to rerun pass `--lib`", b"", b"Caused by:",
                b"  process didn't exit successfully: `/t/deps/yaaihh-0123456789abcdef.exe --test-threads 4` (signal: 9, SIGKILL: kill) (core dumped)"])
check("the first binary killed at its cap: died, and no counts", got == {"died": [{"binary": "yaaihh", "signal": 9, "name": "SIGKILL"}]}, got)
got = steps_of([b"running 1 test", b"test a::y ... FAILED", b"test result: FAILED. 0 passed; 1 failed; 0 ignored", b"error: test failed, to rerun pass `--lib`",
                b"Caused by:", b"  process didn't exit successfully: `/t/deps/x-0123456789abcdef` (exit status: 101)"])
check("a binary that exits 101 for its failed tests did not die", got == {"passed": 0, "failed": 1, "failing": ["a::y"]}, got)
jd, ad, fd = job_lines(req, {"exit": 101, "died": [{"binary": "coordd", "signal": 9, "name": "SIGKILL"}]}, 101)
check("a job whose binary died: died in the fields, its words say it and not no tests run", fd[1].get("died") == [{"binary": "coordd", "signal": 9, "name": "SIGKILL"}]
      and ": rc 101, 0 passed, 0 failed, coordd died (signal 9, SIGKILL), " in ad[1][2] and "no tests run" not in ad[1][2], (fd[1], ad[1][2]))
check("its file too", json.load(open(jd.path)).get("died") == [{"binary": "coordd", "signal": 9, "name": "SIGKILL"}], json.load(open(jd.path)))
st = br.Steps(W(), True); st.refused = "1.0 GB free, below 12 GB"
check("Steps carries the disk's refusal into the last line", st.ended() == {"refused": "1.0 GB free, below 12 GB"}, st.ended())
w = W(); br.refusal(w, why)
ln = w.b.decode().splitlines()
check("refusal: the words, then the end with them", ln[0] == f"build-runner: refused: {why}" and json.loads(ln[1]) == {"exit": 3, "refused": why}, ln)
br.caller_agent = br.job_agent = lambda pid: "w1"  # stand-ins: no /proc of another process is read, no ssh runs
br.caller_coord = lambda pid: "research"
os.makedirs(f"{d}/nogit", exist_ok=True)
k = len(acts())
try:
    br.remote_client({"cwd": f"{d}/nogit", "args": ["test", "-p", "x", "--", "y"], "steps": True}, "m", "stand-in")
    rc = None
except SystemExit as e:
    rc = e.code
ax = acts()[k:]
check("refused by this client before sending: rc 3 to the caller, and the job's two lines, the end refused", rc == 3 and len(ax) == 2 and fields(ax[1]).get("rc") == "3"
      and "is in no git worktree" in fields(ax[1]).get("refused", ""), (rc, ax))
br.remote_files_differ = lambda host: "the build machine's runner files (x) are not this runner's (y)"
k = len(acts()); jm = br.JobFile(req, wt, facts, "w1", "research")
try:
    br.remote_wait("m", "stand-in", "the job", jm)
    rc = None
except SystemExit as e:
    rc = e.code
ax = acts()[k:]
check("the machine's runner files not this one's: rc 3, the end refused with the words", rc == 3 and len(ax) == 2 and fields(ax[1]).get("refused", "").startswith("the build machine's runner files"), (rc, ax))
# p1578: a bring-back that failed names no binaries, though an older file sits at the path (a failed fetch leaves the
# previous build's deps/NAME-HASH there); one that worked names them. Stand-ins: the tree's send (a tree as send gives
# it), the machine answering at once with its job's end naming that path produced and its blobs, and take_back (the
# files back by content, since the storage page's piece A) failing or working; no ssh runs
import io
class FakeRemote:
    def __init__(s, host): pass
    def write(s, b): pass
    def __iter__(s): return iter([json.dumps({"exit": 0, "produced": [f"{d}/coordd-0123456789abcdef"],
                                              "blobs": [{"path": f"{d}/coordd-0123456789abcdef"}], "store": "x"}).encode() + b"\n"])
    def close(s): pass
saved = (br.remote_files_differ, br.send, br.Remote, br.take_back)
br.remote_files_differ, br.Remote = (lambda host: None), FakeRemote
br.send = lambda *a: {"files": [], "incoming": [], "sent": 0, "bytes": 0}
def brought(ok):
    br.take_back = lambda host, m, wt, made_by=None: (ok, 1, 0, 0, 0)
    k = len(acts()); old, sys.stderr = sys.stderr, io.StringIO()
    try:
        rc = br.remote_client({"cwd": wt, "args": ["test", "--no-run", "-p", "coordd"], "steps": True}, "m", "stand-in")
    finally:
        sys.stderr = old
    return rc, [fields(x) for x in acts()[k:]]
rc, fx = brought(False)
check("a bring-back that failed, an older binary at the path: rc 1, and the end names no binaries (p1578)",
      rc == 1 and len(fx) == 2 and fx[1].get("rc") == "1" and "binaries" not in fx[1], (rc, fx[1:]))
rc, fx = brought(True)
check("one that worked names the binary there, with its md5", rc == 0 and len(fx) == 2
      and fx[1].get("binaries") == [{"path": f"{d}/coordd-0123456789abcdef", "md5": md5b}], (rc, fx[1:]))
br.remote_files_differ, br.send, br.Remote, br.take_back = saved
# ---- a red (proto-mind N-1446 p1502, head 3): --red-of REV, the green commit it takes a change out of; its lines say
# REV's commit and tree and the files taken out (where the tree sent differs from REV's)
tree_of = lambda rev: os.popen(f"git -C {wt} rev-parse '{rev}^{{tree}}'").read().strip()
rreq = {"args": ["test", "-p", "coordd", "--", "number_taken"], "red_of": "HEAD"}
k = len(acts()); jr = br.JobFile(rreq, wt, facts, "w1", "research"); jr.end({"passed": 0, "failed": 1, "failing": ["number_taken"]}, 101)
ar = acts()[k:]; fr = [fields(x) for x in ar]
check("a red of HEAD with its fix undone in the working tree: red_of, its tree, the file taken out, at both ends", len(fr) == 2 and jr.red_why is None
      and all(x.get("red_of") == commit and x.get("red_of_tree") == tree_of("HEAD") and x.get("takes_out") == ["a"] and "takes_out_more" not in x for x in fr), fr)
check("its words say it is a red of the commit and what it takes out", ar[0][2] == f"job start: test -p coordd on work {commit} + 1 changed, a red of {commit} taking out 1 file, for I-7, N-1104.p678"
      and ": rc 101, 0 passed, 1 failed (number_taken), " in ar[1][2], [x[2] for x in ar])
check("its file says it too", json.load(open(jr.path)).get("red_of") == commit and json.load(open(jr.path)).get("takes_out") == ["a"], json.load(open(jr.path)))
g = lambda *a: os.popen("git -C " + wt + " -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@t.invalid " + " ".join(a) + " 2>&1").read()
g("commit", "-qam", "'the fix undone\n\nItem: I-7'"); red_commit = os.popen(f"git -C {wt} rev-parse --short=8 HEAD").read().strip()
k = len(acts()); jc = br.JobFile({**rreq, "red_of": commit}, wt, {"branch": "work", "commit": red_commit}, "w1", "research")
fc = fields(acts()[k])
check("a red as a commit of its own: red_of the green commit, the files its commit takes out, 0 changed", fc.get("red_of") == commit and fc.get("takes_out") == ["a"]
      and fc.get("commit") == red_commit and fc.get("changed") == "0" and jc.red_why is None, fc)
br.TAKES_OUT_MAX = 1; open(f"{wt}/b", "a").write("b changed\n")
k = len(acts()); br.JobFile({**rreq, "red_of": commit}, wt, {"branch": "work", "commit": red_commit}, "w1", "research")
fm = fields(acts()[k])
check("past TAKES_OUT_MAX files, the rest counted; the words count them all", fm.get("takes_out") == ["a"] and fm.get("takes_out_more") == "1"
      and "taking out 2 files" in acts()[k][2], (fm, acts()[k][2]))
br.TAKES_OUT_MAX = 200; g("checkout", "-q", "b")
def no_service():  # a stand-in: a request this client did not refuse ends here, and no service or unit is reached
    raise SystemExit("connect: the test's stand-in; no service")
br.connect = no_service
def client_rc(r):
    k = len(acts())
    try:
        br.client({"cwd": wt, "steps": True, **r}); return None, acts()[k:]
    except SystemExit as e:
        return e.code, acts()[k:]
rc, ax = client_rc({**rreq, "red_of": red_commit})
check("a red that takes nothing out (what it sends is REV's tree): refused, rc 3, before the service, the words in its end", rc == 3 and len(ax) == 2
      and fields(ax[1]).get("refused", "").startswith(f"--red-of {red_commit}: what it sends is {red_commit}'s tree") and fields(ax[1]).get("takes_out") == [], (rc, ax))
rc, ax = client_rc({**rreq, "red_of": "no-such-rev"})
check("--red-of no commit: refused, rc 3, its words", rc == 3 and len(ax) == 2 and fields(ax[1]).get("refused") == f"--red-of no-such-rev: no commit of {wt}"
      and fields(ax[1]).get("red_of") == "no-such-rev" and "red_of_tree" not in fields(ax[1]), (rc, ax))
k = len(acts())
try:
    br.remote_client({"cwd": wt, "args": rreq["args"], "steps": True, "red_of": red_commit}, "m", "stand-in"); rc = None
except SystemExit as e:
    rc = e.code
ax = acts()[k:]
check("the same on a build machine's client: refused before sending", rc == 3 and len(ax) == 2 and "takes nothing out" in fields(ax[1]).get("refused", ""), (rc, ax))
# p1565 point 1: what a red takes out beyond REV's own change (red_commit's own change is `a`)
open(f"{wt}/b", "a").write("b changed\n")
k = len(acts()); br.JobFile({**rreq, "red_of": red_commit}, wt, {"branch": "work", "commit": red_commit}, "w1", "research")
fb = fields(acts()[k])
check("a red taking out a file beyond REV's own change: takes_out_beyond names it, and its words say it (p1565 point 1)",
      fb.get("takes_out") == ["b"] and fb.get("takes_out_beyond") == ["b"] and f"a red of {red_commit} taking out 1 file, 1 beyond its own change, " in acts()[k][2], (fb, acts()[k][2]))
g("checkout", "-q", "b"); open(f"{wt}/a", "a").write("a again\n")
k = len(acts()); br.JobFile({**rreq, "red_of": red_commit}, wt, {"branch": "work", "commit": red_commit}, "w1", "research")
fa = fields(acts()[k])
check("a red taking out only REV's own change: takes_out_beyond is [], and its words say nothing beyond", fa.get("takes_out") == ["a"] and fa.get("takes_out_beyond") == []
      and "beyond" not in acts()[k][2], (fa, acts()[k][2]))
check("a root commit's own change is all of it", fr and fr[0].get("takes_out_beyond") == [], fr[0].get("takes_out_beyond") if fr else fr)
# p1578's small one: a name with a space in it is one file, in takes_out and in REV's own change (diff-tree -z)
import subprocess
gz = lambda *a: subprocess.run(["git", "-C", wt, "-c", "core.hooksPath=/dev/null", "-c", "user.name=t", "-c", "user.email=t@t.invalid", *a], capture_output=True)
open(f"{wt}/c d", "w").write("c d\n"); gz("add", "c d"); gz("commit", "-qm", "c d\n\nItem: I-7")
spaced = os.popen(f"git -C {wt} rev-parse --short=8 HEAD").read().strip()
open(f"{wt}/c d", "a").write("undone\n")
k = len(acts()); br.JobFile({**rreq, "red_of": spaced}, wt, {"branch": "work", "commit": spaced}, "w1", "research")
fz = fields(acts()[k])
check("a file named with a space: one name in takes_out and in REV's own change, so nothing beyond it but `a`",
      fz.get("takes_out") == ["a", "c d"] and fz.get("takes_out_beyond") == ["a"], fz)
gz("checkout", "-q", "--", "c d")
# p1565 point 2: --red-of with steps off is refused, not run as a job that is no red
import io
def said(call):  # what CALL says on stderr, and its exit
    old, sys.stderr = sys.stderr, io.StringIO()
    try:
        call(); rc = None
    except SystemExit as e:
        rc = e.code
    finally:
        out, sys.stderr = sys.stderr.getvalue(), old
    return rc, out
steps_off = f"--red-of {red_commit} with BUILD_RUNNER_STEPS=0: a red says what it takes out only in its job's lines, and with steps off there are none"
k = len(acts()); rc, out = said(lambda: br.client({"cwd": wt, "steps": False, **rreq, "red_of": red_commit}))
check("--red-of with steps off: refused, rc 3, with its words, before the service, and no lines (p1565 point 2)", rc == 3 and steps_off in out and acts()[k:] == [], (rc, out))
k = len(acts()); rc, out = said(lambda: br.remote_client({"cwd": wt, "args": rreq["args"], "steps": False, "red_of": red_commit}, "m", "stand-in"))
check("the same on a build machine's client", rc == 3 and steps_off in out and acts()[k:] == [], (rc, out))
g("checkout", "-q", "a")
k = len(acts()); jn = br.JobFile(req, wt, facts, "w1", "research")
check("a job that is no red says nothing of reds", jn.red_why is None and not {"red_of", "red_of_tree", "takes_out"} & set(fields(acts()[k])), fields(acts()[k]))
n = len(acts())
br.JobFile(req, wt, {"branch": "work", "commit": commit}, None, None).end({}, 0)
check("a caller in no agent says nothing", len(acts()) == n, acts()[n:])
os.environ["AGENT_BASE"] = f"{d}/ab"; br.UNIT = "build-runner-test"
j = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w1", "research"); j.end({}, 0)
check("the runner's own tests (UNIT build-runner-test) say nothing", len(acts()) == n, acts()[n:])
br.UNIT = "build-runner"
os.environ["AGENT_BASE"] = f"{d}/none"
j = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w1", "research"); j.end({}, 0)
check("no coordc: nothing said, the file written", len(acts()) == n and json.load(open(j.path)).get("step") == "ended")
# ---- the lines handed off, in order (p1542's rule; proto-window N-1446 p1565): a refused job, whose end comes at once
os.environ["AGENT_BASE"] = f"{d}/slow"
k = len(acts())
t = time.time(); j = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w1", "research"); j.end({"exit": 3, "refused": "x"}, 3)
took, early = time.time() - t, len(acts_now()) - k
check("a slow coordc holds the job not at all: its start and end in under a second, no line landed yet", took < 1 and early == 0, (took, early))
a = acts()[k:]
check("its lines land after the job has gone on, in the job's order: the start, then the end", [fields(x).get("phase") for x in a] == ["start", "end"], [x[2] for x in a])
check("no lock file of the job's lines is left after its last line", not list(br.JOBS_DIR.glob(f"{j.path.stem}.*.act")), list(br.JOBS_DIR.glob("*.act")))
os.environ["AGENT_BASE"] = f"{d}/fail"; os.environ["PATH"] = f"{d}/logbin:" + os.environ["PATH"]
j = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w1", "research"); j.end({}, 0); acts()
jl = open(f"{d}/journal.txt").read().splitlines() if os.path.exists(f"{d}/journal.txt") else []
check("a coordc that fails: the line it lost and coordc's words go to the journal, for each of the job's lines",
      jl == [f"build-runner-act build-runner: the record's line of job {j.path.stem}'s start not recorded: coordc act job failed (rc 1)",
             "build-runner-act coordc act: coordd refused: no such agent",
             f"build-runner-act build-runner: the record's line of job {j.path.stem}'s end not recorded: coordc act job failed (rc 1)",
             "build-runner-act coordc act: coordd refused: no such agent"], jl)
# ---- coordd down (paper-research N-1446 p1872): a line coordc could not take to coordd (rc 3) waits for it and goes
# once it is back, none dropped, in the order the lines were made: a job that starts and ends while research's coordd is
# down, and a second job made meanwhile after it; another coord's lines go on all the while
def downs():
    try:
        return [json.loads(l) for l in open(f"{d}/down-acts.jsonl")]
    except FileNotFoundError:
        return []
def line(x):
    f = fields(x["argv"])
    return (x["sock"], f.get("id"), f.get("phase"))
def journal():
    return open(f"{d}/journal.txt").read().splitlines() if os.path.exists(f"{d}/journal.txt") else []
os.environ["AGENT_BASE"] = f"{d}/down"; open(f"{d}/journal.txt", "w").close()
research, cav = f"{d}/research.sock", f"{d}/cav.sock"
open(f"{d}/down-research.sock", "w").close()  # research's coordd goes down
os.environ["COORDD_SOCK"] = research
t = time.time(); j1 = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w1", "research"); j1.end({}, 0); took = time.time() - t
time.sleep(1.1)  # a job's id is its pid and its second
j2 = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w2", "research")
os.environ["COORDD_SOCK"] = cav
k1 = br.JobFile(req, wt, {"branch": "work", "commit": commit}, "w3", "cav-paper"); k1.end({}, 0)
os.environ["COORDD_SOCK"] = research
j2.end({}, 0)
time.sleep(3)  # the waiting line has tried again
early = [line(x) for x in downs()]
check("coordd down holds the job not at all: its start and end in under a second", took < 1, took)
check("while research's coordd is down none of its lines is recorded, and another coord's go on: cav's start and end",
      early == [("cav.sock", k1.path.stem, "start"), ("cav.sock", k1.path.stem, "end")], early)
jl = journal()
check("the journal says research's first line waits for coordd, with coordc's words; the lines after it wait behind it",
      jl == [f"build-runner-act build-runner: the record's line of job {j1.path.stem}'s start waits for coordd: coordc act job failed (rc 3)",
             f"build-runner-act coord: coordd is not running ({research}: Connection refused)"], jl)
os.remove(f"{d}/down-research.sock")  # research's coordd is back
for p in br.ACT_SENT:
    p.wait(timeout=60)
got = [line(x) for x in downs()][2:]
check("once it is back each of research's lines is recorded, none dropped, in the order they were made: the first job's start and end, then the second's",
      got == [("research.sock", j1.path.stem, "start"), ("research.sock", j1.path.stem, "end"),
              ("research.sock", j2.path.stem, "start"), ("research.sock", j2.path.stem, "end")], got)
jl = journal()
check("the journal says the first went once coordd was back, and says no line lost",
      len(jl) == 3 and re.fullmatch(rf"build-runner-act build-runner: the record's line of job {j1.path.stem}'s start recorded once coordd was back, \d+ s after it first failed", jl[2]) is not None, jl)
left = sorted(p.name for p in br.JOBS_DIR.iterdir() if p.suffix in (".act", ".tail"))
check("no line's lock file is left, nor a coord's last line", left == [], left)
print(f"{fails} failed")
sys.exit(1 if fails else 0)
EOF
rc=$?
cat "$dir/out.log"; [ -s "$dir/err.log" ] && { echo "stderr:"; cat "$dir/err.log"; }
exit $rc
