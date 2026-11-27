# yaaihh-web

One MCP server, `web`, shared by every agent: Claude, Codex, opencode and Agy. It gives raw search results, a
verbatim reading of pages and PDFs that is kept as a source, a real browser, and browser profiles that keep their
sign-ins. It never returns a summary written by a model, and it never gets past a CAPTCHA by itself: a person does.

| Tier | Tools | State |
|---|---|---|
| 1. Search | `web_search` | built, tested |
| 2. Read and keep | `web_fetch` | built, tested (tables in PDFs with `pymupdf4llm`, from the venv) |
| 3. Browser | `web_open`, `web_act`, `web_pdf` | built, tested on the real Chrome |
| 4. Profiles and handoff | `profile`, `read_only`; `needs_human` and `yaaihh-web human FILE` | built, tested; the window's card is proto-window's |

## Wiring

- `install-mcp.sh COORD` links `$COORD/mcp/web` to `yaaihh-web`. roster already gives every executable in
  `$COORD/mcp/` to each agent kind, so no harness change is needed. Codex passes only named variables, so the
  server finds the agent's home from `COORD` and `AGENT_NAME` when `AGENT_HOME` is missing.
- Settings are in `~/.config/yaaihh-web/config.json`, or the file `YAAIHH_WEB_CONFIG` names. Every key is optional:

  ```json
  {"searxng": {"url": "http://127.0.0.1:8888"},
   "exa": {"key_file": "/path/0600"}, "parallel": {"key_file": "/path/0600"}, "brave": {"key_file": "/path/0600"},
   "engines": ["searxng", "exa", "parallel"],
   "fetch": {"max_bytes": 67108864, "timeout": 30, "allow_private": false, "max_redirects": 5,
             "sources_cap": 2147483648, "reader_path": []},
   "profiles_root": "~/.local/share/yaaihh-web/profiles",
   "browser": {"python": null, "driver": null, "timeout": 120, "max_chars": 40000}}
  ```

  An API key is read only from a file that only its owner can read (0600). It is never taken from the command line
  or the environment, and never written to a result or a log.
- `searxng/run.sh` starts SearXNG on 127.0.0.1:8888 in podman, or in docker where podman is not installed:
  - the image is pinned by its digest (`2026.11.26-b5605b8f2`);
  - JSON answers are on, and the limiter is off, since there is only one user;
  - all capabilities are dropped except the three it needs to start;
  - the settings file is read-only;
  - its secret is made once, in a 0600 file.
- The server itself uses only the Python standard library. The browser driver and `pymupdf4llm` run as processes
  of their own, from the venv `install.sh` makes, `~/.local/lib/yaaihh-web/venv` (or `YAAIHH_WEB_VENV`).
  - The server finds it there with no setting: `browser.python` and `fetch.reader_path` default to it. `~` is the
    user's own home, whatever `HOME` an agent runs with.
  - `web_open`, `web_act` and `web_pdf` are listed only when there is a driver and Google Chrome is installed.
    Elsewhere, in an agent's box for one, the server offers `web_search` and `web_fetch` alone.

## Tier 1: `web_search`

Arguments: `query`, `count` (1 to 50, default 10), `engines` (any of `searxng`, `exa`, `parallel`, `brave`), `site`.

It returns `{query, engines, errors, results: [{rank, url, title, snippet, date, mime, engines}]}`.

- Each engine's hits are merged in rank order: rank 1 of every engine, then rank 2, and so on. A URL found twice is
  kept once (scheme and host lowercased, fragment, trailing `/` and `utm_*`/`fbclid`/`gclid` dropped), and `engines`
  lists every engine that gave it.
- A snippet is always the page's own text: SearXNG's `content`, Exa's `highlights` (Exa is never asked for a
  `summary`), Parallel's `excerpts`, Brave's `description`.
- `mime` is guessed from the URL's suffix, so a PDF shows as one before it is fetched.
- An engine that fails or has no key is named in `errors`, and the others still answer. Brave is asked only when it
  is named, or when the default engines give nothing.

## Tier 2: `web_fetch`

