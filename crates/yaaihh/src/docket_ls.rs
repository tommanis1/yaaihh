//! The app's client of docket-ls, the language server for Docket's own language (a2h-speed, N-268; paper-research
//! N-101 p650, p654). The program picture of a running ticket shows, on a name hovered, two layers: the program's (the
//! declaration and its doc, from the server's `textDocument/hover`) and the record's (where this ticket's value came
//! from, coordd's, to come). This is the first.
//!
//! The server is started as the app's own child over stdio, one per project, with the folders coordd reads kinds and
//! calls from (agent-base, the library, the project: `edit::folders`), in that order. It is found at
//! YAAIHH_DOCKET_LS, else agent-base's `bin/docket-ls`. A name is looked up by `workspace/symbol` (its declaration, of
//! the container asked: `kind`, `call`, `type`), then hovered where it is declared; answers are kept while the window
//! lives, as the files do not change under a running ticket (an edit reloads them; the cache is dropped then).
//!
//! On Windows there is no child: the coord's shared server (`$COORD/lsp/docket.sock`, started from the project's
//! `[[lsp]]`) is spoken to through the bridge (`Request::Lsp`, protocol 2; docket-core2, N-412 p130), with the same
//! requests. A coord without one, or a bridge older than protocol 2, gives no hovers there, and says nothing.

use crate::link::{LspIn, LspPipe};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// How long one request may take before the server counts as gone.
const WAIT: Duration = Duration::from_secs(5);

/// One server, started for a project, or the coord's shared one through the bridge.
pub struct Ls {
    to: To,
    from: Receiver<LspIn>,
    id: u64,
    /// Its connection ended, or a request had no answer: started again at the next name.
    gone: bool,
}

/// Where the messages go: the app's own child's stdin, or the bridge's relay (on the pipe it was opened on).
enum To {
    Child(Child, ChildStdin),
    Bridge(LspPipe, u64),
}

impl Drop for Ls {
    fn drop(&mut self) {
        if let To::Child(child, _) = &mut self.to {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The shared server's language, as the project's `[[lsp]]` names it.
const LANG: &str = "docket";

/// A declaration's hover: its signature (the code block) and its doc, as the server gives them.
#[derive(Clone, Debug, PartialEq)]
pub struct Hover {
    pub sig: String,
    pub doc: String,
    pub at: String,
}

/// The picture's state for docs: the servers by project id, the hovers got by (project, container, name), and the
/// name the pointer is on.
#[derive(Default)]
pub struct Docs {
    servers: HashMap<String, Result<Ls, String>>,
    pub got: HashMap<(String, String, String), Result<Option<Hover>, String>>,
    pub on: Option<On>,
}

/// A name under the pointer: its element's key in the picture (the card is anchored there), what it names, and the
/// folders it was looked up in (by `project`: a project's id, or a draft's file).
#[derive(Clone, Debug, PartialEq)]
pub struct On {
    pub key: String,
    pub project: String,
    pub container: String,
    pub name: String,
    pub folders: Vec<PathBuf>,
}

impl On {
    pub fn got_key(&self) -> (String, String, String) {
        (self.project.clone(), self.container.clone(), self.name.clone())
    }
}

impl Docs {
    /// The hover of NAME, a CONTAINER (`call`, `kind`, `type`), in project ID read from FOLDERS: asked once, kept.
    /// BRIDGE is the project's relay to its coord's shared server, given when its bridge speaks protocol 2; on
    /// Windows it is the only way (no server is started there on the WSL machine's folders), and without it there is
    /// no hover, and nothing said (elsewhere.rs). On Linux the app's own child serves, as before.
    pub fn ask(&mut self, id: &str, folders: &[PathBuf], base: &str, container: &str, name: &str, bridge: Option<&LspPipe>) {
        let key = (id.to_string(), container.to_string(), name.to_string());
        if self.got.contains_key(&key) {
            return;
        }
        let away = crate::elsewhere::away();
        let bridge = bridge.filter(|_| away);
        if away && bridge.is_none() {
            self.got.insert(key, Ok(None));
            return;
        }
        // a session on a pipe that has ended since is gone with it
        if let (Some(b), Some(Ok(Ls { to: To::Bridge(_, pipe), .. }))) = (bridge, self.servers.get(id)) {
            if *pipe != b.pipe() {
                self.servers.remove(id);
            }
        }
        let server = self.servers.entry(id.to_string()).or_insert_with(|| match bridge {
            Some(b) => Ls::through(b, folders),
            None => Ls::start(&binary(base), folders),
        });
        let r = match server {
            Ok(ls) => ls.hover_named(container, name),
            Err(e) => Err(e.clone()),
        };
        // a server that stopped answering is started again at the next name; through the bridge, one that could
        // not be opened is asked again too (the coord may have one by then)
        if server.as_ref().map_or(bridge.is_some(), |ls| ls.gone) {
            self.servers.remove(id);
        }
        // through the bridge, a coord with no shared server (or a session that ended) means no hovers, quietly
        let r = match r {
            Err(e) if bridge.is_some() => {
                crate::log::line(&format!("{id}: no docket hover through the bridge: {e}"));
                Ok(None)
            }
            r => r,
        };
        self.got.insert(key, r);
    }

    /// What is kept of project ID's answers dropped (its kinds or calls were edited).
    pub fn forget(&mut self, id: &str) {
        self.got.retain(|(p, _, _), _| p != id);
        self.servers.remove(id);
    }
}

/// The server's binary: YAAIHH_DOCKET_LS, else agent-base BASE's bin/docket-ls.
fn binary(base: &str) -> String {
    std::env::var("YAAIHH_DOCKET_LS").unwrap_or_else(|_| format!("{base}/bin/docket-ls"))
}

impl Ls {
    /// BIN started over stdio on FOLDERS, initialized.
    pub fn start(bin: &str, folders: &[PathBuf]) -> Result<Ls, String> {
        let mut cmd = crate::places::command(bin);
        for f in folders {
            cmd.arg("--folder").arg(f);
        }
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("docket-ls ({bin}) did not start: {e}"))?;
        let to = child.stdin.take().ok_or("docket-ls: no stdin")?;
        let out = child.stdout.take().ok_or("docket-ls: no stdout")?;
        let (tx, from) = channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(out);
            while let Some(m) = read_frame(&mut r) {
                if tx.send(LspIn::Message(m)).is_err() {
                    return;
                }
            }
            let _ = tx.send(LspIn::Closed(String::from("docket-ls ended")));
        });
        let mut ls = Ls { to: To::Child(child, to), from, id: 0, gone: false };
        let root = folders.last().map(|f| format!("file://{}", f.display())).unwrap_or_default();
        ls.initialize(json!(root))?;
        Ok(ls)
    }

