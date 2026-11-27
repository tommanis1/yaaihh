#!/usr/bin/env bash
# The store page's walk (paper-research N-1446 p2926, D3; plan p2928, answers p2929): `build-runner walk` looks in
# each agent's scratch folder and each worktree for what lies outside the store (record copies, binaries, target
# folders' files that are no store links, folders over 1 GB), names each folder's owner, keeps when it first found each
# item, writes $BUILD_RUNNER_HOME/outside.json and gives it as `store`'s `outside` row, a press on it a page of its
# items. It deletes nothing; `free` deletes one target folder or record copy it found, on his press. The service runs
# it at most hourly at idle priority. Each project's coord folder and repositories' worktrees are walked too. build-runner is loaded as a module;
# live records, scratch folders, a repository with two worktrees and a store of the test's own, in a mktemp folder,
# removed after. No service, no compile, no record of a coord.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/home/store/blobs" "$dir/records" "$dir/scratch"
export BUILD_RUNNER_HOME="$dir/home" BUILD_RUNNER_RECORDS="$dir/records" BUILD_RUNNER_WALK_SCRATCH="$dir/scratch" \
  BUILD_RUNNER_WALK_REPOS="$dir/repo" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
python3 -I - "$here/build-runner" "$dir" >"$dir/out.log" 2>"$dir/err.log" <<'EOF'
import hashlib, importlib.machinery, importlib.util, json, os, subprocess, sys, time
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
def write(p, data):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "wb").write(data)
    return p
def git(*a, cwd=None):
    return subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "user.name=t", "-c", "user.email=t@t.invalid", *a], cwd=cwd, capture_output=True, text=True, check=True).stdout
blocks = lambda p: os.lstat(p).st_blocks * 512
# two live records: research's, long; cav's, shorter than 4 KB but a whole line
line = lambda i: json.dumps({"ev": "post", "t": i, "text": "x" * 200}).encode() + b"\n"
research = b"".join(line(i) for i in range(100))
live = write(f"{d}/records/research/tickets/log.jsonl", research)
write(f"{d}/records/cav/tickets/log.jsonl", line(7))
# the store: a binary's blob, linked from a scratch folder and from a worktree's target
elf = lambda tag, n: b"\x7fELF" + tag + b"\0" * (n - 4 - len(tag))
linked = elf(b"linked", 2 << 20)
sha = hashlib.sha256(linked).hexdigest()
blob = write(f"{d}/home/store/blobs/{sha[:2]}/{sha}.x", linked)
# alpha's scratch folder: a whole copy of research's record, a cut one, cav's, a stand-in record, a link of the live
# record, a binary outside the store, one linked from it, a small one, a script, a release folder's binary
a = f"{d}/scratch/alpha"
copy = write(f"{a}/run1/coord/tickets/log.jsonl", research)
cut = write(f"{a}/run2/cut.jsonl", research[:5000])
cav = write(f"{a}/cav-copy.jsonl", line(7))
write(f"{a}/stand-in/tickets/log.jsonl", b"".join(json.dumps({"ev": "post", "t": i}).encode() + b"\n" for i in range(300)))
os.link(live, f"{a}/live-link.jsonl")
big = write(f"{a}/bin/coordd-copy", elf(b"copy", 3 << 20))
os.link(blob, f"{a}/bin/coordd-linked")
write(f"{a}/bin/small", elf(b"small", 4096))
write(f"{a}/bin/script.sh", b"#!/bin/sh\necho hi\n" + b"#" * (2 << 20))
rel = f"{a}/releases/20261124-193252-44ad6fb05"
write(f"{rel}/built-from.sha256", b"x\n")
relbin = write(f"{rel}/coordd", elf(b"release", 2 << 20))
# a scratch coord's never-served places: a copy and a binary there are never opened nor found (consult-lead p3077)
write(f"{a}/scoord/claude-home/projects/s.jsonl", research)
write(f"{a}/scoord/envdir/bin/tool", elf(b"envdir", 2 << 20))
write(f"{a}/scoord/registry.jsonl", b"{}\n")
write(f"{a}/scoord/route/key.jsonl", research)
write(f"{a}/scoord/.proxy-key.new.x1", elf(b"key", 2 << 20))
write(f"{a}/scoord/box/run/ffprof/places.jsonl", research)
os.makedirs(f"{d}/scratch/beta")  # nothing outside the store: not in the walk's row
# a repository and two worktrees: wt1's last commit says its agent; wt2's binaries came from delta's runner job
git("init", "-q", "-b", "main", f"{d}/repo")
write(f"{d}/repo/README", b"r\n")
git("add", "README", cwd=f"{d}/repo")
git("commit", "-q", "-m", "first\n\nAgent: owner-x", cwd=f"{d}/repo")
for w, msg in (("wt1", "wt1's\n\nAgent: gamma"), ("wt2", "wt2's\n\nAgent: gamma")):
    git("worktree", "add", "-q", "-b", w, f"{d}/{w}", cwd=f"{d}/repo")
    git("commit", "-q", "--allow-empty", "-m", msg, cwd=f"{d}/{w}")
