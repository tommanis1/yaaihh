#!/usr/bin/env bash
# A replay as a job of its own (proto-mind N-1446 p1502, head 4): `build-runner replay [--record NAME] [--commit REV]
# [--memory SIZE] -- TEST_BINARY ARGS...` runs a built test binary on a copy of a record on this PC, behind the shared
# (and, with --record NAME [--line N] and no REPLAY_COORD, on the runner's own view of NAME's live record: its first N
# lines, else its lines before its last input, and its project file, read-only under a layer of the run's own, removed
# after; storage step 2, N-1446 p2727)
# replay lock, in a scope with its memory held, and says it in the record as a cargo job is said: the act `job` once as
# it starts and once as it ends, verb `replay`, with the record, its lines, the binary and its md5, the tests it names,
# the worktree's (or REV's) commit and tree, and at the end rc, passed, failed, the failing names and the replay's three
# counts. build-runner is loaded as a module with stand-ins: a coordc that writes down its arguments, a scope that writes
# down its own and runs the rest, a lock file of the test's own, a test binary that prints a replay's lines; the
# caller's agent and coord are given, so no /proc of another process is read. No service, no compile, no record of
# anyone's. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/ab/bin" "$dir/wt" "$dir/rec/research/tickets" "$dir/bin" "$dir/rec/research-project/kinds" "$dir/caller/research/tickets"
cat >"$dir/ab/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$dir/acts.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
cat >"$dir/scope" <<EOF
#!/usr/bin/env bash
echo "\$*" >>"$dir/scope.log"
while [ "\$1" != "--" ]; do shift; done; shift
exec "\$@"
EOF
cat >"$dir/bin/coordd-test" <<EOF
#!/usr/bin/env bash
if flock -n "$dir/replay.lock" true; then echo "lock: free"; else echo "lock: held"; fi
echo "lock fds: \$(ls -l /proc/\$\$/fd 2>/dev/null | grep -c 'replay\.lock')"  # the lock's fds this binary holds
echo "running 1 test"
if [ -n "\${VIEW:-}" ]; then  # what a replay sees at REPLAY_COORD, and a write into it
  echo "view: \$REPLAY_COORD uid \$(id -u) lines \$(wc -l <"\$REPLAY_COORD/tickets/log.jsonl") project \$(cat "\$REPLAY_COORD/project")"
  echo "view: REPLAY_LINES \$REPLAY_LINES REPLAY_BYTES \$REPLAY_BYTES; alive.json there: \$(test -e "\$REPLAY_COORD/tickets/alive.json" && echo yes || echo no)"
  echo '{"ev":"written-in-the-view"}' >>"\$REPLAY_COORD/tickets/log.jsonl" && echo "view: written"
  echo "\$REPLAY_COORD" >"$dir/viewpath"
fi
[ -n "\${DIE:-}" ] && kill -9 \$\$  # killed as at its memory cap
[ -n "\${SLEEP:-}" ] && { echo \$\$ >"$dir/sleeping"; exec sleep "\$SLEEP"; }  # a long replay, its pid said
# a long replay that writes on as SIGTERM ends it, past a pipe's 64 KB, its count line last
[ -n "\${ONTERM:-}" ] && { trap 'yes "a line it writes as it stops" | head -n 4000
  echo "replay \$REPLAY_COORD with \$REPLAY_BASE: 0 problem lines, 0 now makes, 0 now refused"
  echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 822 filtered out; finished in 0.10s"; exit 0' TERM
  echo \$\$ >"$dir/sleeping"; while :; do sleep 0.1; done; }
echo "replay \$REPLAY_COORD with \$REPLAY_BASE: \${PROBLEMS:-0} problem lines, \${MAKES:-0} now makes, 0 now refused"
if [ "\${PROBLEMS:-0}" = 0 ]; then
  echo "test ticket::tests::a_coords_record_replays ... ok"
  echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 822 filtered out; finished in 0.10s"
else
  echo "test ticket::tests::a_coords_record_replays ... FAILED"; echo; echo "failures:"; echo
  echo "---- ticket::tests::a_coords_record_replays stdout ----"; echo
  echo "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 822 filtered out; finished in 0.10s"
  exit 101