    /// The coord's shared server through the bridge's relay BRIDGE, initialized. Its folders are the server's own
    /// (its `[[lsp]]` and the coord's project file, read where it runs); FOLDERS, the app's idea of them, are not
    /// paths there on Windows and are not sent.
    pub fn through(bridge: &LspPipe, folders: &[PathBuf]) -> Result<Ls, String> {
        let _ = folders;
        let (from, pipe) = bridge.open(LANG);
        let mut ls = Ls { to: To::Bridge(bridge.clone(), pipe), from, id: 0, gone: false };
        ls.initialize(Value::Null)?;
        Ok(ls)
    }

    fn initialize(&mut self, root: Value) -> Result<(), String> {
        let r = self.request("initialize", json!({"processId": std::process::id(), "rootUri": root, "capabilities": {}}));
        if r.is_err() {
            self.gone = true;
        }
        r?;
        self.notify("initialized", json!({}))
    }

    fn send(&mut self, m: &Value) -> Result<(), String> {
        let r = match &mut self.to {
            To::Child(_, to) => {
                let body = m.to_string();
                write!(to, "Content-Length: {}\r\n\r\n{body}", body.len()).and_then(|_| to.flush()).map_err(|e| format!("docket-ls: {e}"))
            }
            To::Bridge(b, _) => b.send(LANG, m),
        };
        if r.is_err() {
            self.gone = true;
        }
        r
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// METHOD's result; the server's notifications on the way (diagnostics) are passed over.
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.id += 1;
        let id = self.id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let m = match self.from.recv_timeout(WAIT) {
                Ok(LspIn::Message(m)) => m,
                Ok(LspIn::Closed(why)) => {
                    self.gone = true;
                    return Err(format!("docket-ls: {why}"));
                }
                Err(_) => {
                    self.gone = true;
                    return Err(format!("docket-ls: no answer to {method}"));
                }
            };
            if m.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(e) = m.get("error") {
                return Err(format!("docket-ls: {}", e.get("message").and_then(Value::as_str).unwrap_or("an error")));
            }
            return Ok(m.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// The hover of the declaration named NAME in CONTAINER: found by `workspace/symbol`, hovered where it stands.
    pub fn hover_named(&mut self, container: &str, name: &str) -> Result<Option<Hover>, String> {
        let found = self.request("workspace/symbol", json!({"query": name}))?;
        let Some(s) = found.as_array().into_iter().flatten().find(|s| s["name"] == name && s["containerName"] == container) else { return Ok(None) };
        let (uri, start) = (s["location"]["uri"].clone(), s["location"]["range"]["start"].clone());
        let h = self.request("textDocument/hover", json!({"textDocument": {"uri": uri}, "position": start}))?;
        Ok(h["contents"]["value"].as_str().map(split))
    }
}

/// The hover's markdown as its parts: the code block, the paragraphs after it, and its last line (where it stands).
pub fn split(md: &str) -> Hover {
    let (sig, rest) = match md.strip_prefix("```docket\n").and_then(|r| r.split_once("\n```")) {
        Some((sig, rest)) => (sig.to_string(), rest.trim()),
        None => (String::new(), md.trim()),
    };
    let (doc, at) = match rest.rsplit_once("\n\n") {
        Some((d, a)) => (d.trim().to_string(), a.trim().to_string()),
        None if rest.starts_with('/') => (String::new(), rest.to_string()),
        None => (rest.to_string(), String::new()),
    };
    Hover { sig, doc, at }
}

/// One LSP frame from R (headers, a blank line, the body); None at the end.
fn read_frame(r: &mut impl BufRead) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            len = n.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; len?];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in bridge: `sh -c SCRIPT` reads the app's hello, then answers the lines it reads as SCRIPT says.
    fn bridge(script: &str) -> LspPipe {
        let link = crate::link::Link::start(vec!["sh".into(), "-c".into(), script.into()], std::sync::Arc::new(|| {}), std::sync::Arc::new(|| {}));
        let pipe = link.lsp();
        std::mem::forget(link); // its thread runs to the test's end
        pipe
    }

