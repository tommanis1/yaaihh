"""What every part of the Colab builder shares: names checked by regex, SHA-256 of files and streams, tar.zst packed
and unpacked with no link, device or path outside its folder, JSON written whole, and the machine's facts."""
import hashlib, json, os, re, shutil, subprocess, tarfile, tempfile
from pathlib import Path

JOB_ID = re.compile(r"[A-Za-z0-9_-]{1,80}")
SHA = re.compile(r"[0-9a-f]{64}")
NAME = re.compile(r"[A-Za-z0-9_.-]{1,80}")  # a toolchain's name, a worker's
CHUNK = 1 << 20
ENV_KEY = re.compile(r"[A-Z_][A-Z0-9_]{0,63}")
WORKER_OWN = {"PATH", "HOME", "TOOLCHAIN", "TMPDIR", "LD_PRELOAD", "LD_AUDIT"}  # the worker sets these, or none may


class Refused(Exception):
    """A name, a size, a hash or an archive member that is not allowed."""


def job_id(s):
    if not isinstance(s, str) or not JOB_ID.fullmatch(s):
        raise Refused(f"job id {s!r}: not ^[A-Za-z0-9_-]{{1,80}}$")
    return s


def sha(s):
    if not isinstance(s, str) or not SHA.fullmatch(s):
        raise Refused(f"sha256 {s!r}: not 64 lowercase hex digits")
    return s


def name(s):
    if not isinstance(s, str) or not NAME.fullmatch(s) or s.startswith("."):
        raise Refused(f"name {s!r}: not ^[A-Za-z0-9_.-]{{1,80}}$")
    return s


def env_key(k):
    """A job's or toolchain's variable name: upper case, and not one the worker sets."""
    if not isinstance(k, str) or not ENV_KEY.fullmatch(k) or k in WORKER_OWN:
        raise Refused(f"variable {k!r}: [A-Z_][A-Z0-9_]*, not one of {', '.join(sorted(WORKER_OWN))}")
    return k


def rel_path(s):
    """A path inside a job's folder: relative, no `..`, no empty part."""
    if not isinstance(s, str) or not s or s.startswith("/") or "\0" in s or "\\" in s:
        raise Refused(f"path {s!r}: not a relative path")
    parts = s.split("/")
    if any(p in ("", ".", "..") for p in parts):
        raise Refused(f"path {s!r}: an empty, . or .. part")
    return s


def file_sha(path, limit=None):
    h, n = hashlib.sha256(), 0
    with open(path, "rb") as f:
        while b := f.read(CHUNK):
            n += len(b)
            if limit is not None and n > limit:
                raise Refused(f"{path}: over {limit} bytes")
            h.update(b)
    return h.hexdigest()


def check_file(path, want, limit=None):
    """PATH's SHA-256 must be WANT; Refused otherwise."""
    got = file_sha(path, limit)
    if got != sha(want):
        raise Refused(f"{Path(path).name}: sha256 {got}, expected {want}")


def write_json(path, obj):
    """OBJ into PATH whole: a temp file beside it, then a rename."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=".tmp-")
    with os.fdopen(fd, "w") as f:
        json.dump(obj, f, indent=1, sort_keys=True)
    os.replace(tmp, path)


def read_json(path, limit=1 << 20):
    if os.stat(path).st_size > limit:
        raise Refused(f"{path}: over {limit} bytes")
    return json.loads(Path(path).read_bytes()[:limit + 1])


def pack(src, dest, members=None, level=3):
    """SRC's files (or only MEMBERS, relative paths) into DEST as tar.zst, in name order, owners and times zeroed, so
    the same files give the same bytes. Regular files and folders only. Returns DEST's SHA-256."""
    src = Path(src)
    names = sorted(members) if members is not None else sorted(
        str(p.relative_to(src)) for p in src.rglob("*") if not p.is_symlink() and (p.is_file() or p.is_dir()))
    fd, tmp = tempfile.mkstemp(dir=Path(dest).parent, prefix=".tmp-")
    z = None
    try:
        with os.fdopen(fd, "wb") as out:
            z = subprocess.Popen(["zstd", "-q", f"-{level}", "-T0", "-c"], stdin=subprocess.PIPE, stdout=out)
            with z.stdin, tarfile.open(fileobj=z.stdin, mode="w|", format=tarfile.PAX_FORMAT) as t:
                for n in names:
                    p = src / rel_path(n)
                    if p.is_symlink() or not (p.is_file() or p.is_dir()):
                        raise Refused(f"{n}: not a regular file or folder")
                    info = t.gettarinfo(str(p), arcname=n)
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    info.mtime = 0
                    if info.isfile():
                        with open(p, "rb") as f:
                            t.addfile(info, f)
                    else:
                        t.addfile(info)
            if z.wait() != 0:
                raise RuntimeError("zstd failed")
        os.replace(tmp, dest)
    except BaseException:
        if z is not None and z.poll() is None:
            z.kill()
        if z is not None:
            z.wait()
        os.unlink(tmp)
        raise
    return file_sha(dest)


