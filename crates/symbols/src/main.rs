//! `symbols`: the symbol tools (N-117) as an MCP server on stdio. An agent reads and edits code by definition,
//! through the language servers its project file names (`[[lsp]]`); code is addressed by symbol, never by line
//! number or by old text. Linked as `$COORD/mcp/symbols`, roster makes it a server of every agent session; Claude
//! Code starts it as the session's child, so in a box it and its language servers run there, as the agent's user,
//! on the layer's view.
//!
//!   symbols [--root DIR]      the MCP server; DIR (default: the working folder) is the code's root
//!   symbols --call TOOL JSON  one call, its answer on stdout (for trying it out)
//!
//! The project file comes from SYMBOLS_PROJECT, else from $COORD/project's first line.

mod index;
mod lsp;
mod ops;
mod records;
mod view;

use anyhow::{Result, anyhow};
use index::Index;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;

fn tools() -> Value {
    let sym = json!({"type": "string", "description": "A symbol as the language names it: `store::Store::open` (Rust), `Ticket.Store.open` (Haskell, Agda); the end of the name is enough when it is unique (`Store::open`, `open`). A file may go in front: `src/store.rs:Store::open`; then only that language's server is asked (and started, on first use), where a name alone starts them all. When a name fits several symbols nothing is done and the answer lists the names that pick each out."});
    // `anthropic/alwaysLoad`: Claude Code puts the tool in the prompt from the first turn instead of deferring it behind
    // its tool search, where an agent sees the name only and has to load it first (run 4's designer never did:
    // paper-research, N-101 p1157)
    let t = |name: &str, desc: &str, props: Value, req: &[&str]| json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": req}, "_meta": {"anthropic/alwaysLoad": true}});
    json!([
        t("definition", "A symbol's source, whole, with its type and its place (file, begin and end line:column). Whole is the server's range (in Haskell with the type signature; in Agda from the signature to the last clause, `where` included) and, in every language, the comment lines directly above it with no blank line between.", json!({"symbol": sym}), &["symbol"]),
        t("references", "Every use of a symbol outside its own definition: file, line, the symbol it is in, the line's text.", json!({"symbol": sym}), &["symbol"]),
        t("type_at", "The type at a place in a symbol's definition: the place is a piece of its text (`at`), the NTH occurrence (default 1). Also what the server reports there (a hole's type).", json!({"symbol": sym, "at": {"type": "string"}, "nth": {"type": "integer"}}), &["symbol", "at"]),
        t("outline", "A file's import lines, then its symbols without their bodies: name, kind, line, type.", json!({"file": {"type": "string", "description": "the file's path from the code's root"}}), &["file"]),
        t("replace", "Replace a symbol's whole definition (as `definition` shows it) with new source. Answers with the errors and warnings of the changed file and of the files whose errors changed; an edit that breaks typechecking is made all the same. Rust's come from one shared server within seconds, with no build; not among them: the borrow checker's errors and failing tests (a runner job has them), errors the edit caused in other files until the server next reports them, anything for an agent in a box, and on a branch with a new Cargo.lock every `#[derive]` reads as an error until a runner job has made its build data.", json!({"symbol": sym, "new_source": {"type": "string"}}), &["symbol", "new_source"]),
        t("insert_after", "Insert source after a symbol's definition. Answers with errors and warnings as replace does.", json!({"symbol": sym, "source": {"type": "string"}}), &["symbol", "source"]),
        t("insert_before", "Insert source before a symbol's definition (above the comment lines directly above it). Answers with errors and warnings as replace does.", json!({"symbol": sym, "source": {"type": "string"}}), &["symbol", "source"]),
        t("rename", "Rename a symbol everywhere in the project, through the language server. Comments and strings are not changed: the answer counts the places where the old name still stands as text, and lists them. Answers with errors and warnings as replace does.", json!({"symbol": sym, "new_name": {"type": "string"}}), &["symbol", "new_name"]),
        t("remove", "Remove a symbol's definition. Refused while uses remain: the answer lists them.", json!({"symbol": sym}), &["symbol"])
    ])
}

/// A tool call; an edit that wrote files is recorded as the agent's act.
/// A tool call that came at ARRIVED (None: not from an MCP client, no deadline). An earlier edit's check still going
/// on is waited on first and its report goes before this call's answer. A wait cut at the client's limit (lsp's
/// deadline) is an answer, not an error: what the server is doing, and that the same call made again goes on with it.
/// After an earlier edit's report E (if any), the files changed outside symbols, sent and checked (ops::outside_edits):
/// both reports, joined. Cut at the deadline with E in hand, E is not lost: it goes into the cut's text.
fn before_call(ix: &mut Index, e: Option<String>) -> Result<Option<String>> {
    match ops::outside_edits(ix) {
        Ok(o) => Ok(match (e, o) {
            (Some(e), Some(o)) => Some(format!("{e}\n\n----\n\n{o}")),
            (e, o) => e.or(o),
        }),
        Err(err) => match (e, lsp::still_working(&err)) {
            (Some(e), Some(_)) => Err(anyhow::Error::new(lsp::StillWorking(format!("{err}\n\n(before it: {e})")))),
            (Some(e), None) => Err(anyhow!("{err:#}\n\n(before it: {e})")),
            (None, _) => Err(err),
        },
    }
}

fn call(ix: &mut Index, name: &str, a: &Value, arrived: Option<std::time::Instant>) -> Result<String> {
    ix.written.clear();
    lsp::begin_call(arrived);
    // an earlier edit's check, then what changed outside symbols: each a report before this call's own answer
    let r = match ops::earlier_edit(ix).and_then(|e| before_call(ix, e)) {
        Ok(first) => match tool(ix, name, a) {
            Ok(t) => Ok(match first {
                Some(f) => format!("{f}\n\n----\n\n{t}"),
                None => t,
            }),
            Err(e) => match first {
                // the earlier report must not be lost with this call's refusal
                Some(f) if lsp::still_working(&e).is_none() => Ok(format!("{f}\n\n----\n\nrefused: {e:#}")),
                Some(f) => Ok(format!("{f}\n\n----\n\nNot done yet: {e}.\nCall {name} again: the work goes on meanwhile, and the call takes it up where it is.")),
                None => Err(e),
            },
        },
        Err(e) => Err(e),
    };
    let r = match r {
        Err(e) if lsp::still_working(&e).is_some() => Ok(format!("Not done yet: {e}.\nCall {name} again: the work goes on meanwhile, and the call takes it up where it is.")),
        r => r,
    };
    if let Ok(t) = &r {
        if !ix.written.is_empty() {
            act(ix, name, a.get("symbol").and_then(Value::as_str).unwrap_or(""), t);
        }
    }
    // a server silent through a wait says so at the answer's end (paper-research, N-117.p29)
    let notes = lsp::take_notes();
    match r {
        Ok(t) if !notes.is_empty() => Ok(format!("{t}\n({})", notes.join("; "))),
        r => r,
    }
}

