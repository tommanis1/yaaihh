#!/usr/bin/env bash
# A job outlives neither its caller nor its hold's end (a2h-speed N-1446 p3251; paper-research p3253). At 01:01Z 25 Nov
# the build machine's link went in the middle of eng-80's check: the hold ended there, its tests job ran on unread to
# the 1800 s limit, and sent again with the ended hold's token it waited for that hold for ever. Now:
#   1. a job naming a hold that has ended runs as an ordinary job, with a line saying so; one naming a hold this
#      runner never gave runs too, under the rules, with its own line; the queue's words never say "a hold that is
#      not running";
#   2. a running job whose caller's connection ends is stopped then (its process group), not at its next line; the
#      caller's blank lines meanwhile do not stop it;
#   3. the relay on the build machine ends the connection when its input is silent for RELAY_SILENT_SEC, and not while
#      the caller's blank lines come;
#   4. the caller over ssh (Remote) says a blank line each BEAT_SEC after its request, so its relay stays, and its
#      close ends the connection;
#   5. no time limit per job: no RuntimeMaxSec, no BUILD_RUNNER_MAX_SEC.
# build-runner is loaded as a module with stand-ins: a cargo that answers what the runner asks of it and, for `build`,
# says one line and sleeps; a service socket of the test's own for the relay; an ssh that runs its command here. No
# service unit, no compile, no record. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'pkill -f "$dir/" 2>/dev/null; rm -rf "$dir"' EXIT
mkdir -p "$dir/home" "$dir/rh" "$dir/h/.cargo/bin" "$dir/wt/src"
cat >"$dir/h/.cargo/bin/cargo" <<EOF
#!/usr/bin/env bash
# the runner's cargo, here: what it asks besides a build; a build says a line, writes its pid and sleeps
for a in "\$@"; do case "\$a" in
  locate-project) echo "$dir/wt/Cargo.toml"; exit 0 ;;
  metadata) echo '{"packages": [], "workspace_root": "$dir/wt", "target_directory": "$dir/t", "resolve": null, "workspace_members": []}'; exit 0 ;;
  build) echo "   Compiling app v0.1.0 ($dir/wt)" >&2; echo \$\$ >"$dir/cargo.pid"; exec sleep 60 ;;
esac; done
exit 0
EOF
cat >"$dir/ssh" <<EOF
#!/usr/bin/env bash
exec sh -c "\$2"
EOF
chmod +x "$dir/h/.cargo/bin/cargo" "$dir/ssh"
printf '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2021"\n' >"$dir/wt/Cargo.toml"
echo 'fn main() {}' >"$dir/wt/src/main.rs"
git -C "$dir/wt" init -q && git -C "$dir/wt" add -A && git -C "$dir/wt" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@t commit -qm a
export HOME="$dir/h" BUILD_RUNNER_HOME="$dir/home" BUILD_RUNNER_STORE=0 BUILD_RUNNER_RULES=0 BUILD_RUNNER_SCOPE=0 \
  BUILD_RUNNER_MIN_FREE_GB=0 BUILD_RUNNER_LOCAL=1 BUILD_RUNNER_BEAT_SEC=0.3 BUILD_RUNNER_RELAY_SILENT_SEC=1.5 \
  BUILD_RUNNER_UNIT=build-runner-linktest BUILD_RUNNER_REMOTE_UNIT=build-runner-linktest \
  BUILD_RUNNER_SSH="$dir/ssh" BUILD_RUNNER_REMOTE_LIB="$here" BUILD_RUNNER_REMOTE_HOME="$dir/rh" GIT_CONFIG_NOSYSTEM=1
python3 -I - "$here/build-runner" "$dir" <<'EOF'
import importlib.machinery, importlib.util, json, os, signal, socket, subprocess, sys, threading, time
path, d = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_loader("br", importlib.machinery.SourceFileLoader("br", path))
br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
fails = 0
def check(what, cond, got=None):
    global fails
    print(("ok - " if cond else "not ok - ") + what + ("" if cond or got is None else f": {got}"))
    fails += not cond
def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    st = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[0]
    return st != "Z"
def until(cond, sec):
    t = time.time() + sec
    while time.time() < t:
        if cond():
            return True
        time.sleep(0.05)
    return cond()

# 1. the queue: a job whose token is not the running hold's
svc = br.Service()
entry = ("t", f"{d}/wt", ["build"], False, "x-debug", time.time(), os.getpid(), True, False, "ended-token")
with svc.cv:
    fits_none, why_none = svc.fits(entry), svc.why(entry)
    svc.hold, svc.hold_pid = "another-token", None
    svc.now["hold-1"] = ("h", "/", ["hold"], time.time(), True, True)
    fits_other, why_other = svc.fits(entry), svc.why(entry)
    svc.hold = svc.hold_pid = None
    del svc.now["hold-1"]
check("a job naming a hold that is not running takes a free place, no hold running", fits_none, why_none)
check("and with another hold running, a place the hold leaves", fits_other, why_other)
check("the queue's words never say it names a hold that is not running",
      "names a hold that is not running" not in why_none + why_other, (why_none, why_other))

def job(req, sec=10):
    """Service.run on REQ through a socket pair: (what the caller read, the caller's end of the pair, the thread)."""
    a, b = socket.socketpair()
    f = a.makefile("rwb", buffering=0)
    got = bytearray()
    def serve():
        try:
            svc.run({"cwd": f"{d}/wt", "args": ["build"], "folder": "x", "pid": os.getpid(), **req}, f, a)
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            a.close()
    t = threading.Thread(target=serve, daemon=True); t.start()
    def read():
        while True:
            try:
                x = b.recv(65536)
            except OSError:
                return
            if not x:
                return
            got.extend(x)
    threading.Thread(target=read, daemon=True).start()
    return got, b, t