Arguments: `url`, `pages` (for a PDF, like `1-3,7`), `start` and `max_chars` (a long page comes in parts, with
`more` saying where the next part starts).

- **Network.**
  - Only http and https are fetched, and a URL with a user name or password is refused.
  - Every address the host resolves to must be public. Loopback, private ranges, link-local (169.254.169.254),
    IPv4-mapped forms and 0.0.0.0 are refused unless `allow_private` is set.
  - The connection goes to the address that was checked, so a second DNS lookup cannot change it; TLS still checks
    the host's own name.
  - Every redirect is checked the same way, and the body is capped at `max_bytes`.
  - The whole fetch, redirects included, must end within 4 × `timeout`, so a server that sends a byte now and then
    can't hold it open.
  - An IPv4 address inside a NAT64 address (`64:ff9b::/96`, `64:ff9b:1::/48`) is checked as that IPv4 address.
- **Reading.**
  - HTML becomes Markdown, keeping the headings, lists, links (made absolute), code and tables. It takes the
    `<article>`, else `<main>`, else the body, and leaves out scripts, navigation, footers, forms and hidden
    parts.
  - Only the first 8 MiB of HTML is converted, and tags nested deeper than 256 are flattened, so the cost stays
    linear. The bytes are still kept whole.
  - A PDF is read page by page, each page marked `<!-- page N -->`. `pymupdf4llm` is used when it is installed, and
    turns tables into Markdown tables. Otherwise poppler's `pdftotext -layout` is used, and table columns are kept
    by spacing.
  - Either PDF reader runs as a process of its own (`pdfread.py` for pymupdf4llm), with at most 2 GiB of memory,
    120 s of CPU and 180 s in all. `reader_path` names folders where pymupdf4llm is installed outside the server's
    Python, such as a venv's site-packages.
  - A page range is clipped to the PDF's own pages.
  - Text is returned as it is. Other types are kept, with no text.
- **Kept.** Everything goes in the agent's `home/sources/`:
  - `SHA.{pdf,html,txt,bin}` holds the bytes exactly as they came;
  - `SHA.md` holds what the agent read;
  - `SHA.meta.json` holds `{sha256, mime, kind, bytes, title, engine, pages, fetches: [{url, final_url,
    fetched_at, status}]}`.

  SHA is the SHA-256 of the bytes. The same bytes fetched again add one entry to `fetches`, under a lock in
  `sources/.lock`; a meta file that can't be read is taken as empty. Files' PDF view opens `SHA.pdf` like any other
  PDF.

  `sources/` holds at most `sources_cap` bytes (2 GiB by default). A fetch that would go over it is refused with a
  message saying so, and nothing is removed: the sources are the agent's record of what it read. The agent removes
  what it no longer needs, or the owner raises the cap. The total is kept in `sources/.used`, under the lock, as
  files are written and removed. The folder is counted only when `.used` is missing or unreadable, and once more
  before a refusal, in case the agent removed sources by hand.

## Tier 3: the browser

The browser is the installed Google Chrome, driven by `yaaihh_web/browserd.py` (Playwright for Python) in the venv
`install.sh` makes. An agent's tabs run headless; only the owner's side of a handoff opens a window. Playwright's own
MCP server is not used: the profile locks, the source archive and the gate rule stay in this one server.

- **`web_open {url, profile, read_only}`** opens a tab and answers its id with the page's accessibility tree, a `ref`
  on each element that can be acted on (`button "Go" [ref=e4]`). It sends no screenshot and no DOM, and the tree is
  cut at `browser.max_chars` (40,000). `profile` is a role's (`shopping`, `papers-uni`, `research-general`, the
  default) or `own`.
- **`web_act {tab, action, ref, text}`** does one of these, then answers the page as `web_open` does:
  - `click`, `type` (the text), `press` (a key, Enter by default) or `select`, each on a ref;
  - `snapshot`, `back`, `scroll`;
  - `close`, which closes that tab and removes the PDFs caught in it, and lets the profile go when it was the
    profile's last tab.
