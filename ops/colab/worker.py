#!/usr/bin/env python3
"""A stateless burst worker for an open Colab notebook (or any machine): it claims a job, runs it on local disk, and
gives back a result the tower can check. Nothing it holds matters if it dies: the lease runs out and the tower
puts the job back.

  worker.py [--rpc URL --token-file F [--ca-file F]] [--store S] [--work DIR] [--worker NAME] [--once] [--idle-exit SEC]

--rpc: Layer 2, outbound HTTPS to the tower's rpc_server (claims, live log, CAS); when unset or not answering, Layer 1:
--store, the Drive folder through rclone (or a local folder). DIR (default /content/work, else /tmp/work) is the
machine's own disk, never the Drive mount. Each job: its bundle and toolchain fetched and checked against their
SHA-256, unpacked (no links, no path outside), the commit checked out, the command run with no shell, in a session
of its own, killed at its timeout, memory or disk cap or when its lease is lost; its outputs and log packed into a
cas/SHA.tar.zst, and a record (exit code, times, hardware, log and output hashes) into jobs/done/ID.json."""
import argparse, json, os, re, shutil, signal, socket, subprocess, sys, threading, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (Refused, check_file, env_key, file_sha, hardware, job_id, manifest, name, pack,  # noqa: E402
                    read_json, rel_path, sha, unpack)
import jobq  # noqa: E402
from rpc_client import Rpc  # noqa: E402
from store import open_store  # noqa: E402

GB = 1 << 30
HOST_GPU_ENV = ("LD_LIBRARY_PATH", "CUDA_VISIBLE_DEVICES", "NVIDIA_VISIBLE_DEVICES", "NVIDIA_DRIVER_CAPABILITIES")


class Drive:
    """Layer 1: the queue in a store."""

    def __init__(self, store, ttl):
        self.s, self.ttl, self.indexes = store, ttl, {}

    def claim(self, worker, hw):
        return jobq.claim(self.s, worker, hw, self.ttl, cache=self.indexes)

    def bundle(self, idx, dest, limit):
        return self._get(f"jobs/leased/{idx['job_id']}.tar.zst", dest, idx["bundle_sha256"], limit)

    def toolchain(self, tc, dest, limit):
        return self._get(f"toolchains/toolchain-{name(tc['name'])}-{sha(tc['sha256'])}.tar.zst", dest, tc["sha256"], limit)

    def _get(self, key, dest, digest, limit):
        n = self.s.size(key)
        if n is None:
            return False
        if n > limit:
            raise Refused(f"{key}: {n} bytes, over {limit}")
        self.s.get(key, dest)
        try:
            check_file(dest, digest, limit)
        except Refused:
            Path(dest).unlink()
            raise
        return True

    def heartbeat(self, jid, worker):
        return jobq.heartbeat(self.s, jid, worker, self.ttl)

    def log(self, jid, chunk):
        pass  # Layer 1 has no live log: the whole log comes in the result

    def put(self, path, digest):
        check_file(path, digest)
        if self.s.size(f"cas/{digest}.tar.zst") is None:
            self.s.put(path, f"cas/{digest}.tar.zst")

    def complete(self, jid, worker, record):
        return jobq.complete(self.s, jid, worker, record)

    def release(self, jid):
        self.s.delete(f"jobs/leased/{jid}.lease.json")
        self.s.move(f"jobs/leased/{jid}.tar.zst", f"jobs/pending/{jid}.tar.zst")


class Remote:
    """Layer 2: the tower's rpc_server."""

    def __init__(self, rpc):
        self.r = rpc

    def claim(self, worker, hw):
        return self.r.claim(worker, hw)

    def bundle(self, idx, dest, limit):
        return self.r.cas_get(idx["bundle_sha256"], dest, limit)

    def toolchain(self, tc, dest, limit):
        return self.r.cas_get(sha(tc["sha256"]), dest, limit)

    def heartbeat(self, jid, worker):
        try:
            return self.r.heartbeat(jid, worker)
        except OSError:
            return True  # the tower not answering for a moment is no lost lease; its own clock decides

    def log(self, jid, chunk):
        try:
            self.r.log(jid, chunk)
        except OSError:
            pass

    def put(self, path, digest):
        self.r.cas_put(path, digest)

    def complete(self, jid, worker, record):
        return self.r.complete(jid, {**record, "worker": worker})

    def release(self, jid):
        pass  # the lease runs out on the tower


def session_rss(sid):
    """The resident bytes of every process in session SID."""
    total = 0
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            stat = Path("/proc", d, "stat").read_text()
            if int(stat.rsplit(")", 1)[1].split()[3]) == sid:
                total += int(Path("/proc", d, "statm").read_text().split()[1]) * os.sysconf("SC_PAGE_SIZE")
        except (OSError, IndexError, ValueError):
            pass
    return total


def free_bytes(path):
    st = os.statvfs(path)
    return st.f_bavail * st.f_frsize