fi
EOF
chmod +x "$dir/ab/bin/coordc" "$dir/scope" "$dir/bin/coordd-test"
printf '{"ev":"a"}\n{"ev":"b"}\n{"ev":"c"}\n' >"$dir/rec/research/tickets/log.jsonl"
# the caller's own record (a stand-in, as a leg's): no live record's, so the runner takes it
printf '{"ev":"x"}\n{"ev":"y"}\n{"ev":"z"}\n' >"$dir/caller/research/tickets/log.jsonl"
g() { git -C "$dir/wt" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@t.invalid "$@"; }
g init -q -b train-67 && echo a >"$dir/wt/a" && g add a && g commit -q -m $'the tip\n\nItem: N-1446\nCo-Authored-By: x <x@x.invalid>'
export BUILD_RUNNER_HOME="$dir/home" GIT_CONFIG_NOSYSTEM=1 BUILD_RUNNER_REPLAY_LOCK="$dir/replay.lock" \
  BUILD_RUNNER_REPLAY_SCOPE="$dir/scope -p MemoryMax={memory} --" AGENT_BASE="$dir/ab" \
  REPLAY_COORD="$dir/caller/research" REPLAY_BASE="$dir/base" REPLAY_PROJECT="$dir/rec/research-project"
python3 -I - "$here/build-runner" "$dir" >"$dir/out.log" 2>"$dir/err.log" <<'EOF'
import hashlib, importlib.machinery, importlib.util, json, os, re, shutil, signal, subprocess, sys, time
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
br.job_agent = lambda pid: "w1"  # stand-ins: no /proc of another process is read
br.caller_coord = lambda pid: "research"
br.RECORDS = br.Path(f"{d}/rec")  # the live records --record NAME names: the test's own, never this PC's
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
def acts():  # once the lines sent so far have landed: a job's go out from children of its own (`JobFile.send`)
    for p in getattr(br, "ACT_SENT", []):
        p.wait(timeout=30)
    try:
        return [json.loads(l) for l in open(f"{d}/acts.jsonl")]
    except FileNotFoundError:
        return []
def fields(a):
    out = {}
    for kv in a[3:]:
        k, v = kv.split("=", 1)
        out[k] = json.loads(v) if v[:1] in "[{" else v
    return out
def ends(xs):  # a job's start and end lines, its `runs` (as it leaves the queue, H-1651) aside: each check reads those two
    return [x for x in xs if fields(x).get("phase") != "runs"]
whole = []  # the last run's lines, its `runs` among them
def run(argv, env=None):
    k = len(acts()); old, saved = sys.stdout, dict(os.environ)
    os.environ.update(env or {})
    sys.stdout = open(f"{d}/pass.out", "w")
    try:
        rc = br.replay(argv)
    except SystemExit as e:
        rc = e.code
    finally:
        sys.stdout.close(); sys.stdout = old; os.environ.clear(); os.environ.update(saved)
    # its own lines: the end of a job whose caller is gone, which its start says first (`end_gone`), is another job's
    whole[:] = [a for a in acts()[k:] if "gone" not in fields(a)]
    return rc, ends(whole), open(f"{d}/pass.out").read()
wt, tb = f"{d}/wt", f"{d}/bin/coordd-test"
os.chdir(wt)
commit = subprocess.run(["git", "rev-parse", "--short=8", "HEAD"], capture_output=True, text=True).stdout.strip()
md5 = hashlib.md5(open(tb, "rb").read()).hexdigest()[:8]
name = "ticket::tests::a_coords_record_replays"
rc, a, out = run(["--", tb, name, "--ignored", "--nocapture", "--exact"])
check("a clean replay gives its binary's rc 0", rc == 0, rc)
check("three acts, start, runs and end, all `act job`", [fields(x).get("phase") for x in whole] == ["start", "runs", "end"]
      and all(x[:2] == ["act", "job"] for x in whole), whole)
check("`runs` says the replay's words and its id", len(whole) == 3 and whole[1][2] == f"job runs: replay of research on train-67 {commit}, for N-1446"
      and fields(whole[1]).get("id") == fields(whole[0]).get("id"), whole[1:2])