- **`web_pdf {tab, pick | url, pages, start, max_chars}`**:
  - With nothing but `tab`, it lists the PDFs the tab got and downloaded.
  - With `pick` (an `n` from that list), that PDF is read and kept in `sources/` as `web_fetch` keeps it: verbatim,
    under `sources/.lock`, counted in `.used`.
  - With `url`, that URL is fetched in the tab's session (its cookies) and kept the same way.

  A PDF is caught on the wire: every document, fetch and XHR request of an agent's tab is fetched by the driver and
  handed on to the page, and a PDF's bytes are kept as they pass, at most 20 per tab. Chrome itself keeps no copy of
  a body the page reads as a blob, which is how many JS viewers read a paper. Service workers are blocked, so that
  no request goes around this. A request with a `Range` header (a video, a resumed download), and every image,
  media, font, script and style request, goes on untouched and is never held whole. A PDF whose Content-Length is
  over `fetch.max_bytes` isn't kept, and `web_pdf url=` refuses it before reading its body.
- **Addresses.** Chrome reaches the net only through `proxy.py`, an HTTP proxy on loopback that the server runs.
  - Loopback goes through it too. QUIC is off, and WebRTC sends no UDP off the proxy.
  - For each connection, the proxy looks the name up with `web_fetch`'s rule (`net.resolve`), and connects to the
    checked address alone. A name that answers public and then private reaches nothing private. That holds for
    CONNECT (https, wss) and for plain http and ws, one request a connection.
  - The driver's own requests go through the same proxy: `web_pdf`'s fetches, and each redirect, which the driver
    follows itself, 5 at most.
  - `web_open` refuses a host that isn't public before it opens anything. An answer names the addresses the proxy
    refused for that page (`refused`).
  - `fetch.allow_private` lifts the rule, for tests.

Checked live on 27 Nov 2026:
- On arXiv `1706.03762`, the abstract page's tree was 17,057 characters. Clicking "View PDF" got the PDF caught,
  and its 15 pages were read by pymupdf4llm in 2.4 s.
- `github.com/login` gave `needs_human` (a sign-in form).

## Tier 4: profiles and the person at the gate

- **Profiles.** A profile is one Chrome user-data-dir:
  - a role's, under `profiles_root` (`~/.local/share/yaaihh-web/profiles/NAME`), for the agents of that role in
    turn;
  - or `own`, the agent's own, in its home (`home/profile`), which no other agent opens.

  One browser holds a profile at a time, under an exclusive lock, and a second caller is told who holds it.
- **`read_only`** gives a throwaway copy, holding the profile's sign-ins as they are now and keeping none of its
  writes.
  - The copy is made only under the profile's lock. While a browser has the profile open, no copy is made, so no
    database is copied mid-write.
  - Each entry is checked as it is copied: a file is opened with `O_NOFOLLOW` and must be a plain file. A link or a
    special file anywhere in the profile refuses the whole copy and leaves nothing behind.
  - Chrome's own locks and caches are left out.
  - The copy goes in the agent's own home (`home/.web-copies/`, mode 700). It is removed when its last tab closes,
    and those a killed server left are cleared when the next browser starts. So a role profile's cookies reach only
    the agent that asked, and never `/tmp`.
- **The gate** (`gate.py`). Every page `web_open` and `web_act` answer is checked first. Any of these counts:
  - a challenge frame: Cloudflare Turnstile, reCAPTCHA, hCaptcha, Arkose, DataDome or PerimeterX;
  - `cf-mitigated: challenge`;
  - a "Just a moment" or "verify you are human" page (by its title, or by its text on a 403, 429 or 503);
  - a 401 or 407;
  - an institutional sign-in, by its host (Microsoft, Google, Okta, OpenAthens, Shibboleth/`idp.`, `sso.`, `wayf.`)
    or its path (SAML, CAS, WAYF);
  - a visible password field;
  - a sign-in path (`/login`, `/signin`, `oauth/authorize`).

  Nothing in this server solves, clicks through, waits out or retries a gate.
