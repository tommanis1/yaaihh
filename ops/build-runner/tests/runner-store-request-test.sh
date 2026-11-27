#!/usr/bin/env bash
# Home's Store (a2h-speed N-1446 p2815; docket-edge8's names p2904, agreed p2927): `build-runner store [ROW] [--from N]`
# and the service's {"store": {"row", "from"}} (what coordd's Request::Store asks) answer what the store holds as JSON:
# the build machine's compiled crates (read from it over its ssh, at most once a minute, with its name as `device`) and
# this PC's binaries (take_back's part, its most-linked programs named from the index take_back writes); a press on a
# row, its entries, a page at a time, newest use first. build-runner is loaded as a module; the build machine is a
# second runner home of the test's own, reached through an ssh stand-in that runs the command here. No service, no
# compile, no record. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/home" "$dir/rh" "$dir/wt"
cat >"$dir/ssh" <<EOF
#!/usr/bin/env bash
# the build machine's ssh, here: HOST, then the command; it says each call, answers 255 while $dir/down is there, and
# takes 2 s while $dir/slow is
echo "\$2" >>"$dir/ssh.log"
[ -e "$dir/down" ] && exit 255
[ -e "$dir/slow" ] && sleep 2
exec sh -c "\$2"
EOF
chmod +x "$dir/ssh"
export BUILD_RUNNER_HOME="$dir/home" BUILD_RUNNER_REMOTE_HOME="$dir/rh" BUILD_RUNNER_REMOTE_LIB="$here" \
  BUILD_RUNNER_SSH="$dir/ssh" GIT_CONFIG_NOSYSTEM=1
python3 -I - "$here/build-runner" "$dir" >"$dir/out.log" 2>"$dir/err.log" <<'EOF'
import hashlib, importlib.machinery, importlib.util, json, os, socket, subprocess, sys, threading, time
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
def blob(store, content, ex):
    sha = hashlib.sha256(content).hexdigest()
    name = sha + (".x" if ex else "")
    p = f"{store}/blobs/{sha[:2]}/{name}"
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "wb").write(content)
    return sha, name, p
def entry(store, key, kind, outs, by, at, used, inputs):
    p = f"{store}/entries/{kind}/{key[:2]}/{key}.json"
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "w").write(json.dumps([{"inputs": inputs, "env": {}, "out": outs, "stderr": [], "stdout": [], "by": by, "at": at}]))
    os.utime(p, (used, used))
# the build machine's store: two entries, a registry crate's and one of ours, their blobs, its runs of today and before
rs = f"{d}/rh/store"
now = time.time()
s1, _, _ = blob(rs, b"serde rlib" * 100, False)
s2, _, _ = blob(rs, b"coordd test binary" * 1000, True)
s3, _, _ = blob(rs, b"coordd deps" * 10, False)
entry(rs, "a" * 64, "rustc", {"libserde-0123456789abcdef.rlib": [s1, False]}, {"who": "docket-core", "branch": "eng-79", "commit": "9755019f", "wt": "/x", "train": False},
      1795500000, now - 3600, {"/home/t/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde-1.0.217/src/lib.rs": "h"})
entry(rs, "b" * 64, "rustc", {"coordd-fedcba9876543210": [s2, True], "coordd-fedcba9876543210.d": [s3, False]},
      {"who": "a2h-speed", "branch": "a2h-store-request", "commit": "1234abcd"}, 1795540000, now - 60,
      {"/home/t/agent-coord/build-runner/at/tree/crates/coordd/src/main.rs": "h"})
fmt = lambda t: time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(t))
since = br.day_start()
with open(f"{rs}/runs.jsonl", "w") as r:
    r.write(json.dumps({"at": fmt(since - 7200), "hit": 50, "miss": 50, "pass": 0}) + "\n")  # yesterday's: not today's
    r.write("not a line that reads\n")
    r.write(json.dumps({"at": fmt(since + 60), "hit": 97, "miss": 2, "pass": 1}) + "\n")
    r.write(json.dumps({"at": fmt(since + 120), "hit": 3, "miss": 0, "pass": 0}) + "\n")
crates_total = os.path.getsize(f"{rs}/blobs/{s1[:2]}/{s1}") + os.path.getsize(f"{rs}/blobs/{s2[:2]}/{s2}.x") + os.path.getsize(f"{rs}/blobs/{s3[:2]}/{s3}")
def cli(*args, env=None):
    p = subprocess.run([sys.executable, "-I", path, "store", *args], capture_output=True, text=True, env={**os.environ, **(env or {})})
    try:
        return p.returncode, json.loads(p.stdout)
    except ValueError:
        return p.returncode, p.stdout + p.stderr