if len(a) == 2:
    s, e = fields(a[0]), fields(a[1])
    check("its words: a replay of the record on the worktree's commit", a[0][2] == f"job start: replay of research on train-67 {commit}, for N-1446", a[0][2])
    check("verb replay, the record and its lines, the binary and its md5", s.get("verb") == "replay" and s.get("record") == "research" and s.get("lines") == "3"
          and s.get("binary") == tb and s.get("binary_md5") == md5, s)
    check("the tests it names, --exact, --ignored; not whole", s.get("tests") == [name] and s.get("exact") == "1" and s.get("ignored") == "only" and "whole" not in s, s)
    check("the worktree's commit, branch, tree and items", s.get("commit") == commit and s.get("branch") == "train-67" and s.get("changed") == "0"
          and s.get("tree") == subprocess.run(["git", "rev-parse", "HEAD^{tree}"], capture_output=True, text=True).stdout.strip() and s.get("items") == ["N-1446"], s)
    check("no packages and no command line", s.get("packages") == [] and "--nocapture" not in json.dumps(a), s.get("packages"))
    check("the end: rc 0, its test's counts and the replay's three", e.get("rc") == "0" and e.get("passed") == "1" and e.get("failed") == "0"
          and e.get("problem_lines") == "0" and e.get("now_makes") == "0" and e.get("now_refused") == "0" and "failing" not in e, e)
    check("the end's words say them", ": rc 0, 1 passed, 0 failed, 0 problem lines, 0 now makes, 0 now refused, " in a[1][2], a[1][2])
    check("the end says the record and binary again, under the start's id", e.get("id") == s.get("id") and e.get("record") == "research" and e.get("binary_md5") == md5, e)
    at = lambda p: time.strftime("%FT%TZ", time.gmtime(os.stat(p).st_ctime))
    check("way forward, no back (p1522)", s.get("way") == "forward" and e.get("way") == "forward" and "back" not in s, s)
    check("the copies it ran on: the record's path and its log's change time, the project's path and its",
          s.get("copy") == f"{d}/caller/research" and s.get("copy_at") == at(f"{d}/caller/research/tickets/log.jsonl")
          and s.get("project") == f"{d}/rec/research-project" and s.get("project_at") == at(f"{d}/rec/research-project")
          and e.get("copy_at") == s.get("copy_at") and e.get("project") == s.get("project"), s)
check("the binary ran while the replay lock was held", "lock: held" in out, out)
check("the binary holds the lock itself, as under `flock LOCK CMD` (p1564 point 1)", "lock fds: 1" in out, out)
check("its output came through", "problem lines, 0 now makes, 0 now refused" in out and "test result: ok. 1 passed" in out, out)
check("the lock is free once it has ended", subprocess.run(["flock", "-n", f"{d}/replay.lock", "true"]).returncode == 0)
check("in the scope, with MemoryMax 14G", open(f"{d}/scope.log").read().splitlines()[-1].startswith("-p MemoryMax=14G -- " + tb), open(f"{d}/scope.log").read())
jf = json.load(open(sorted((br.JOBS_DIR).glob("*.json"), key=os.path.getmtime)[-1]))
check("its job file: verb replay, ended, rc and counts, the record", jf.get("verb") == "replay" and jf.get("step") == "ended" and jf.get("rc") == 0
      and jf.get("problem_lines") == 0 and jf.get("record") == "research", jf)
rc, a, out = run(["--memory", "8G", "--", tb, name, "--exact"], {"PROBLEMS": "2", "MAKES": "1"})
e = fields(a[1]) if len(a) == 2 else {}
check("a replay with problem lines: its rc 101, the counts, the failing test named", rc == 101 and e.get("rc") == "101" and e.get("problem_lines") == "2"
      and e.get("now_makes") == "1" and e.get("failed") == "1" and e.get("failing") == [name], (rc, e))
check("its words say the counts and the name", f"0 passed, 1 failed ({name}), 2 problem lines, 1 now makes, 0 now refused, " in (a[1][2] if len(a) == 2 else ""), a)
check("--memory: the scope's MemoryMax", open(f"{d}/scope.log").read().splitlines()[-1].startswith("-p MemoryMax=8G -- "), open(f"{d}/scope.log").read())
open(f"{wt}/a", "a").write("later\n")
rc, a, out = run(["--commit", "HEAD", "--record", "cav-paper", "--", tb, name])
s = fields(a[0]) if a else {}
check("--commit REV: REV's commit and tree, 0 changed, though the worktree moved on", s.get("commit") == commit and s.get("changed") == "0" and "branch" not in s
      and s.get("tree") == subprocess.run(["git", "rev-parse", "HEAD^{tree}"], capture_output=True, text=True).stdout.strip(), s)
check("--record NAME names the record", s.get("record") == "cav-paper", s)
rc, a, out = run(["--", tb, name], {"REPLAY_COORD": "", "REPLAY_PROJECT": ""})
s = fields(a[0]) if a else {}
check("no copy named: no record, no lines, no copy, no project", not {"record", "lines", "copy", "copy_at", "project", "project_at"} & set(s) and rc == 0, s)
os.makedirs(f"{d}/rec/written/tickets"); open(f"{d}/rec/written/tickets/log.jsonl", "w").write('{"ev":"a"}\n{"ev":"b"}\n')
rc, a, out = run(["--back", "20261121-235501-a1b2c3d4", "--commit", "HEAD", "--record", "research-written", "--", tb, name],
                 {"REPLAY_COORD": f"{d}/rec/written"})
s, e = (fields(a[0]), fields(a[1])) if len(a) == 2 else ({}, {})
check("--back RELEASE: way back, back RELEASE, in both lines (p1522)", s.get("way") == "back" and s.get("back") == "20261121-235501-a1b2c3d4"
      and e.get("way") == "back" and e.get("back") == s.get("back"), (s, e))