TAIL_SLACK = 1 << 20  # a tar's end padding is under 10 KiB; anything past this after it is refused


def unpack(archive, dest, limit=None):
    """ARCHIVE (tar.zst) into DEST, a new folder: only regular files and folders, every name a relative path inside
    DEST; a link, a device, an absolute name, a `..` or more than LIMIT bytes unpacked refuses the whole archive and
    leaves no DEST."""
    dest = Path(dest)
    tmp = Path(tempfile.mkdtemp(dir=dest.parent, prefix=".tmp-"))
    total, z = 0, None
    try:
        z = subprocess.Popen(["zstd", "-q", "-dc", str(archive)], stdout=subprocess.PIPE)
        with z.stdout, tarfile.open(fileobj=z.stdout, mode="r|") as t:
            for m in t:
                if m.name in (".", "./"):
                    continue  # tar -C DIR . names the folder itself
                m.name = rel_path(m.name.removeprefix("./").rstrip("/"))
                if not (m.isfile() or m.isdir()):
                    raise Refused(f"{m.name}: a link or special file")
                total += m.size
                if limit is not None and total > limit:
                    raise Refused(f"{archive}: over {limit} bytes unpacked")
                m.mode = (m.mode & 0o755) | (0o700 if m.isdir() else 0o600)
                t.extract(m, tmp, filter="data")
            tail = 0
            while b := z.stdout.read(CHUNK):
                tail += len(b)
                if tail > TAIL_SLACK:
                    raise Refused(f"{archive}: more than {TAIL_SLACK} bytes after the tar's end")
        if z.wait() != 0:
            raise Refused(f"{archive}: not a zstd archive")
        os.replace(tmp, dest)
    except BaseException:
        if z is not None and z.poll() is None:
            z.kill()
        if z is not None:
            z.wait()
        shutil.rmtree(tmp, ignore_errors=True)
        raise
    return dest


def manifest(folder):
    """{relative path: sha256} of every regular file under FOLDER."""
    folder = Path(folder)
    return {str(p.relative_to(folder)): file_sha(p) for p in sorted(folder.rglob("*"))
            if p.is_file() and not p.is_symlink()}


def check_manifest(folder, want):
    """FOLDER's files must be exactly WANT's, each with its hash."""
    got = manifest(folder)
    if set(got) != set(want):
        raise Refused(f"files differ from the manifest: {sorted(set(got) ^ set(want))[:5]}")
    bad = [n for n in got if got[n] != want[n]]
    if bad:
        raise Refused(f"sha256 differs from the manifest: {bad[:5]}")


def hardware():
    """The machine's facts for a job's record: CPU model and cores, RAM, GPUs."""
    model, ram = "", 0
    try:
        with open("/proc/cpuinfo") as f:
            model = next((l.split(":", 1)[1].strip() for l in f if l.startswith("model name")), "")
        with open("/proc/meminfo") as f:
            ram = next(int(l.split()[1]) for l in f if l.startswith("MemTotal:")) // 1024
    except (OSError, StopIteration, ValueError):
        pass
    gpus = []
    if shutil.which("nvidia-smi"):
        r = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"],
                           capture_output=True, text=True, timeout=30)
        gpus = [{"name": a.strip(), "mem_mb": int(b)} for a, b in
                (l.rsplit(",", 1) for l in r.stdout.splitlines() if "," in l)] if r.returncode == 0 else []
    return {"cpu": model, "cores": os.cpu_count() or 0, "ram_mb": ram, "gpus": gpus}


def fits(needs, hw):
    """Whether a machine HW meets a job's NEEDS ({cpu, gpu, ram_gb})."""
    return (hw["cores"] >= int(needs.get("cpu", 1)) and len(hw["gpus"]) >= int(needs.get("gpu", 0))
            and hw["ram_mb"] >= int(needs.get("ram_gb", 0)) * 1024)
