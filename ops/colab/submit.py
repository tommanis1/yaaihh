#!/usr/bin/env python3
"""The tower's side of Layer 1: into a store (a folder, or Drive through rclone) go

  submit.py toolchain --store S FILE          a toolchain-NAME-SHA.tar.zst from pack-toolchain.sh, its hash checked
  submit.py worker --store S                  this folder's worker code, for colab_worker.ipynb to fetch
  submit.py job --store S --repo R --rev REF --toolchain NAME:SHA [--name N] [--cpu N] [--gpu N] [--ram-gb N]
                [--timeout SEC] [--mem-gb N] [--disk-gb N] [--env K=V]... [--out PATH]... [--oci] -- CMD [ARG...]
                                              a job bundle: REF's git bundle, the command, its env and limits, the
                                              outputs to bring back, a SHA-256 manifest; prints the job's id

The command is an argument list, run with no shell; its outputs are paths in the checkout. --oci: the command builds
an OCI image with buildah, no daemon (BUILDAH_ISOLATION=chroot, STORAGE_DRIVER=vfs); it pushes it to an oci-archive:
path among its outputs."""
import argparse, json, re, subprocess, sys, tempfile, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import Refused, env_key, file_sha, job_id, manifest, name, pack, rel_path, sha, write_json  # noqa: E402
import jobq  # noqa: E402
from store import open_store  # noqa: E402

TOOLCHAIN_FILE = re.compile(r"toolchain-([A-Za-z0-9_.-]{1,80})-([0-9a-f]{64})\.tar\.zst")
WORKER_FILES = ["common.py", "store.py", "jobq.py", "worker.py", "rpc_client.py"]


def toolchain(store, path):
    path = Path(path)
    m = TOOLCHAIN_FILE.fullmatch(path.name)
    if not m:
        raise Refused(f"{path.name}: not toolchain-NAME-SHA256.tar.zst")
    got = file_sha(path)
    if got != m.group(2):
        raise Refused(f"{path.name}: its sha256 is {got}")
    store.put(path, f"toolchains/{path.name}")
    return m.group(1), got


def worker_code(store):
    here = Path(__file__).resolve().parent
    files = {f: file_sha(here / f) for f in WORKER_FILES}
    for f in WORKER_FILES:
        store.put(here / f, f"worker/{f}")
    store.write("worker/MANIFEST.json", json.dumps(files, sort_keys=True).encode())
    return files


def env_pairs(pairs):
    env = {}
    for p in pairs:
        k, eq, v = p.partition("=")
        if not eq or "\0" in v:
            raise Refused(f"--env {p!r}: NAME=VALUE")
        env[env_key(k)] = v
    return env


def job(store, a):
    tname, _, tsha = a.toolchain.partition(":")
    name(tname), sha(tsha)
    if store.size(f"toolchains/toolchain-{tname}-{tsha}.tar.zst") is None:
        raise Refused(f"toolchain {tname}:{tsha} is not in the store: submit.py toolchain first")
    if not a.cmd:
        raise Refused("no command after --")
    if a.rev.startswith("-") or not a.rev:
        raise Refused(f"--rev {a.rev!r}: a ref name, not an option")
    commit = subprocess.run(["git", "-C", a.repo, "rev-parse", "--verify", f"{a.rev}^{{commit}}"],
                            capture_output=True, text=True, check=True).stdout.strip()
    needs = {"cpu": a.cpu, "gpu": a.gpu, "ram_gb": a.ram_gb}
    with tempfile.TemporaryDirectory() as t:
        d = Path(t, "b")
        d.mkdir()
        subprocess.run(["git", "-C", a.repo, "bundle", "create", "-q", str(d / "repo.bundle"), a.rev], check=True)
        env = {**({"BUILDAH_ISOLATION": "chroot", "STORAGE_DRIVER": "vfs"} if a.oci else {}), **env_pairs(a.env)}
        spec = {"commit": commit, "cmd": list(a.cmd), "env": env, "toolchain": {"name": tname, "sha256": tsha},
                "needs": needs, "timeout_s": a.timeout, "mem_gb": a.mem_gb, "disk_gb": a.disk_gb,
                "outputs": [rel_path(o) for o in a.out], "max_log_mb": a.max_log_mb}
        write_json(d / "job.json", spec)
        write_json(d / "MANIFEST.json", manifest(d))
        out = Path(t, "bundle.tar.zst")
        bsha = pack(d, out)
        jid = job_id(a.id or f"{name(a.name)}-{bsha[:16]}")
        jobq.submit(store, jid, out, {"job_id": jid, "bundle_sha256": bsha, "toolchain": spec["toolchain"],
                                      "needs": needs, "submitted": time.time(), "bytes": out.stat().st_size})
    return jid


def parser():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="what", required=True)
    t = sub.add_parser("toolchain")
    t.add_argument("--store", required=True)
    t.add_argument("file")
    w = sub.add_parser("worker")
    w.add_argument("--store", required=True)
    j = sub.add_parser("job")
    j.add_argument("--store", required=True)
    j.add_argument("--repo", required=True)
    j.add_argument("--rev", required=True)
    j.add_argument("--toolchain", required=True)
    j.add_argument("--name", default="job")
    j.add_argument("--id")
    j.add_argument("--cpu", type=int, default=1)
    j.add_argument("--gpu", type=int, default=0)
    j.add_argument("--ram-gb", type=int, default=0)
    j.add_argument("--timeout", type=int, default=3600)
    j.add_argument("--mem-gb", type=int, default=0)
    j.add_argument("--disk-gb", type=int, default=0)
    j.add_argument("--max-log-mb", type=int, default=64)
    j.add_argument("--env", action="append", default=[])
    j.add_argument("--out", action="append", default=[])
    j.add_argument("--oci", action="store_true")
    j.add_argument("cmd", nargs=argparse.REMAINDER)
    return p


def args_of(argv):
    a = parser().parse_args(argv)
    if a.what == "job":
        a.cmd = a.cmd[1:] if a.cmd[:1] == ["--"] else a.cmd
    return a


def main(argv):
    a = args_of(argv)
    store = open_store(a.store)
    try:
        if a.what == "toolchain":
            print("%s %s" % toolchain(store, a.file))
        elif a.what == "worker":
            print(json.dumps(worker_code(store), indent=1))
        else:
            print(job(store, a))
    except Refused as e:
        print(f"submit: refused: {e}", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