check("--back with --commit: the tip's commit and tree, the written coord as the record and its copy",
      s.get("commit") == commit and s.get("record") == "research-written" and s.get("lines") == "2" and s.get("copy") == f"{d}/rec/written", s)
check("its words say back and the release", a and a[0][2] == f"job start: replay of research-written back, 20261121-235501-a1b2c3d4's binary, on ? {commit}, for N-1446", a[0][2] if a else a)
# the store page's check (paper-research N-1446 p2926): no replay on a live record or a copy of one; the way-back one
# above (rec/written, research's first two lines) went on
live0 = open(f"{d}/rec/research/tickets/log.jsonl", "rb").read()
for name_, body in (("whole", live0), ("cut", live0[:live0.index(b"\n") + 1])):
    os.makedirs(f"{d}/copies/{name_}/tickets"); open(f"{d}/copies/{name_}/tickets/log.jsonl", "wb").write(body)
for what, coord in (("a whole copy of research's live record", f"{d}/copies/whole"), ("a cut copy of it", f"{d}/copies/cut"),
                   ("research's live folder itself", f"{d}/rec/research")):
    rc, a, out = run(["--", tb, name], {"REPLAY_COORD": coord})
    check(f"REPLAY_COORD {what}: refused, saying to send --record research, no lines",
          isinstance(rc, str) and f"REPLAY_COORD {coord} is research's live record or a copy of it: send --record research" in rc
          and a == [], (rc, a))
# a live record whose first line is longer than the 4 KB compared (cav-paper's is): its copy is still one
os.makedirs(f"{d}/rec/longline/tickets"); os.makedirs(f"{d}/copies/longline/tickets")
longrec = b'{"ev":"open","text":"' + b"x" * 5000 + b'"}\n{"ev":"post"}\n'
open(f"{d}/rec/longline/tickets/log.jsonl", "wb").write(longrec); open(f"{d}/copies/longline/tickets/log.jsonl", "wb").write(longrec)
check("a copy of a live record whose first line is longer than 4 KB: taken for a copy", br.live_record_of(f"{d}/copies/longline") == "longline",
      br.live_record_of(f"{d}/copies/longline"))
shutil.rmtree(f"{d}/rec/longline")
check("a folder with no record, and the caller's stand-in record: not taken for a copy",
      br.live_record_of(f"{d}/nothing") is None and br.live_record_of(f"{d}/caller/research") is None)
# the store page's second check (p2926): a binary of ours that a replay runs, or that its arguments name, is a link
# to this PC's store (the test's own, BUILD_RUNNER_HOME/store), not a copy; system programs and scripts go on
os.makedirs(f"{d}/handed"); os.makedirs(f"{d}/rel/releases/R1"); os.makedirs(br.STORE_DIR, exist_ok=True)
open(f"{d}/rel/releases/R1/built-from.sha256", "w").write("x\n")  # what makes a folder a release, as release.sh's are
os.makedirs(f"{d}/rel/releases/notone"); os.makedirs(f"{d}/home-of/.cargo/bin"); os.makedirs(f"{d}/home-of/.ours")
shutil.copy2("/usr/bin/true", f"{d}/bin/true-copy"); shutil.copy2("/usr/bin/true", f"{d}/rel/releases/R1/true")
bname = br.store_blob(f"{d}/bin/true-copy", br.Path(f"{d}/handed"))
blob = br.STORE_DIR / "blobs" / bname[:2] / bname
os.link(blob, f"{d}/bin/true-linked")
for what, argv in (("a binary linked from the store", [f"{d}/bin/true-linked"]),
                   ("a system program", ["/usr/bin/true"]),
                   ("a script, and a script naming a stored binary", ["/usr/bin/bash", "-c", "exit 0", tb, f"{d}/bin/true-linked"]),
                   ("a release folder's binary on the way back (until release.sh links from the store)",
                    ["--back", "R1", "--", f"{d}/rel/releases/R1/true"])):
    rc, a, out = run(argv if argv[0] == "--back" else ["--", *argv])
    check(f"{what}: goes on", rc == 0 and len(a) == 2, (rc, a))
    if argv[0] == f"{d}/bin/true-linked":
        check("its lines' md5 as before (one read gives the md5 and the sha256)", len(a) == 2 and fields(a[0]).get("binary_md5")
              == hashlib.md5(open("/usr/bin/true", "rb").read()).hexdigest()[:8], a)