class Log:
    """The job's output into a file of at most CAP bytes, and what came since the last take() for the live log."""

    def __init__(self, path, cap):
        self.f, self.cap, self.n, self.new, self.lock, self.cut = open(path, "wb"), cap, 0, bytearray(), threading.Lock(), False

    def pump(self, pipe):
        while b := pipe.read1(1 << 16) if hasattr(pipe, "read1") else pipe.read(1 << 16):
            with self.lock:
                keep = b[:max(0, self.cap - self.n)]
                if len(keep) < len(b) and not self.cut:
                    self.cut, keep = True, keep + b"\n[worker: log cut at its cap]\n"
                self.f.write(keep)
                self.n += len(keep)
                if len(self.new) < (1 << 20):
                    self.new += keep
        self.f.close()

    def take(self):
        with self.lock:
            b, self.new = bytes(self.new), bytearray()
            return b


def check_spec(spec, idx):
    if spec.get("toolchain") != idx["toolchain"]:
        raise Refused("the bundle's toolchain is not its index's")
    if not spec.get("cmd") or not all(isinstance(c, str) for c in spec["cmd"]):
        raise Refused("cmd: a list of strings")
    if not all(env_key(k) and isinstance(v, str) and "\0" not in v for k, v in (spec.get("env") or {}).items()):
        raise Refused("env: strings")
    for o in spec.get("outputs") or []:
        rel_path(o)
    for k in ("timeout_s", "mem_gb", "disk_gb", "max_log_mb"):
        if not isinstance(spec.get(k, 0), int) or spec.get(k, 0) < 0:
            raise Refused(f"{k}: a whole number")
    if not isinstance(spec.get("commit"), str) or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", spec["commit"]):
        raise Refused("commit: a full git object id")
    return spec


def toolchain_dir(be, tc, work, limit):
    """The toolchain unpacked on local disk, once per machine (by its hash)."""
    d = work / "toolchains" / sha(tc["sha256"])
    if not d.is_dir():
        d.parent.mkdir(parents=True, exist_ok=True)
        z = d.parent / f"{tc['sha256']}.tar.zst"
        try:
            if not be.toolchain(tc, z, limit):
                raise Refused(f"toolchain {tc['name']}:{tc['sha256']} is not there")
            unpack(z, d, limit * 4)
        finally:
            z.unlink(missing_ok=True)
    return d


def job_env(spec, tdir, jd, gpu):
    meta = read_json(tdir / "toolchain.json") if (tdir / "toolchain.json").is_file() else {}
    paths = [str(tdir / rel_path(p)) for p in meta.get("path", [])]
    env = {"PATH": ":".join(paths + ["/usr/local/bin", "/usr/bin", "/bin"]), "HOME": str(jd / "home"),
           "TOOLCHAIN": str(tdir), "LANG": "C.UTF-8", "TMPDIR": str(jd / "tmp")}
    env.update({env_key(k): str(tdir / rel_path(v)) for k, v in (meta.get("env") or {}).items()})
    if gpu:
        env.update({k: os.environ[k] for k in HOST_GPU_ENV if k in os.environ})
    env.update(spec.get("env") or {})
    return env


def stop(p):
    for sig, wait in ((signal.SIGTERM, 10), (signal.SIGKILL, 30)):
        try:
            os.killpg(p.pid, sig)
        except ProcessLookupError:
            return
        try:
            p.wait(wait)
            return
        except subprocess.TimeoutExpired:
            pass


def run_job(be, idx, work, worker, hw, ttl, limits):
    jid = job_id(idx["job_id"])
    jd = work / "jobs" / jid
    shutil.rmtree(jd, ignore_errors=True)
    jd.mkdir(parents=True)
    try:
        bz = jd / "bundle.tar.zst"
        if not be.bundle(idx, bz, limits["bundle"]):
            raise Refused(f"{jid}: its bundle is not there")
        b = unpack(bz, jd / "b", limits["bundle"] * 4)
        want = read_json(b / "MANIFEST.json")
        got = manifest(b)
        got.pop("MANIFEST.json")
        if got != want:
            raise Refused(f"{jid}: the bundle's files are not its manifest's")
        spec = check_spec(read_json(b / "job.json"), idx)
        tdir = toolchain_dir(be, spec["toolchain"], work, limits["toolchain"])
        disk = spec.get("disk_gb") or 0
        if disk and free_bytes(work) < disk * GB:
            be.release(jid)
            return None
        subprocess.run(["git", "clone", "-q", "--no-checkout", str(b / "repo.bundle"), str(jd / "src")], check=True)
        subprocess.run(["git", "-C", str(jd / "src"), "checkout", "-q", "--detach", spec["commit"]], check=True)
        for d in ("home", "tmp", "r"):
            (jd / d).mkdir()
        return execute(be, idx, spec, tdir, jd, worker, hw, ttl, disk)
    finally:
        shutil.rmtree(jd, ignore_errors=True)


def inside(root, rel):
    """ROOT/REL when it is there and no part of it from ROOT down is a link (a link part way would move files from
    outside ROOT), else None."""
    p = root
    for part in Path(rel).parts:
        p = p / part
        if p.is_symlink() or not p.exists():
            return None
    return p if p.resolve().is_relative_to(root.resolve()) and p != root else None