tag = write(f"{d}/wt1/yaaihh/target/CACHEDIR.TAG", b"Signature: 8a477f597d28d172789f06886806bc55\n")
src_target = write(f"{d}/wt1/yaaihh/src/target/mod.rs", b"// a source folder named target\n" * 400)
t1 = write(f"{d}/wt1/yaaihh/target/quick/deps/coordd-0123", elf(b"local build", 1 << 20))
t2 = write(f"{d}/wt1/yaaihh/target/quick/deps/coordd-0123.d", b"deps\n" * 100)
os.link(blob, f"{d}/wt1/yaaihh/target/quick/coordd")
write(f"{d}/wt2/Cargo.toml", b"[workspace]\n")
t3 = write(f"{d}/wt2/target/debug/x", b"x" * 9000)
with open(f"{d}/home/store/binaries.jsonl", "w") as f:
    f.write(json.dumps({"at": 1, "blob": sha + ".x", "dest": f"{d}/wt2/target/debug/y", "who": "epsilon"}) + "\n")
    f.write(json.dumps({"at": 2, "blob": sha + ".x", "dest": f"{d}/wt2/target/debug/y", "who": "delta"}) + "\n")
    f.write(json.dumps({"at": 3, "blob": sha + ".x", "dest": f"{d}/wt22/target/y", "who": "zeta"}) + "\n")  # no worktree's
    f.write(json.dumps({"at": 0, "blob": "b" * 64 + ".x", "dest": f"{d}/wt2/target/debug/z", "who": "eta"}) + "\n")  # older, a later blob
    f.write(json.dumps({"at": 4, "blob": "c" * 64, "dest": None, "who": "theta"}) + "\n")  # a line with no dest
json.dump({f"{d}/repo": "paper-research"}, open(f"{d}/home/walk-owners.json", "w"))
write(f"{d}/repo/target/CACHEDIR.TAG", b"x\n")
write(f"{d}/repo/target/old", b"o" * 5000)
# the walk, as the verb
r = subprocess.run([sys.executable, "-I", path, "walk"], capture_output=True, text=True)
check("`build-runner walk` walks every scratch folder and worktree once and says what it found and where it wrote it",
      r.returncode == 0 and "5 folders" in r.stdout and "in 4 folders" in r.stdout and "outside.json" in r.stdout, (r.returncode, r.stdout, r.stderr))
o = json.load(open(f"{d}/home/outside.json"))
by = {f["path"]: f for f in o["folders"]}
check("the folders that hold anything outside the store, and only those (beta's empty folder is left out)",
      set(by) == {a, f"{d}/wt1", f"{d}/wt2", f"{d}/repo"}, sorted(by))
fa = by.get(a, {"items": []})
items = {(i["what"], i["path"]): i for i in fa["items"]}
check("a whole copy of a live record, a cut one and a short record's copy are each found, with the record they copy",
      items.get(("copy", copy), {}).get("of") == "research" and items.get(("copy", cut), {}).get("of") == "research"
      and items.get(("copy", cav), {}).get("of") == "cav", sorted(items))
check("a stand-in record and a link of the live record itself are no copies",
      not any(k[1].endswith(("stand-in/tickets/log.jsonl", "live-link.jsonl")) for k in items), sorted(items))