# the build machine's side, `store --here`, as the client asks it over ssh
rc, a = cli("--here", "--since", str(since), env={"BUILD_RUNNER_HOME": f"{d}/rh", "BUILD_RUNNER_TRUST_AGENT": "1"})
row = (a.get("rows") or [{}])[0] if isinstance(a, dict) else {}
check("the build machine's own rows: its compiled crates, held against its cap, its entries, today's jobs and compiles "
      "and those from the store (runs.jsonl since the day's start; a hit, a miss and one passed by each a compile)",
      rc == 0 and len(a["rows"]) == 1 and row == {"id": "crates", "what": "Compiled crates", "held_bytes": crates_total,
      "cap_bytes": 60 * 2**30, "count": 2, "count_of": "entries", "today": {"jobs": 2, "compiles": 103, "from_store": 100}}, (rc, a))
rc, a = cli("--here", "crates", env={"BUILD_RUNNER_HOME": f"{d}/rh", "BUILD_RUNNER_TRUST_AGENT": "1"})
e = a.get("entries") if isinstance(a, dict) else None
check("a press on it: its entries newest use first, each what it was made from (a registry crate with its version, one "
      "of ours by its outputs' crate), who made it on which branch and commit, when, its last use and its bytes",
      rc == 0 and a.get("total") == 2 and e and [x["made_from"] for x in e] == ["coordd", "serde 1.0.217"]
      and e[0]["by"] == {"who": "a2h-speed", "branch": "a2h-store-request", "commit": "1234abcd"} and e[0]["kind"] == "rustc"
      and e[0]["made_at"] == br.iso(1795540000) and e[0]["used_at"] == br.iso(int(now - 60))
      and e[0]["bytes"] == crates_total - os.path.getsize(f"{rs}/blobs/{s1[:2]}/{s1}") and "wt" not in e[1]["by"], (rc, a))
br.STORE_PAGE, br.STORE_DIR = 1, br.Path(rs)
check("a page at a time, from N", br.store_answer("crates", 1, here=True) == {"entries": [e[1]], "total": 2} if e else False,
      br.store_answer("crates", 1, here=True))
br.STORE_PAGE, br.STORE_DIR = 200, br.Path(f"{d}/home/store")
# this PC: no build machine named, and no entries of its own (its builds run there): no rows
check("this PC with no build machine and no entries: no rows", br.store_answer() == {"rows": []}, br.store_answer())
# take_back links a job's files from this PC's store, and says each in the index with the job that made it
ls = f"{d}/home/store"
t1, n1, _ = blob(ls, b"coordd binary here" * 1000, True)
t2, n2, _ = blob(ls, b"yaaihh exe" * 2000, True)
t3, n3, _ = blob(ls, b"dep info", False)
os.makedirs(f"{d}/wt/target/quick/deps")
m = {"blobs": [[f"{d}/wt/target/quick/deps/coordd-0123456789abcdef", n1], [f"{d}/wt/target/quick/coordd", n1],
               [f"{d}/wt/target/quick/deps/coordd-0123456789abcdef.d", n3]], "store": "/r/store/handed/12-ab"}
ok, n, got, size, copied = br.take_back("host", m, f"{d}/wt", {"job": "4242-1795550000", "who": "docket-edge8", "branch": "edge8-store", "commit": "6a4550b9", "x": None})
lines = [json.loads(l) for l in open(br.BINARIES_INDEX)]
check("take_back: linked, and each file in the index: where, its blob, not fetched, the job that made it",
      ok and n == 3 and got == 0 and len(lines) == 3 and lines[0]["dest"] == m["blobs"][0][0] and lines[0]["blob"] == n1
      and lines[0]["fetched"] is False and lines[0]["job"] == "4242-1795550000" and lines[0]["who"] == "docket-edge8"
      and lines[0]["branch"] == "edge8-store" and lines[0]["commit"] == "6a4550b9" and "x" not in lines[0], lines)
os.makedirs(f"{d}/wt2"); os.link(f"{ls}/blobs/{t2[:2]}/{n2}", f"{d}/wt2/yaaihh.exe")
br.index_binaries([(f"{d}/wt2/yaaihh.exe", n2, True)], {"job": "1-1", "who": "docket-edge8"})
# a build machine named: its crates' row read over ssh, this PC's binaries' row from its own store
open(f"{d}/home/machines", "w").write("yaaihh-build-1 build@192.0.2.1\n")
a = br.store_answer()
rows = {r.get("id"): r for r in a.get("rows") or []}
c, b = rows.get("crates") or {}, rows.get("binaries") or {}
check("with a build machine: its crates' row, with its name as the row's device (as Devices says it) and when it was read",
      c.get("device") == "yaaihh-build-1" and c.get("count") == 2 and c.get("today") == {"jobs": 2, "compiles": 103, "from_store": 100}
      and abs(time.mktime(time.strptime(c.get("as_of", "1970-01-01T00:00:00Z"), "%Y-%m-%dT%H:%M:%SZ")) - time.mktime(time.gmtime())) < 5, c)