def execute(be, idx, spec, tdir, jd, worker, hw, ttl, disk):
    jid = idx["job_id"]
    log = Log(jd / "r" / "log.txt", (spec.get("max_log_mb") or 64) << 20)
    start, free0, why = time.time(), free_bytes(jd), None
    p = subprocess.Popen(spec["cmd"], cwd=jd / "src", env=job_env(spec, tdir, jd, spec["needs"].get("gpu", 0)),
                         stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    pump = threading.Thread(target=log.pump, args=(p.stdout,), daemon=True)
    pump.start()
    beat = time.time()
    while p.poll() is None:
        time.sleep(1)
        if log.new:
            be.log(jid, log.take())
        if spec.get("timeout_s") and time.time() - start > spec["timeout_s"]:
            why = f"timeout {spec['timeout_s']} s"
        elif spec.get("mem_gb") and session_rss(p.pid) > spec["mem_gb"] * GB:
            why = f"memory over {spec['mem_gb']} GB"
        elif disk and free0 - free_bytes(jd) > disk * GB:
            why = f"disk over {disk} GB"
        elif time.time() - beat > ttl / 3:
            beat = time.time()
            if not be.heartbeat(jid, worker):
                stop(p)
                return None  # the lease is another's now: nothing of this run goes back
        if why:
            stop(p)
    pump.join()
    p.stdout.close()
    be.log(jid, log.take())
    end = time.time()
    missing = []
    for o in spec.get("outputs") or []:
        src, dst = inside(jd / "src", o), jd / "r" / "out" / o
        if src is None:
            missing.append(o)
            continue
        dst.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.rename(src, dst)
        except OSError:  # another file system mounted in the tree (EXDEV), or gone
            missing.append(o)
    outs = {k[4:]: v for k, v in manifest(jd / "r").items() if k.startswith("out/")}
    (jd / "r" / "MANIFEST.json").write_text(json.dumps(manifest(jd / "r"), sort_keys=True))
    res = jd / "result.tar.zst"
    rsha = pack(jd / "r", res)
    be.put(res, rsha)
    record = {"job_id": jid, "worker": worker, "exit_code": p.returncode, "killed": why, "started": start, "ended": end,
              "secs": round(end - start, 3), "hardware": hw, "log_sha256": file_sha(jd / "r" / "log.txt"),
              "log_bytes": log.n, "result_sha256": rsha, "outputs": outs, "missing": missing,
              "bundle_sha256": idx["bundle_sha256"]}
    be.complete(jid, worker, record)
    return record


def backend(a):
    if a.rpc:
        rpc = Rpc(a.rpc, a.token_file, a.ca_file)
        if rpc.ping():
            return Remote(rpc), "rpc"
        print(f"worker: {a.rpc} does not answer; Drive only", file=sys.stderr)
    if not a.store:
        raise SystemExit("worker: no --store, and no answering --rpc")
    return Drive(open_store(a.store), a.ttl), "store"


def main(argv):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--rpc")
    p.add_argument("--token-file")
    p.add_argument("--ca-file")
    p.add_argument("--store")
    p.add_argument("--work", default="/content/work" if Path("/content").is_dir() else "/tmp/work")
    p.add_argument("--worker", default=f"w-{socket.gethostname()[:40]}-{os.getpid()}".replace(".", "-"))
    p.add_argument("--ttl", type=int, default=300)
    p.add_argument("--once", action="store_true")
    p.add_argument("--idle-exit", type=int, default=0)
    p.add_argument("--poll", type=int, default=30)
    p.add_argument("--max-bundle-gb", type=int, default=8)
    p.add_argument("--max-toolchain-gb", type=int, default=20)
    a = p.parse_args(argv)
    if a.rpc and not a.token_file:
        raise SystemExit("worker: --rpc needs --token-file (a token never goes on the command line)")
    work = Path(a.work).resolve()
    if str(work).startswith("/content/drive"):
        raise SystemExit("worker: --work is the machine's own disk, not the Drive mount")
    work.mkdir(parents=True, exist_ok=True)
    name(a.worker)
    be, mode = backend(a)
    hw, idle = hardware(), time.time()
    limits = {"bundle": a.max_bundle_gb * GB, "toolchain": a.max_toolchain_gb * GB}
    print(f"worker {a.worker} ({mode}): {hw['cores']} cores, {hw['ram_mb']} MB, {len(hw['gpus'])} GPU", file=sys.stderr)
    while True:
        idx = be.claim(a.worker, hw)
        if idx is None:
            if a.once or (a.idle_exit and time.time() - idle > a.idle_exit):
                return 0
            time.sleep(a.poll)
            continue
        try:
            r = run_job(be, idx, work, a.worker, hw, a.ttl, limits)
            print(f"{idx['job_id']}: " + (f"exit {r['exit_code']} in {r['secs']} s" if r else "given back"), file=sys.stderr)
        except (Refused, subprocess.CalledProcessError, OSError, ValueError, KeyError) as e:
            print(f"{idx['job_id']}: not run: {e}", file=sys.stderr)  # the lease runs out; the tower decides again
        idle = time.time()
        if a.once:
            return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