check("a binary outside the store is found, and one in a release folder says so; one linked from the store, one under "
      "1 MB and a script are not",
      ("binary", big) in items and items.get(("binary", relbin), {}).get("release") is True and "release" not in items[("binary", big)]
      and not any(k[1].endswith(("coordd-linked", "small", "script.sh")) for k in items), sorted(items))
check("nothing under a coord's homes, keys, own route or the browser's profile is found (coordd's never served)",
      not any("/scoord/" in k[1] for k in items), sorted(k for k in items if "/scoord/" in k[1]))
check("a scratch folder is its agent's; its counts and the bytes outside the store, the largest item first",
      fa.get("owner") == "alpha" and (fa.get("copies"), fa.get("binaries"), fa.get("targets")) == (3, 2, 0) and fa.get("kind") == "scratch"
      and fa.get("bytes") == sum(blocks(p) for p in (copy, cut, cav, big, relbin))
      and [i["bytes"] for i in fa["items"]] == sorted((i["bytes"] for i in fa["items"]), reverse=True), fa)
w1 = by.get(f"{d}/wt1", {"items": []})
check("a source folder only named `target` is no target folder: cargo's mark (CACHEDIR.TAG) or a Cargo.toml beside it",
      not any(i["path"].startswith(f"{d}/wt1/yaaihh/src") for i in w1["items"]), w1)
check("a worktree's target folder: the files in it that are no store links, counted as one item with their bytes; a "
      "worktree whose runner jobs say no one is its last commit's `Agent:`",
      w1.get("owner") == "gamma" and w1.get("kind") == "worktree" and w1["items"] == [{"what": "target", "path": f"{d}/wt1/yaaihh/target",
      "bytes": blocks(t1) + blocks(t2) + blocks(tag), "files": 3, "since": o["at"]}], w1)
check("a worktree the runner's jobs handed binaries into is the agent's of the newest of them (the binaries' index), "
      "across blobs: an older job on a blob listed later does not win; a line with no dest is passed over",
      by.get(f"{d}/wt2", {}).get("owner") == "delta", by.get(f"{d}/wt2"))
check("an owner no rule finds is named in walk-owners.json (main's own folder, p2928)",
      by.get(f"{d}/repo", {}).get("owner") == "paper-research", by.get(f"{d}/repo"))
check("the folders, the largest outside the store first, and the total of all of them",
      [f["bytes"] for f in o["folders"]] == sorted((f["bytes"] for f in o["folders"]), reverse=True)
      and o["total_bytes"] == sum(f["bytes"] for f in o["folders"]) and o["walked"]["folders"] == 5, o)
check("nothing is deleted or moved", all(os.path.exists(p) for p in (copy, cut, cav, big, relbin, t1, t2, t3)))
# again, with one new copy: what was found before keeps when it was found; the new one is new, and its folder changed
first = o["at"]
time.sleep(1.1)
new = write(f"{a}/run3/again.jsonl", research)
br.walk()
o2 = json.load(open(f"{d}/home/outside.json"))
fa2 = {f["path"]: f for f in o2["folders"]}[a]
since = {i["path"]: i["since"] for i in fa2["items"]}
w12 = {f["path"]: f for f in o2["folders"]}[f"{d}/wt1"]
check("a second walk: each item found before keeps its `since`, a new copy has the second walk's, and its folder's "
      "`changed` is that; a folder with nothing new keeps its own",
      o2["at"] != first and since.get(copy) == first and since.get(new) == o2["at"] and fa2["changed"] == o2["at"]
      and w12["changed"] == first, (first, o2["at"], since, w12.get("changed")))
# a folder over 1 GB (here: over the test's bound) is a finding of its own
br.OUTSIDE_OVER = 100_000
br.walk()
fb = {f["path"]: f for f in json.load(open(f"{d}/home/outside.json"))["folders"]}
check("a folder over 1 GB is an item of its own (`folder`, its size), and the folder says `over`; its bytes outside the "
      "store do not count it",
      any(i["what"] == "folder" and i["path"] == a and i["bytes"] == fb[a]["folder_bytes"] for i in fb[a]["items"])
      and fb[a]["over"] is True and fb[a]["bytes"] == fa2["bytes"] and fb[f"{d}/wt2"]["over"] is False, fb[a])
