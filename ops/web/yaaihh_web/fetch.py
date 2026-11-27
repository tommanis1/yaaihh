"""Tier 2: a page or PDF read verbatim, and kept. Every fetch is saved in the agent's home/sources/ as
SHA.{pdf,html,txt,bin} (the bytes as they came), SHA.md (what the agent read) and SHA.meta.json (each URL it came
from, when, its type and the engine that read it), so the window opens the PDF itself."""
import fcntl, hashlib, json, os, re, resource, shutil, subprocess, sys, tempfile
from pathlib import Path

from . import config, html2md, net
from .mcp import ToolError

READER = Path(__file__).with_name("pdfread.py")
READER_SECS = 180
TEXT = ("text/plain", "text/markdown", "text/csv", "application/json", "application/xml", "text/xml")


def kind(page):
    b, m = page.body, page.mime
    if b.startswith(b"%PDF-") or m == "application/pdf":
        return "pdf"
    if m in ("text/html", "application/xhtml+xml") or (not m and b.lstrip()[:15].lower().startswith((b"<!doctype", b"<html"))):
        return "html"
    if m in TEXT or m.startswith("text/"):
        return "txt"
    return "bin"


def decode(page):
    m = re.search(r"charset=([\w-]+)", page.headers.get("content-type") or "")
    mb = None if m else re.search(rb"<meta[^>]+charset=[\"']?([\w-]+)", page.body[:4096], re.I)
    enc = m.group(1) if m else mb.group(1).decode() if mb else "utf-8"
    try:
        return page.body.decode(enc, errors="replace")
    except LookupError:
        return page.body.decode("utf-8", errors="replace")


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (2 << 30, 2 << 30))
    resource.setrlimit(resource.RLIMIT_CPU, (120, 120))


def reader(argv, what):
    """ARGV's output, run as a process of its own with 2 GiB, 120 s of CPU and READER_SECS of wall time at most."""
    try:
        r = subprocess.run(argv, capture_output=True, timeout=READER_SECS, preexec_fn=limits, cwd=tempfile.gettempdir(),
                           stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        raise ToolError(f"{what}: not done in {READER_SECS} s")
    return r


def pdf_pages(path, reader_path=()):
    """[(page number, markdown)] of the PDF at PATH: pymupdf4llm's (tables as Markdown) when it is installed, else
    poppler's pdftotext -layout (columns kept by spacing). Either reader runs under reader()'s limits."""
    r = reader([sys.executable, "-I", str(READER), str(path), *reader_path], "pymupdf4llm")
    if r.returncode == 0:
        try:
            return [(int(n), t) for n, t in json.loads(r.stdout)], "pymupdf4llm"
        except (ValueError, TypeError):
            raise ToolError("pymupdf4llm: an answer that is not its pages")
    if r.returncode != 3:
        raise ToolError(f"pymupdf4llm: exit {r.returncode}: {r.stderr.decode(errors='replace').strip()[-300:]}")
    if not shutil.which("pdftotext"):
        raise ToolError("no PDF reader: install pymupdf4llm or poppler's pdftotext")
    r = reader(["pdftotext", "-layout", "-enc", "UTF-8", str(path), "-"], "pdftotext")
    if r.returncode != 0:
        raise ToolError(f"pdftotext: {r.stderr.decode(errors='replace').strip()[-300:]}")
    pages = r.stdout.decode("utf-8", errors="replace").split("\f")
    if pages and not pages[-1].strip():
        pages.pop()
    return [(i + 1, p.rstrip()) for i, p in enumerate(pages)], "pdftotext"


def page_range(spec, n):
    if not spec:
        return set(range(1, n + 1))
    got = set()
    for part in spec.split(","):
        a, _, b = part.strip().partition("-")
        if not a.isdigit() or (b and not b.isdigit()):
            raise ToolError(f"pages {spec!r}: like 1-3,7")
        got |= set(range(max(int(a), 1), min(int(b or a), n) + 1))
    return got


def write(path, data):
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=".tmp-")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data if isinstance(data, bytes) else data.encode())
        os.replace(tmp, path)
    except BaseException:
        os.unlink(tmp)
        raise


def counted(d):
    """The bytes of sources/'s own files, counted from the folder and kept in sources/.used."""
    n = sum(e.stat().st_size for e in os.scandir(d) if e.is_file(follow_symlinks=False) and not e.name.startswith("."))
    write(d / ".used", str(n))
    return n


def used(d):
    """sources/.used, the running total; the folder counted when it is missing or unreadable."""
    try:
        return int((d / ".used").read_text())
    except (OSError, ValueError):
        return counted(d)


def put(d, path, data):
    """DATA at PATH in sources/, the running total moved by the change in PATH's size (the caller holds the lock)."""
    old = path.stat().st_size if path.exists() else 0
    write(path, data)
    write(d / ".used", str(max(used(d) + path.stat().st_size - old, 0)))


def drop(d, path):
    with open(d / ".lock", "a") as lk:
        fcntl.flock(lk, fcntl.LOCK_EX)
        if path.exists():
            n = path.stat().st_size
            path.unlink()
            write(d / ".used", str(max(used(d) - n, 0)))