def ran(got):
    return until(lambda: os.path.exists(f"{d}/cargo.pid") and b"Compiling app" in got, 10)

def stop_job(b, t):
    pid = int(open(f"{d}/cargo.pid").read())
    os.unlink(f"{d}/cargo.pid")
    b.shutdown(socket.SHUT_RDWR)  # as a caller that ends: its reader thread here would hold a close back
    b.close()
    return pid, until(lambda: not alive(pid), 5), t

# a hold this service gave and that has ended (the token in `given`, no hold running)
svc.given.add("given-ended") if hasattr(svc, "given") else None
got, b, t = job({"in_hold": "given-ended"})
started = ran(got)
check("a job naming a hold that has ended runs (no wait for ever)", started, bytes(got)[-300:])
check("with a line saying the hold is not running and it runs as an ordinary job",
      b"the hold this job names is not running" in got and b"it runs as an ordinary job" in got, bytes(got)[-300:])
if started:
    stop_job(b, t)
else:
    b.close()
t.join(5)

# 2. a plain job whose caller's connection ends while it runs: stopped at once; blank lines before do not stop it
until(lambda: not svc.now and not svc.waiting, 5)
got, b, t = job({})
started = ran(got)
check("a plain job runs", started, bytes(got)[-300:])
if started:
    pid = int(open(f"{d}/cargo.pid").read())
    for _ in range(4):
        b.send(b"\n"); time.sleep(0.3)
    check("the caller's blank lines do not stop the job", alive(pid))
    t0 = time.time()
    pid, stopped, t = stop_job(b, t)
    check("the job whose caller's connection ended is stopped then (its process group)", stopped,
          f"cargo {pid} still running after {time.time() - t0:.1f} s")
    t.join(5)
    check("and the service lets it go: its place free, its log line", not t.is_alive() and not svc.now
          and open(f"{d}/home/build-runner.log").read().count("stopped (caller gone: its connection ended)") == 2, svc.now)
else:
    b.close()
    t.join(1)

# a token this runner never gave (from before a restart, or made up)
until(lambda: not svc.now and not svc.waiting, 5)
got, b, t = job({"in_hold": "never-given"})
started = ran(got)
check("a job naming a hold this runner never gave runs, under the rules, with a line", started
      and b"was not given by this runner" in got and b"under the rules" in got, bytes(got)[-300:])
if started:
    stop_job(b, t)
else:
    b.close()
t.join(5)

# 3. the relay: its input silent for RELAY_SILENT_SEC ends the connection; blank lines keep it
os.makedirs(f"{d}/rel", exist_ok=True)
srv = socket.socket(socket.AF_UNIX); srv.bind(f"{d}/rel/sock"); srv.listen(4)
srv.settimeout(10)
def relay():
    return subprocess.Popen([sys.executable, "-I", path, "relay"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            env={**os.environ, "BUILD_RUNNER_HOME": f"{d}/rel"})
def ends(c, sec):
    """C's end seen within SEC (what came before it read and kept): (ended, what came)."""
    c.settimeout(0.2)
    got, t = b"", time.time() + sec
    while time.time() < t:
        try:
            x = c.recv(4096)
        except socket.timeout:
            continue
        except OSError:
            return True, got
        if not x:
            return True, got
        got += x
    return False, got
r = relay()
r.stdin.write(b'{"status": true}\n'); r.stdin.flush()
c, _ = srv.accept()
for _ in range(10):  # 3 s of blank lines, each 0.3 s: twice the silence allowed, never silent that long
    time.sleep(0.3); r.stdin.write(b"\n"); r.stdin.flush()
gone, came = ends(c, 0.1)
check("the relay keeps the connection while the caller's blank lines come", not gone and came.startswith(b'{"status"'), came)
t0 = time.time()
gone, _ = ends(c, 6)
took = time.time() - t0
check("the relay ends the connection when its input is silent for RELAY_SILENT_SEC (1.5 s)", gone and 1.0 < took < 4,
      f"ended {gone} after {took:.1f} s")
check("and the relay has ended", until(lambda: r.poll() is not None, 3), r.poll())
c.close()

# 4. the caller over ssh: a blank line each BEAT_SEC after its request, so the relay keeps its connection; close ends it
os.makedirs(f"{d}/rh", exist_ok=True)
srv2 = socket.socket(socket.AF_UNIX); srv2.bind(f"{d}/rh/sock"); srv2.listen(4); srv2.settimeout(10)
rm = br.Remote("machine")
rm.write(b'{"status": true}\n')
c, _ = srv2.accept()
gone, came = ends(c, 4)
check("over ssh the caller's link stays past the relay's silence (4 s; blank lines each 0.3 s)", not gone, came)
check("its request came first, then blank lines", came.startswith(b'{"status": true}\n') and came.count(b"\n") >= 6, came[:80])
rm.close()
gone, _ = ends(c, 3)
check("the caller's close ends the connection", gone)

# 5. no time limit per job
text = open(path).read()
check("no time limit per job: no RuntimeMaxSec, no BUILD_RUNNER_MAX_SEC", "RuntimeMaxSec" not in text and "MAX_SEC" not in text)
print(f"{fails} failed")
sys.exit(1 if fails else 0)
EOF