br.OUTSIDE_OVER = 2**30
br.walk()
# the finding to its owner (paper-research N-1446 p3061): one line on the owning agent's own job output, once per change
n1 = br.outside_note("alpha")
check("an agent's job ends with a line on what its folders hold outside the store, the first time",
      n1 is not None and "4 record copies, 2 binaries" in n1 and "in 1 folder," in n1 and "build-runner store outside" in n1, n1)
check("and not again while nothing is new or changed; an agent with no folder in the walk gets none",
      br.outside_note("alpha") is None and br.outside_note("nobody") is None and br.outside_note(None) is None
      and br.outside_note("gamma") is not None and br.outside_note("gamma") is None)
time.sleep(1.1)
write(f"{a}/run4/more.jsonl", research)
br.walk()
n2 = br.outside_note("alpha")
check("a new finding in its folder: the line again, with it counted", n2 is not None and "5 record copies" in n2, n2)
os.unlink(f"{a}/run4/more.jsonl")
br.walk()
# Home's row and a press on it
def cli(*args):
    p = subprocess.run([sys.executable, "-I", path, "store", *args], capture_output=True, text=True)
    try:
        return p.returncode, json.loads(p.stdout)
    except ValueError:
        return p.returncode, p.stdout + p.stderr
rc, s = cli()
row = next((x for x in (s.get("rows") or []) if x.get("id") == "outside"), None) if isinstance(s, dict) else None
o3 = json.load(open(f"{d}/home/outside.json"))
check("`store` gives the walk's row: when, the total, and each folder without its items but with where they start in a "
      "press (`from`, `count`)",
      rc == 0 and row is not None and row["what"] == "Outside the store" and row["at"] == o3["at"] and row["total_bytes"] == o3["total_bytes"]
      and [f["path"] for f in row["folders"]] == [f["path"] for f in o3["folders"]] and all("items" not in f for f in row["folders"])
      and [f["from"] for f in row["folders"]] == [sum(len(g["items"]) for g in o3["folders"][:i]) for i in range(len(o3["folders"]))], (rc, s))
rc, pg = cli("outside", "--from", "0")
fr = next((f for f in (row or {}).get("folders", []) if f["path"] == a), {"from": 0, "count": 0})
mine = pg.get("entries", [])[fr["from"]:fr["from"] + fr["count"]] if isinstance(pg, dict) else []
check("a press: the items folder by folder, each with its folder and owner; a folder's `from` and `count` are its own",
      rc == 0 and pg["total"] == sum(len(f["items"]) for f in o3["folders"]) and mine and all(i["folder"] == a and i["owner"] == "alpha" for i in mine)
      and len(mine) == len({f["path"]: f for f in o3["folders"]}[a]["items"]), (rc, pg))
rc, h = cli("--here")
check("the build machine's side (`--here`) has no walk's row, nor a press on it",
      rc == 0 and not any(x.get("id") == "outside" for x in h.get("rows", [])) and cli("--here", "outside")[0] == 1, (rc, h))
os.unlink(f"{d}/home/outside.json")
rc, s = cli()
rc2, pg = cli("outside")
check("before the first walk: no row, and a press says there is no walk yet",
      rc == 0 and not any(x.get("id") == "outside" for x in s.get("rows", [])) and rc2 == 1 and "no walk yet" in pg.get("error", ""), (s, pg))
# D1's check reads the live records as the walk does
check("D1's check, on the same reading of the live records: a coord holding a copy, the live one, a stand-in",
      br.live_record_of(f"{a}/run1/coord") == "research" and br.live_record_of(f"{d}/records/research") == "research"
      and br.live_record_of(f"{a}/stand-in") is None)
