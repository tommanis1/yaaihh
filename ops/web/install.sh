#!/usr/bin/env bash
# The venv yaaihh-web's browser driver (browserd.py) and PDF reader (pdfread.py) run in: ~/.local/lib/yaaihh-web/venv
# (YAAIHH_WEB_VENV), every wheel checked against requirements.lock's hash, nothing built. It drives the Google Chrome
# already installed (no browser download). The server finds it there with no setting, and then offers the browser.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
venv=${YAAIHH_WEB_VENV:-$HOME/.local/lib/yaaihh-web/venv}
py=$(command -v python3.12 || command -v python3)
"$py" -c 'import sys; sys.exit(sys.version_info[:2] != (3, 12))' || { echo "install.sh: the lock is for CPython 3.12" >&2; exit 1; }
[ -x "$venv/bin/python" ] || "$py" -m venv "$venv"
"$venv/bin/python" -m pip install -q --require-hashes --no-deps --only-binary :all: -r "$here/requirements.lock"
"$venv/bin/python" -I -c 'import importlib.metadata as m, playwright.sync_api, pymupdf4llm; print("yaaihh-web venv: playwright", m.version("playwright"), "pymupdf4llm", m.version("pymupdf4llm"))'
[ -n "${YAAIHH_WEB_VENV:-}" ] && echo "the server finds it while YAAIHH_WEB_VENV=$venv is set for it"
