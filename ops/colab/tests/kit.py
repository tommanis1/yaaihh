"""What both layers' tests build: a git repo, a toolchain packed by pack-toolchain.sh, jobs submitted by submit.py."""
import io, subprocess, sys, tarfile, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
import submit  # noqa: E402
from store import LocalStore  # noqa: E402

GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false"]


class Kit(unittest.TestCase):
    def setUp(self):
        self.t = Path(tempfile.mkdtemp(prefix="colab-test-"))
        self.addCleanup(subprocess.run, ["rm", "-rf", str(self.t)])
        self.repo = self.t / "repo"
        self.repo.mkdir()
        (self.repo / "make.sh").write_text("mkdir -p out\ngreet > out/hello.txt\necho built on $(greet)\n")
        subprocess.run(GIT + ["-C", str(self.repo), "init", "-q"], check=True)
        subprocess.run(GIT + ["-C", str(self.repo), "add", "-A"], check=True)
        subprocess.run(GIT + ["-C", str(self.repo), "commit", "-qm", "one"], check=True)
        tc = self.t / "tc"
        (tc / "bin").mkdir(parents=True)
        (tc / "bin" / "greet").write_text("#!/bin/sh\necho hello-from-toolchain\n")
        (tc / "bin" / "greet").chmod(0o755)
        (tc / "toolchain.json").write_text('{"path": ["bin"]}')
        r = subprocess.run([str(HERE / "pack-toolchain.sh"), "greet", str(tc), str(self.t / "packs")],
                           capture_output=True, text=True, check=True)
        self.toolchain_file = Path(r.stdout.strip())

    def queue(self, root):
        store = LocalStore(root)
        tname, tsha = submit.toolchain(store, self.toolchain_file)
        self.tc = f"{tname}:{tsha}"
        return store

    def job(self, store, cmd=("sh", "make.sh"), out=("out",), **kw):
        args = ["job", "--store", "-", "--repo", str(self.repo), "--rev", "HEAD", "--toolchain", self.tc]
        for k, v in kw.items():
            args += [f"--{k.replace('_', '-')}={v}"]
        for o in out:
            args += ["--out", o]
        return submit.job(store, submit.args_of(args + ["--", *cmd]))


def evil_tar(path, kind):
    """A tar.zst at PATH holding one member of KIND: symlink, hardlink, dotdot, absolute, device."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as t:
        i = tarfile.TarInfo({"dotdot": "../escape", "absolute": "/tmp/escape"}.get(kind, "x"))
        if kind == "symlink":
            i.type, i.linkname = tarfile.SYMTYPE, "/etc/passwd"
        elif kind == "hardlink":
            i.type, i.linkname = tarfile.LNKTYPE, "/etc/passwd"
        elif kind == "device":
            i.type = tarfile.CHRTYPE
        data = b"" if kind in ("symlink", "hardlink", "device") else b"x"
        i.size = len(data)
        t.addfile(i, io.BytesIO(data))
    subprocess.run(["zstd", "-q", "-f", "-o", str(path)], input=buf.getvalue(), check=True)
    return path