# each project of this PC (the owner's walk, items 20 and 23): its coord folder walked as its own (its record live, its
# homes never), its repositories' worktrees, and each folder's project; the projects' parts and the whole
write(f"{d}/proj/alpha.toml", ('[project]\nrepos = { main = "' + f"{d}/repo" + '", other = "~/no-such-folder-x" }\n').encode())
write(f"{d}/proj/cavp.toml", b'[project]\nname = "cav"\n')
write(f"{d}/records/research/project", f"{d}/proj/alpha.toml\n".encode())
write(f"{d}/records/research/registry.jsonl", b'{"ev":"add","name":"alpha"}\n{"ev":"add","name":"gamma"}\n{"ev":"session","name":"zeta"}\n')
write(f"{d}/records/cav/project", f"{d}/proj/cavp.toml\n".encode())
write(f"{d}/records/cav/registry.jsonl", b'{"ev":"add","name":"delta"}\n')
write(f"{d}/records/cav/notes/big.txt", b"n" * 50_000)
write(f"{d}/records/cav/claude-home/x.jsonl", b"h" * 50_000)
write(f"{d}/records/odd/registry.jsonl", b'{"ev":"add","name":"delta"}\n')  # no project file: no project
os.makedirs(f"{d}/wt3"); git("worktree", "add", "-q", "-b", "wt3", f"{d}/elsewhere", cwd=f"{d}/repo")
prs = {pr["id"]: pr for pr in br.walk_projects()}
check("the projects: each coord folder whose `project` file names a project.toml, its repositories (`~` read as home; "
      "one that is no folder kept, walked as nothing) and the agents its registry added",
      sorted(prs) == ["cav", "research"] and prs["research"]["repos"][0] == br.Path(f"{d}/repo") and prs["research"]["agents"] == {"alpha", "gamma"}
      and prs["cav"]["agents"] == {"delta"} and prs["cav"]["repos"] == [], prs)
br.OUTSIDE_OVER = 40_000
br.walk()
o4 = json.load(open(f"{d}/home/outside.json"))
f4 = {f["path"]: f for f in o4["folders"]}
check("a project's coord folder is a folder of the walk (`coord`), of no agent; its live record is no copy, its homes are "
      "not walked, so the folder counts its notes and record alone",
      f4.get(f"{d}/records/cav", {}).get("kind") == "coord" and f4[f"{d}/records/cav"]["owner"] == ""
      and f4[f"{d}/records/cav"]["project"] == "cav" and f4[f"{d}/records/cav"]["folder_bytes"] < 100_000
      and not any(i["what"] == "copy" for i in f4[f"{d}/records/cav"]["items"]), f4.get(f"{d}/records/cav"))
check("a worktree of a project's repository is that project's, one its registry does not know too (delta's wt2); a "
      "worktree added anywhere is walked; a scratch folder is its agent's project's, none when no registry adds it",
      f4.get(f"{d}/wt1", {}).get("project") == "research" and f4.get(f"{d}/wt2", {}).get("project") == "research"
      and f4.get(a, {}).get("project") == "research" and o4["walked"]["folders"] == 8, {k: v.get("project") for k, v in f4.items()})
parts = {x["id"]: x for x in o4["projects"]}
check("each project's part: its folders' bytes outside the store, their size and count; the whole's size beside the total",
      parts["research"]["bytes"] == sum(f["bytes"] for f in o4["folders"] if f["project"] == "research")
      and parts["cav"]["folders"] == 1 and o4["folder_bytes"] == sum(f["folder_bytes"] for f in o4["folders"])
      and [x["folder_bytes"] for x in o4["projects"]] == sorted((x["folder_bytes"] for x in o4["projects"]), reverse=True), o4["projects"])
row = next(x for x in br.store_answer()["rows"] if x.get("id") == "outside")
check("Home's row carries the projects' parts and the whole", row["projects"] == o4["projects"] and row["folder_bytes"] == o4["folder_bytes"], row)
br.OUTSIDE_OVER = 2**30
# a press on Home deletes one finding (the owner's walk, item 20: "a way to deal with it"): a target folder or a record
# copy the last walk named, and nothing else
br.walk()
o5 = json.load(open(f"{d}/home/outside.json"))
w1t = f"{d}/wt1/yaaihh/target"
tb = next(i["bytes"] for f in o5["folders"] for i in f["items"] if i["path"] == w1t)
got = br.free(w1t)
o6 = json.load(open(f"{d}/home/outside.json"))
check("a target folder the walk found is deleted, whole; the row is written again without it, its bytes off its folder, "
      "its project and the whole, and a folder left with nothing found leaves the row",
      got == {"freed": tb, "path": w1t} and not os.path.exists(w1t) and os.path.exists(f"{d}/wt1/yaaihh/src/target/mod.rs")
      and o6["total_bytes"] == o5["total_bytes"] - tb and f"{d}/wt1" not in {f["path"] for f in o6["folders"]}
      and next(x for x in o6["projects"] if x["id"] == "research")["bytes"] == next(x for x in o5["projects"] if x["id"] == "research")["bytes"] - tb, (got, o6["total_bytes"]))
