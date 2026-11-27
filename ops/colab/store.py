"""Where bundles, toolchains and results lie: a local folder (the tower's queue; the tests), or a Google Drive folder
through rclone, whole files at a time with parallel chunked transfers (never small writes through the FUSE mount).
Keys are relative paths of checked names: toolchains/, jobs/{pending,leased,done,index}/, cas/."""
import collections, json, os, shutil, subprocess, tempfile
from pathlib import Path

from common import Refused, rel_path

RCLONE_FAST = ["--transfers", "8", "--checkers", "16", "--drive-chunk-size", "128M", "--retries", "3",
               "--low-level-retries", "10"]


class LocalStore:
    def __init__(self, root):
        self.root = Path(root)
        self.root.mkdir(parents=True, exist_ok=True)

    def path(self, key):
        return self.root / rel_path(key)

    def put(self, local, key):
        """LOCAL's bytes at KEY, whole: a temp file beside it, then a rename."""
        p = self.path(key)
        p.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=p.parent, prefix=".tmp-")
        os.close(fd)
        shutil.copyfile(local, tmp)
        os.replace(tmp, p)

    def get(self, key, local):
        shutil.copyfile(self.path(key), local)

    def read(self, key, limit=1 << 20):
        p = self.path(key)
        if p.stat().st_size > limit:
            raise Refused(f"{key}: over {limit} bytes")
        return p.read_bytes()

    def write(self, key, data):
        with tempfile.NamedTemporaryFile(delete=False) as t:
            t.write(data)
        try:
            self.put(t.name, key)
        finally:
            os.unlink(t.name)

    def size(self, key):
        try:
            return self.path(key).stat().st_size
        except FileNotFoundError:
            return None

    def list(self, prefix):
        d = self.path(prefix)
        return sorted(n for n in os.listdir(d) if not n.startswith(".")) if d.is_dir() else []

    def move(self, src, dst):
        """SRC to DST; False when SRC is gone (another worker took it) or DST is there. A rename in one folder tree is
        atomic, so of two claims one wins."""
        d = self.path(dst)
        d.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.link(self.path(src), d)  # fails if DST is there: never replaces another's
        except (FileNotFoundError, FileExistsError):
            return False
        try:
            os.unlink(self.path(src))
        except FileNotFoundError:
            os.unlink(d)
            return False
        return True

    def delete(self, key):
        try:
            os.unlink(self.path(key))
        except FileNotFoundError:
            pass


class RcloneStore:
    """REMOTE is an rclone path, `gdrive:yaaihh-colab`; rclone's config (0600) holds the Drive sign-in."""

    def __init__(self, remote, rclone="rclone"):
        self.remote, self.rclone = remote.rstrip("/"), rclone
        self.calls = collections.Counter()  # rclone calls by command: each one is Drive API calls

    def _run(self, *args, data=None, ok=(0,)):
        self.calls[args[0]] += 1
        r = subprocess.run([self.rclone, *args], input=data, capture_output=True, timeout=3600)
        if r.returncode not in ok:
            raise OSError(f"rclone {args[0]}: {r.stderr.decode(errors='replace').strip()[-300:]}")
        return r

    def at(self, key):
        return self.remote + ("" if self.remote.endswith(":") else "/") + rel_path(key)

    def put(self, local, key):
        # Drive writes a file whole when its upload ends: a reader sees the old one or the new one, never part
        self._run("copyto", *RCLONE_FAST, str(local), self.at(key))

    def get(self, key, local):
        self._run("copyto", *RCLONE_FAST, self.at(key), str(local))

    def read(self, key, limit=1 << 20):
        """KEY's bytes in one call; one byte past LIMIT is enough to refuse it."""
        r = self._run("cat", "--count", str(limit + 1), self.at(key), ok=(0, 3, 4))
        if r.returncode != 0:
            raise FileNotFoundError(key)
        if len(r.stdout) > limit:
            raise Refused(f"{key}: over {limit} bytes")
        return r.stdout

    def write(self, key, data):
        self._run("rcat", self.at(key), data=data)

    def _ls(self, key):
        r = self._run("lsjson", "--files-only", "--no-modtime", "--no-mimetype", self.at(key), ok=(0, 3, 4))
        return json.loads(r.stdout or b"[]") if r.returncode == 0 else []

    def size(self, key):
        """KEY's size from one call on the file itself (rclone lists a file path as that one file), never its folder."""
        leaf = key.rpartition("/")[2]
        hit = [e for e in self._ls(key) if e.get("Name") == leaf]
        return hit[0]["Size"] if hit else None

    def list(self, prefix):
        return sorted(e["Name"] for e in self._ls(prefix) if not e["Name"].startswith("."))

    def move(self, src, dst):
        """Drive's move is one call that changes the file's folder: of two claims the second finds nothing to move.
        Drive does not promise it, so a job may run twice; the bridge keeps the first result (jobs are deterministic
        and idempotent, see README)."""
        if self.size(dst) is not None:
            return False
        r = self._run("moveto", self.at(src), self.at(dst), ok=(0, 1, 3, 4))
        return r.returncode == 0 and self.size(dst) is not None and self.size(src) is None

    def delete(self, key):
        self._run("deletefile", self.at(key), ok=(0, 3, 4))


def open_store(where):
    """A local folder, or an rclone remote (NAME:path)."""
    return RcloneStore(where) if ":" in where.split("/", 1)[0] else LocalStore(where)
