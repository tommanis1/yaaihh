"""Tier 1: raw search. Each engine's ranked hits as {url, title, snippet, date, mime, engines}, merged by URL in
rank order; an engine's text is the page's own words (SearXNG's content, Exa's highlights, Parallel's excerpts,
Brave's description), never a model's summary of them."""
import mimetypes, urllib.parse

from . import config, net
from .mcp import ToolError

ENGINES = ("searxng", "exa", "parallel", "brave")
DROP_PARAMS = ("utm_", "fbclid", "gclid")


def norm(url):
    u = urllib.parse.urlsplit(url.strip())
    q = [(k, v) for k, v in urllib.parse.parse_qsl(u.query, keep_blank_values=True) if not k.startswith(DROP_PARAMS)]
    return urllib.parse.urlunsplit((u.scheme.lower(), (u.hostname or "").lower() + (f":{u.port}" if u.port else ""),
                                    u.path.rstrip("/") or "/", urllib.parse.urlencode(q), ""))


def hit(url, title, snippet, date):
    if not isinstance(url, str) or not url.startswith(("http://", "https://")):
        return None
    return {"url": url, "title": (title or "").strip(), "snippet": (snippet or "").strip(), "date": date or None,
            "mime": mimetypes.guess_type(urllib.parse.urlsplit(url).path)[0]}


def searxng(cfg, q, n):
    base = cfg["searxng"]["url"].rstrip("/")
    d = net.api("GET", f"{base}/search?" + urllib.parse.urlencode({"q": q, "format": "json"}))
    return [hit(r.get("url"), r.get("title"), r.get("content"), r.get("publishedDate"))
            for r in (d.get("results") or [])[:n]]


def exa(cfg, q, n):
    k = config.key(cfg, "exa")
    if not k:
        return None
    d = net.api("POST", cfg["exa"]["url"], {"x-api-key": k},
                {"query": q, "numResults": n, "type": "auto", "contents": {"highlights": {"numSentences": 3}}})
    return [hit(r.get("url"), r.get("title"), " … ".join(r.get("highlights") or []), r.get("publishedDate"))
            for r in d.get("results") or []]


def parallel(cfg, q, n):
    k = config.key(cfg, "parallel")
    if not k:
        return None
    d = net.api("POST", cfg["parallel"]["url"], {"x-api-key": k},
                {"objective": q, "search_queries": [q], "max_results": n})
    return [hit(r.get("url"), r.get("title"), " … ".join(r.get("excerpts") or []), r.get("publish_date"))
            for r in d.get("results") or []]


def brave(cfg, q, n):
    k = config.key(cfg, "brave")
    if not k:
        return None
    d = net.api("GET", cfg["brave"]["url"] + "?" + urllib.parse.urlencode({"q": q, "count": min(n, 20)}),
                {"X-Subscription-Token": k})
    return [hit(r.get("url"), r.get("title"), r.get("description"), r.get("page_age"))
            for r in (d.get("web") or {}).get("results") or []]


RUN = {"searxng": searxng, "exa": exa, "parallel": parallel, "brave": brave}


def search(cfg, args):
    q, n = args["query"].strip(), args.get("count", 10)
    if not q:
        raise ToolError("an empty query")
    if args.get("site"):
        q = f"site:{args['site']} {q}"
    asked = args.get("engines")
    order = asked or list(cfg["engines"])
    lists, used, errors = [], [], {}
    for e in order:
        try:
            got = RUN[e](cfg, q, n)
        except ToolError as err:
            errors[e] = str(err)
            continue
        if got is None:
            errors[e] = "no key_file set"
            continue
        used.append(e)
        lists.append((e, [h for h in got if h]))
    if not asked and not any(hs for _, hs in lists) and "brave" not in order:
        try:
            got = brave(cfg, q, n)
            if got is not None:
                used.append("brave")
                lists.append(("brave", [h for h in got if h]))
        except ToolError as err:
            errors["brave"] = str(err)
    merged, at = [], {}
    for rank in range(max((len(hs) for _, hs in lists), default=0)):
        for e, hs in lists:
            if rank >= len(hs):
                continue
            h, key = hs[rank], norm(hs[rank]["url"])
            if key in at:
                m = at[key]
                m["engines"].append(e)
                m["snippet"] = m["snippet"] or h["snippet"]
                m["date"] = m["date"] or h["date"]
                continue
            at[key] = {**h, "engines": [e], "rank": len(merged) + 1}
            merged.append(at[key])
    if not merged and errors and not used:
        raise ToolError("no engine answered: " + "; ".join(f"{e}: {m}" for e, m in errors.items()))
    return {"query": q, "engines": used, "errors": errors, "results": merged[:n]}


TOOL = {
    "name": "web_search",
    "description": "Raw ranked web results {url, title, snippet, date, mime, engines} from SearXNG, Exa and Parallel "
                   "(Brave when they give nothing or when named). Snippets are the pages' own words, not a summary: "
                   "fetch a result with web_fetch to read it.",
    "inputSchema": {"type": "object", "additionalProperties": False, "required": ["query"], "properties": {
        "query": {"type": "string", "maxLength": 2000},
        "count": {"type": "integer", "minimum": 1, "maximum": 50},
        "engines": {"type": "array", "items": {"type": "string", "enum": list(ENGINES)}},
        "site": {"type": "string", "maxLength": 253}}},
}