got = br.free(copy)
check("a record copy the walk found is deleted, the live record and the other copies kept",
      got.get("freed") and not os.path.exists(copy) and os.path.exists(live) and os.path.exists(cut), got)
refused = lambda pth, says: (lambda g: says in g.get("error", "") and "freed" not in g)(br.free(pth))
check("refused: a binary (not deleted from here), a path the walk did not find, one found but no longer cargo's, and "
      "a path through a link",
      refused(big, "not deleted from here") and os.path.exists(big) and refused(f"{d}/wt1/README", "not one of the last walk's")
      and (os.unlink(f"{d}/wt2/Cargo.toml") or refused(f"{d}/wt2/target", "no cargo build folder")) and os.path.exists(f"{d}/wt2/target"))
os.symlink(f"{d}/wt2", f"{d}/wt2link")
o7 = json.load(open(f"{d}/home/outside.json"))
o7["folders"].append({"path": f"{d}/wt2link", "kind": "worktree", "owner": "x", "project": "", "bytes": 1, "folder_bytes": 1,
                      "targets": 1, "copies": 0, "binaries": 0, "items": [{"what": "target", "path": f"{d}/wt2link/target", "bytes": 1}]})
json.dump(o7, open(f"{d}/home/outside.json", "w"))
check("a finding whose path goes through a link is refused, and its target kept",
      refused(f"{d}/wt2link/target", "not a plain path") and os.path.exists(f"{d}/wt2/target"))
r = subprocess.run([sys.executable, "-I", path, "free", f"{d}/nowhere"], capture_output=True, text=True)
check("`build-runner free PATH` says the same as JSON, rc 1 when refused", r.returncode == 1 and "error" in json.loads(r.stdout), r.stdout)
os.unlink(f"{d}/home/outside.json")
# the service's walk: at most every OUTSIDE_SEC, a process of its own at idle priority
calls, sleeps = [], []
class Stop(Exception): pass
real_run, real_sleep = br.subprocess.run, br.time.sleep
def fake_run(argv, **kw):
    calls.append(argv)
    return subprocess.CompletedProcess(argv, 0, "build-runner walk: ...", "")
def fake_sleep(s):
    sleeps.append(s)
    if len(sleeps) >= 2:
        raise Stop
    json.dump({"folders": []}, open(f"{d}/home/outside.json", "w"))  # as the walk would
br.subprocess.run, br.time.sleep = fake_run, fake_sleep
try:
    br.walk_hourly()
except Stop:
    pass
finally:
    br.subprocess.run, br.time.sleep = real_run, real_sleep
check("the service's walk: none written yet, so it walks at once, as `nice -n 19 ionice -c 3 ... walk`, then waits an "
      "hour; a walk an instant old is not walked again, and the wait is what is left of the hour",
      len(calls) == 1 and calls[0][:5] == ["/usr/bin/nice", "-n", "19", "/usr/bin/ionice", "-c"] and calls[0][-1] == "walk"
      and sleeps[0] == br.OUTSIDE_SEC and br.OUTSIDE_SEC - 5 < sleeps[1] <= br.OUTSIDE_SEC, (calls, sleeps))
sys.exit(1 if fails else 0)
EOF
rc=$?
cat "$dir/out.log"
[ -s "$dir/err.log" ] && { echo "stderr:"; tail -20 "$dir/err.log"; }
echo "$(grep -c '^ok' "$dir/out.log") ok, $(grep -c '^not ok' "$dir/out.log") not ok"
exit $rc
