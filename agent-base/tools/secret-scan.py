#!/usr/bin/env python3
"""Scan git repos for secrets before they leave the machine.

Usage: secret-scan.py REPO...

Two kinds of match:
  known    a secret that exists on this machine: string values under key/token/
           secret-like names in the proxy, Claude, AWS SSO and gh credential
           stores. The first 24 characters are searched for, so truncated
           pastes match too.
  pattern  a well-known token shape (Anthropic, GitHub, AWS, Google, Slack,
           OpenAI-style keys, private keys, JWTs).

Scanned per repo: every blob reachable from any ref (all history, all
branches, so every worktree's commits), plus the uncommitted and untracked
files of every worktree. The output names rule, repo, path and commit or
worktree; it never prints a secret, only its length.
Exit status: 0 clean, 1 findings.
"""
import glob, json, os, re, subprocess, sys

HOME = os.path.expanduser("~")
STORES = [
    f"{HOME}/.local/opt/EasyCLIProxyAPI/config.toml",
    f"{HOME}/.local/opt/EasyCLIProxyAPI/cpa-core/config.yaml",
    *glob.glob(f"{HOME}/.local/opt/EasyCLIProxyAPI/oauth/*.json"),
    f"{HOME}/.claude/.credentials.json",
    *glob.glob(f"{HOME}/.aws/sso/cache/*.json"),
]
SECRET_NAME = re.compile(r"key|token|secret|password|cookie|credential|refresh|access", re.I)
MIN_LEN, NEEDLE = 20, 24

PATTERNS = {
    "anthropic-key": rb"sk-ant-[A-Za-z0-9_-]{20,}",
    "github-token": rb"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})",
    "aws-access-key": rb"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
    "google-api-key": rb"\bAIza[0-9A-Za-z_-]{35}\b",
    "slack-token": rb"\bxox[abprs]-[A-Za-z0-9-]{10,}",
    "openai-style-key": rb"\bsk-(?:proj-)?[A-Za-z0-9]{32,}\b",
    "private-key": rb"-----BEGIN [A-Z ]*PRIVATE KEY-----",
    "jwt": rb"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
}
PATTERNS = {k: re.compile(v) for k, v in PATTERNS.items()}


def plausible(v):
    return len(v) >= MIN_LEN and not re.search(r"\s", v) and not v.startswith(("http", "/", "~"))


def known_secrets():
    """Map needle -> (store, length) for every secret-like value on this machine."""
    found = {}

    def add(v, src):
        if isinstance(v, str) and plausible(v):
            found.setdefault(v[:NEEDLE].encode(), (src, len(v)))

    def walk(o, src, named=False):
        if isinstance(o, dict):
            for k, v in o.items():
                walk(v, src, named or bool(SECRET_NAME.search(str(k))))
        elif isinstance(o, list):
            for v in o:
                walk(v, src, named)
        elif named:
            add(o, src)

    for p in STORES:
        try:
            text = open(p).read()
        except OSError:
            continue
        name = os.path.basename(p)
        try:
            walk(json.loads(text), name)
            continue
        except ValueError:
            pass
        # TOML/YAML: `name = value`, `name: value`, and list items under a secret-like name.
        under = False
        for line in text.splitlines():
            if line.lstrip().startswith("#"):
                continue
            m = re.match(r"\s*([\w.-]+)\s*[:=]\s*(.*)$", line)
            if m:
                under = bool(SECRET_NAME.search(m[1]))
                vals = re.findall(r"['\"]([^'\"]+)['\"]", m[2]) or [m[2].strip()]
            elif (m := re.match(r"\s*-\s*['\"]?([^'\"\s]+)", line)):
                vals = [m[1]]
            else:
                continue
            if under:
                for v in vals:
                    add(v, name)
    # every account gh is logged in to ("Logged in to github.com account NAME")
    status = subprocess.run(["gh", "auth", "status"], capture_output=True, text=True)
    for user in re.findall(r"account (\S+)", status.stdout + status.stderr):
        r = subprocess.run(["gh", "auth", "token", "--user", user], capture_output=True, text=True)
        if r.returncode == 0:
            add(r.stdout.strip(), f"gh:{user}")
    for k, v in os.environ.items():
        if SECRET_NAME.search(k):
            add(v, f"env:{k}")
    return found


def git(repo, *args, **kw):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, check=True, **kw).stdout


def history_blobs(repo):
    """Yield (path, first commit seen, data) for every blob reachable from any ref."""
    objs = git(repo, "rev-list", "--all", "--objects").decode(errors="replace").splitlines()
    paths = {}
    for line in objs:
        sha, _, path = line.partition(" ")
        if path:
            paths.setdefault(sha, path)
    commit_of = {}
    log = git(repo, "log", "--all", "--format=%h", "--raw", "--no-abbrev", "--no-renames").decode(errors="replace")
    cur = None
    for line in log.splitlines():
        if line.startswith(":"):
            sha = line.split()[3]
            commit_of[sha] = cur
        elif line.strip():
            cur = line.strip()
    batch = subprocess.Popen(["git", "-C", repo, "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    for sha, path in paths.items():
        batch.stdin.write(sha.encode() + b"\n")
        batch.stdin.flush()
        header = batch.stdout.readline().split()
        size = int(header[2]) if len(header) == 3 else 0
        data = batch.stdout.read(size) if size else b""
        batch.stdout.read(1)
        if header[1:2] == [b"blob"]:
            yield path, commit_of.get(sha, "?"), data
    batch.stdin.close()
    batch.wait()


def worktree_files(repo):
    """Yield (path, worktree, data) for modified and untracked files of every worktree."""
    out = git(repo, "worktree", "list", "--porcelain").decode().splitlines()
    for wt in (l[9:] for l in out if l.startswith("worktree ")):
        if not os.path.isdir(wt):
            continue
        try:
            names = git(wt, "ls-files", "-z", "-m", "-o", "--exclude-standard").split(b"\0")
        except subprocess.CalledProcessError:
            continue
        for n in filter(None, names):
            p = os.path.join(wt, n.decode(errors="replace"))
            if os.path.isfile(p) and os.path.getsize(p) < 50_000_000:
                with open(p, "rb") as f:
                    yield n.decode(errors="replace"), f"worktree {wt}", f.read()


def main(repos):
    secrets = known_secrets()
    print(f"{len(secrets)} known secret values loaded from local stores", file=sys.stderr)
    hits = set()
    for repo in repos:
        for path, where, data in (*history_blobs(repo), *worktree_files(repo)):
            for needle, (src, n) in secrets.items():
                if needle in data:
                    hits.add(("known", src, f"len {n}", repo, path, where))
            for rule, rx in PATTERNS.items():
                for m in rx.finditer(data):
                    if m.group(0)[:NEEDLE] not in secrets:
                        hits.add(("pattern", rule, f"len {len(m.group(0))}", repo, path, where))
    for h in sorted(hits):
        print("\t".join(h))
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]) if sys.argv[1:] else print(__doc__) or 2)
