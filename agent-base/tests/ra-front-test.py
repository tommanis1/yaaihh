#!/usr/bin/env python3
"""ra-front-test.py: the shared Rust server's front (hs-ls, N-412 posts 911, 916, 940) against a stand-in
rust-analyzer that records what reaches it. docket-core3's four conditions (post 916), each refused and each ordinary
case passed:

1. only worktrees: a root in a scratch home with a Cargo.toml, the owner's dot folder, production's checkout (--refuse)
   are refused in words; two worktrees of the repository are taken, a subfolder of one is taken as its top;
2. an edit stays in its worktree: a rename into the other worktree or into a path package (rune) is refused whole,
   naming the files; one inside passes; the server's applyEdit outside is refused, inside reaches its connection;
3. (proc macros only from the runner's build) the server is told to read them through `ra-front build-data`, whose
   answer is the entry for the Cargo.lock, or none;
4. what it writes: no text an agent sends reaches the log; diagnostics go only to the worktree that holds the file,
   the longest prefix winning (a worktree inside another's folder).
docket-core3's four faults of post 957: only the symbol tools' requests pass; every file a request names and every
location an answer gives stays in the asker's worktree; a caller in a box is refused; rust-analyzer runs the
toolchain's cargo and rustc whatever a worktree configures, and a worktree with a rust-analyzer.toml is refused.
Also: the server's progress reaches every connection without its titles and messages;
a request cancelled by a change is sent again; the server is not told the client pulls diagnostics; a file
outside its worktree is refused for a request; the projects are linked as worktrees connect; a closed
connection's open files are closed in the server. Nothing is started but the front and the stand-in.
"""
import json, os, socket, subprocess, sys, tempfile, time, shutil

here = os.path.dirname(os.path.abspath(__file__))
front = os.path.join(here, "..", "bin", "ra-front")
t = tempfile.mkdtemp()
fails = 0


def ok(name, got, want):
    global fails
    if got == want:
        print("ok  ", name)
    else:
        print(f"FAIL {name}: got {got!r}, want {want!r}")
        fails += 1


def sh(*a, **k):
    return subprocess.run(a, check=True, capture_output=True, text=True, **k).stdout


STUB = r'''
import json, sys, os
rec = open(os.environ["STUB_REC"], "a")
rec.write(json.dumps({"env": {k: os.environ.get(k) for k in ("CARGO", "RUSTC", "CARGO_BUILD_RUSTC", "RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER")}}) + "\n"); rec.flush()
def read():
    n = None
    while True:
        l = sys.stdin.buffer.readline()
        if not l: sys.exit(0)
        l = l.strip()
        if not l:
            if n is None: continue
            break
        if l.lower().startswith(b"content-length"): n = int(l.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(n))
def send(m):
    b = json.dumps(m).encode(); sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); sys.stdout.buffer.flush()
nid = 1000
while True:
    m = read()
    rec.write(json.dumps(m) + "\n"); rec.flush()
    meth = m.get("method")
    if meth == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {"hoverProvider": True, "renameProvider": True}}})
    elif meth == "workspace/didChangeConfiguration":
        nid += 1; send({"jsonrpc": "2.0", "id": nid, "method": "workspace/configuration", "params": {"items": [{"section": "rust-analyzer"}]}})
    elif meth == "textDocument/didOpen":
        globals().setdefault("docs", set()).add(m["params"]["textDocument"]["uri"])
    elif "id" in m and "method" not in m and isinstance(m.get("result"), list):
        # the front's configuration: the stand-in loads it as rust-analyzer does, busy, publishing false errors for
        # the open files before its build data is in, then quiescent
        send({"jsonrpc": "2.0", "method": "experimental/serverStatus", "params": {"health": "ok", "quiescent": False}})
        for u in sorted(globals().get("docs", ())):
            send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {"uri": u, "diagnostics": [{"message": "early false"}]}})
        send({"jsonrpc": "2.0", "method": "experimental/serverStatus", "params": {"health": "ok", "quiescent": True}})
    elif meth == "textDocument/diagnostic":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"kind": "full", "items": [{"message": "pulled"}]}})
    elif meth == "textDocument/didChange":
        u = m["params"]["textDocument"]["uri"]
        send({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "rustAnalyzer/x", "value": {"kind": "begin", "title": "Checking " + u, "message": u}}})
        send({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "rustAnalyzer/x", "value": {"kind": "report", "message": u, "percentage": 50}}})
        send({"jsonrpc": "2.0", "method": "experimental/serverStatus", "params": {"health": "ok", "quiescent": True, "message": u}})
        send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {"uri": m["params"]["textDocument"]["uri"], "diagnostics": [{"message": "E0308"}]}})
    elif meth == "textDocument/definition":
        send({"jsonrpc": "2.0", "id": m["id"], "result": [{"uri": u, "range": {}} for u in m["params"]["answer"]]})
    elif meth == "textDocument/hover" and m["params"]["position"]["line"] == 99:
        # the stand-in's way to make the server ask the client to apply an edit
        nid += 1; send({"jsonrpc": "2.0", "id": nid, "method": "workspace/applyEdit", "params": {"edit": {"changes": {u: [] for u in m["params"]["answer"]}}}})
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    elif meth == "textDocument/hover":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"contents": "hover"}})
    elif meth == "textDocument/rename":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"changes": {u: [] for u in m["params"]["newName"].split(",")}}})
    elif meth == "test/applyEdit":
        nid += 1; send({"jsonrpc": "2.0", "id": nid, "method": "workspace/applyEdit", "params": {"edit": {"changes": {u: [] for u in m["params"]["uris"]}}}})
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    elif meth == "textDocument/prepareRename":
        flaky = globals().get("flaky", 0) + 1; globals()["flaky"] = flaky
        if flaky <= 2:
            send({"jsonrpc": "2.0", "id": m["id"], "error": {"code": -32801, "message": "content modified"}})
        else:
            send({"jsonrpc": "2.0", "id": m["id"], "result": "answered"})
    elif "id" in m and "method" in m:
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
'''

