"""Tier 4's profiles: one Chromium user-data-dir per role (shopping, papers-uni, research-general) under
profiles_root, or the agent's own (`own`, in its home), held by one browser at a time under an exclusive lock. A
reader that needs no writes takes a throwaway copy, made only under the same lock (so no browser writes the profile's
databases meanwhile), refusing any link or special file in it, and kept in the agent's own home."""
import fcntl, os, re, shutil, stat, tempfile
from pathlib import Path

from . import config
from .mcp import ToolError

NAME = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")
LOCK = ".yaaihh-web.lock"
# Chromium's own locks and what a copy never needs: caches, crash dumps, the running browser's sockets
NOT_COPIED = {"SingletonLock", "SingletonSocket", "SingletonCookie", "Cache", "Code Cache", "GPUCache",
              "ShaderCache", "GrShaderCache", "Crashpad", "lockfile", LOCK}


def path(cfg, name, home):
    """The profile NAME's folder: `own` is the agent's own (HOME/profile), any other a role's under profiles_root."""
    if not NAME.match(name or ""):
        raise ToolError(f"profile {name!r}: lowercase letters, digits, - and _")
    return Path(home) / "profile" if name == "own" else Path(cfg["profiles_root"]).expanduser() / name


def folder(cfg, name):
    p = path(cfg, name, config.agent_home())
    p.mkdir(parents=True, exist_ok=True, mode=0o700)
    return p


class Held:
    """The profile at DIR locked for one browser: ToolError naming who holds it when another does."""

    def __init__(self, d, who):
        self.dir = Path(d)
        self.lockf = open(self.dir / LOCK, "a+")
        try:
            fcntl.flock(self.lockf, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.lockf.seek(0)
            holder = self.lockf.read().strip() or "another browser"
            self.lockf.close()
            raise ToolError(f"profile {self.dir.name} is held by {holder}: wait, or open it read_only for a copy")
        self.lockf.seek(0)
        self.lockf.truncate()
        self.lockf.write(f"{who} (pid {os.getpid()})")
        self.lockf.flush()

    def release(self):
        if self.lockf and not self.lockf.closed:
            self.lockf.seek(0)
            self.lockf.truncate()
            fcntl.flock(self.lockf, fcntl.LOCK_UN)
            self.lockf.close()


def copies_dir():
    d = config.agent_home() / ".web-copies"
    d.mkdir(mode=0o700, parents=True, exist_ok=True)
    return d


def clear_copies():
    """The copies a server that was killed left in the agent's home (sign-in cookies), gone."""
    for c in copies_dir().iterdir():
        if c.is_dir() and not c.is_symlink():
            shutil.rmtree(c, ignore_errors=True)


class Copy:
    """A throwaway copy of the profile at DIR for a reader: its sign-ins as they are now, none of its writes kept.
    Made under the profile's lock (no browser of ours writes its databases meanwhile), in the agent's own home; a
    link or a special file anywhere in the profile refuses it, checked on each entry as it is copied."""

    def __init__(self, d, who):
        try:
            held = Held(d, f"{who}, copying")
        except ToolError as e:
            raise ToolError(f"{e.args[0].split(':')[0]}: a copy is made only while no browser has it open")
        self.dir = Path(tempfile.mkdtemp(prefix=f"{Path(d).name}-", dir=copies_dir()))
        try:
            copy_plain(Path(d), self.dir, Path(d))
        except BaseException:
            self.release()
            raise
        finally:
            held.release()

    def release(self):
        shutil.rmtree(self.dir, ignore_errors=True)


def copy_plain(src, dst, top):
    """SRC's plain files and folders into DST; ToolError at the first link or special file (Chromium's own locks and
    caches are left out, links among them too)."""
    with os.scandir(src) as it:
        for e in it:
            if e.name in NOT_COPIED:
                continue
            where = os.path.relpath(e.path, top)
            if e.is_dir(follow_symlinks=False):
                os.mkdir(dst / e.name, 0o700)
                copy_plain(Path(e.path), dst / e.name, top)
            elif e.is_file(follow_symlinks=False):
                fd = os.open(e.path, os.O_RDONLY | os.O_NOFOLLOW)
                with os.fdopen(fd, "rb") as f:
                    if not stat.S_ISREG(os.fstat(fd).st_mode):
                        raise ToolError(f"profile {top.name}: {where} is not a plain file: no copy is made")
                    with open(os.open(dst / e.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as out:
                        shutil.copyfileobj(f, out)
            else:
                raise ToolError(f"profile {top.name}: {where} is a link or a special file: no copy is made")
