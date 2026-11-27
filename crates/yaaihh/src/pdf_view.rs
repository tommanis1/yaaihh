//! A PDF in the window's own web view (owner, post-v041-smooth-pdf-viewer): WebView2's PDF viewer scrolls every page,
//! zooms, finds text (Ctrl+F) and selects it, drawn as vectors. The window serves the file itself, from the bytes coordd
//! read for it (`Request::ReadFile`), on 127.0.0.1 only. Each file has a token of its own, made from the system's
//! random source; nothing is served without it, and no path is ever taken from an address. No browser is opened: the
//! view is rune's `WebSurface`, which loads only loopback pages.
//!
//! - `/view/TOKEN`: the page the view shows: the PDF in a frame filling it, and a script that waits on
//!   `/version/TOKEN?after=N` and points the frame at the new bytes when the file changed. One wait at a time, none
//!   while the page is hidden.
//! - `/pdf/TOKEN`: the file. `/version/TOKEN`: its version, a number that moves each time new bytes are served; with
//!   `after=N` it is answered once the version is not N, or after [`WAIT`].
//!
//! A request is answered only from a loopback peer, with `Host` this server's own and any `Origin` its own too (so a
//! page elsewhere cannot reach it through a name it points at 127.0.0.1).

use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

#[cfg(test)]
mod tests;

/// The files kept served at once; the least lately shown goes first.
const KEPT: usize = 4;
/// The most a request's head may be.
const HEAD_MAX: usize = 8 << 10;
/// The longest a `/version?after=` waits for a change before it answers the version as it stands.
const WAIT: Duration = Duration::from_secs(25);
/// The longest a reply may take to go out: a peer that stops reading loses its connection.
const WRITE: Duration = Duration::from_secs(10);
/// The most connections answered at once; one past them is closed unanswered (a 503 sent without reading its request
/// would reach the peer as a reset, and reading it would hold the accept loop).
const OPEN_MAX: usize = 16;

/// One file served: where it is, its token, its bytes and their version.
struct Entry {
    path: PathBuf,
    token: String,
    bytes: Arc<Vec<u8>>,
    stamp: (u64, Option<u64>),
    version: u64,
    used: u64,
}

#[derive(Default)]
struct Shelf {
    entries: Vec<Entry>,
    clock: u64,
}

/// The files served, and the bell rung when one's version moves.
type Held = Arc<(Mutex<Shelf>, Condvar)>;

/// The loopback server: its port and the files it serves.
pub struct Server {
    port: u16,
    shelf: Held,
}

/// Where a served file's page is.
#[derive(Clone, Debug, PartialEq)]
pub struct Served {
    pub url: String,
    pub origin: String,
    pub version: u64,
}

/// The window's one server, started at its first PDF.
pub fn server() -> Result<&'static Server, String> {
    static S: OnceLock<Result<Server, String>> = OnceLock::new();
    S.get_or_init(|| Server::start().map_err(|e| format!("The PDF view could not start: {e}"))).as_ref().map_err(Clone::clone)
}

/// Whether BYTES are a whole PDF: its header first and its end marker in its last kilobyte (a file pdflatex is still
/// writing has not got one yet).
pub fn whole(bytes: &[u8]) -> bool {
    let tail = &bytes[bytes.len().saturating_sub(1024)..];
    bytes.starts_with(b"%PDF-") && tail.windows(5).any(|w| w == b"%%EOF")
}

/// Whether PATH may be served: a `.pdf`, with no `..` in it.
pub fn servable(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) && !path.components().any(|c| matches!(c, Component::ParentDir))
}