shutil.copy2("/usr/bin/true", f"{d}/rel/releases/notone/true")
for what, argv in (("a copy of a built binary", [f"{d}/bin/true-copy"]),
                   ("a script that names a copy", ["/usr/bin/bash", "-c", "exit 0", f"{d}/bin/true-copy"]),
                   ("a release folder's copy, forward", [f"{d}/rel/releases/R1/true"]),
                   ("with --back, a copy in a folder called releases that is no release (no built-from.sha256)",
                    ["--back", "R1", "--", f"{d}/rel/releases/notone/true"])):
    rc, a, out = run(argv if argv[0] == "--back" else ["--", *argv])
    p = argv[-1]
    check(f"{what}: refused in words before anything, no lines", isinstance(rc, str)
          and f"{p} is a binary outside the store, no link to the blob of its content" in rc and "not a copy" in rc and a == [], (rc, a))
check("a release folder is one by its built-from.sha256, wherever it sits", br.release_folder_of(f"{d}/rel/releases/R1/true")
      == br.Path(f"{d}/rel/releases/R1").resolve() and br.release_folder_of(f"{d}/rel/releases/notone/true") is None
      and br.release_folder_of(f"{d}/bin/true-copy") is None)
br.STORE_MODE = False
rc, a, out = run(["--", f"{d}/bin/true-copy"])
br.STORE_MODE = True
check("with the store off (BUILD_RUNNER_STORE=0, nothing linked): no check, and both lines say so, the end in words",
      rc == 0 and len(a) == 2 and all(fields(x).get("unchecked") == "the store is off (BUILD_RUNNER_STORE=0)" for x in a)
      and "its binary not checked against the store: the store is off (BUILD_RUNNER_STORE=0)" in a[1][2], (rc, a))
rc, a, out = run(["--", f"{d}/bin/true-linked"])
check("with the store on, no such field", rc == 0 and len(a) == 2 and "unchecked" not in fields(a[1]), a)
shutil.copy2("/usr/bin/true", f"{d}/home-of/.cargo/bin/tool"); shutil.copy2("/usr/bin/true", f"{d}/home-of/.ours/copy")
old_home = os.environ.get("HOME"); os.environ["HOME"] = f"{d}/home-of"
check("a tool's own folder of his home (~/.cargo) holds no build of ours; another dot-folder's copy is one (p2949)",
      not br.built_binary(f"{d}/home-of/.cargo/bin/tool") and br.built_binary(f"{d}/home-of/.ours/copy"))
os.environ["HOME"] = old_home
def stored_cli(*paths):
    p = subprocess.run([sys.executable, "-I", path, "stored", *paths], capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr
rc, out = stored_cli(f"{d}/bin/true-linked")
check("stored: a store link gives rc 0 and the count", rc == 0 and "1 files, each a link to this PC's store" in out, (rc, out))
rc, out = stored_cli(f"{d}/bin/true-linked", f"{d}/bin/true-copy", f"{d}/nothing-here")
check("stored: a copy and a missing path give rc 1, a line each, and not the link",
      rc == 1 and f"{d}/bin/true-copy: not in the store" in out and f"{d}/nothing-here: not there" in out and "true-linked" not in out, (rc, out))
os.makedirs(f"{d}/relfolder/bin"); os.link(blob, f"{d}/relfolder/bin/coordd"); shutil.copy2(tb, f"{d}/relfolder/run.sh")
rc, out = stored_cli(f"{d}/relfolder")
check("stored FOLDER: its binaries, not its scripts: all links, rc 0", rc == 0 and "1 files" in out, (rc, out))
shutil.copy2("/usr/bin/true", f"{d}/relfolder/bin/yaaihh")
rc, out = stored_cli(f"{d}/relfolder")
check("stored FOLDER: a copied binary in it, named, rc 1", rc == 1 and f"{d}/relfolder/bin/yaaihh: not in the store" in out
      and "coordd" not in out, (rc, out))
rc, out = stored_cli()
check("stored with no path: the usage", rc == 1 and "build-runner stored PATH" in out, (rc, out))
rc, a, out = run(["--back", "", "--", tb, name])
check("--back with no release: the usage, no lines", isinstance(rc, str) and "--back RELEASE" in rc and a == [], (rc, a))
waiter = subprocess.Popen(["flock", f"{d}/replay.lock", "sleep", "2"]); time.sleep(0.3)
t = time.time(); rc, a, out = run(["--", tb, name])
check("a replay waits for the lock another holds (flock(1)'s, as the scripts take it)", time.time() - t >= 1.4 and "lock: held" in out, time.time() - t)
waiter.wait()
rc, a, out = run(["--", tb, name, "--exact"], {"DIE": "1"})
e = fields(a[1]) if len(a) == 2 else {}
check("a replay whose binary is killed (as at its memory cap): died, with the binary and SIGKILL, and not no tests run", rc == -9
      and e.get("died") == [{"binary": "coordd-test", "signal": 9, "name": "SIGKILL"}] and len(a) == 2
      and ", coordd-test died (signal 9, SIGKILL), " in a[1][2] and "no tests run" not in a[1][2], (rc, e, a[1:]))
os.makedirs(f"{d}/nogit", exist_ok=True); os.chdir(f"{d}/nogit")
rc, a, out = run(["--", tb, name])
os.chdir(wt)
check("run outside a worktree: refused, saying where to run it, and no lines (p1564 point 2)",
      isinstance(rc, str) and f"{d}/nogit is in no git worktree: run it from the worktree whose tree the binary is" in rc and a == [], (rc, a))
# the runner in a process of its own, as a script runs it, the binary a long replay that says its pid
wrapper = ("import importlib.machinery, importlib.util, sys\n"
           "spec = importlib.util.spec_from_loader('br', importlib.machinery.SourceFileLoader('br', sys.argv[1]))\n"
           "br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)\n"
           "br.job_agent = lambda pid: 'w1'; br.caller_coord = lambda pid: 'research'\n"
           "sys.exit(br.replay(sys.argv[2:]))\n")
def long_replay(more={"SLEEP": "30"}):
    try:
        os.remove(f"{d}/sleeping")
    except FileNotFoundError:
        pass
    k = len(acts())
    w = subprocess.Popen([sys.executable, "-I", "-c", wrapper, path, "--", tb, name], cwd=wt, env={**os.environ, **more},
                         stdout=open(f"{d}/w.out", "w"), stderr=subprocess.STDOUT)
    t = time.time()
    while not os.path.exists(f"{d}/sleeping") and time.time() - t < 10:
        time.sleep(0.05)
    time.sleep(0.1)
    return w, k, t
def gone(pid, wait=5):
    t = time.time()
    while time.time() - t < wait:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.05)
    return False