try:
    stub = os.path.join(t, "stub.py")
    open(stub, "w").write(STUB)
    ra = os.path.join(t, "rust-analyzer")
    open(ra, "w").write(f"#!/bin/sh\nexec {sys.executable} {stub}\n")
    os.chmod(ra, 0o755)
    rec = os.path.join(t, "rec.jsonl")
    open(rec, "w").close()

    # a repository with two worktrees, each with a yaaihh workspace; a production checkout; a scratch home's folder
    repo = os.path.join(t, "repo")
    os.makedirs(os.path.join(repo, "yaaihh/src"))
    open(os.path.join(repo, "Cargo.toml"), "w").write("[workspace]\n")
    open(os.path.join(repo, "yaaihh/src/lib.rs"), "w").write("\n")
    g = ["git", "-C", repo, "-c", "user.name=t", "-c", "user.email=t@t"]
    sh(*g, "init", "-q", "-b", "dev")
    sh(*g, "add", "-A")
    sh(*g, "commit", "-q", "-m", "base")
    wa, wb = os.path.join(t, "wa"), os.path.join(t, "wa", "nested-wb")
    sh(*g, "worktree", "add", "-q", "-b", "a", wa)
    sh(*g, "worktree", "add", "-q", "-b", "b", wb)   # a worktree inside another worktree's folder
    prod = os.path.join(t, "prod")
    sh(*g, "worktree", "add", "-q", "-b", "prod", prod)
    loose = os.path.join(t, "scratch-home", "proj")
    os.makedirs(os.path.join(loose, "yaaihh"))
    open(os.path.join(loose, "Cargo.toml"), "w").write("[workspace]\n")
    rune = os.path.join(t, "src/rune/crates/rune/src")
    os.makedirs(rune)

    data = os.path.join(t, "data")
    sock = os.path.join(t, "lsp", "rust.sock")
    logf = os.path.join(t, "front.log")
    p = subprocess.Popen([sys.executable, front, "serve", "--socket", sock, "--ra", ra, "--data", data, "--repo", repo,
                          "--refuse", prod, "--log", logf], env=dict(os.environ, STUB_REC=rec))
    while not os.path.exists(sock):
        time.sleep(0.05)

    class Client:
        def __init__(self):
            self.s = socket.socket(socket.AF_UNIX)
            self.s.connect(sock)
            self.f = self.s.makefile("rb")
            self.n = 0
            self.seen = []

        def send(self, m):
            b = json.dumps(m).encode()
            self.s.sendall(b"Content-Length: %d\r\n\r\n" % len(b) + b)

        def read(self):
            n = None
            while True:
                l = self.f.readline()
                if not l:
                    return None
                l = l.strip()
                if not l:
                    if n is None:
                        continue
                    break
                if l.lower().startswith(b"content-length"):
                    n = int(l.split(b":")[1])
            return json.loads(self.f.read(n))

        def call(self, method, params):
            self.n += 1
            i = self.n
            self.send({"jsonrpc": "2.0", "id": i, "method": method, "params": params})
            while True:
                m = self.read()
                if m is None:
                    return None
                if m.get("id") == i and "method" not in m:
                    return m
                self.seen.append(m)

        def note(self, method, params):
            self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def uri(p):
        return "file://" + os.path.realpath(p)

    def init(root):
        c = Client()
        return c, c.call("initialize", {"rootUri": uri(root), "capabilities": {}})

    def err(r):
        return (r or {}).get("error", {}).get("message", "")

    # 1. only worktrees
    _, r = init(loose)
    ok("a root in a scratch home with a Cargo.toml is refused", "not in a worktree" in err(r), True)
    home_dot = os.path.join(os.path.expanduser("~"), ".config")
    _, r = init(home_dot)
    ok("the owner's dot folder is refused", "never served" in err(r), True)
    _, r = init(prod)
    ok("production's checkout (--refuse) is refused", "not served" in err(r), True)
    a, r = init(os.path.join(wa, "yaaihh", "src"))
    ok("a worktree's subfolder is taken as the worktree", "result" in r and "capabilities" in r["result"], True)
    b, r = init(wb)
    ok("a second worktree is taken", "result" in r, True)

    # the projects linked: the configuration the stand-in was last given names both worktrees' manifests
    time.sleep(0.3)
    cfgs = [json.loads(l) for l in open(rec)]
    last = [m["result"][0] for m in cfgs if "result" in m and isinstance(m["result"], list) and m["result"]][-1]
    ok("both worktrees' projects are linked", sorted(last["linkedProjects"]),
       sorted([os.path.realpath(os.path.join(wa, "Cargo.toml")), os.path.realpath(os.path.join(wb, "Cargo.toml"))]))
    ok("check on save is off", last["checkOnSave"], False)
    bs = last["cargo"]["buildScripts"]["overrideCommand"]
    ok("build scripts and proc macros are read through ra-front build-data", bs[-2:], ["build-data", data])

    # a file outside one's worktree
    fa = os.path.join(wa, "yaaihh/src/lib.rs")
    fb = os.path.join(wb, "yaaihh/src/lib.rs")
    r = a.call("textDocument/hover", {"textDocument": {"uri": uri(fb)}, "position": {"line": 0, "character": 0}})
    ok("a request on another worktree's file is refused", "outside your worktree" in err(r), True)
    r = a.call("textDocument/hover", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0}})
    ok("a request on its own file is answered", r.get("result"), {"contents": "hover"})

    # 4. diagnostics to the longest prefix only; no text in the log
    secret = "fn hs_ls_unsaved_text_marker() {}"
    a.note("textDocument/didOpen", {"textDocument": {"uri": uri(fa), "languageId": "rust", "version": 1, "text": secret}})
    b.note("textDocument/didOpen", {"textDocument": {"uri": uri(fb), "languageId": "rust", "version": 1, "text": secret}})
    b.note("textDocument/didChange", {"textDocument": {"uri": uri(fb), "version": 2}, "contentChanges": [{"text": secret + "x"}]})
    a.note("textDocument/didChange", {"textDocument": {"uri": uri(fa), "version": 2}, "contentChanges": [{"text": secret + "y"}]})
    a.call("textDocument/hover", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0}})
    b.call("textDocument/hover", {"textDocument": {"uri": uri(fb)}, "position": {"line": 0, "character": 0}})
    da = [m["params"]["uri"] for m in a.seen if m.get("method") == "textDocument/publishDiagnostics"]
    db = [m["params"]["uri"] for m in b.seen if m.get("method") == "textDocument/publishDiagnostics"]
    ok("worktree a gets its own diagnostics only (b's folder is inside a's)", sorted(set(da)), [uri(fa)])
    ok("worktree b gets its own diagnostics only", sorted(set(db)), [uri(fb)])
    time.sleep(0.3)
    ok("a changed file's errors are asked for and published",
       any(m.get("method") == "textDocument/publishDiagnostics" and m["params"]["diagnostics"] == [{"message": "pulled"}] for m in a.seen), True)
    # another worktree comes: the server reloads, and what it publishes meanwhile (before its build data) is not
    # passed on; the open files are asked again once it is quiescent
    wd = os.path.join(t, "wd")
    sh(*g, "worktree", "add", "-q", "-b", "d", wd)
    n0 = len(a.seen)
    d, _ = init(wd)
    time.sleep(0.5)
    a.call("textDocument/hover", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0}})
    later = [m for m in a.seen[n0:] if m.get("method") == "textDocument/publishDiagnostics"]
    ok("errors published while the server loads are held back", any(m["params"]["diagnostics"] == [{"message": "early false"}] for m in later), False)
    ok("and the open file is asked again once it is quiescent", [(m["params"]["uri"], m["params"]["diagnostics"]) for m in later],
       [(uri(fa), [{"message": "pulled"}])])
    heard = [m for m in a.seen if m.get("method") in ("$/progress", "experimental/serverStatus")]
    ok("a hears the server work", any(m["method"] == "$/progress" and m["params"]["value"].get("kind") == "begin" for m in heard), True)
    ok("but none of its words: no file of b's in a progress or status", any(os.path.realpath(wb) in json.dumps(m) for m in heard), False)
    a.note("textDocument/didOpen", {"textDocument": {"uri": uri(fb), "languageId": "rust", "version": 1, "text": "x"}})
    time.sleep(0.2)
    opened_b_by_a = sum(1 for l in open(rec) if '"didOpen"' in l and json.loads(l)["params"]["textDocument"]["text"] == "x")
    ok("opening another worktree's file does not reach the server", opened_b_by_a, 0)

    # 2. edits stay in their worktree
    r = a.call("textDocument/rename", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0},
                                        "newName": uri(fa)})
    ok("a rename inside the worktree passes", list(r.get("result", {}).get("changes", {})), [uri(fa)])
    r = a.call("textDocument/rename", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0},
                                        "newName": uri(fa) + "," + uri(os.path.join(rune, "lib.rs"))})
    ok("a rename reaching into rune is refused whole, naming the file",
       ("outside your worktree" in err(r), os.path.join(rune, "lib.rs") in err(r), "result" in r), (True, True, False))
    r = b.call("textDocument/rename", {"textDocument": {"uri": uri(fb)}, "position": {"line": 0, "character": 0},
                                        "newName": uri(fb) + "," + uri(fa)})
    ok("a rename reaching into the other worktree is refused", "outside your worktree" in err(r), True)
    a.call("textDocument/hover", {"textDocument": {"uri": uri(fa)}, "position": {"line": 99, "character": 0},
                                  "answer": [uri(os.path.join(rune, "lib.rs"))]})
    time.sleep(0.3)
    answers = [json.loads(l) for l in open(rec)]
    refused = [m for m in answers if isinstance(m.get("result"), dict) and m["result"].get("applied") is False]
    ok("the server's applyEdit outside every worktree is refused", len(refused), 1)
    b.n += 1
    b.send({"jsonrpc": "2.0", "id": b.n, "method": "textDocument/hover", "params": {
        "textDocument": {"uri": uri(fb)}, "position": {"line": 99, "character": 0}, "answer": [uri(fb)]}})
    got = None
    while got is None:
        m = b.read()
        if m.get("method") == "workspace/applyEdit":
            got = m
            b.send({"jsonrpc": "2.0", "id": m["id"], "result": {"applied": True}})
    ok("the server's applyEdit inside one worktree reaches that connection", list(got["params"]["edit"]["changes"]), [uri(fb)])
    time.sleep(0.3)
    answers = [json.loads(l) for l in open(rec)]
    ok("its answer goes back to the server", any(isinstance(m.get("result"), dict) and m["result"].get("applied") is True for m in answers), True)

    # docket-core3's read of 0f4e4b87 (post 957), its four faults
    # 1. only what the symbol tools ask passes
    for meth in ("rust-analyzer/runFlycheck", "experimental/runTest", "workspace/symbol"):
        r = a.call(meth, {"textDocument": {"uri": uri(fa)}})
        ok(f"{meth} is refused", "not taken by the shared Rust server" in err(r), True)
    time.sleep(0.2)
    ok("none of them reached the server", sum(1 for l in open(rec) if "runFlycheck" in l or "runTest" in l or "workspace/symbol" in l), 0)
    # 2. every file a request names, and every location an answer gives, stays in the asker's worktree
    r = a.call("textDocument/references", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0},
                                            "context": {"includeDeclaration": True, "targetUri": uri(fb)}})
    ok("a request naming another worktree's file anywhere in its parameters is refused", "outside your worktree" in err(r), True)
    r = a.call("textDocument/definition", {"textDocument": {"uri": uri(fa)}, "position": {"line": 0, "character": 0},
                                            "answer": [uri(fa), uri(fb), uri(os.path.join(rune, "lib.rs"))]})
    ok("an answer's locations in another worktree or a library are taken out", [x["uri"] for x in r["result"]], [uri(fa)])
    # 3. a caller in a box is refused at the socket (another mount namespace; made with unshare, no root)
    probe = os.path.join(t, "boxed.py")
    open(probe, "w").write(f"""
import json, socket
s = socket.socket(socket.AF_UNIX); s.connect({sock!r})
b = json.dumps({{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {{"rootUri": {uri(wa)!r}, "capabilities": {{}}}}}}).encode()
s.sendall(b"Content-Length: %d\\r\\n\\r\\n" % len(b) + b)
f = s.makefile("rb"); n = None
while True:
    l = f.readline().strip()
    if not l and n is not None: break
    if l.lower().startswith(b"content-length"): n = int(l.split(b":")[1])
print(json.loads(f.read(n)).get("error", {{}}).get("message", ""))
""")
    u = subprocess.run(["unshare", "-rm", sys.executable, probe], capture_output=True, text=True)
    if u.returncode == 0:
        ok("a caller in another mount namespace (a box) is refused", "box" in u.stdout, True)
    else:
        print("skip a caller in a box: unshare -rm is not allowed here:", u.stderr.strip()[:100])
    # 4. the cargo and rustc rust-analyzer runs are the toolchain's, named in its environment over any .cargo/config.toml
    env = [json.loads(l)["env"] for l in open(rec) if l.startswith('{"env"')][0]
    ok("rust-analyzer runs the toolchain's cargo and rustc, with no wrapper",
       (env["RUSTC"], env["CARGO_BUILD_RUSTC"], env["CARGO"], env["RUSTC_WRAPPER"], env["CARGO_BUILD_RUSTC_WRAPPER"]),
       (os.path.join(t, "rustc"), os.path.join(t, "rustc"), os.path.join(t, "cargo"), "", ""))
    wc = os.path.join(t, "wc")
    sh(*g, "worktree", "add", "-q", "-b", "c", wc)
    open(os.path.join(wc, "yaaihh", "rust-analyzer.toml"), "w").write("[cargo]\n")
    _, r = init(wc)
    ok("a worktree holding a rust-analyzer.toml is refused", "rust-analyzer.toml" in err(r), True)

    # a request cancelled by a change (another worktree's edit) is sent again, not answered with the error
    r = b.call("textDocument/prepareRename", {"textDocument": {"uri": uri(fb)}, "position": {"line": 0, "character": 0}})
    ok("a request the server answers with ContentModified is sent again", r.get("result"), "answered")
    first = next(m for m in map(json.loads, open(rec)) if m.get("method") == "initialize")
    ok("the server is not told the client pulls diagnostics (it would stop publishing them)",
       "diagnostic" in first["params"]["capabilities"]["textDocument"], False)

    # a closed connection's files are closed in the server
    a.f.close()
    a.s.close()
    time.sleep(0.3)
    closes = [json.loads(l)["params"]["textDocument"]["uri"] for l in open(rec) if '"textDocument/didClose"' in l]
    ok("a closed connection's open files are closed in the server", closes, [uri(fa)])

    # 3. build-data: the entry for the folder's Cargo.lock, or none
    import hashlib
    lock = os.path.join(wa, "yaaihh", "Cargo.lock")
    open(lock, "w").write("# lock\n")
    h = hashlib.sha256(b"# lock\n").hexdigest()[:16]
    out = subprocess.run([sys.executable, front, "build-data", data], cwd=os.path.dirname(lock), capture_output=True, text=True)
    ok("no entry for a Cargo.lock: build-finished false and a line saying so",
       (out.stdout.strip(), "none for this Cargo.lock" in out.stderr), ('{"reason": "build-finished", "success": false}', True))
    os.makedirs(os.path.join(data, h))
    open(os.path.join(data, h, "messages.jsonl"), "w").write('{"reason":"build-finished","success":true}\n')
    out = subprocess.run([sys.executable, front, "build-data", data], cwd=os.path.dirname(lock), capture_output=True, text=True)
    ok("the entry for the Cargo.lock is given", out.stdout, '{"reason":"build-finished","success":true}\n')

    logtext = open(logf).read()
    ok("no text an agent sent is in the log", "hs_ls_unsaved_text_marker" in logtext, False)
    p.terminate()
    p.wait()
finally:
    if "p" in globals() and p.poll() is None:  # the front and its stand-in go, whatever failed
        p.terminate()
        p.wait()
    shutil.rmtree(t, ignore_errors=True)

print("all passed" if fails == 0 else f"{fails} failed")
sys.exit(1 if fails else 0)
