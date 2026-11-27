"""v04-paths.py plan MAP | members | fix MAP ORIG: the path half of v04-layout.sh (owner N-1831)."""
import fnmatch, os, re, subprocess, sys

def read_map(path):
    rows = [l.split() for l in open(path) if l.strip() and not l.startswith("#")]
    return [(o, n) for o, n in rows]

def place(rows, p):
    """P's new place by the first row that takes it: the same path when none does, "-" when it goes."""
    for o, n in rows:
        if "*" in o:
            if fnmatch.fnmatchcase(p, o):
                return n if n == "-" else f"{n}/{os.path.basename(p)}"
        elif p == o or p.startswith(o + "/"):
            return n if n == "-" else n + p[len(o):]
    return p

def git(*a):
    return subprocess.run(["git", *a], check=True, capture_output=True, text=True).stdout

def plan(rows):
    for p in sys.stdin.read().splitlines():
        q = place(rows, p)
        if q != p:
            print(f"{p}\t{q}")

def members():
    crates = sorted(os.path.dirname(p) for p in git("ls-files", "crates").splitlines()
                    if re.fullmatch(r"crates/[^/]+(/[^/]+)?/Cargo\.toml", p)
                    and not os.path.isfile(os.path.join(os.path.dirname(os.path.dirname(p)), "Cargo.toml"))
                    and "[package]" in open(p).read())
    text = open("Cargo.toml").read()
    line = "members = [" + ", ".join(f'"{c}"' for c in crates) + "]"
    new = re.sub(r"^members = \[[^\]]*\]", lambda _: line, text, count=1, flags=re.M)
    if new != text:
        open("Cargo.toml", "w").write(new)

# Files whose words are history, data or outside the map's reach: never renamed in (relative paths are still re-aimed).
NO_RENAME = ("docs/archive/", "ops/dev/v04-")
NO_RENAME_ENDS = (".rs", ".jsonl", ".lock", ".png", ".gif", ".mp4", ".json")
# Where a path starts: the line's start, a space or quote, or a root a script names it from ($here/, ~/, a checkout).
ROOT = re.compile(r"(?:^|[\s\"'`(\[=:,^]|\$\{?\w+\}?\"?/|\{\w+\}/|\)/|~/|\./|soft-swarm[\w.-]*/|at/tree/)$")
# A root file (no / in its name) is renamed only after a root, never as a word in a sentence.
PATH_ROOT = re.compile(r"(?:\$\{?\w+\}?/|\)/|~/|\./|soft-swarm[\w.-]*/|at/tree/)$")
# Checkouts beside the tree's own files, in its folder but not in git: a path into them is a place too.
OUTSIDE = ("flavor", "proxy", "releases")
TAIL = re.compile(r"[A-Za-z0-9_-]|\.[A-Za-z0-9]")