free = lambda: subprocess.run(["flock", "-n", f"{d}/replay.lock", "true"]).returncode == 0
w, k, t = long_replay()
w.send_signal(signal.SIGTERM)
w.wait(timeout=20)
t2 = time.time()  # the runner's process has ended; its lines' children land them after it
while len(ends(acts()[k:])) < 2 and time.time() - t2 < 10:
    time.sleep(0.05)
a = ends(acts()[k:])
e = fields(a[1]) if len(a) == 2 else {}
check("SIGTERM: it goes on to the binary, and the job still ends: rc -15, the binary died of SIGTERM (p1564 point 1)",
      len(a) == 2 and e.get("rc") == "-15" and e.get("died") == [{"binary": "coordd-test", "signal": 15, "name": "SIGTERM"}]
      and time.time() - t < 15, (a[1:], open(f"{d}/w.out").read()))
check("the lock is free once the stopped replay has ended", free())
w, k, t = long_replay({"ONTERM": "1"})
pid = int(open(f"{d}/sleeping").read() or 0)
w.send_signal(signal.SIGTERM)
try:
    w.wait(timeout=15); ended = True
except subprocess.TimeoutExpired:  # the runner waiting on a binary that waits on the full pipe
    ended = False; w.kill(); w.wait()
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    gone(pid)
t2 = time.time()
while ended and len(ends(acts()[k:])) < 2 and time.time() - t2 < 10:
    time.sleep(0.05)
a = ends(acts()[k:])
e = fields(a[1]) if len(a) == 2 else {}
wout = open(f"{d}/w.out").read()
check("SIGTERM to a binary that writes on as it ends, past a pipe's 64 KB: the runner reads it to its end, so the binary"
      " ends and its last count line counts: rc 0, 1 passed, the replay's counts (p1576 point 1)",
      ended and len(a) == 2 and e.get("rc") == "0" and e.get("passed") == "1" and e.get("problem_lines") == "0" and "died" not in e,
      (ended, a[1:], wout[-300:]))
check("what it wrote as it ended came through, to its last line", wout.count("a line it writes as it stops") == 4000
      and "test result: ok. 1 passed" in wout, (wout.count("a line it writes as it stops"), wout[-200:]))