held_here = sum(os.path.getsize(f"{ls}/blobs/{x[:2]}/{y}") for x, y in ((t1, n1), (t2, n2), (t3, n3)))
check("and this PC's binaries' row: held against its cap, its blobs, today's files handed back and fetched, its programs "
      "most linked first, named without cargo's hash; no device (the engine's own machine)",
      b == {"id": "binaries", "what": "Binaries", "held_bytes": held_here, "cap_bytes": 20 * 2**30, "count": 3, "count_of": "blobs",
            "today": {"handed": 4, "fetched": 1}, "holds": [{"name": "coordd", "hash8": n1[:8], "links": 2}, {"name": "yaaihh.exe", "hash8": n2[:8], "links": 1}]}, b)
calls = len(open(f"{d}/ssh.log").read().splitlines())
br.store_answer(); br.store_answer()
check("read from the build machine at most once a minute", len(open(f"{d}/ssh.log").read().splitlines()) == calls, calls)
open(f"{d}/down", "w").close()
br.MACHINE_STORE_SEC = 0
c2 = next((r for r in br.store_answer()["rows"] if r.get("id") == "crates"), {})
check("the build machine not answering: the rows last read, saying why they were not read now", c2.get("count") == 2
      and c2.get("unread") == "yaaihh-build-1 does not answer" and c2.get("as_of") == c.get("as_of"), c2)
os.unlink(br.MACHINE_STORE)
check("and with none read before: its crates' row, saying only that", next((r for r in br.store_answer()["rows"] if r.get("id") == "crates"), {})
      == {"id": "crates", "what": "Compiled crates", "device": "yaaihh-build-1", "unread": "yaaihh-build-1 does not answer"}, br.store_answer())
check("a press on the crates' row then: an error in words", br.store_answer("crates") == {"error": "yaaihh-build-1 does not answer"}, br.store_answer("crates"))
os.unlink(f"{d}/down")
pg = br.store_answer("crates")
check("a press on the crates' row with a build machine: its entries, from it", pg.get("total") == 2 and len(pg.get("entries") or []) == 2
      and "crates --from 0" in open(f"{d}/ssh.log").read().splitlines()[-1], pg)
pg = br.store_answer("binaries")
e = pg.get("entries") or []
cd = next((x for x in e if x["hash8"] == n1[:8]), {})
check("a press on the binaries' row: each blob, where it is linked, the job that first had it handed, its links and size",
      pg.get("total") == 3 and cd.get("names") == sorted([m["blobs"][0][0], m["blobs"][1][0]]) and cd.get("links") == 2
      and cd.get("made_by") == {"job": "4242-1795550000", "who": "docket-edge8", "branch": "edge8-store", "commit": "6a4550b9"}
      and cd.get("bytes") == len(b"coordd binary here" * 1000) and e[0]["used_at"] >= e[-1]["used_at"], pg)
check("a row that is not there: an error in words", br.store_answer("states") == {"error": "no row 'states' here: crates, binaries, outside"})
# the index kept to the blobs the store holds; of old lines, the newest of each blob and place
old = int(time.time()) - 3 * 86400
with open(br.BINARIES_INDEX, "a") as f:
    f.write(json.dumps({"at": old, "blob": "f" * 64 + ".x", "dest": "/gone"}) + "\n")
    f.write(json.dumps({"at": old - 10, "blob": n2, "dest": f"{d}/wt3/yaaihh.exe", "job": "0-old"}) + "\n")
    f.write(json.dumps({"at": old, "blob": n2, "dest": f"{d}/wt3/yaaihh.exe", "job": "0-old2"}) + "\n")
    f.write(json.dumps({"at": old, "blob": n2, "dest": f"{d}/wt2/yaaihh.exe", "job": "0-old3"}) + "\n")
br.trim_index()
kept = [json.loads(l) for l in open(br.BINARIES_INDEX)]
check("the index trimmed: a blob gone goes; of a blob's older lines, only the newest of each place stays, and none "
      "where a younger line is; today's stay",
      len(kept) == 5 and not any(x["blob"].startswith("f" * 8) for x in kept) and [x.get("job") for x in kept if x["blob"] == n2] == ["0-old2", "1-1"], kept)
