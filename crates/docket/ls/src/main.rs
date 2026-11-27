//! docket-ls: the language server for Docket's own language (N-101 p646): `.kind`, `.call`, the types declared in
//! them, and the policy's `.eflint`. Definition, hover, references, document and workspace symbols, and the loader's
//! refusals as diagnostics.
//!
//! ```text
//! docket-ls [--folder DIR]...                    an LSP server on stdin and stdout (an `[[lsp]] command`)
//! docket-ls serve --socket PATH [--folder DIR]... [--log FILE]  a shared server: one LSP session per connection (`[[lsp]] socket`)
//! docket-ls check [--folder DIR]...              the refusals, one a line as FILE:LINE: words; exit 1 if any
//! ```
//!
//! The folders, in coordd's order (agent-base, the library, the project), come from, first found: `initialize`'s
//! options (`folders`, or under `docket`), `--folder`, else the project file (SYMBOLS_PROJECT, else the first line of
//! $COORD/project): its folder is the project, its `library` the library, and agent-base is DOCKET_BASE, else
//! AGENT_BASE, else the options' `base`. A client that gives `viewPid` has its files read at /proc/PID/root.

mod index;

use index::{Fs, Index, Space};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut folders = Vec::new();
    let (mut mode, mut socket) = ("stdio", None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--folder" => {
                i += 1;
                folders.push(args.get(i).cloned().unwrap_or_default());
            }
            "--socket" => {
                i += 1;
                socket = args.get(i).cloned();
            }
            "--log" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    let _ = LOG.set(p.clone());
                }
            }
            "serve" => mode = "serve",
            "check" => mode = "check",
            "--help" | "-h" => {
                eprintln!("docket-ls [--folder DIR]... | docket-ls serve --socket PATH [--folder DIR]... | docket-ls check [--folder DIR]...");
                return;
            }
            a => {
                eprintln!("docket-ls: unknown argument {a}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    match mode {
        "check" => {
            let folders = if folders.is_empty() { from_project(&json!({})) } else { folders.iter().map(|f| absolute(f)).collect() };
            let ix = Index::build(&folders, &Fs::default());
            let mut n = 0;
            for (f, ds) in &ix.diags {
                for (l, m) in ds {
                    n += 1;
                    if m.starts_with(f.as_str()) || f.is_empty() { println!("{m}") } else { println!("{f}:{}: {m}", l + 1) }
                }
            }
            eprintln!("docket-ls: {} files in {}; {n} refusals", ix.files.len(), folders.join(", "));
            std::process::exit(if n > 0 { 1 } else { 0 });
        }
        "serve" => {
            let Some(path) = socket else {
                eprintln!("docket-ls serve: --socket PATH");
                std::process::exit(2);
            };
            serve(&path, folders);
        }
        _ => {
            let stdin = std::io::stdin();
            Session::new(folders).run(&mut stdin.lock(), &mut std::io::stdout());
        }
    }
}

/// `--log PATH`: every message in and out appended to PATH, one a line (`-->` in, `<--` out), to show a client's
/// exchanges as they were.
static LOG: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn log(dir: &str, msg: &str) {
    if let Some(p) = LOG.get() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{dir} {msg}");
        }
    }
}

/// P with its `.` and `..` parts taken out and no trailing `/`, as the client's paths come (no link is followed: the
/// path is the agent's, read maybe under /proc/PID/root).
fn normal(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    format!("/{}", out.join("/"))
}

fn absolute(p: &str) -> String {
    let p = std::path::Path::new(p);
    let p = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    p.canonicalize().unwrap_or(p).display().to_string()
}

/// A shared server on the unix socket PATH: each connection its own session, in a thread of its own.
fn serve(path: &str, folders: Vec<String>) {
    use std::os::unix::net::{UnixListener, UnixStream};
    if std::path::Path::new(path).exists() {
        if UnixStream::connect(path).is_ok() {
            eprintln!("docket-ls: a server already answers on {path}");
            std::process::exit(1);
        }
        let _ = std::fs::remove_file(path);
    }
    if let Some(d) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let l = UnixListener::bind(path).unwrap_or_else(|e| {
        eprintln!("docket-ls: binding {path}: {e}");
        std::process::exit(1)
    });
    eprintln!("docket-ls: serving on {path}");
    for c in l.incoming().flatten() {
        let folders = folders.clone();
        std::thread::spawn(move || {
            let Ok(w) = c.try_clone() else { return };
            let mut r = BufReader::new(c);
            let mut w = std::io::BufWriter::new(w);
            Session::new(folders).run(&mut r, &mut w);
        });
    }
}