check("the lock is free once it has ended", free())
w, k, t = long_replay()
pid = int(open(f"{d}/sleeping").read() or 0)
w.kill(); w.wait()
held = not free()
os.kill(pid, signal.SIGKILL); gone(pid)
check("the runner killed alone: the lock stays held while its binary still runs, and frees once it has ended (p1139)", held and free(), (held, pid))
killed = fields(acts()[k]).get("id") if len(acts()) > k else None
# storage step 2 (N-1446 p2727): --record NAME and no REPLAY_COORD, the runner's own view of NAME's live record
open(f"{d}/rec/research/project", "w").write("/flavor/projects/research/project.toml\n")
open(f"{d}/rec/research/tickets/alive.json", "w").write("{}\n")
live = open(f"{d}/rec/research/tickets/log.jsonl", "rb").read()
os.makedirs(f"{d}/home/replays/999999999-left")  # a view a runner left that died
views = {"REPLAY_COORD": "", "VIEW": "1"}
rc, a, out = run(["--record", "research", "--line", "2", "--", tb, name], views)
s = fields(a[0]) if a else {}
vp = open(f"{d}/viewpath").read().strip() if os.path.exists(f"{d}/viewpath") else ""
check("--record NAME --line N: the binary sees the record's first N lines and its project file, as this user, at REPLAY_COORD",
      rc == 0 and f"uid {os.getuid()} lines 2 project /flavor/projects/research/project.toml" in out and vp.startswith(f"{d}/home/replays/"), (rc, out))
check("REPLAY_LINES and REPLAY_BYTES say where the replay stops; no alive.json (a copy had none)",
      f"REPLAY_LINES 2 REPLAY_BYTES {len(live.split(b'\n')[0]) + len(live.split(b'\n')[1]) + 2}; alive.json there: no" in out, out)
check("its write went into its own layer: the live record is as it was", "view: written" in out
      and open(f"{d}/rec/research/tickets/log.jsonl", "rb").read() == live, out)
check("the view is gone once it has ended, and the one a dead runner left", os.listdir(f"{d}/home/replays") == [], os.listdir(f"{d}/home/replays"))
g = [fields(x) for x in acts() if fields(x).get("id") == killed and fields(x).get("phase") == "end"]
check("the job of the runner killed alone is ended by the next job, as gone (docket-edge5 H-1651 p3)",
      len(g) == 1 and "is gone with no end of its own" in g[0].get("gone", ""), (killed, g))
cut = len(live.split(b'\n')[0]) + len(live.split(b'\n')[1]) + 2
check("its lines: the record, its N lines, the live record it reads as `view`, when its line was taken, the bytes copied "
      "to make it (the cut record and the project file); no `copy`, since no caller made one (p2805)",
      s.get("record") == "research" and s.get("lines") == "2" and s.get("view") == f"{d}/rec/research"
      and re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ", s.get("view_at") or "")
      and s.get("copied_bytes") == str(cut + len("/flavor/projects/research/project.toml\n"))
      and not {"copy", "copy_at"} & set(s), s)
jf = json.load(open(sorted(br.JOBS_DIR.glob("*.json"), key=os.path.getmtime)[-1]))
ly = jf.get("layer") or {}
check("its job file holds its run layer (Home's Layers now, p2815, p2927): the view's own folder, when, and what lies "
      "under it: the record to its line, and the binary", ly.get("up") == vp[:-len("/view")] + "/up" and ly.get("at") == s.get("view_at")
      and ly.get("under") == [{"what": "record", "record": "research", "line": 2, "bytes": cut},
                              {"what": "binary", "path": tb, "md5": md5, "bytes": os.path.getsize(tb)}]
      and "layer" not in json.dumps(a), ly)
# the record's end as a live coord leaves it while it writes: an input (a post), its outputs so far, a line that does
# not read, and a last line not ended yet
open(f"{d}/rec/research/tickets/log.jsonl", "ab").write(
    b'{"ev":"post","t":1}\n{"ev":"posted","t":1}\n{"ev":"event","name":"delivery_opened","value":{},"fired":[]}\n'
    b'not a line that reads\n{"ev":"being written')
rc, a, out = run(["--record", "research", "--", tb, name], views)
s = fields(a[0]) if a else {}
check("no --line: its lines before the last input among its whole lines, as save_on cuts (N-1556 p3): not the input "
      "whose outputs are still being written, nor a line still being written",
      rc == 0 and "lines 3 project" in out and f"REPLAY_LINES 3 REPLAY_BYTES {len(live)};" in out and s.get("lines") == "3",
      (rc, out, s))
os.makedirs(f"{d}/rec/outputs/tickets")
open(f"{d}/rec/outputs/tickets/log.jsonl", "w").write('{"ev":"posted"}\n{"ev":"step"}\n{"ev":"post"')
big = (b'{"ev":"a"}\n{"ev":"post","pad":"' + b"x" * 600000 + b'"}\n'
       + b"".join(b'{"ev":"step","pad":"' + b"y" * 100000 + b'"}\n' for _ in range(10)))
open(f"{d}/big.jsonl", "wb").write(big)  # its input straddles the first block read back from its end (1 MiB)
check("an input read back across two blocks: the cut before it", br.record_upto(f"{d}/big.jsonl", None) == (1, 11),
      br.record_upto(f"{d}/big.jsonl", None))