# the service's request, as coordd asks it: one line of JSON, the answer one line
svc = br.Service()
x, y = socket.socketpair()
threading.Thread(target=svc.handle, args=(x,), daemon=True).start()
y.sendall(b'{"store": {"row": "binaries", "from": 2}}\n')
ans = json.loads(y.makefile().readline())
check("the service answers {\"store\": {\"row\", \"from\"}} as `store ROW --from N` does", ans.get("total") == 3 and len(ans.get("entries") or []) == 1, ans)
x, y = socket.socketpair()
threading.Thread(target=svc.handle, args=(x,), daemon=True).start()
y.sendall(b'{"store": {}}\n')
ans = json.loads(y.makefile().readline())
check("and {\"store\": {}} with the rows", [r.get("id") for r in ans.get("rows") or []] == ["crates", "binaries"], ans)
x, y = socket.socketpair()
threading.Thread(target=svc.handle, args=(x,), daemon=True).start()
y.sendall(json.dumps({"free": f"{d}/not-found"}).encode() + b"\n")
ans = json.loads(y.makefile().readline())
check("the service answers {\"free\": PATH} as `free PATH` does: a path no walk found is refused, nothing deleted",
      "error" in ans and "freed" not in ans, ans)
# the service: rows of the day older than a minute come back at once as they are, and are read again behind them, one
# read at a time, so a page waits on a slow build machine only for the day's first read (consult-lead N-1446 p2980)
br.MACHINE_STORE_SEC = 60
since = br.day_start()
old_rows = [{"id": "crates", "what": "Compiled crates", "count": 1, "device": "yaaihh-build-1", "as_of": "2026-11-24T08:00:00Z"}]
json.dump({"at": time.time() - 120, "since": since, "rows": old_rows}, open(br.MACHINE_STORE, "w"))
open(f"{d}/slow", "w").close()
calls = len(open(f"{d}/ssh.log").read().splitlines())
br.SERVING = True
t0 = time.time()
r1 = br.machine_rows(br.machine(), since)
r2 = br.machine_rows(br.machine(), since)
took = time.time() - t0
check("in the service, rows over a minute old come back at once as they are, and a second ask meanwhile starts no "
      "second read", r1 == old_rows and r2 == old_rows and took < 1.5, (took, r1))
for _ in range(100):
    if json.load(open(br.MACHINE_STORE)).get("at", 0) > t0:
        break
    time.sleep(0.1)
kept = json.load(open(br.MACHINE_STORE))
check("and behind them the build machine is read once, and its rows kept with when they were read",
      kept.get("at", 0) > t0 and kept["rows"][0].get("count") == 2 and kept["rows"][0].get("as_of") != old_rows[0]["as_of"]
      and len(open(f"{d}/ssh.log").read().splitlines()) == calls + 1 and br.MACHINE_READING == [], kept)
check("the next ask has the new rows, from what was kept", br.machine_rows(br.machine(), since) == kept["rows"]
      and len(open(f"{d}/ssh.log").read().splitlines()) == calls + 1)
os.unlink(f"{d}/slow")
open(f"{d}/down", "w").close()
json.dump({**kept, "at": time.time() - 120}, open(br.MACHINE_STORE, "w"))
br.machine_rows(br.machine(), since)
for _ in range(100):
    if json.load(open(br.MACHINE_STORE)).get("tried"):
        break
    time.sleep(0.1)
time.sleep(0.2)
r3 = br.machine_rows(br.machine(), since)
check("a build machine that does not answer behind them: the rows kept say so (`unread`), and it is tried again only a "
      "minute on", all(r.get("unread") == "yaaihh-build-1 does not answer" and r.get("count") == 2 for r in r3)
      and len(open(f"{d}/ssh.log").read().splitlines()) == calls + 2, (r3, len(open(f"{d}/ssh.log").read().splitlines()) - calls))
json.dump({"at": time.time() - 120, "since": since - 86400, "rows": old_rows}, open(br.MACHINE_STORE, "w"))
os.unlink(f"{d}/down")
r4 = br.machine_rows(br.machine(), since)
check("rows of another day are not answered as they are: the day's first read waits for the build machine",
      r4 and r4[0].get("count") == 2 and r4[0].get("as_of") != old_rows[0]["as_of"], r4)
br.SERVING = False
rc, a = cli("binaries", "--from", "x")
check("the verb's usage on what it does not take", rc == 1 and "build-runner store [ROW] [--from N]" in a, (rc, a))
print(f"{fails} failed")
sys.exit(1 if fails else 0)
EOF
rc=$?
cat "$dir/out.log"; [ -s "$dir/err.log" ] && { echo "stderr:"; cat "$dir/err.log"; }
exit $rc
