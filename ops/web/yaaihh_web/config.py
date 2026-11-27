"""Where yaaihh-web reads its settings and keys, and where it writes: one JSON file, keys only in 0600 files."""
import json, os, pwd, stat
from pathlib import Path

from .mcp import ToolError

REAL_HOME = Path(pwd.getpwuid(os.getuid()).pw_dir)  # the user's, whatever HOME an agent runs with
VENV = Path(os.environ.get("YAAIHH_WEB_VENV") or REAL_HOME / ".local/lib/yaaihh-web/venv")  # install.sh's
CHROME = ("/opt/google/chrome/chrome", "/opt/google/chrome-beta/chrome")  # where channel="chrome" finds it

DEFAULTS = {
    "searxng": {"url": "http://127.0.0.1:8888"},
    "exa": {"url": "https://api.exa.ai/search", "key_file": None},
    "parallel": {"url": "https://api.parallel.ai/v1beta/search", "key_file": None},
    "brave": {"url": "https://api.search.brave.com/res/v1/web/search", "key_file": None},
    "engines": ["searxng", "exa", "parallel"],  # brave only when named, or when these give nothing
    "fetch": {"max_bytes": 64 << 20, "timeout": 30, "allow_private": False, "max_redirects": 5,
              "sources_cap": 2 << 30, "reader_path": []},
    "profiles_root": str(REAL_HOME / ".local/share/yaaihh-web/profiles"),
    # the driver: browser.driver (a command, a later CDP binary) or browser.python running browserd.py; both found
    # in install.sh's venv when it is there, with no setting
    "browser": {"python": None, "driver": None, "timeout": 120, "max_chars": 40000},
}


def venv_defaults(cfg):
    py = VENV / "bin/python"
    if not py.exists():
        return
    if not cfg["browser"]["python"] and not cfg["browser"]["driver"]:
        cfg["browser"]["python"] = str(py)
    if not cfg["fetch"]["reader_path"]:
        cfg["fetch"]["reader_path"] = [str(p) for p in sorted((VENV / "lib").glob("python3*/site-packages"))]


def load(path=None):
    path = Path(path or os.environ.get("YAAIHH_WEB_CONFIG") or Path.home() / ".config/yaaihh-web/config.json")
    given = json.loads(path.read_text()) if path.is_file() else {}
    cfg = {k: (dict(v) if isinstance(v, dict) else v) for k, v in DEFAULTS.items()}
    for k, v in given.items():
        if k not in cfg:
            raise ValueError(f"{path}: no setting {k}")
        cfg[k] = {**cfg[k], **v} if isinstance(cfg[k], dict) else v
    venv_defaults(cfg)
    return cfg


def key(cfg, engine):
    """ENGINE's API key from its key_file, which only its owner may read; None when no file is set."""
    f = cfg[engine].get("key_file")
    if not f:
        return None
    st = os.stat(f)
    if st.st_mode & (stat.S_IRWXG | stat.S_IRWXO) or st.st_uid != os.getuid():
        raise ToolError(f"{f}: a key file must be yours and mode 0600")
    return Path(f).read_text().strip()


def agent_home():
    """The calling agent's home: AGENT_HOME, else COORD/agents/AGENT_NAME/home (Codex passes no AGENT_HOME). Both are
    the one path inside the agent's box and outside it."""
    home = os.environ.get("AGENT_HOME")
    if not home and os.environ.get("COORD") and os.environ.get("AGENT_NAME"):
        home = str(Path(os.environ["COORD"]) / "agents" / os.environ["AGENT_NAME"] / "home")
    if not home:
        raise ToolError("no AGENT_HOME (nor COORD and AGENT_NAME): nowhere to keep the source")
    return Path(home)


def sources_dir():
    d = agent_home() / "sources"
    d.mkdir(parents=True, exist_ok=True, mode=0o700)
    return d