open(f"{d}/first.jsonl", "wb").write(b'{"ev":"post"}\n{"ev":"posted"}\n')
check("the only input the first line: no line before it (save_on's cut)", br.record_upto(f"{d}/first.jsonl", None) == (0, 0),
      br.record_upto(f"{d}/first.jsonl", None))
check("an input: a post, an answer, an event that fired, a delivery event on a ticket, another event, a kind not listed",
      all(br.record_input(x) for x in (b'{"ev":"post"}', b'{"ev":"answer"}',
                                       b'{"ev":"event","name":"delivery_opened","value":1,"fired":[{"t":1}]}',
                                       b'{"ev":"event","name":"delivery_acted","value":1,"fired":[],"on":5}',
                                       b'{"ev":"event","name":"x","value":1,"fired":[]}', b'{"ev":"new_kind"}')))
check("no input: an output, a delivery event that fired nothing on no ticket, a line that does not read",
      not any(br.record_input(x) for x in (b'{"ev":"posted"}', b'{"ev":"closed"}', b'{"ev":"step_args"}',
                                           b'{"ev":"event","name":"delivery_acted","value":1,"fired":[]}',
                                           b'not json', b'[1]', b'{"ev":3}', b'{}', b'')))
# the outputs listed are those of the engine's Line that Line::is_input does not name, and the two delivery events
src = br.Path(path).resolve().parents[2] / "crates/docket/machine/src"
line_rs, syntax_rs = (src / "line.rs").read_text(), (src / "syntax.rs").read_text()
body, depth, variants = line_rs[line_rs.index("pub enum Line {") + len("pub enum Line {"):], 0, set()
for l in body.splitlines():
    if depth == 0 and l.startswith("}"):
        break
    if l.lstrip().startswith("//"):
        continue
    m = re.match(r"    ([A-Z][A-Za-z]*)\b", l)
    if depth == 0 and m:
        variants.add(re.sub(r"(?<!^)([A-Z])", r"_\1", m.group(1)).lower())
    depth += sum(l.count(o) - l.count(c) for o, c in ("{}", "()", "[]"))
named = line_rs[line_rs.index("pub fn is_input"):]
inputs = {re.sub(r"(?<!^)([A-Z])", r"_\1", x).lower()
          for x in re.findall(r"Line::([A-Z][A-Za-z]*) \{ \.\. \}", named[:named.index("\n    }\n")])}
check("the runner's outputs are the engine's Line variants that is_input does not name (line.rs)",
      len(variants) > 50 and inputs < variants and br.RECORD_OUTPUTS == variants - inputs,
      (sorted(br.RECORD_OUTPUTS ^ (variants - inputs)), len(variants), len(inputs)))
consts = dict(re.findall(r'pub const ([A-Z_]+): &str = "([^"]*)";', syntax_rs))
dl = re.search(r"pub fn delivery_output\(ev: &str\) -> bool \{\s*\[([A-Z_, ]+)\]\.contains", syntax_rs)
check("its delivery events are syntax::delivery_output's", dl is not None
      and br.RECORD_EVENT_OUTPUTS == {consts.get(x.strip()) for x in dl.group(1).split(",")}, dl and dl.group(1))
for argv, env, want in ((["--record", "research", "--line", "9"], views, "--line 9: research's record has 7 lines"),
                        (["--record", "outputs"], views, "outputs's record holds no input among its whole lines"),
                        (["--record", "nobody"], views, f"{d}/rec/nobody/tickets/log.jsonl is not there"),
                        (["--record", "../research"], views, "--record '../research' is no record's name"),
                        (["--record", "research", "--line", "2"], {**views, "REPLAY_COORD": f"{d}/rec/research"}, "--line N reads NAME's live record"),
                        (["--line", "2"], views, "--line N reads NAME's live record")):
    rc, a, out = run(argv + ["--", tb, name], env)
    check(f"{' '.join(argv)}: refused in words, no lines", isinstance(rc, str) and want in rc and a == [], (rc, a))
rc, a, out = run(["--commit", "nope", "--", tb, name])
check("--commit that is no commit: refused before anything, no lines", rc != 0 and rc is not None and a == [], (rc, a))
rc, a, out = run(["--record", "x"])
check("no binary: the usage, no lines", isinstance(rc, str) and "build-runner replay" in rc and a == [], (rc, a))
print(f"{fails} failed")
sys.exit(1 if fails else 0)
EOF
rc=$?
cat "$dir/out.log"; [ -s "$dir/err.log" ] && { echo "stderr:"; cat "$dir/err.log"; }
exit $rc