fn token() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| format!("no random token for the PDF view: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Equal in time whatever where they differ, so a token cannot be guessed a character at a time.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl Server {
    /// A server on 127.0.0.1, on a port the system gives, answering each connection on a thread of its own.
    pub fn start() -> std::io::Result<Server> {
        let l = TcpListener::bind("127.0.0.1:0")?;
        let port = l.local_addr()?.port();
        let shelf: Held = Arc::default();
        let held = shelf.clone();
        let open = Arc::new(AtomicUsize::new(0));
        std::thread::Builder::new().name("pdf-view".into()).spawn(move || {
            for c in l.incoming().flatten() {
                if open.fetch_add(1, Ordering::AcqRel) >= OPEN_MAX {
                    open.fetch_sub(1, Ordering::AcqRel);
                    drop(c);
                    continue;
                }
                let (shelf, mine) = (held.clone(), open.clone());
                let spawned = std::thread::Builder::new().name("pdf-view-1".into()).spawn(move || {
                    answer_on(c, port, &shelf);
                    mine.fetch_sub(1, Ordering::AcqRel);
                });
                // no thread for it: the connection is dropped, and its place freed
                if spawned.is_err() {
                    open.fetch_sub(1, Ordering::AcqRel);
                }
            }
        })?;
        Ok(Server { port, shelf })
    }

    pub fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// PATH's BYTES served, as STAMP (its size and change time, in ms) says they are: its page's address, the same for
    /// the same path while it stays served, and the version, which moves only when the stamp does. A path that may not
    /// be served or bytes that are not a whole PDF are refused, and what was served for the path stays.
    pub fn serve(&self, path: &Path, bytes: Vec<u8>, stamp: (u64, Option<u64>)) -> Result<Served, String> {
        if !servable(path) {
            return Err(format!("{} is not a PDF the view serves", path.display()));
        }
        if !whole(&bytes) {
            return Err(format!("{} is not a whole PDF yet", path.display()));
        }
        let mut sh = self.shelf.0.lock().unwrap_or_else(|e| e.into_inner());
        sh.clock += 1;
        let now = sh.clock;
        let at = match sh.entries.iter().position(|e| e.path == path) {
            Some(i) => {
                let e = &mut sh.entries[i];
                if e.stamp != stamp {
                    e.bytes = Arc::new(bytes);
                    e.stamp = stamp;
                    e.version += 1;
                }
                e.used = now;
                i
            }
            None => {
                if sh.entries.len() >= KEPT
                    && let Some(old) = sh.entries.iter().enumerate().min_by_key(|(_, e)| e.used).map(|(i, _)| i)
                {
                    sh.entries.remove(old);
                }
                sh.entries.push(Entry { path: path.to_path_buf(), token: token()?, bytes: Arc::new(bytes), stamp, version: 1, used: now });
                sh.entries.len() - 1
            }
        };
        // whoever waits on a version hears it
        self.shelf.1.notify_all();
        let e = &sh.entries[at];
        Ok(Served { url: format!("{}/view/{}", self.origin(), e.token), origin: self.origin(), version: e.version })
    }
}

/// What a request is answered with.
#[derive(Debug, PartialEq)]
pub(crate) struct Reply {
    pub status: u16,
    pub kind: &'static str,
    pub body: Arc<Vec<u8>>,
}

fn refuse(status: u16) -> Reply {
    Reply { status, kind: "text/plain; charset=utf-8", body: Arc::new(reason(status).as_bytes().to_vec()) }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    }
}

/// HEAD's values of header NAME, case aside.
fn headers<'a>(head: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> {
    head.lines().skip(1).filter_map(|l| l.split_once(':')).filter(move |(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim())
}

/// HEAD's header NAME's value when it is given once; `Err` when it is given twice or more.
fn header<'a>(head: &'a str, name: &'a str) -> Result<Option<&'a str>, ()> {
    let mut all = headers(head, name);
    let first = all.next();
    if all.next().is_some() { Err(()) } else { Ok(first) }
}

/// The answer to the request whose head is HEAD, from PEER, on PORT.
fn answer(shelf: &Shelf, port: u16, peer: IpAddr, head: &str) -> Reply {
    if !peer.is_loopback() {
        return refuse(403);
    }
    let mut first = head.lines().next().unwrap_or("").split(' ');
    let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
    if method != "GET" {
        return refuse(405);
    }
    let own = format!("127.0.0.1:{port}");
    let (Ok(host), Ok(origin)) = (header(head, "host"), header(head, "origin")) else { return refuse(400) };
    if host != Some(own.as_str()) || origin.is_some_and(|o| o != format!("http://{own}")) {
        return refuse(403);
    }
    let path = target.split(['?', '#']).next().unwrap_or("");
    // nothing is looked up by path, but an address that tries to climb or hides what it says is refused as such
    if path.contains("..") || path.contains('%') || path.contains('\\') {
        return refuse(400);
    }
    let mut parts = path.trim_start_matches('/').split('/');
    let (route, tok, rest) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next());
    if rest.is_some() || !matches!(route, "view" | "pdf" | "version") {
        return refuse(404);
    }
    let Some(e) = shelf.entries.iter().find(|e| same(&e.token, tok)) else { return refuse(403) };
    match route {
        "pdf" => Reply { status: 200, kind: "application/pdf", body: e.bytes.clone() },
        "version" => Reply { status: 200, kind: "text/plain; charset=utf-8", body: Arc::new(e.version.to_string().into_bytes()) },
        _ => Reply { status: 200, kind: "text/html; charset=utf-8", body: Arc::new(page(&e.token, e.version, &e.path).into_bytes()) },
    }
}