/// The folders the project file names (see the module's comment), OPTS the client's options.
fn from_project(opts: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let base = std::env::var("DOCKET_BASE").or_else(|_| std::env::var("AGENT_BASE")).ok().or_else(|| opts.get("base").and_then(Value::as_str).map(String::from));
    if let Some(b) = base {
        out.push(b);
    }
    let file = std::env::var("SYMBOLS_PROJECT").ok().or_else(|| {
        let coord = std::env::var("COORD").ok()?;
        Some(std::fs::read_to_string(format!("{coord}/project")).ok()?.lines().next()?.trim().to_string())
    });
    if let Some(file) = file.filter(|f| !f.is_empty()) {
        let dir = std::path::Path::new(&file).parent().map(|p| p.display().to_string()).unwrap_or_default();
        let lib = std::fs::read_to_string(&file).ok().and_then(|t| t.parse::<toml::Table>().ok()).and_then(|t| t.get("project")?.get("library")?.as_str().map(String::from));
        if let Some(l) = lib {
            let p = match l.strip_prefix("~/") {
                Some(r) => format!("{}/{r}", std::env::var("HOME").unwrap_or_default()),
                None if l.starts_with('/') => l,
                None => format!("{dir}/{l}"),
            };
            out.push(std::path::Path::new(&p).canonicalize().map(|p| p.display().to_string()).unwrap_or(p));
        }
        out.push(dir);
    }
    out
}

