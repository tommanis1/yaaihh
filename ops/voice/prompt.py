"""The words a take's speech is told to expect: the terms of the global vocab.md and of the take's project's, read
fresh at each take so an edit counts at the next press, made into Whisper's `initial_prompt`."""

import re
from pathlib import Path

# a relative vocab path is read from this folder, wherever the checkout is
HERE = Path(__file__).resolve().parent
# Whisper keeps about 223 prompt tokens; terms are cut at TERMS_CHARS and the take's own words at BEFORE_CHARS
TERMS_CHARS = 500
BEFORE_CHARS = 150
LONGEST_TERM = 40

BOLD = re.compile(r"\*\*(.+?)\*\*")
ITEM = re.compile(r"^\s*[-*]\s+(.*)$")


def terms(text):
    """A vocab.md's terms: each list item's bold words, else its text before `:` or ` — `, else the whole item."""
    found = []
    for line in text.splitlines():
        m = ITEM.match(line)
        if not m:
            continue
        item = m.group(1).strip()
        bold = BOLD.findall(item)
        named = bold if bold else [re.split(r":| — | -- ", item, maxsplit=1)[0]]
        for t in named:
            for one in t.split(","):
                one = one.strip().strip("`").strip()
                if one and len(one) <= LONGEST_TERM:
                    found.append(one)
    return found


def read_terms(path):
    """PATH's terms, or none when it is not there (a project with no vocab.md of its own)."""
    if not path:
        return []
    p = Path(path).expanduser()
    p = p if p.is_absolute() else HERE / p
    return terms(p.read_text(encoding="utf-8")) if p.is_file() else []


def glossary(global_path, project_path):
    """The project's terms first, then the global ones, each once (case aside), cut at TERMS_CHARS."""
    seen, kept, size = set(), [], 0
    for t in read_terms(project_path) + read_terms(global_path):
        if t.lower() in seen:
            continue
        if size + len(t) + 2 > TERMS_CHARS:
            break
        seen.add(t.lower())
        kept.append(t)
        size += len(t) + 2
    return ", ".join(kept)


def prompt(words, before):
    """Whisper's prompt: the glossary, then the end of what this take has already settled, so its sentence goes on."""
    tail = before[-BEFORE_CHARS:].strip()
    parts = [words + "." if words else "", tail]
    return " ".join(p for p in parts if p) or None