/// An edit's act (docket-core5, N-153): `$AGENT_BASE/bin/coordc act edit TEXT tool=T symbol=S files=[..]`, coordd
/// taking the agent from the caller's cgroup. TEXT is the answer's first line. Without AGENT_BASE (not run by a coord
/// agent) nothing is sent; a failure is said in symbols' log only.
fn act(ix: &Index, tool: &str, symbol: &str, answer: &str) {
    let Ok(base) = std::env::var("AGENT_BASE") else { return };
    let files: Vec<String> = ix.written.iter().map(|p| ix.rel(p)).collect();
    let text = answer.lines().next().unwrap_or("").to_string();
    let r = std::process::Command::new(format!("{base}/bin/coordc"))
        .args(["act", "edit", &text, &format!("tool={tool}"), &format!("symbol={symbol}"), &format!("files={}", json!(files))])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .output();
    match r {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!("symbols: coordc act edit: {}: {}", o.status, String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => eprintln!("symbols: coordc act edit: {e}"),
    }
}

fn tool(ix: &mut Index, name: &str, a: &Value) -> Result<String> {
    let s = |k: &str| a.get(k).and_then(Value::as_str).ok_or_else(|| anyhow!("{name}: `{k}` is missing"));
    match name {
        "definition" => ops::definition(ix, s("symbol")?),
        "references" => ops::references(ix, s("symbol")?),
        "type_at" => ops::type_at(ix, s("symbol")?, s("at")?, a.get("nth").and_then(Value::as_u64).unwrap_or(1) as usize),
        "outline" => ops::outline(ix, s("file")?),
        "replace" => ops::replace(ix, s("symbol")?, s("new_source")?),
        "insert_after" => ops::insert(ix, s("symbol")?, s("source")?, true),
        "insert_before" => ops::insert(ix, s("symbol")?, s("source")?, false),
        "rename" => ops::rename(ix, s("symbol")?, s("new_name")?),
        "remove" => ops::remove(ix, s("symbol")?),
        _ => Err(anyhow!("no tool {name}")),
    }
}

/// Told to stop (SIGTERM, SIGINT, SIGHUP): the signals are blocked in every thread from the start and one thread
/// waits for them; it ends every server (lsp::stop_all) and then symbols. Its stdin closing ends the MCP loop instead,
/// and the servers end as the Index drops (Server's Drop: the same Link::stop).
fn stop_on_signals() {
    // SAFETY: plain libc calls on a local signal set, before any other thread is started
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for s in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&mut set, s);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        std::thread::spawn(move || {
            let mut sig = 0;
            libc::sigwait(&set, &mut sig);
            eprintln!("symbols: signal {sig}, ending the servers");
            lsp::stop_all();
            std::process::exit(0);
        });
    }
}

/// The id of the MCP request being answered: a cancel names it.
static RUNNING: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