fn uri_path(uri: &str) -> String {
    let p = uri.strip_prefix("file://").unwrap_or(uri);
    let b = p.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn path_uri(p: &str) -> String {
    let mut s = String::from("file://");
    for c in p.chars() {
        match c {
            ' ' => s.push_str("%20"),
            '#' => s.push_str("%23"),
            '%' => s.push_str("%25"),
            '?' => s.push_str("%3F"),
            c => s.push(c),
        }
    }
    s
}

fn range(p: &index::Place) -> Value {
    json!({"start": {"line": p.line, "character": p.col}, "end": {"line": p.line, "character": p.col + p.len}})
}

fn location(p: &index::Place) -> Value {
    json!({"uri": path_uri(&p.file), "range": range(p)})
}

/// LSP's SymbolKind for a declaration.
fn symbol_kind(what: &str) -> u32 {
    match what {
        "kind" => 5,             // Class
        "call" => 12,            // Function
        "decision" => 12,        // Function
        "type" => 23,            // Struct
        "fact" => 11,            // Interface
        "act" => 24,             // Event
        "policy decision" => 25, // Operator
        "fact extended" => 11,   // Interface
        "field" => 8,            // Field
        _ => 13,
    }
}

struct Session {
    cli: Vec<String>,
    folders: Vec<String>,
    fs: Fs,
    ix: Index,
    published: std::collections::BTreeSet<String>,
    out: Vec<Value>,
}

impl Session {
    fn new(cli: Vec<String>) -> Session {
        Session { cli, folders: Vec::new(), fs: Fs::default(), ix: Index::default(), published: Default::default(), out: Vec::new() }
    }

    fn run(&mut self, r: &mut dyn BufRead, w: &mut dyn Write) {
        while let Some(msg) = read_message(r) {
            log("-->", &msg);
            let Ok(v) = serde_json::from_str::<Value>(&msg) else { continue };
            let exit = v.get("method").and_then(Value::as_str) == Some("exit");
            self.handle(&v);
            for m in std::mem::take(&mut self.out) {
                let s = m.to_string();
                log("<--", &s);
                if write!(w, "Content-Length: {}\r\n\r\n{s}", s.len()).and_then(|_| w.flush()).is_err() {
                    return;
                }
            }
            if exit {
                return;
            }
        }
    }

    fn reply(&mut self, id: &Value, result: Value) {
        self.out.push(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    fn reindex(&mut self) {
        self.ix = Index::build(&self.folders, &self.fs);
        // the files with refusals now, and those that had some before (cleared): no empty list for every file
        let mut files: std::collections::BTreeSet<String> = self.ix.diags.keys().cloned().collect();
        files.extend(std::mem::take(&mut self.published));
        for f in &files {
            if f.is_empty() {
                continue;
            }
            let ds: Vec<Value> = self.ix.diags.get(f).into_iter().flatten().map(|(l, m)| json!({"range": {"start": {"line": l, "character": 0}, "end": {"line": l, "character": 1000}}, "severity": 1, "source": "docket", "message": m})).collect();
            if !ds.is_empty() {
                self.published.insert(f.clone());
            }
            self.out.push(json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {"uri": path_uri(f), "diagnostics": ds}}));
        }
        for (_, m) in self.ix.diags.get("").into_iter().flatten() {
            self.out.push(json!({"jsonrpc": "2.0", "method": "window/logMessage", "params": {"type": 1, "message": m}}));
        }
    }

    fn here(&self, p: &Value) -> Option<(Space, String)> {
        let file = uri_path(p.pointer("/textDocument/uri")?.as_str()?);
        let line = p.pointer("/position/line")?.as_u64()? as usize;
        let col = p.pointer("/position/character")?.as_u64()? as usize;
        self.ix.at(&file, line, col)
    }

    fn handle(&mut self, v: &Value) {
        let method = v.get("method").and_then(Value::as_str).unwrap_or("");
        let p = v.get("params").cloned().unwrap_or(Value::Null);
        let id = v.get("id").cloned();
        match method {
            "initialize" => {
                let opts = p.get("initializationOptions").cloned().unwrap_or(Value::Null);
                let mine = opts.get("docket").cloned().unwrap_or(opts.clone());
                let root = p.get("rootUri").and_then(Value::as_str).map(uri_path).or_else(|| opts.get("root").and_then(Value::as_str).map(String::from));
                let given: Vec<String> = mine.get("folders").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
                let rel = |f: &String| normal(&if f.starts_with('/') { f.clone() } else { format!("{}/{f}", root.clone().unwrap_or_default()) });
                self.folders = if !given.is_empty() {
                    given.iter().map(rel).collect()
                } else if !self.cli.is_empty() {
                    self.cli.iter().map(|f| absolute(f)).collect()
                } else {
                    let mut f = from_project(&mine);
                    if f.is_empty() {
                        f.extend(root.clone());
                    }
                    f
                };
                if let Some(pid) = opts.get("viewPid").and_then(Value::as_u64) {
                    let r = format!("/proc/{pid}/root");
                    if std::path::Path::new(&r).is_dir() {
                        self.fs.root = r;
                    }
                }
                eprintln!("docket-ls: folders {}{}", self.folders.join(", "), if self.fs.root.is_empty() { String::new() } else { format!(", read at {}", self.fs.root) });
                let caps = json!({
                    "textDocumentSync": {"openClose": true, "change": 1, "save": {"includeText": false}},
                    "definitionProvider": true, "hoverProvider": true, "referencesProvider": true,
                    "documentSymbolProvider": true, "workspaceSymbolProvider": true,
                });
                self.reply(&id.unwrap_or(Value::Null), json!({"capabilities": caps, "serverInfo": {"name": "docket-ls", "version": env!("CARGO_PKG_VERSION")}}));
            }
            "initialized" => self.reindex(),
            "textDocument/didOpen" | "textDocument/didChange" => {
                let Some(uri) = p.pointer("/textDocument/uri").and_then(Value::as_str) else { return };
                let text = p.pointer("/textDocument/text").and_then(Value::as_str).map(String::from).or_else(|| p.pointer("/contentChanges").and_then(Value::as_array).and_then(|a| a.last()).and_then(|c| c.get("text")).and_then(Value::as_str).map(String::from));
                if let Some(t) = text {
                    self.fs.open.insert(uri_path(uri), t);
                    self.reindex();
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = p.pointer("/textDocument/uri").and_then(Value::as_str) {
                    self.fs.open.remove(&uri_path(uri));
                    self.reindex();
                }
            }
            "textDocument/didSave" | "workspace/didChangeWatchedFiles" => self.reindex(),
            "textDocument/definition" => {
                let r = match self.here(&p) {
                    Some((Space::File, path)) => json!([{"uri": path_uri(&path), "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}}]),
                    Some((s, n)) => json!(self.ix.decls_of(s, &n).iter().take(if s == Space::Field { usize::MAX } else { 1 }).map(|d| location(&d.at)).collect::<Vec<_>>()),
                    None => Value::Null,
                };
                self.reply(&id.unwrap_or(Value::Null), r);
            }
            "textDocument/hover" => {
                let r = match self.here(&p) {
                    Some((Space::File, path)) => {
                        let head: Vec<String> = self.fs.read(&path).unwrap_or_default().lines().take(12).map(String::from).collect();
                        json!({"contents": {"kind": "markdown", "value": format!("{path}\n\n```\n{}\n```", head.join("\n"))}})
                    }
                    Some((Space::Field, n)) if !self.ix.decls_of(Space::Field, &n).is_empty() => {
                        let hs: Vec<String> = self.ix.decls_of(Space::Field, &n).iter().filter(|d| d.stands).map(|d| d.hover.clone()).collect();
                        json!({"contents": {"kind": "markdown", "value": hs.join("\n\n---\n\n")}})
                    }
                    Some((s, n)) => match self.ix.decls_of(s, &n).first() {
                        Some(d) => json!({"contents": {"kind": "markdown", "value": d.hover}}),
                        None => json!({"contents": {"kind": "markdown", "value": format!("`{n}`: declared nowhere this server reads ({})", self.folders.join(", "))}}),
                    },
                    None => Value::Null,
                };
                self.reply(&id.unwrap_or(Value::Null), r);
            }
            "textDocument/references" => {
                let decl = p.pointer("/context/includeDeclaration").and_then(Value::as_bool).unwrap_or(false);
                let r = match self.here(&p) {
                    Some((s, n)) => {
                        let mut out: Vec<Value> = Vec::new();
                        for (s, n) in std::iter::once((s, n.clone())).chain(self.ix.twin(s, &n)) {
                            if decl {
                                out.extend(self.ix.decls_of(s, &n).iter().map(|d| location(&d.at)));
                            }
                            out.extend(self.ix.uses_of(s, &n).iter().map(|u| location(&u.at)));
                        }
                        json!(out)
                    }
                    None => Value::Null,
                };
                self.reply(&id.unwrap_or(Value::Null), r);
            }
            "textDocument/documentSymbol" => {
                let file = p.pointer("/textDocument/uri").and_then(Value::as_str).map(uri_path).unwrap_or_default();
                let r: Vec<Value> = self
                    .ix
                    .decls
                    .iter()
                    .filter(|d| d.at.file == file)
                    .map(|d| json!({"name": d.name.trim_start_matches(docket_machine::syntax::DECIDE), "detail": d.sig, "kind": symbol_kind(d.what), "range": {"start": {"line": d.at.line, "character": if d.space == Space::Field { d.at.col } else { 0 }}, "end": {"line": d.end.0, "character": d.end.1}}, "selectionRange": range(&d.at)}))
                    .collect();
                self.reply(&id.unwrap_or(Value::Null), json!(r));
            }
            "workspace/symbol" => {
                let q = p.get("query").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let r: Vec<Value> = self
                    .ix
                    .decls
                    .iter()
                    .filter(|d| d.name.to_lowercase().contains(&q))
                    .map(|d| json!({"name": d.name.trim_start_matches(docket_machine::syntax::DECIDE), "kind": symbol_kind(d.what), "location": location(&d.at), "containerName": d.what}))
                    .collect();
                self.reply(&id.unwrap_or(Value::Null), json!(r));
            }
            "shutdown" => self.reply(&id.unwrap_or(Value::Null), Value::Null),
            "exit" => {}
            _ => {
                if let Some(id) = id {
                    if !method.starts_with("$/") {
                        self.out.push(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("docket-ls does not answer {method}")}}));
                    }
                }
            }
        }
    }
}

/// One message of the base protocol (`Content-Length: N`, a blank line, N bytes); none at the end of input.
fn read_message(r: &mut dyn BufRead) -> Option<String> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            if len.is_some() {
                break;
            }
            continue;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            len = n.trim().parse::<usize>().ok();
        }
    }
    let mut buf = vec![0; len?];
    r.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}