/// The page the view shows: the PDF filling it in a frame, reloaded in place when its version moves.
fn page(token: &str, version: u64, path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let title: String = name.chars().map(|c| if "<>&\"'".contains(c) { '_' } else { c }).collect();
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>html,body{{margin:0;height:100%;overflow:hidden;background:var(--yaaihh-bg,#3c3c3c)}}iframe{{border:0;width:100%;height:100%;display:block}}</style></head><body><iframe id=\"pdf\" src=\"/pdf/{token}\"></iframe><script>(()=>{{let v={version},busy=false;const f=document.getElementById('pdf');const at=()=>{{try{{const h=f.contentWindow.location.hash;return /page=/.test(h)?h:'';}}catch(e){{return '';}}}};const look=async()=>{{if(busy||document.hidden)return;busy=true;try{{const r=await fetch('/version/{token}?after='+v,{{cache:'no-store'}});if(!r.ok)throw r.status;const n=+(await r.text());if(n&&n!==v){{f.src='/pdf/{token}?v='+n+at();v=n;}}}}catch(e){{await new Promise(z=>setTimeout(z,5000));}}busy=false;look();}};document.addEventListener('visibilitychange',look);look();}})();</script></body></html>"
    )
}

/// The version N a request waits past (`/version/TOKEN?after=N`), when it asks to wait.
fn after(head: &str) -> Option<u64> {
    let target = head.lines().next()?.split(' ').nth(1)?;
    let (path, query) = target.split_once('?')?;
    path.starts_with("/version/").then_some(())?;
    query.split('&').find_map(|kv| kv.strip_prefix("after=")).and_then(|n| n.parse().ok())
}

/// One connection: its head read (within HEAD_MAX, five seconds), answered (a version asked to wait, once it moved
/// or after WAIT), closed; its reply given at most WRITE to go out.
fn answer_on(mut c: TcpStream, port: u16, shelf: &(Mutex<Shelf>, Condvar)) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = c.set_write_timeout(Some(WRITE));
    let Ok(peer) = c.peer_addr().map(|a| a.ip()) else { return };
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match c.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
        if head.len() > HEAD_MAX {
            return send(&mut c, &refuse(400));
        }
    }
    let head = String::from_utf8_lossy(&head);
    let (lock, bell) = shelf;
    let mut sh = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut r = answer(&sh, port, peer, &head);
    // only an answered request waits: one refused is told at once
    if let Some(n) = after(&head).filter(|n| r.status == 200 && r.body.as_slice() == n.to_string().as_bytes()) {
        let until = std::time::Instant::now() + WAIT;
        loop {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            sh = bell.wait_timeout(sh, left).unwrap_or_else(|e| e.into_inner()).0;
            r = answer(&sh, port, peer, &head);
            if r.status != 200 || r.body.as_slice() != n.to_string().as_bytes() {
                break;
            }
        }
    }
    drop(sh);
    send(&mut c, &r);
}

fn send(c: &mut TcpStream, r: &Reply) {
    // the page only: a policy on the PDF itself could keep the browser's own viewer from drawing it
    let policy = if r.kind.starts_with("text/html") { "Content-Security-Policy: default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; frame-ancestors 'self'\r\n" } else { "" };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: SAMEORIGIN\r\n{policy}Referrer-Policy: no-referrer\r\nConnection: close\r\n\r\n",
        r.status,
        reason(r.status),
        r.kind,
        r.body.len()
    );
    let _ = c.write_all(head.as_bytes()).and_then(|_| c.write_all(&r.body));
}