# Exact edits the map cannot say: (file glob, old, new), each made where OLD is found and NEW is not yet.
EDITS = [
    ("ops/release/release.sh", 'here=$(cd "$(dirname "$0")" && pwd)', 'here=$(cd "$(dirname "$0")/../.." && pwd)'),
    ("ops/build-runner/tests/runner-*-test.sh", "$here/bin/build-runner", "$here/build-runner"),
    ("ops/build-runner/tests/runner-*-test.sh", 'BUILD_RUNNER_REMOTE_LIB="$here/bin"', 'BUILD_RUNNER_REMOTE_LIB="$here"'),
    ("ops/release/train-check.sh", '[ -d "$W/yaaihh" ]', '[ -d "$W/crates/yaaihh" ]'),
    ("ops/release/train-check.sh", " ../flavor/app/Cargo.toml", " flavor/app/Cargo.toml"),
    ("agent-base/tests/release-*-test.sh", 'cp "$HERE/ops/release/release.sh" "$T/ops/release/release.sh"',
     'mkdir -p "$T/ops/release" && cp "$HERE/ops/release/release.sh" "$T/ops/release/release.sh"'),
    ("ops/dev/README.md", "The switch page (`ops/tovrin-on-the-machine/README.md`)",
     "The switch page (Tovrin's own now; in this tree to 5915fb10a as `ops/tovrin-on-the-machine/README.md`)"),
    ("docket/README.md", "`stdlib/std/", "`std/"),
    ("docket/README.md", "`yaaihh/", "`defaults/"),
    ("docket/README.md", "`tovrin/", "`examples/tovrin/"),
    ("docket/README.md", "`examples/*.eflint`", "`examples/eflint/*.eflint`"),
    ("docket/README.md", "`examples/run.sh", "`examples/eflint/run.sh"),
    ("docket/scenarios/README.md", "`yaaihh/", "`defaults/"),
    ("crates/yaaihh/Cargo.toml", "the recipe in tries/speech-build/recipe", "ops/windows/speech/build.sh"),
    ("crates/symbols/Cargo.toml", "\n# a workspace of its own: not a member of yaaihh/'s\n[workspace]\n", "\n"),
]
# snapshot.py's reader of classifier-watch.py's report, whole with the blank lines before it
CLASSIFIER_ALARMS = """

def classifier_alarms(now):
    \"\"\"Sessions tools/classifier-watch.py found on the client fallback; an alarm too if it stopped writing.\"\"\"
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
"""
# Edits made on a file's text as it was, before its paths are re-aimed: code that joins the programs' folders by name.
SOURCE_EDITS = [
    ("crates/*/tests/*.rs", '.join("../../../docs/design")', '.join("../../../docket")'),
    ("crates/*/*/tests/*.rs", '.join("../../../docs/design")', '.join("../../../docket")'),
    ("crates/docket/v2/tests/programs.rs", 'walk(&design().join("phase2"), &mut v);\n    walk(&design().join("phase2-programs"), &mut v);',
     'walk(&design(), &mut v);'),
    ("crates/docket/v2/tests/programs.rs", 'p.starts_with(design().join("phase2")) && !p.starts_with(design().join("phase2-programs"))',
     'p.starts_with(design().join("gate"))'),
    ("crates/docket/v2/tests/programs.rs", '.filter(|p| p.starts_with(design().join("phase2-programs")))',
     '.filter(|p| !p.starts_with(design().join("gate")))'),
    ("crates/docket/v2/tests/programs.rs", 'partition(|p| !p.starts_with(design().join("phase2-programs")))',
     'partition(|p| p.starts_with(design().join("gate")))'),
    ("crates/coordd-v2/tests/gate.rs", '["phase2/gate", "phase2-programs/scenarios"]', '["gate", "scenarios"]'),
    ("crates/yaaihh/src/*/*.rs", '.join("../../../docs/design/phase2-programs")', '.join("../../../docket")'),
    ("crates/yaaihh/src/v2/boot.rs", '"stdlib/std/', '"std/'),
    ("crates/yaaihh/src/v2/boot.rs", '"yaaihh/', '"defaults/'),
    ("crates/yaaihh/src/policy_v2/page.rs", 'for dir in ["stdlib", "yaaihh"]', 'for dir in ["std", "defaults"]'),
    # classifier-watch.py goes, so its report's reader goes too
    ("agent-base/view/snapshot.py", "Also ~/agent-coord/.classifier.json (tools/classifier-watch.py): sessions whose\n"
     "auto-mode checks run on the client fallback become host alarms. Also the plan's", "Also the plan's"),
    ("agent-base/view/snapshot.py", '"alarms": classifier_alarms(now),', '"alarms": [],'),
    ("agent-base/view/snapshot.py", CLASSIFIER_ALARMS, ""),
]
# Old places code must not name after the moves: the script stops and lists each line that still does.
STALE = re.compile(r'docs/design/phase2|"phase2(-programs)?[/"]|classifier-watch|\.classifier\.json|tries/speech-build')
# The tree's folder yaaihh/ is the root now: a script's cd into it goes to the root.
CD_YAAIHH = re.compile(r'cd "(\$\{?\w+\}?)/yaaihh"')

def rename(rows, text):
    rules = sorted(((o, n) for o, n in rows if n != "-" and "*" not in o), key=lambda r: -len(r[0]))
    pat = re.compile("|".join(re.escape(o) for o, _ in rules))
    new_of = dict(rules)
    out, at = [], 0
    for m in pat.finditer(text):
        o, n, i = m.group(0), new_of[m.group(0)], m.start()
        before = text[max(0, i - 60):i]
        head = n[:-len(o)] if n.endswith(o) else None
        ok = (PATH_ROOT if "/" not in o else ROOT).search(before) and not TAIL.match(text[m.end():m.end() + 2])
        if ok and not (head and before.endswith(head)):
            out.append(text[at:i] + n)
            at = m.end()
    return "".join(out) + text[at:]