- **The handoff.** At a gate the tool answers
  `{status: "needs_human", reason, url, profile, handoff, tell_owner}` and nothing of the page.
  1. The tab's profile is closed and its lock let go. The handoff is written in the agent's home as
     `.web-handoffs/ID.json`, holding the agent, profile, URL, reason and state `waiting`. The agent's home
     (`$COORD/agents/NAME/home`) is the same path inside its box and outside it.
  2. The agent posts `tell_owner` to the owner: "Site needs you: REASON at URL. Run `$COORD/mcp/web human FILE` …".
     `$COORD/mcp/web` is the link every agent's sessions run, and it is the same path in a box too.
  3. That command opens the profile headed at the page, through the same proxy, and holds its lock. The owner signs
     in or passes the check there, then closes the window, and the handoff becomes `done`.
     - The command takes only a plain file named `ID.json` in a `.web-handoffs` folder, opened without following a
       link.
     - Its URL must be http or https with a public address.
     - It opens only the profile that name owns: a role's under `profiles_root`, or `own` beside that handoff
       folder. A handoff naming any other folder is refused.
  4. The owner tells the agent Done. Its next `web_open` on the same profile goes on with the saved cookies.

  The card in the yaaihh window, with a button that runs step 3 and a Done that answers the agent, reads the same
  handoff file. It is a window piece, proto-window's to build, and it needs its own item, asked of paper-research on
  N-1977. Until then the post and the command do it.

## Decided (the owner, Q-1982)

1. **Keys:** optional 0600 files on this PC (Exa, Parallel, Brave), and no paid key now. SearXNG needs none.
2. **SearXNG:** never started at login; `searxng/run.sh` starts it when it is wanted.
   - Idle, measured on this PC on 27 Nov 2026 (docker, the pinned image): 116 MiB of memory and 0.2 % of one core,
     with 8 processes.
   - After a search it held 123 MiB, and was back under 0.5 % of a core within 20 s.
   - It answers 4 s after it starts. The image is 277 MB on disk.
3. **The venv:** `~/.local/lib/yaaihh-web/venv`, made by `install.sh` from `requirements.lock`, where each wheel is
   pinned by its SHA-256 and none is built. It holds 7 wheels and 367 MB:
   - Playwright (Apache-2.0) and greenlet;
   - PyMuPDF and pymupdf4llm (AGPL-3.0), with numpy, psutil and tabulate.

   pymupdf4llm's layout model is left out (onnxruntime and four more wheels). On that paper it cost 63 s of CPU
   against 3 s, and it hung under the reader's 2 GiB cap.

## Shipping without Python

The browser sits behind one boundary: the driver is a process that speaks JSON lines on stdin and stdout. Its ops
are `context`, `page`, `snapshot`, `act`, `captures`, `get`, `close_page`, `close` and `wait_closed`, and its signals are url,
title, frames, status, headers, text and password. `browserd.py`'s docstring is that protocol.
- The policy stays in the server: what is a gate, which profile is whose, what is kept and where.
- Setting `browser.driver` to any command that speaks it replaces Playwright. For shipping yaaihh, that can be one
  static Rust binary that talks the Chrome DevTools Protocol to the installed Chrome:
  - the tree from `Accessibility.getFullAXTree`;
  - the catch from `Fetch.enable` at the response stage;
  - the address rule as here.
- The server's own work (search, fetch, HTML to Markdown, the source archive) is plain standard-library Python, and
  can move into the same binary. The PDF reader then has to be a Rust crate (`pdf-extract`, or `mupdf` bindings),
  and pdftotext stays its fallback meanwhile.

## Tests

```
cd ops/web && python3 -W error::ResourceWarning -m unittest tests/test_server.py tests/test_search_fetch.py tests/test_browser.py
```

`tests/test_browser.py` drives the real Chrome through the venv, and its Chrome tests are skipped when the venv
isn't there.
- The address tests bind a "public" stand-in on 127.0.0.2 and a private one on 127.0.0.1, on the same port: a
  redirect, a name that turns private, a WebSocket.
- The owner's side of a handoff is tested with a stand-in driver, so no window opens. One test writes the handoff
  as an agent whose `HOME` is its own, and runs the command with another `HOME`, through `$COORD/mcp/web`.

Everything runs on loopback against a stand-in web, using a PDF made in the test. The redirect test also binds
127.0.0.2.