/// Whether symbols is outlining a file behind the calls (no call waiting): a message that comes cancels that
/// outline's wait, so that the message is taken at once (the file is outlined later).
static BEHIND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() -> Result<()> {
    stop_on_signals();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = std::env::current_dir()?;
    let mut one: Option<(String, Value)> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                root = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--call" => {
                one = Some((args[i + 1].clone(), serde_json::from_str(&args[i + 2])?));
                i += 3;
            }
            other => return Err(anyhow!("unknown argument {other}")),
        }
    }
    let root = root.canonicalize()?;
    let confs = index::servers_of(&index::project_file()?)?;
    let mut ix = Index::new(root, confs);
    if let Some((tool, a)) = one {
        match call(&mut ix, &tool, &a, None) {
            Ok(t) => println!("{t}"),
            Err(e) => println!("refused: {e:#}"),
        }
        return Ok(());
    }
    // MCP: JSON-RPC, one message a line. A thread reads them while a call runs, so a cancel of that call (the session
    // interrupted) reaches its waits (lsp::cancel); the other messages are taken here in order.
    let mut out = std::io::stdout();
    // each message with when it was read: a call's clock in the client runs from about then, also while it waits here
    // behind another call
    let (tx, rx) = std::sync::mpsc::channel::<(Value, std::time::Instant)>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            if msg["method"] == "notifications/cancelled" {
                let of = msg.pointer("/params/requestId").cloned().unwrap_or(Value::Null);
                if *RUNNING.lock().unwrap() == Some(of) {
                    lsp::cancel();
                }
                continue;
            }
            if tx.send((msg, std::time::Instant::now())).is_err() {
                break;
            }
            // after the send: an outline begun before it is cut here, one begun after it finds the message first
            if BEHIND.load(std::sync::atomic::Ordering::SeqCst) {
                lsp::cancel();
            }
        }
        // the client is gone: a wait running now (an outline behind the calls, on no clock, on a server busy with its
        // load) ends, and the loop sees the end (it ran on, 10 min after its input closed: docket-core5, N-233); the
        // channel closed first, so the loop finds it so and starts no other wait
        drop(tx);
        lsp::cancel();
    });
    match lsp::client_limit() {
        Some(l) => eprintln!("symbols: the client ends a call after {} s (MCP_TOOL_TIMEOUT); a call answers by {} s", l.as_secs(), (l - lsp::margin(l)).as_secs()),
        None => eprintln!("symbols: no MCP_TOOL_TIMEOUT: calls wait on no clock"),
    }
    loop {
        // while no message waits: the servers started (from the session's start, not at its first call: the owner,
        // N-190.p5), then the files not yet outlined outlined one by one (Index::start_next, outline_next)
        let (msg, arrived) = match rx.try_recv() {
            Ok(m) => m,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) if ix.has_behind() => {
                lsp::begin_call(None);
                BEHIND.store(true, std::sync::atomic::Ordering::SeqCst);
                let m = rx.try_recv();
                if m.is_err() && !ix.start_next() {
                    ix.outline_next();
                }
                BEHIND.store(false, std::sync::atomic::Ordering::SeqCst);
                lsp::take_notes();
                match m {
                    Ok(m) => m,
                    Err(_) => continue,
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => match rx.recv() {
                Ok(m) => m,
                Err(_) => break,
            },
        };
        let Some(id) = msg.get("id").cloned() else { continue };
        *RUNNING.lock().unwrap() = Some(id.clone());
        let method = msg["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => json!({
                "protocolVersion": msg.pointer("/params/protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "symbols", "version": env!("CARGO_PKG_VERSION")}
            }),
            "tools/list" => json!({"tools": tools()}),
            "tools/call" => {
                let name = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or("");
                let a = msg.pointer("/params/arguments").cloned().unwrap_or(json!({}));
                match call(&mut ix, name, &a, Some(arrived)) {
                    Ok(t) => json!({"content": [{"type": "text", "text": t}]}),
                    Err(e) => json!({"content": [{"type": "text", "text": format!("{e:#}")}], "isError": true}),
                }
            }
            "ping" => json!({}),
            _ => {
                let e = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("no method {method}")}});
                writeln!(out, "{e}")?;
                out.flush()?;
                continue;
            }
        };
        writeln!(out, "{}", json!({"jsonrpc": "2.0", "id": id, "result": result}))?;
        out.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use index::ServerConf;

    /// A file changed outside symbols (Claude Code's Edit, the shell) is sent to the server again before the next
    /// call, and that call's answer stands on it (paper-research, N-101 post 219): references sees a use written
    /// with no symbols tool, and no longer one taken out that way.
    #[test]
    fn an_edit_made_outside_symbols_is_seen_by_the_next_call() {
        let root = std::env::temp_dir().join(format!("symbols-outside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.fk"), "def foo\n").unwrap();
        std::fs::write(root.join("b.fk"), "x\n").unwrap();
        let fake = root.join("fake.py");
        // a server that answers only from the texts it was given: outline from `def NAME` lines, references by word
        std::fs::write(&fake, r#"
import sys, json, re
i, o = sys.stdin.buffer, sys.stdout.buffer
docs = {}
def send(m):
    b = json.dumps(m).encode(); o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def rng(l, c, n): return {"start": {"line": l, "character": c}, "end": {"line": l, "character": c + n}}
while True:
    n = None
    while True:
        l = i.readline()
        if not l: sys.exit(0)
        l = l.strip()
        if not l and n is not None: break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    m = json.loads(i.read(n)); k = m.get("method"); p = m.get("params") or {}
    if k == "textDocument/didOpen": docs[p["textDocument"]["uri"]] = p["textDocument"]["text"]
    elif k == "textDocument/didChange": docs[p["textDocument"]["uri"]] = p["contentChanges"][0]["text"]
    elif k == "textDocument/didClose": docs.pop(p["textDocument"]["uri"], None)
    if "id" not in m: continue
    r = None
    if k == "initialize": r = {"capabilities": {}}
    elif k == "textDocument/documentSymbol":
        r = [{"name": w, "kind": 12, "range": rng(ln, 0, len(t)), "selectionRange": rng(ln, 4, len(w))}
             for ln, t in enumerate(docs.get(p["textDocument"]["uri"], "").split("\n")) for w in re.findall(r"^def (\w+)", t)]
    elif k == "textDocument/references":
        t = docs[p["textDocument"]["uri"]].split("\n")[p["position"]["line"]]
        w = re.match(r"\w+", t[p["position"]["character"]:]).group(0)
        r = [{"uri": u, "range": rng(ln, mm.start(), len(w))} for u, d in sorted(docs.items())
             for ln, t in enumerate(d.split("\n")) for mm in re.finditer(r"\b%s\b" % w, t)]
    send({"jsonrpc": "2.0", "id": m["id"], "result": r})
"#).unwrap();
        let conf = ServerConf { lang: "fake".into(), command: vec!["python3".into(), fake.display().to_string()], files: vec!["*.fk".into()], skip: Vec::new(), socket: None, options: None, records: None };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        let refs = |ix: &mut Index| call(ix, "references", &json!({"symbol": "a.fk:foo"}), None).unwrap();
        let first = refs(&mut ix);
        assert!(first.contains("no uses outside its definition"), "{first}");

        // the agent's own edit, with no symbols tool: b.fk now uses foo
        std::fs::write(root.join("b.fk"), "x\nfoo here\n").unwrap();
        let second = refs(&mut ix);
        assert!(second.contains("Before this call: 1 file changed outside symbols") && second.contains("b.fk"), "{second}");
        assert!(second.contains("1 uses") && second.contains("b.fk:2:1"), "{second}");

        // nothing changed since: nothing is sent, nothing said
        let third = refs(&mut ix);
        assert!(!third.contains("Before this call") && third.contains("1 uses"), "{third}");

        // the use taken out again, and a file gone: both seen
        std::fs::write(root.join("b.fk"), "x\n").unwrap();
        std::fs::write(root.join("c.fk"), "foo\n").unwrap();
        let fourth = refs(&mut ix);
        assert!(fourth.contains("2 files changed") && fourth.contains("c.fk:1:1") && !fourth.contains("b.fk:2"), "{fourth}");
        std::fs::remove_file(root.join("c.fk")).unwrap();
        let fifth = refs(&mut ix);
        assert!(fifth.contains("1 gone (c.fk)") && fifth.contains("no uses outside its definition"), "{fifth}");
        drop(ix);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The first call answers from the file asked (the owner, N-320; paper-research, N-101 p1120): `a.fk:foo`
    /// outlines a.fk only, its references outline the files they are in, and the rest is outlined behind the calls,
    /// one file at a time; a name without its file outlines every file first.
    #[test]
    fn a_first_call_outlines_the_file_asked_and_the_rest_behind() {
        let root = std::env::temp_dir().join(format!("symbols-lazy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.fk"), "def foo\n").unwrap();
        std::fs::write(root.join("b.fk"), "def bar\n  foo\n").unwrap();
        std::fs::write(root.join("c.fk"), "def baz\n").unwrap();
        std::fs::write(root.join("d.fk"), "def qux\n").unwrap();
        let fake = root.join("fake.py");
        let asked = root.join("asked");
        // outlines from `def NAME` lines (each file asked is written down), references by word
        std::fs::write(&fake, format!(r#"
import sys, json, re
i, o = sys.stdin.buffer, sys.stdout.buffer
docs = {{}}
def send(m):
    b = json.dumps(m).encode(); o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def rng(l, c, n): return {{"start": {{"line": l, "character": c}}, "end": {{"line": l, "character": c + n}}}}
while True:
    n = None
    while True:
        l = i.readline()
        if not l: sys.exit(0)
        l = l.strip()
        if not l and n is not None: break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    m = json.loads(i.read(n)); k = m.get("method"); p = m.get("params") or {{}}
    if k == "textDocument/didOpen": docs[p["textDocument"]["uri"]] = p["textDocument"]["text"]
    if "id" not in m: continue
    r = None
    if k == "initialize": r = {{"capabilities": {{}}}}
    elif k == "textDocument/documentSymbol":
        u = p["textDocument"]["uri"]
        open({asked:?}, "a").write(u.rsplit("/", 1)[1] + "\n")
        t = docs.get(u, "").split("\n")
        r = [{{"name": w, "kind": 12, "range": rng(ln, 0, 0) | {{"end": {{"line": len(t) - 1, "character": 0}}}}, "selectionRange": rng(ln, 4, len(w))}}
             for ln, x in enumerate(t) for w in re.findall(r"^def (\w+)", x)]
    elif k == "textDocument/references":
        t = docs[p["textDocument"]["uri"]].split("\n")[p["position"]["line"]]
        w = re.match(r"\w+", t[p["position"]["character"]:]).group(0)
        r = [{{"uri": u, "range": rng(ln, mm.start(), len(w))}} for u, d in sorted(docs.items())
             for ln, x in enumerate(d.split("\n")) for mm in re.finditer(r"\b%s\b" % w, x)]
    send({{"jsonrpc": "2.0", "id": m["id"], "result": r}})
"#, asked = asked.display().to_string())).unwrap();
        let conf = ServerConf { lang: "fake".into(), command: vec!["python3".into(), fake.display().to_string()], files: vec!["*.fk".into()], skip: Vec::new(), socket: None, options: None, records: None };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        let asked_now = || std::fs::read_to_string(&asked).unwrap_or_default().lines().map(String::from).collect::<Vec<_>>();

        let d = call(&mut ix, "definition", &json!({"symbol": "a.fk:foo"}), None).unwrap();
        assert!(d.contains("def foo"), "{d}");
        assert_eq!(asked_now(), ["a.fk"], "the first call outlines the file asked only");
        assert!(ix.has_unread());

        // its references are said with the symbol they are in: b.fk is outlined for it, c.fk and d.fk are not
        let r = call(&mut ix, "references", &json!({"symbol": "a.fk:foo"}), None).unwrap();
        assert!(r.contains("b.fk:2:3  in bar"), "{r}");
        assert_eq!(asked_now(), ["a.fk", "b.fk"]);

        // behind the calls: one file at a time, then nothing is left
        ix.outline_next();
        assert_eq!(asked_now().len(), 3);
        ix.outline_next();
        assert_eq!(asked_now().len(), 4);
        assert!(!ix.has_unread());
        ix.outline_next();
        assert_eq!(asked_now().len(), 4, "nothing is outlined twice behind the calls");
        let q = call(&mut ix, "definition", &json!({"symbol": "qux"}), None).unwrap();
        assert!(q.contains("def qux"), "{q}");

        // behind the calls from a session's start: the server started first, nothing outlined by it
        std::fs::remove_file(&asked).unwrap();
        let conf = ServerConf { lang: "fake".into(), command: vec!["python3".into(), fake.display().to_string()], files: vec!["*.fk".into()], skip: Vec::new(), socket: None, options: None, records: None };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        assert!(ix.has_behind() && !ix.has_unread());
        assert!(ix.start_next());
        assert!(asked_now().is_empty() && ix.has_unread());
        assert!(!ix.start_next(), "a started server is not started again");
        drop(ix);

        // the tools are in an agent's prompt from its first turn, never deferred behind Claude Code's tool search
        assert!(tools().as_array().unwrap().iter().all(|t| t["_meta"]["anthropic/alwaysLoad"] == json!(true)));

        // a name without its file, on a fresh index: every file outlined before it is looked for
        let _ = std::fs::remove_file(&asked);
        let conf = ServerConf { lang: "fake".into(), command: vec!["python3".into(), fake.display().to_string()], files: vec!["*.fk".into()], skip: Vec::new(), socket: None, options: None, records: None };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        let b = call(&mut ix, "definition", &json!({"symbol": "baz"}), None).unwrap();
        assert!(b.contains("def baz"), "{b}");
        let mut all = asked_now();
        all.sort();
        assert_eq!(all, ["a.fk", "b.fk", "c.fk", "d.fk"]);
        assert!(!ix.has_unread());
        drop(ix);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A server that takes the view as data (design.md 4.1): it is given the bases at initialize and the difference
    /// as `view/update`s, never a file by didOpen, and every answer stands on base plus difference: a use written in
    /// an uncommitted change, a new file, a deleted one, a file put back as its base was, a base that moved.
    #[test]
    fn a_view_server_is_given_base_and_difference() {
        let root = std::env::temp_dir().join(format!("symbols-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| {
            let st = std::process::Command::new("git").arg("-C").arg(&root).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
            assert!(st.status.success(), "git {args:?}");
            String::from_utf8_lossy(&st.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        for (f, t) in [("a.fk", "def foo\n"), ("b.fk", "x\n"), ("c.fk", "foo\n"), (".gitignore", "server.log\nfake.py\n")] {
            std::fs::write(root.join(f), t).unwrap();
        }
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        let base = git(&["rev-parse", "HEAD"]);
        // the agent's change before symbols starts: b.fk uses foo
        std::fs::write(root.join("b.fk"), "foo bar\n").unwrap();
        let log = root.join("server.log");
        let fake = root.join("fake.py");
        std::fs::write(&fake, r#"
import sys, json, re, subprocess
i, o = sys.stdin.buffer, sys.stdout.buffer
log = sys.argv[1]
repos, table = {}, {}          # name -> (root, base); (name, path) -> text, or None when deleted
def note(m):
    with open(log, "a") as f: f.write(json.dumps(m) + "\n")
def send(m):
    b = json.dumps(m).encode(); o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def git(name, *a): return subprocess.run(["git", "-C", repos[name][0]] + list(a), capture_output=True, text=True).stdout
def files():
    out = {}
    for name in repos:
        for p in git(name, "ls-tree", "-r", "--name-only", repos[name][1]).split():
            out[(name, p)] = None
    for k, v in table.items(): out[k] = v
    return [k for k in out if table.get(k, "") is not None]
def text(k):
    if k in table: return table[k]
    return git(k[0], "show", "%s:%s" % (repos[k[0]][1], k[1]))
def of_uri(u):
    path = u[len("file://"):]
    for name, (r, _) in repos.items():
        if path.startswith(r + "/"): return (name, path[len(r) + 1:])
def uri(k): return "file://" + repos[k[0]][0] + "/" + k[1]
def rng(l, c, n): return {"start": {"line": l, "character": c}, "end": {"line": l, "character": c + n}}
while True:
    n = None
    while True:
        l = i.readline()
        if not l: sys.exit(0)
        l = l.strip()
        if not l and n is not None: break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    m = json.loads(i.read(n)); k = m.get("method"); p = m.get("params") or {}
    if k and k.startswith("textDocument/did"): note({"did": k})
    if "id" not in m: continue
    r = None
    if k == "initialize":
        v = p["initializationOptions"]["view"]; note({"init": v})
        for x in v["repos"]: repos[x["name"]] = (x["root"], x["base"])
        r = {"capabilities": {"experimental": {"view": 1}}}
    elif k == "view/update":
        note({"update": p})
        if p.get("full"): table.clear()
        for name, b in (p.get("base") or {}).items(): repos[name] = (repos[name][0], b)
        for c in p["changed"]: table[(c["repo"], c["path"])] = c["text"]
        for d in p["deleted"]:
            if d.get("same_as_base"): table.pop((d["repo"], d["path"]), None)
            else: table[(d["repo"], d["path"])] = None
        r = {"taken": len(p["changed"]) + len(p["deleted"])}
    elif k == "textDocument/documentSymbol":
        kk = of_uri(p["textDocument"]["uri"])
        r = [{"name": w, "kind": 12, "range": rng(ln, 0, len(t)), "selectionRange": rng(ln, 4, len(w))}
             for ln, t in enumerate(text(kk).split("\n")) for w in re.findall(r"^def (\w+)", t)] if kk in files() else []
    elif k == "textDocument/references":
        kk = of_uri(p["textDocument"]["uri"])
        t = text(kk).split("\n")[p["position"]["line"]]
        w = re.match(r"\w+", t[p["position"]["character"]:]).group(0)
        r = [{"uri": uri(f), "range": rng(ln, mm.start(), len(w))} for f in sorted(files())
             for ln, tt in enumerate(text(f).split("\n")) for mm in re.finditer(r"\b%s\b" % w, tt)]
    send({"jsonrpc": "2.0", "id": m["id"], "result": r})
"#).unwrap();
        let conf = ServerConf { lang: "fake".into(), command: vec!["python3".into(), fake.display().to_string(), log.display().to_string()], files: vec!["*.fk".into()], skip: Vec::new(), socket: None, options: Some(json!({"libraries": {"repo": "r", "path": "libs"}})), records: None };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        ix.repos = Some(vec![view::Repo { name: "r".into(), root: root.clone(), base: base.clone() }]);
        let lines = || -> Vec<Value> { std::fs::read_to_string(&log).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect() };
        let updates = || -> Vec<Value> { lines().into_iter().filter_map(|v| v.get("update").cloned()).collect() };
        let refs = |ix: &mut Index| call(ix, "references", &json!({"symbol": "a.fk:foo"}), None).unwrap();

        let first = refs(&mut ix);
        assert!(first.contains("2 uses") && first.contains("b.fk:1:1") && first.contains("c.fk:1:1"), "{first}");
        let l = lines();
        let init = l.iter().find_map(|v| v.get("init")).unwrap();
        assert_eq!(init["repos"][0]["base"], base.as_str());
        assert_eq!(init["repos"][0]["root"], root.display().to_string());
        let u = updates();
        assert_eq!(u.len(), 1);
        assert_eq!(u[0]["full"], true);
        assert_eq!(u[0]["changed"], json!([{"repo": "r", "path": "b.fk", "text": "foo bar\n"}]));
        assert!(!l.iter().any(|v| v.get("did").is_some()), "a view server is never sent a file by didOpen");

        // outside symbols: c.fk deleted, d.fk new
        std::fs::remove_file(root.join("c.fk")).unwrap();
        std::fs::write(root.join("d.fk"), "foo\n").unwrap();
        let second = refs(&mut ix);
        assert!(second.contains("2 uses") && second.contains("d.fk:1:1") && !second.contains("c.fk:1"), "{second}");
        let u = updates();
        assert_eq!(u.len(), 2);
        assert_eq!(u[1]["full"], false);
        assert_eq!(u[1]["changed"], json!([{"repo": "r", "path": "d.fk", "text": "foo\n"}]));
        assert_eq!(u[1]["deleted"], json!([{"repo": "r", "path": "c.fk"}]));

        // nothing changed: no update
        let third = refs(&mut ix);
        assert!(!third.contains("Before this call"), "{third}");
        assert_eq!(updates().len(), 2);

        // b.fk put back as its base was: told as same_as_base
        std::fs::write(root.join("b.fk"), "x\n").unwrap();
        let fourth = refs(&mut ix);
        assert!(fourth.contains("1 uses") && fourth.contains("d.fk:1:1") && !fourth.contains("b.fk:1"), "{fourth}");
        assert_eq!(updates()[2]["deleted"], json!([{"repo": "r", "path": "b.fk", "same_as_base": true}]));

        // the base moves (the layer brought onto a new line commit): the whole view again, with the new base
        git(&["add", "-A"]);
        git(&["commit", "-qm", "line"]);
        let new_base = git(&["rev-parse", "HEAD"]);
        ix.repos = Some(vec![view::Repo { name: "r".into(), root: root.clone(), base: new_base.clone() }]);
        let fifth = refs(&mut ix);
        assert!(fifth.contains("1 uses") && fifth.contains("d.fk:1:1"), "{fifth}");
        let u = updates();
        assert_eq!(u[3]["full"], true);
        assert_eq!(u[3]["base"], json!({"r": new_base}));
        assert_eq!(u[3]["changed"], json!([]));
        assert!(!lines().iter().any(|v| v.get("did").is_some()));
        drop(ix);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A view on the commit's records (paper-research2 N-101 p1625, d): a repository at its base, the records of
    /// three modules written by hand in the agreed form (N-387 post 4), and a server that logs every request.
    struct RecView {
        root: std::path::PathBuf,
        log: std::path::PathBuf,
        ix: Index,
    }

    fn rec_view(tag: &str, toolchain_written: &str) -> RecView {
        let top = std::env::temp_dir().join(format!("symbols-rec-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let root = top.join("r");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.fk"), "module A\ndef foo\ndef bar\n").unwrap();
        std::fs::write(root.join("b.fk"), "module B\nimport A\ndef baz\nuse foo\n").unwrap();
        std::fs::write(root.join("c.fk"), "module C\nuse foo\n").unwrap();
        let git = |a: &[&str]| {
            let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(a).current_dir(&root).output().unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        let base = git(&["rev-parse", "HEAD"]);
        // the records, as prepare writes them
        let dir = top.join("recs").join(toolchain_written);
        for d in ["manifests", "records", "uses"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let put = |rel: &str, v: Value| {
            let f = dir.join(format!("{rel}.json"));
            std::fs::write(&f, serde_json::to_string(&v).unwrap()).unwrap();
            assert!(std::process::Command::new("zstd").args(["-qf", "--rm"]).arg(&f).status().unwrap().success());
        };
        let set = format!("r@{}", &base[..12]);
        let def = |name: &str, line: u64, ty: &str| json!({"name": name, "kind": 12, "kind_name": "function", "range": [line, 0, line, 4 + name.len() as u64], "sel": [line, 4, line, 4 + name.len() as u64], "signature": {"range": [line, 0, line, 4 + name.len() as u64], "text": format!("def {name}")}, "type": ty});
        put(&format!("manifests/{set}"), json!({"format": 1, "set": set, "commits": {"r": base},
            "files": {"r/a.fk": {"sha256": "-", "module": "A", "record": "ka"}, "r/b.fk": {"sha256": "-", "module": "Lib.B", "record": "kb"}, "r/c.fk": {"sha256": "-", "module": "C", "record": "kc"}},
            "imports": {"A": [], "Lib.B": ["A"], "C": ["A"]},
            "names": [["A.foo", "A", "function", [1, 4, 1, 7]], ["A.bar", "A", "function", [2, 4, 2, 7]], ["Lib.B.baz", "Lib.B", "function", [2, 4, 2, 7]]]}));
        put("records/ka", json!({"format": 1, "module": "A", "outline": [def("foo", 1, "{n : Nat} → Vec n"), def("bar", 2, "Nat")], "sites": [], "defines": []}));
        put("records/kb", json!({"format": 1, "module": "Lib.B", "outline": [{"name": "Lib.B", "kind": 2, "kind_name": "module", "range": [0, 0, 3, 7], "sel": [0, 7, 0, 8], "pieces": [], "signature": null, "type": null, "children": [def("baz", 2, "Nat → Nat")]}], "sites": [[3, 4, 3, 7, "A.foo"]], "defines": []}));
        put("records/kc", json!({"format": 1, "module": "C", "outline": [], "sites": [[1, 4, 1, 7, "A.foo"]], "defines": []}));
        put(&format!("uses/{set}"), json!({"format": 1, "uses": {"A.foo": ["kb", "kc"]}}));
        // a server that logs each request and outlines `module`/`def` lines
        let log = top.join("asked.log");
        let fake = top.join("fake.py");
        std::fs::write(&fake, r#"
import sys, json, re
i, o = sys.stdin.buffer, sys.stdout.buffer
log = open(sys.argv[1], "a"); docs = {}
def send(m):
    b = json.dumps(m).encode(); o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def rng(l, c, n): return {"start": {"line": l, "character": c}, "end": {"line": l, "character": c + n}}
while True:
    n = None
    while True:
        l = i.readline()
        if not l: sys.exit(0)
        l = l.strip()
        if not l and n is not None: break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    m = json.loads(i.read(n)); k = m.get("method"); p = m.get("params") or {}
    log.write(json.dumps({"method": k, "uri": (p.get("textDocument") or {}).get("uri")}) + "\n"); log.flush()
    if k in ("textDocument/didOpen",): docs[p["textDocument"]["uri"]] = p["textDocument"]["text"]
    elif k == "textDocument/didChange": docs[p["textDocument"]["uri"]] = p["contentChanges"][0]["text"]
    if "id" not in m: continue
    r = None
    if k == "initialize": r = {"capabilities": {}}
    elif k == "textDocument/documentSymbol":
        t = docs.get(p["textDocument"]["uri"], "").split("\n")
        mod = next((x.split()[1] for x in t if x.startswith("module ")), "M")
        kids = [{"name": w, "kind": 12, "range": rng(ln, 0, len(x)), "selectionRange": rng(ln, 4, len(w))} for ln, x in enumerate(t) for w in re.findall(r"^def (\w+)", x)]
        r = [{"name": mod, "kind": 2, "range": rng(0, 0, 1), "selectionRange": rng(0, 7, len(mod)), "children": kids}]
    send({"jsonrpc": "2.0", "id": m["id"], "result": r})
"#).unwrap();
        let mut conf = ServerConf { lang: "fake".into(), command: vec!["python3".into(), fake.display().to_string(), log.display().to_string()], files: vec!["*.fk".into()], skip: Vec::new(), socket: None, options: None, records: None };
        conf.records = Some(records::Conf { dir: top.join("recs"), toolchain: "tc1".into() });
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        ix.repos = Some(vec![view::Repo { name: "r".into(), root: root.clone(), base }]);
        RecView { root, log, ix }
    }

    fn asked(v: &RecView) -> Vec<String> {
        std::fs::read_to_string(&v.log).unwrap_or_default().lines().map(String::from).collect()
    }

    #[test]
    fn the_commits_records_answer_a_view_that_changed_nothing_with_no_server() {
        let mut v = rec_view("none", "tc1");
        // 1. a first definition, by bare name and with its file: the record's type as Agda printed it
        for name in ["A.foo", "foo", "a.fk:foo"] {
            let d = call(&mut v.ix, "definition", &json!({"symbol": name}), None).unwrap();
            assert!(d.starts_with("A.foo  (function), a.fk:2:1-2:8"), "{name}: {d}");
            assert!(d.contains("type: {n : Nat} → Vec n\n      (as Agda printed it at the commit)"), "{name}: {d}");
        }
        // 2. references from the uses index
        let r = call(&mut v.ix, "references", &json!({"symbol": "A.foo"}), None).unwrap();
        assert!(r.starts_with("A.foo: 2 uses\n"), "{r}");
        assert!(r.contains("b.fk:4:5  at the top  | use foo") && r.contains("c.fk:2:5  at the top  | use foo"), "{r}");
        assert!(!r.contains("as of the commit"), "{r}");
        // a miss: the near names from the names table
        let e = call(&mut v.ix, "definition", &json!({"symbol": "A.fooo"}), None).unwrap_err().to_string();
        assert!(e.contains("near it") && e.contains("A.foo  (function, a.fk)"), "{e}");
        // an outline of an unchanged file: its record's, with the types printed there
        let o = call(&mut v.ix, "outline", &json!({"file": "a.fk"}), None).unwrap();
        assert!(o.starts_with("a.fk (2 symbols; as the commit's build checked it)"), "{o}");
        assert!(o.contains("A.foo  (function, line 2)  : {n : Nat} → Vec n") && o.contains("A.bar  (function, line 3)  : Nat"), "{o}");
        // a record whose outline has its module as the top entry, as agda-ls writes it: its names once qualified
        let d = call(&mut v.ix, "definition", &json!({"symbol": "Lib.B.baz"}), None).unwrap();
        assert!(d.starts_with("Lib.B.baz  (function), b.fk:3:1-3:8"), "{d}");
        let o = call(&mut v.ix, "outline", &json!({"file": "b.fk"}), None).unwrap();
        assert!(o.contains("Lib.B.baz  (function, line 3)") && !o.contains("Lib.B.Lib"), "{o}");
        assert!(asked(&v).is_empty(), "no server was started: {:?}", asked(&v));
        let _ = std::fs::remove_dir_all(v.root.parent().unwrap());
    }

    #[test]
    fn a_file_the_commits_build_did_not_check_says_so() {
        let mut v = rec_view("unchecked", "tc1");
        // the manifest says c.fk did not check
        let dir = v.root.parent().unwrap().join("recs/tc1/manifests");
        let f = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let mut m = records::read_zst(&f).unwrap();
        m["files"]["r/c.fk"] = json!({"sha256": "-", "module": "C", "unchecked": "Interface.Missing not found"});
        let plain = f.with_extension("");
        std::fs::write(&plain, serde_json::to_string(&m).unwrap()).unwrap();
        assert!(std::process::Command::new("zstd").args(["-qf", "--rm"]).arg(&plain).status().unwrap().success());
        let o = call(&mut v.ix, "outline", &json!({"file": "c.fk"}), None).unwrap();
        assert!(o.contains("the commit's build did not check c.fk (Interface.Missing not found)"), "{o}");
        let _ = std::fs::remove_dir_all(v.root.parent().unwrap());
    }

    #[test]
    fn a_changed_file_is_the_servers_and_its_importers_are_as_of_the_commit() {
        let mut v = rec_view("changed", "tc1");
        // the agent's change: foo moves down a line, bar goes
        std::fs::write(v.root.join("a.fk"), "module A\n\ndef foo\n").unwrap();
        // 3. its definition from the server's fresh outline of that file alone
        let d = call(&mut v.ix, "definition", &json!({"symbol": "A.foo"}), None).unwrap();
        assert!(d.starts_with("A.foo  (function), a.fk:3:1-3:8"), "{d}");
        assert!(!d.contains("at the commit"), "{d}");
        let outlined: Vec<String> = asked(&v).into_iter().filter(|l| l.contains("documentSymbol")).collect();
        assert!(outlined.len() == 1 && outlined[0].contains("a.fk"), "only the changed file is outlined: {outlined:?}");
        // a name gone from it is said to be gone
        let e = call(&mut v.ix, "definition", &json!({"symbol": "A.bar"}), None).unwrap_err().to_string();
        assert!(e.contains("A.bar (function) was defined in a.fk at the commit (line 3); that file has changed since, and it no longer defines it"), "{e}");
        // its uses in unchanged importers, from their records, said as of the commit
        let r = call(&mut v.ix, "references", &json!({"symbol": "A.foo"}), None).unwrap();
        assert!(r.starts_with("A.foo: 2 uses\n") && r.contains("b.fk:4:5") && r.contains("c.fk:2:5"), "{r}");
        assert!(r.contains("(as of the commit in b.fk, c.fk: each imports a file you changed and is not checked against it yet)"), "{r}");
        // 4. an importer of the changed file answers from its record, as of the commit
        let d = call(&mut v.ix, "definition", &json!({"symbol": "Lib.B.baz"}), None).unwrap();
        assert!(d.contains("type: Nat → Nat\n      (as of the commit; this module imports a file you changed and is not checked against it yet)"), "{d}");
        let outlined = asked(&v).into_iter().filter(|l| l.contains("documentSymbol")).count();
        assert_eq!(outlined, 1, "b.fk and c.fk are never outlined by the server");
        // the change undone: the record again
        std::fs::write(v.root.join("a.fk"), "module A\ndef foo\ndef bar\n").unwrap();
        let d = call(&mut v.ix, "definition", &json!({"symbol": "A.bar"}), None).unwrap();
        assert!(d.contains("(as Agda printed it at the commit)"), "{d}");
        let _ = std::fs::remove_dir_all(v.root.parent().unwrap());
    }

    #[test]
    fn records_of_another_build_are_not_read() {
        // 5. the folder holds tc0's records; the project's build is tc1
        let mut v = rec_view("other", "tc0");
        let d = call(&mut v.ix, "definition", &json!({"symbol": "A.foo"}), None).unwrap();
        assert!(d.starts_with("A.foo  (function), a.fk:2:1-2:8") && !d.contains("at the commit"), "{d}");
        assert!(asked(&v).iter().any(|l| l.contains("documentSymbol")), "the server's, as before");
        let _ = std::fs::remove_dir_all(v.root.parent().unwrap());
    }

    /// Haskell's form of the records (hs-ls, N-387 p13 and p14): names with their unit, a type and its constructor
    /// apart by namespace, a name exported from another module, a class method's instances. No server.
    #[test]
    fn haskell_records_with_units_namespaces_exports_and_instances() {
        let top = std::env::temp_dir().join(format!("symbols-rec-hs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let root = top.join("r");
        let files = [
            ("src/Kernel/Value.hs", "module Kernel.Value where\ndata Foo = Foo\nmapK :: Int\nmapK = 1\n"),
            ("app/a/Main.hs", "module Main where\nmain = print mapK\n"),
            ("app/b/Main.hs", "module Main where\nmain = print Foo\n"),
            ("src/Re.hs", "module Re (mapK) where\nimport Kernel.Value\n"),
            ("src/Cls.hs", "module Cls where\nclass Sh a where\n  sh :: a -> String\n"),
            ("src/Inst.hs", "module Inst where\ninstance Sh Foo where\n  sh _ = \"\"\n"),
        ];
        for (f, t) in files {
            std::fs::create_dir_all(root.join(f).parent().unwrap()).unwrap();
            std::fs::write(root.join(f), t).unwrap();
        }
        let git = |a: &[&str]| {
            let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(a).current_dir(&root).output().unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        let base = git(&["rev-parse", "HEAD"]);
        let dir = top.join("recs").join("ghc-9.12.4-f1-x");
        for d in ["manifests", "records", "uses"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let put = |rel: &str, v: Value| {
            let f = dir.join(format!("{rel}.json"));
            std::fs::write(&f, serde_json::to_string(&v).unwrap()).unwrap();
            assert!(std::process::Command::new("zstd").args(["-qf", "--rm"]).arg(&f).status().unwrap().success());
        };
        let set = format!("r@{}", &base[..12]);
        let e = |m: &str, k: &str| json!({"sha256": "-", "module": m, "record": k});
        put(&format!("manifests/{set}"), json!({"format": 1, "set": set,
            "files": {"r/src/Kernel/Value.hs": e("lib:Kernel.Value", "kv"), "r/app/a/Main.hs": e("exe-a:Main", "ma"), "r/app/b/Main.hs": e("exe-b:Main", "mb"), "r/src/Re.hs": e("lib:Re", "re"), "r/src/Cls.hs": e("lib:Cls", "cl"), "r/src/Inst.hs": e("lib:Inst", "in")},
            "imports": {},
            "names": [["lib:Kernel.Value.Foo", "lib:Kernel.Value", "data", [1, 5, 1, 8]], ["lib:Kernel.Value.Foo", "lib:Kernel.Value", "constructor", [1, 11, 1, 14]],
                      ["lib:Kernel.Value.mapK", "lib:Kernel.Value", "function", [2, 0, 2, 4]], ["exe-a:Main.main", "exe-a:Main", "function", [1, 0, 1, 4]], ["exe-b:Main.main", "exe-b:Main", "function", [1, 0, 1, 4]],
                      ["lib:Cls.sh", "lib:Cls", "method", [2, 2, 2, 4]], ["lib:Cls.sh", "lib:Inst", "instance method", [2, 2, 2, 4], "instance Sh Foo"]],
            "exports": {"lib:Re": [["lib:Re.mapK", "lib:Kernel.Value.mapK"]]}}));
        let ent = |name: &str, kind: u64, kn: &str, r: [u64; 4], ty: Option<&str>, children: Value| json!({"name": name, "kind": kind, "kind_name": kn, "range": r, "sel": r, "signature": null, "type": ty, "children": children});
        put("records/kv", json!({"format": 1, "module": "lib:Kernel.Value", "outline": [
            ent("Foo", 10, "data", [1, 0, 1, 14], None, json!([ent("Foo", 9, "constructor", [1, 11, 1, 14], Some("Foo"), json!([]))])),
            ent("mapK", 12, "function", [2, 0, 3, 8], Some("Int"), json!([]))], "sites": []}));
        put("records/ma", json!({"format": 1, "module": "exe-a:Main", "outline": [ent("main", 12, "function", [1, 0, 1, 17], Some("IO ()"), json!([]))], "sites": [[1, 13, 1, 17, "lib:Kernel.Value.mapK"]]}));
        put("records/mb", json!({"format": 1, "module": "exe-b:Main", "outline": [ent("main", 12, "function", [1, 0, 1, 16], Some("IO ()"), json!([]))], "sites": [[1, 13, 1, 16, "lib:Kernel.Value.Foo"]]}));
        put("records/re", json!({"format": 1, "module": "lib:Re", "outline": [], "sites": [[0, 7, 0, 9, "lib:Re", null]]}));
        put("records/cl", json!({"format": 1, "module": "lib:Cls", "outline": [ent("Sh", 5, "class", [1, 0, 2, 19], None, json!([ent("sh", 6, "method", [2, 2, 2, 19], Some("Sh a => a -> String"), json!([]))]))], "sites": []}));
        put("records/in", json!({"format": 1, "module": "lib:Inst", "outline": [ent("instance Sh Foo", 5, "instance", [1, 0, 2, 11], None, json!([ent("sh", 6, "method", [2, 2, 2, 11], Some("Foo -> String"), json!([]))]))],
            "sites": [[1, 9, 1, 11, "lib:Cls.Sh", "type"], [1, 12, 1, 15, "lib:Kernel.Value.Foo", "type"]]}));
        put(&format!("uses/{set}"), json!({"format": 1, "uses": {"lib:Kernel.Value.mapK": ["ma"], "lib:Kernel.Value.Foo": ["mb", "in"], "lib:Cls.Sh": ["in"]}}));
        let log = top.join("asked.log");
        let conf = ServerConf { lang: "haskell".into(), command: vec!["sh".into(), "-c".into(), format!("echo started >> {}; exit 1", log.display())], files: vec!["*.hs".into()], skip: Vec::new(), socket: None, options: None, records: Some(records::Conf { dir: top.join("recs"), toolchain: "ghc-9.12.4-f1-x".into() }) };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        ix.repos = Some(vec![view::Repo { name: "r".into(), root: root.clone(), base }]);
        let mut ask = |tool: &str, a: Value| call(&mut ix, tool, &a, None).map_err(|e| e.to_string());
        // a bare name, matched with its unit dropped; GHC's printed type
        let d = ask("definition", json!({"symbol": "mapK"})).unwrap();
        assert!(d.starts_with("Kernel.Value.mapK  (function), src/Kernel/Value.hs:3:1-4:9") && d.contains("type: Int\n      (as GHC printed it at the commit)"), "{d}");
        // the same module name in two units: both offered
        let e = ask("definition", json!({"symbol": "Main.main"})).unwrap_err();
        assert!(e.contains("names 2 symbols") && e.contains("app/a/Main.hs") && e.contains("app/b/Main.hs"), "{e}");
        // a name exported from a module not its own
        let d = ask("definition", json!({"symbol": "Re.mapK"})).unwrap();
        assert!(d.contains("Kernel.Value.mapK  (function)") && d.contains("Re.mapK is exported from Re; it is Kernel.Value.mapK there"), "{d}");
        // a type and its constructor of one spelling: each its own uses, by namespace
        let t = ask("references", json!({"symbol": "Kernel.Value.Foo"})).unwrap();
        assert!(t.starts_with("Kernel.Value.Foo: 1 uses\n") && t.contains("src/Inst.hs:2:13"), "{t}");
        let c = ask("references", json!({"symbol": "Kernel.Value.Foo.Foo"})).unwrap();
        assert!(c.starts_with("Kernel.Value.Foo.Foo: 1 uses\n") && c.contains("app/b/Main.hs:2:14"), "{c}");
        // a class method: its instances; the instance's own method is not a second candidate
        let d = ask("definition", json!({"symbol": "sh"})).unwrap();
        assert!(d.starts_with("Cls.Sh.sh  (method)") && d.contains("instances (1): instance Sh Foo (src/Inst.hs:3)"), "{d}");
        drop(ask);
        assert!(!log.exists(), "no server was started");
        let _ = std::fs::remove_dir_all(&top);
    }
}