REL = re.compile(r"(?:(?<=\}/)|(?<=\"/)|(?<![A-Za-z0-9_.$/-]))(?:\.\./)+[A-Za-z0-9_{][A-Za-z0-9_.{}/*-]*")

def relink(rows, orig_files, orig_dirs, new_known, old, new, text):
    """Each ../ path re-aimed at the same known place, tried from its crate first after CARGO_MANIFEST_DIR or {x}/."""
    def crate_of(p, files):
        d = os.path.dirname(p)
        while d and f"{d}/Cargo.toml" not in files:
            d = os.path.dirname(d)
        return d
    def known_place(t):
        return not t.startswith("..") and (t in orig_files or t in orig_dirs or t.split("/")[0] in OUTSIDE
                                           or t in new_known or any(t == o or t.startswith(o + "/") for o in untracked))
    untracked = [o for o, _ in rows if "*" not in o and o not in orig_files and o not in orig_dirs]
    new_files = new_known
    lines = text.split("\n")
    for k, line in enumerate(lines):
        if ".." not in line:
            continue
        def one(m):
            rel = m.group(0)
            crate_first = "CARGO_MANIFEST_DIR" in line or new.endswith("Cargo.toml") or line[:m.start()].endswith("}/")
            bases = [(crate_of(old, orig_files), crate_of(new, new_files)), (os.path.dirname(old), os.path.dirname(new))]
            known = re.split(r"[{*$]", rel)[0].rstrip("/")
            for ob, nb in bases if crate_first else bases[::-1]:
                t = os.path.normpath(os.path.join(ob, known))
                if known_place(t):
                    to = os.path.relpath(place(rows, t), nb or ".")
                    return to + rel[len(known):] if to != "." else rel
            return rel
        lines[k] = REL.sub(one, line)
    return "\n".join(lines)

def fix(rows, orig):
    orig_files = set(git("ls-tree", "-r", "--name-only", orig).splitlines())
    orig_dirs = {os.path.dirname(p) for p in orig_files}
    orig_dirs |= {d for p in list(orig_dirs) for d in [p.rsplit("/", i)[0] for i in range(p.count("/") + 1)]}
    old_of = {place(rows, p): p for p in orig_files if place(rows, p) != "-"}
    new_files = set(git("ls-files").splitlines())
    new_known = new_files | {d for p in new_files for d in [p.rsplit("/", i)[0] for i in range(1, p.count("/") + 1)]}
    changed, stale = [], []
    for new in sorted(new_files):
        if not os.path.isfile(new) or os.path.islink(new):
            continue
        raw = open(new, "rb").read()
        if b"\0" in raw[:8192]:
            continue
        try:
            text = raw.decode()
        except UnicodeDecodeError:
            continue
        old = old_of.get(new, new)
        out = text
        for g, a, b in SOURCE_EDITS:
            if fnmatch.fnmatchcase(new, g):
                out = out.replace(a, b)
        out = relink(rows, orig_files, orig_dirs, new_known, old, new, out)
        if new.endswith(".rs") and not new.startswith(NO_RENAME):
            out = "\n".join(rename(rows, l) if l.lstrip().startswith("//") else l for l in out.split("\n"))
        elif not new.startswith(NO_RENAME) and not new.endswith(NO_RENAME_ENDS):
            out = CD_YAAIHH.sub(r'cd "\1"', rename(rows, out))
        for g, a, b in EDITS:
            if fnmatch.fnmatchcase(new, g) and a in out and not (a in b and b in out):
                out = out.replace(a, b)
        if out != text:
            open(new, "w").write(out)
            changed.append(new)
        if not new.startswith(NO_RENAME):
            stale += [f"{new}:{k}: {l.strip()[:120]}" for k, l in enumerate(out.split("\n"), 1) if STALE.search(l)]
    print(f"v04-paths: {len(changed)} files with paths renamed or re-aimed", file=sys.stderr)
    if stale:
        sys.exit("v04-paths: old places still named (add a SOURCE_EDITS row):\n" + "\n".join(stale))

if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "plan":
        plan(read_map(sys.argv[2]))
    elif cmd == "members":
        members()
    elif cmd == "fix":
        fix(read_map(sys.argv[2]), sys.argv[3])