    /// One server message as the bridge sends it.
    fn said(m: Value) -> String {
        // printf, not echo: dash's echo reads the JSON's \n as a newline
        format!("printf '%s\\n' '{}'", json!({"lsp": {"lang": "docket", "message": m}}))
    }

    #[test]
    fn a_hover_comes_through_the_bridge_from_the_coords_shared_server() {
        let md = "```docket\nkind thread(words: Text)\n```\n\nA thread.\n\n/p/core.kind:4";
        let sym = json!([{"name": "thread", "containerName": "kind", "location": {"uri": "file:///p/core.kind", "range": {"start": {"line": 3, "character": 5}}}}]);
        // each message must be a Request::Lsp for docket: the symbol's answer waits on that
        let script = [
            "read hello; read init".to_string(),
            said(json!({"jsonrpc": "2.0", "id": 1, "result": {"capabilities": {}}})),
            "read inited; read sym".into(),
            r#"case "$sym" in *'"lang":"docket"'*'workspace/symbol'*) ;; *) exit 1;; esac"#.into(),
            r#"case "$sym" in *'"op":"lsp"'*) ;; *) exit 1;; esac"#.into(),
            said(json!({"jsonrpc": "2.0", "id": 2, "result": sym})),
            "read hover".into(),
            r#"case "$hover" in *'textDocument/hover'*'"line":3'*) ;; *) exit 1;; esac"#.into(),
            // the bridge's replies (queued) are passed over
            format!("echo '{}'", json!({"id": 1u64 << 48, "reply": {"ok": true, "out": ""}})),
            said(json!({"jsonrpc": "2.0", "id": 3, "result": {"contents": {"kind": "markdown", "value": md}}})),
            "cat >/dev/null".into(),
        ]
        .join("\n");
        let pipe = bridge(&script);
        // the pipe is up once the bridge has been started: wait for its stdin
        let mut ls = (0..100).find_map(|_| Ls::through(&pipe, &[]).ok().or_else(|| { std::thread::sleep(Duration::from_millis(30)); None })).expect("initialized");
        let h = ls.hover_named("kind", "thread").unwrap().unwrap();
        assert_eq!((h.sig.as_str(), h.doc.as_str(), h.at.as_str()), ("kind thread(words: Text)", "A thread.", "/p/core.kind:4"));
    }

    #[test]
    fn a_coord_without_a_shared_server_gives_no_hover_on_windows_and_says_nothing() {
        // the bridge refuses the first message: no socket in this coord
        let refused = json!({"id": 1u64 << 48, "reply": {"ok": false, "out": "no shared docket server in this coord"}});
        let pipe = bridge(&format!("read hello; read init; echo '{refused}'; cat >/dev/null"));
        let e = (0..100).find_map(|_| match Ls::through(&pipe, &[]) {
            Err(e) if e.contains("disconnected") => { std::thread::sleep(Duration::from_millis(30)); None }
            r => Some(r.err()),
        });
        assert_eq!(e.flatten().as_deref(), Some("docket-ls: no shared docket server in this coord"));
        crate::elsewhere::as_on_windows(|| {
            let mut docs = Docs::default();
            docs.ask("p", &[], "/nowhere", "kind", "thread", Some(&pipe));
            assert_eq!(docs.got.values().next(), Some(&Ok(None)));
            assert!(docs.servers.is_empty(), "asked again at the next name");
        });
    }

    #[test]
    fn a_hover_is_cut_into_its_signature_doc_and_place() {
        let h = split("```docket\ncall assign(words: Text) -> Ask(question: Text)\n  task assign\n```\n\nWho writes a piece.\n\n/p/lib.call:40");
        assert_eq!(h, Hover { sig: "call assign(words: Text) -> Ask(question: Text)\n  task assign".into(), doc: "Who writes a piece.".into(), at: "/p/lib.call:40".into() });
        let h = split("```docket\nFact role Identified by agent * String\n```\n\n/p/prelude.eflint:37");
        assert_eq!((h.doc.as_str(), h.at.as_str()), ("", "/p/prelude.eflint:37"));
    }
}
