#!/usr/bin/env bash
# A value a caller passes on with --env names its worktree's paths; a job runs in a view of its tree (piece B), so each
# such path, anywhere in the value, is the same path under the job's tree (docket-core N-1446 p3118: windows-speech/
# env.sh's CFLAGS `-include WT/…/mingw11-throttling.h` failed every Windows exe after the 20:51Z restart). build-runner
# is loaded as a module; no service, no compile.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
python3 -I - "$here/build-runner" <<'PY'
import importlib.machinery, importlib.util, sys
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", sys.argv[1]))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, got, want):
    global fails
    ok = got == want
    print(("ok - " if ok else "not ok - ") + what + ("" if ok else f": {got!r}"))
    fails += not ok
wt, t = "/home/t/Documents/papers/soft-swarm-docket-train-36", "/build/at/tree"
check("CFLAGS' -include of a header in the worktree: the header under the job's tree",
      br.in_tree(f"-include {wt}/ops/windows/speech/mingw11-throttling.h -O2", wt, t), f"-include {t}/ops/windows/speech/mingw11-throttling.h -O2")
check("the worktree itself, alone, and two paths in one value", br.in_tree(f"{wt}", wt, t) + "|" + br.in_tree(f"{wt}/a:{wt}/b", wt, t), f"{t}|{t}/a:{t}/b")
check("after an =, and quoted", br.in_tree(f'-I={wt}/inc "{wt}/x y"', wt, t), f'-I={t}/inc "{t}/x y"')
check("a sibling whose name only starts as the worktree's is left as it is", br.in_tree(f"{wt}-old/x {wt}2", wt, t), f"{wt}-old/x {wt}2")
check("a value with no worktree path, and one that is no text, as they are", (br.in_tree("-O2 /usr/include", wt, t), br.in_tree(["a"], wt, t)), ("-O2 /usr/include", ["a"]))
check("the job's environment is made with it", "in_tree(v, wt, str(AT_TREE)) for k, v in cenv.items()" in open(sys.argv[1]).read(), True)
print(f"{fails} failed")
sys.exit(1 if fails else 0)
PY
