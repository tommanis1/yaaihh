"""HTML to Markdown with the standard library: the page's article (or main, or body) with headings, lists, links,
code and tables kept; scripts, styles, navigation and forms left out."""
import html.parser, re, urllib.parse

VOID = {"br", "img", "hr", "meta", "link", "input", "source", "wbr", "col", "area", "base", "embed", "track", "param"}
SKIP = {"script", "style", "noscript", "svg", "nav", "footer", "aside", "form", "template", "iframe", "button",
        "select", "canvas", "object", "head"}
MAX_DEPTH = 256  # deeper tags are flattened into the one at this depth
MAX_HTML = 8 << 20  # the HTML converted; the page's bytes are kept whole all the same
BLOCK = {"p", "div", "section", "article", "main", "header", "figure", "figcaption", "dl", "dt", "dd", "address",
         "details", "summary", "body", "html", "center"}


class Node:
    __slots__ = ("tag", "attrs", "kids", "up")

    def __init__(self, tag, attrs, up):
        self.tag, self.attrs, self.kids, self.up = tag, dict(attrs), [], up


class Tree(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = self.at = Node("#root", [], None)
        self.stack = [self.root]
        self.title, self.in_title = "", False

    def handle_starttag(self, tag, attrs):
        if tag == "title":
            self.in_title = True
        if tag in ("p", "li", "tr", "td", "th", "dt", "dd") and self._open(tag, stop={"ul", "ol", "table", "dl"}) is not None:
            self.handle_endtag(tag)  # an unclosed <p>/<li>/<td> ends at the next of its kind
        if tag not in VOID and len(self.stack) > MAX_DEPTH:
            return
        n = Node(tag, attrs, self.at)
        self.at.kids.append(n)
        if tag not in VOID:
            self.stack.append(n)
            self.at = n

    def handle_startendtag(self, tag, attrs):
        self.at.kids.append(Node(tag, attrs, self.at))

    def _open(self, tag, stop=()):
        """The depth of the innermost open TAG, not looking past a STOP tag; the stack is at most MAX_DEPTH deep."""
        for i in range(len(self.stack) - 1, 0, -1):
            if self.stack[i].tag in stop:
                return None
            if self.stack[i].tag == tag:
                return i
        return None

    def handle_endtag(self, tag):
        if tag == "title":
            self.in_title = False
        i = self._open(tag)
        if i is not None:
            del self.stack[i:]
            self.at = self.stack[-1]

    def handle_data(self, data):
        if self.in_title:
            self.title += data
        self.at.kids.append(data)


def find(n, pred):
    if isinstance(n, str):
        return None
    if pred(n):
        return n
    for k in n.kids:
        f = find(k, pred)
        if f is not None:
            return f
    return None


class Render:
    def __init__(self, base):
        self.base = base

    def kids(self, n, pre=False):
        return "".join(self.node(k, pre) for k in n.kids)

    def node(self, n, pre=False):
        if isinstance(n, str):
            return n if pre else re.sub(r"\s+", " ", n)
        t = n.tag
        if t in SKIP or "hidden" in n.attrs or n.attrs.get("aria-hidden") == "true":
            return ""
        if re.fullmatch(r"h[1-6]", t):
            return f"\n\n{'#' * int(t[1])} {self.kids(n).strip()}\n\n"
        if t in BLOCK:
            return f"\n\n{self.kids(n, pre)}\n\n"
        if t == "br":
            return "\n" if pre else "  \n"
        if t == "hr":
            return "\n\n---\n\n"
        if t in ("strong", "b"):
            s = self.kids(n).strip()
            return f"**{s}** " if s else ""
        if t in ("em", "i"):
            s = self.kids(n).strip()
            return f"*{s}* " if s else ""
        if t == "code" and not pre:
            return f"`{self.kids(n).strip()}`"
        if t == "pre":
            return f"\n\n```\n{self.kids(n, True).strip(chr(10))}\n```\n\n"
        if t == "a":
            s, href = self.kids(n).strip(), n.attrs.get("href") or ""
            if not s or not href or href.startswith(("javascript:", "#")):
                return s
            return f"[{s}]({urllib.parse.urljoin(self.base, href)})"
        if t == "img":
            alt, src = (n.attrs.get("alt") or "").strip(), n.attrs.get("src") or ""
            return f"![{alt}]({urllib.parse.urljoin(self.base, src)})" if alt and src else ""
        if t in ("ul", "ol"):
            return "\n\n" + self.items(n, t == "ol") + "\n\n"
        if t == "blockquote":
            body = squeeze(self.kids(n)).strip()
            return "\n\n" + "\n".join("> " + ln if ln else ">" for ln in body.split("\n")) + "\n\n"
        if t == "table":
            return "\n\n" + self.table(n) + "\n\n"
        return self.kids(n, pre)

    def items(self, n, ordered):
        out, i = [], 0
        for k in n.kids:
            if isinstance(k, str) or k.tag != "li":
                continue
            i += 1
            mark = f"{i}. " if ordered else "- "
            body = [ln for ln in squeeze(self.kids(k)).strip().split("\n") if ln.strip()] or [""]
            out.append(mark + body[0] + "".join("\n" + " " * len(mark) + ln for ln in body[1:]))
        return "\n".join(out)

    def table(self, n):
        rows = []

        def walk(m):
            for k in m.kids:
                if isinstance(k, str) or k.tag == "table":
                    continue
                if k.tag == "tr":
                    cells = []
                    for c in k.kids:
                        if isinstance(c, str) or c.tag not in ("td", "th"):
                            continue
                        s = re.sub(r"\s+", " ", squeeze(self.kids(c))).strip().replace("|", "\\|")
                        span = int(c.attrs.get("colspan") or 1) if (c.attrs.get("colspan") or "1").isdigit() else 1
                        cells += [s] + [""] * (min(span, 50) - 1)
                    if cells:
                        rows.append(cells)
                else:
                    walk(k)
        walk(n)
        if not rows:
            return ""
        w = max(len(r) for r in rows)
        rows = [r + [""] * (w - len(r)) for r in rows]
        lines = ["| " + " | ".join(rows[0]) + " |", "|" + "---|" * w]
        return "\n".join(lines + ["| " + " | ".join(r) + " |" for r in rows[1:]])


def squeeze(s):
    s = re.sub(r"[ \t]+\n", "\n", s)
    return re.sub(r"\n{3,}", "\n\n", s)


def convert(text, base=""):
    """(title, markdown) of the HTML TEXT, links made absolute against BASE."""
    t = Tree()
    t.feed(text[:MAX_HTML])
    t.close()
    root = (find(t.root, lambda n: n.tag == "article") or find(t.root, lambda n: n.tag == "main")
            or find(t.root, lambda n: n.attrs.get("role") == "main") or find(t.root, lambda n: n.tag == "body")
            or t.root)
    md, fenced, out = squeeze(Render(base).node(root)).strip(), False, []
    for ln in md.split("\n"):
        fenced ^= ln.startswith("```")
        out.append(ln if fenced or ln.startswith("```") else re.sub(r"(?<=\S) {2,}(?=\S)", " ", ln.rstrip()))
    return re.sub(r"\s+", " ", t.title).strip(), "\n".join(out) + "\n"