def room(d, path, n, cap):
    """N more bytes at PATH (none when it is there) fit in sources/ under CAP, else ToolError: nothing is removed, the
    sources are the agent's record of what it read. The running total is checked; the folder is counted again only
    before a refusal (the agent may have removed sources by hand)."""
    if path.exists():
        return
    if used(d) + n <= cap:
        return
    used_now = counted(d)
    if used_now + n > cap:
        raise ToolError(f"{d} holds {used_now} bytes; {n} more is over its cap of {cap} (fetch.sources_cap): "
                        "remove the sources no longer needed, or ask the owner to raise the cap")


def keep(page, sha, ext, md, meta, cap):
    """The fetch in sources/: the bytes and their Markdown once per SHA, the meta with each fetch of it added (under
    sources/.lock, a meta that cannot be read taken as none)."""
    d = config.sources_dir()
    raw, mdp, metap = d / f"{sha}.{ext}", d / f"{sha}.md", d / f"{sha}.meta.json"
    with open(d / ".lock", "a") as lk:
        fcntl.flock(lk, fcntl.LOCK_EX)
        room(d, raw, len(page.body), cap)
        if not raw.exists():
            put(d, raw, page.body)
        if md is not None and not mdp.exists():
            room(d, mdp, len(md.encode()), cap)
            put(d, mdp, md)
        try:
            old = json.loads(metap.read_text())
        except (OSError, ValueError):
            old = {}
        fetches = (old.get("fetches") if isinstance(old, dict) and isinstance(old.get("fetches"), list) else []) + [
            {"url": page.url, "final_url": page.final_url, "fetched_at": page.fetched_at, "status": page.status}]
        put(d, metap, json.dumps({**meta, "fetches": fetches[-100:]}, indent=1, ensure_ascii=False))
    return {"raw": str(raw), "md": str(mdp) if md is not None else None, "meta": str(metap)}


def fetch(cfg, args):
    page = net.get(args["url"], cfg["fetch"],
                   accept="text/html,application/xhtml+xml,application/pdf;q=0.9,text/*;q=0.8,*/*;q=0.5")
    return read(cfg, page, args)


def read(cfg, page, args):
    """PAGE (a net.Page, from web_fetch or the browser) read as its kind and kept in sources/; ARGS' pages, start
    and max_chars pick what is returned."""
    if page.status >= 400:
        raise ToolError(f"{page.final_url}: HTTP {page.status}")
    sha, k = hashlib.sha256(page.body).hexdigest(), kind(page)
    title, md, pages, engine = "", None, None, None
    cap = cfg["fetch"]["sources_cap"]
    if k == "pdf":
        d = config.sources_dir()
        raw = d / f"{sha}.pdf"
        with open(d / ".lock", "a") as lk:
            fcntl.flock(lk, fcntl.LOCK_EX)
            room(d, raw, len(page.body), cap)
            new = not raw.exists()
            if new:
                put(d, raw, page.body)
        try:
            got, engine = pdf_pages(raw, cfg["fetch"]["reader_path"])
        except BaseException:
            if new:  # a PDF no reader could read is not kept with no meta beside it
                drop(d, raw)
            raise
        pages = len(got)
        md = "".join(f"\n\n<!-- page {n} -->\n\n{t}" for n, t in got).strip() + "\n"
    elif k == "html":
        title, md = html2md.convert(decode(page), page.final_url)
        engine = "html2md"
    elif k == "txt":
        md, engine = decode(page), "text"
    meta = {"sha256": sha, "mime": page.mime or None, "kind": k, "bytes": len(page.body), "title": title or None,
            "engine": engine, "pages": pages}
    saved = keep(page, sha, k, md, meta, cap)
    out = {"url": page.url, "final_url": page.final_url, "status": page.status, "mime": page.mime or None, "kind": k,
           "sha256": sha, "title": title or None, "pages": pages, "engine": engine, "saved": saved}
    if md is None:
        out["markdown"] = None
        out["note"] = "not text: the bytes are kept at saved.raw"
        return out
    if k == "pdf" and args.get("pages"):
        want = page_range(args["pages"], pages)
        md = "".join(f"\n\n<!-- page {n} -->\n\n{t}" for n, t in got if n in want).strip() + "\n"
    start, size = args.get("start", 0), args.get("max_chars", 40000)
    out["chars"], out["start"] = len(md), start
    out["markdown"] = md[start:start + size]
    if start + size < len(md):
        out["more"] = f"{len(md) - start - size} characters more: call again with start={start + size}"
    return out


TOOL = {
    "name": "web_fetch",
    "description": "A URL read verbatim: HTML as Markdown with its tables, a PDF as its text page by page "
                   "(<!-- page N --> marks), text as it is. The bytes, the Markdown and a meta file (URL, time, "
                   "SHA-256, type) are kept in your home/sources/, so the PDF opens in the window's viewer. Long "
                   "pages come in parts: start and max_chars.",
    "inputSchema": {"type": "object", "additionalProperties": False, "required": ["url"], "properties": {
        "url": {"type": "string", "maxLength": 8192},
        "pages": {"type": "string", "maxLength": 200},
        "start": {"type": "integer", "minimum": 0, "maximum": 1 << 31},
        "max_chars": {"type": "integer", "minimum": 1000, "maximum": 200000}}},
}
