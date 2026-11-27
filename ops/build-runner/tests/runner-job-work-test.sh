#!/usr/bin/env bash
# A runner job names its work in its file (paper-research N-1446 p1372, the owner's N-1104 p764): the caller's side
# (build-runner's JobFile, the file a page reads, N-1334) writes `items`, the commit's Item: trailers, and `tree`, the
# git tree of the tracked files it sent as they were in the worktree, made in a copy of the worktree's index: the
# commit's tree when nothing tracked changed, else the tree a later commit of that state has; the worktree's own
# index and status untouched; an untracked file not in it; `changed`, how many files that tree holds the commit does
# not. The same work goes in the job's lines in its coord's record (runner-job-act-test.sh, p1386). JobFile is loaded
# from build-runner as a module, with a stand-in coordc; no service is started and no coordd is asked. Everything in a
# mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/ab/bin" "$dir/wt" "$dir/plain"
printf '#!/bin/sh\necho "$@" >> "%s/acts"\n' "$dir" >"$dir/ab/bin/coordc"; chmod +x "$dir/ab/bin/coordc"
g() { git -C "$dir/wt" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@t.invalid "$@"; }
g init -q -b work && echo a >"$dir/wt/a" && echo b >"$dir/wt/b" && g add a b && g commit -q -m $'the work\n\nItem: I-7\nItem: N-1104.p678\nCo-Authored-By: x <x@x.invalid>'
export BUILD_RUNNER_HOME="$dir/home" GIT_CONFIG_NOSYSTEM=1
AGENT_BASE="$dir/ab" python3 -I - "$here/build-runner" "$dir" <<'EOF'
import hashlib, importlib.machinery, importlib.util, json, os, subprocess, sys
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
wt = f"{d}/wt"
def git(*a):
    return subprocess.run(["git", "-C", wt, "-c", "core.hooksPath=/dev/null", "-c", "user.name=t", "-c", "user.email=t@t.invalid", *a],
                          capture_output=True, text=True).stdout.strip()
def job(where=wt):
    c = git("rev-parse", "--short=8", "HEAD") if where == wt else None
    j = br.JobFile({"args": ["test", "-p", "coordd"]}, where, {"branch": "work", "commit": c}, "w1", "research")
    j.step("queued")  # the file is written at a job's first step
    return j
def filed(j):
    return json.load(open(j.path))
idx = os.path.join(wt, git("rev-parse", "--git-path", "index"))
j = job(); f = filed(j)
check("the commit's Item: trailers, in order", f.get("items") == ["I-7", "N-1104.p678"], f.get("items"))
check("nothing tracked changed: the commit's tree", f.get("tree") == git("rev-parse", "HEAD^{tree}"), (f.get("tree"), git("rev-parse", "HEAD^{tree}")))
check("nothing tracked changed: 0 changed", f.get("changed") == 0, f.get("changed"))
open(f"{wt}/a", "a").write("changed\n"); open(f"{wt}/new", "w").write("untracked\n")
before = (hashlib.sha1(open(idx, "rb").read()).hexdigest(), git("status", "--short"))
j = job(); f = filed(j)
check("a tracked file changed: not the commit's tree", f.get("tree") not in (None, git("rev-parse", "HEAD^{tree}")), f.get("tree"))
check("a tracked file changed: 1 changed (the untracked one not counted)", f.get("changed") == 1, f.get("changed"))
check("the worktree's own index and status untouched", (hashlib.sha1(open(idx, "rb").read()).hexdigest(), git("status", "--short")) == before, git("status", "--short"))
git("add", "-u"); git("commit", "-q", "-m", "the change\n\nItem: I-8")
check("the commit that takes that state whole has the job's tree", git("rev-parse", "HEAD^{tree}") == f.get("tree"), (git("rev-parse", "HEAD^{tree}"), f.get("tree")))
check("the untracked file is not in it", "new" not in git("ls-tree", "--name-only", f.get("tree") or "HEAD"))
f = filed(job())
check("the next job names the new commit's item", f.get("items") == ["I-8"], f.get("items"))
git("commit", "-q", "--allow-empty", "-m", "none")
f = filed(job())
check("a commit with no Item: gives []", f.get("items") == [], f.get("items"))
j = job(f"{d}/plain"); f = filed(j)
check("outside git: no tree, no items, the file written", f.get("tree") is None and f.get("items") == [] and f.get("changed") is None and f.get("verb") == "test", f)
j = job(); j.end({"passed": 3, "failed": 1}, 101); f = filed(j)
check("its end as before: ended, rc, counts, with its work", f.get("step") == "ended" and f.get("rc") == 101 and f.get("passed") == 3
      and f.get("failed") == 1 and f.get("tree") == git("rev-parse", "HEAD^{tree}") and f.get("items") == [], f)
print(f"{fails} failed")
sys.exit(1 if fails else 0)
EOF
