//! The PDF view's server: what it serves, to whom, and when its version moves.

use super::*;
use std::net::{Ipv4Addr, Ipv6Addr};

fn pdf(body: &str) -> Vec<u8> {
    format!("%PDF-1.7\n{body}\n%%EOF\n").into_bytes()
}

const LOCAL: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// A server with PATH served, and its token.
fn served(path: &str) -> (Server, String) {
    let s = Server::start().unwrap();
    let got = s.serve(Path::new(path), pdf("one"), (10, Some(1))).unwrap();
    let token = got.url.rsplit('/').next().unwrap().to_string();
    (s, token)
}

fn get(s: &Server, peer: IpAddr, target: &str, extra: &str) -> Reply {
    let head = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{extra}\r\n", s.port);
    answer(&s.shelf.0.lock().unwrap(), s.port, peer, &head)
}

/// Only a `.pdf` with no `..` in its path is served, and only whole: a file pdflatex is still writing is refused,
/// and what was served for that path stays.
#[test]
fn only_a_whole_pdf_is_served() {
    assert!(servable(Path::new("/srv/paper/main.pdf")) && servable(Path::new("C:/papers/MAIN.PDF")));
    for p in ["/srv/paper/main.tex", "/srv/paper/../../etc/passwd.pdf", "../main.pdf", "/srv/paper/pdf", "/srv/main.pdf.txt"] {
        assert!(!servable(Path::new(p)), "{p}");
    }
    assert!(whole(&pdf("x")));
    assert!(!whole(b"%PDF-1.7\nhalf written"), "no end marker yet");
    assert!(!whole(b"<html>%%EOF"), "not a PDF's header");
    let s = Server::start().unwrap();
    assert!(s.serve(Path::new("/srv/notes.txt"), pdf("x"), (1, None)).unwrap_err().contains("not a PDF the view serves"));
    assert!(s.serve(Path::new("/srv/a/../b.pdf"), pdf("x"), (1, None)).is_err());
    let first = s.serve(Path::new("/srv/main.pdf"), pdf("one"), (10, Some(1))).unwrap();
    assert!(s.serve(Path::new("/srv/main.pdf"), b"%PDF-1.7\nhalf".to_vec(), (14, Some(2))).unwrap_err().contains("not a whole PDF yet"));
    assert_eq!(s.serve(Path::new("/srv/main.pdf"), pdf("one"), (10, Some(1))).unwrap(), first, "the old bytes stand");
}

/// The address stays one per path, so the view never navigates; the version moves only when the size or change time
/// does, which is what the page's reload waits for.
#[test]
fn a_changed_file_moves_its_version_and_keeps_its_address() {
    let s = Server::start().unwrap();
    let a = s.serve(Path::new("/srv/main.pdf"), pdf("one"), (10, Some(1))).unwrap();
    assert_eq!(a.version, 1);
    assert!(rune::web_surface::loopback(&a.url) && a.url.starts_with(&a.origin));
    let same = s.serve(Path::new("/srv/main.pdf"), pdf("one"), (10, Some(1))).unwrap();
    assert_eq!(same, a, "nothing changed: no reload");
    let b = s.serve(Path::new("/srv/main.pdf"), pdf("two, rebuilt"), (17, Some(2))).unwrap();
    assert_eq!((b.url.as_str(), b.version), (a.url.as_str(), 2));
    let tok = a.url.rsplit('/').next().unwrap();
    assert_eq!(get(&s, LOCAL, &format!("/version/{tok}"), "").body.as_slice(), b"2");
    assert_eq!(get(&s, LOCAL, &format!("/pdf/{tok}"), "").body.as_slice(), pdf("two, rebuilt").as_slice());
    let other = s.serve(Path::new("/srv/other.pdf"), pdf("x"), (1, None)).unwrap();
    assert_ne!(other.url, a.url, "each file its own token");
}

/// Past KEPT files, the one least lately shown is no longer served.
#[test]
fn the_least_lately_shown_file_goes_first() {
    let s = Server::start().unwrap();
    let urls: Vec<String> = (0..KEPT).map(|i| s.serve(Path::new(&format!("/srv/{i}.pdf")), pdf("x"), (1, None)).unwrap().url).collect();
    s.serve(Path::new("/srv/0.pdf"), pdf("x"), (1, None)).unwrap();
    s.serve(Path::new("/srv/new.pdf"), pdf("x"), (1, None)).unwrap();
    let tok = |u: &str| u.rsplit('/').next().unwrap().to_string();
    assert_eq!(get(&s, LOCAL, &format!("/pdf/{}", tok(&urls[0])), "").status, 200, "shown again: kept");
    assert_eq!(get(&s, LOCAL, &format!("/pdf/{}", tok(&urls[1])), "").status, 403, "the oldest went");
}

/// The file, its version and its page, each only with its token, from this machine, to its own host and origin.
#[test]
fn only_its_token_from_here_on_its_own_origin_is_answered() {
    let (s, tok) = served("/srv/paper/main.pdf");
    let ok = get(&s, LOCAL, &format!("/pdf/{tok}"), "");
    assert_eq!((ok.status, ok.kind), (200, "application/pdf"));
    assert_eq!(get(&s, LOCAL, &format!("/pdf/{tok}?v=3"), "").status, 200, "a query is no part of the path");
    assert_eq!(get(&s, LOCAL, &format!("/version/{tok}"), "").kind, "text/plain; charset=utf-8");
    let page = get(&s, LOCAL, &format!("/view/{tok}"), "");
    let html = String::from_utf8_lossy(&page.body).into_owned();
    assert_eq!(page.kind, "text/html; charset=utf-8");
    assert!(html.contains(&format!("src=\"/pdf/{tok}\"")) && html.contains(&format!("'/version/{tok}?after='")) && html.contains("document.hidden") && html.contains("f.src=") && html.contains("if(!r.ok)throw") && html.contains("<title>main.pdf</title>"));
    let origin = format!("Origin: http://127.0.0.1:{}\r\n", s.port);
    assert_eq!(get(&s, LOCAL, &format!("/version/{tok}"), &origin).status, 200, "its own origin");
    assert_eq!(get(&s, IpAddr::V6(Ipv6Addr::LOCALHOST), &format!("/pdf/{tok}"), "").status, 200);
    let refused = [
        (get(&s, LOCAL, "/pdf/0123456789abcdef0123456789abcdef", ""), 403, "a token it never gave"),
        (get(&s, LOCAL, &format!("/pdf/{}", &tok[..31]), ""), 403, "a token cut short"),
        (get(&s, LOCAL, "/pdf/", ""), 403, "no token"),
        (get(&s, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)), &format!("/pdf/{tok}"), ""), 403, "a peer off this machine"),
        (get(&s, LOCAL, &format!("/pdf/{tok}"), "Origin: http://evil.example\r\n"), 403, "another origin"),
        (get(&s, LOCAL, &format!("/pdf/{tok}"), "Origin: http://127.0.0.1:1\r\n"), 403, "another loopback port"),
        (get(&s, LOCAL, &format!("/pdf/{tok}/../../etc/passwd"), ""), 400, "climbing"),
        (get(&s, LOCAL, &format!("/pdf/%2e%2e/{tok}"), ""), 400, "climbing, encoded"),
        (get(&s, LOCAL, &format!("/pdf/{tok}/main.pdf"), ""), 404, "a path after the token"),
        (get(&s, LOCAL, "/etc/passwd", ""), 404, "no such route"),
        (get(&s, LOCAL, "/srv/paper/main.pdf", ""), 404, "a path is never served by itself"),
    ];
    for (r, want, why) in refused {
        assert_eq!(r.status, want, "{why}");
        assert!(r.kind.starts_with("text/plain") && !r.body.starts_with(b"%PDF"), "{why}: no bytes of the file");
    }
    let shelf = s.shelf.0.lock().unwrap();
    let foreign_host = format!("GET /pdf/{tok} HTTP/1.1\r\nHost: attacker.example:{}\r\n\r\n", s.port);
    assert_eq!(answer(&shelf, s.port, LOCAL, &foreign_host).status, 403, "a name pointed at 127.0.0.1 (rebinding)");
    assert_eq!(answer(&shelf, s.port, LOCAL, &format!("GET /pdf/{tok} HTTP/1.1\r\n\r\n")).status, 403, "no host");
    assert_eq!(answer(&shelf, s.port, LOCAL, &format!("POST /pdf/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", s.port)).status, 405);
    let two_hosts = format!("GET /pdf/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{0}\r\nHost: attacker.example:{0}\r\n\r\n", s.port);
    assert_eq!(answer(&shelf, s.port, LOCAL, &two_hosts).status, 400, "a host given twice");
    let two_origins = format!("GET /pdf/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{0}\r\nOrigin: http://127.0.0.1:{0}\r\nOrigin: http://evil.example\r\n\r\n", s.port);
    assert_eq!(answer(&shelf, s.port, LOCAL, &two_origins).status, 400, "an origin given twice");
}

/// Over a real connection: the file comes whole with its type; a request with another host gets no byte of it.
#[test]
fn the_file_comes_over_loopback() {
    let (s, tok) = served("/srv/paper/main.pdf");
    let ask = |head: String| {
        let mut c = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        c.write_all(head.as_bytes()).unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).unwrap();
        String::from_utf8_lossy(&out).into_owned()
    };
    let got = ask(format!("GET /pdf/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", s.port));
    assert!(got.starts_with("HTTP/1.1 200 OK\r\n") && got.contains("Content-Type: application/pdf\r\n") && got.contains("Cache-Control: no-store\r\n"), "{got}");
    assert!(got.ends_with(&String::from_utf8(pdf("one")).unwrap()));
    assert!(got.contains("X-Frame-Options: SAMEORIGIN\r\n"), "framed by its own page only");
    assert!(!got.contains("Content-Security-Policy"), "the browser's own viewer draws it unhindered");
    let page = ask(format!("GET /view/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", s.port));
    assert!(page.contains("Content-Security-Policy: default-src 'self'"), "{page}");
    let refused = ask(format!("GET /pdf/{tok} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"));
    assert!(refused.starts_with("HTTP/1.1 403 Forbidden\r\n") && !refused.contains("%PDF"), "{refused}");
}

/// Past OPEN_MAX connections at once, one more is closed unanswered and gets nothing of the file; once they close,
/// it is answered again.
#[test]
fn past_its_open_connections_one_more_is_closed() {
    let (s, tok) = served("/srv/paper/main.pdf");
    let ask = || {
        let mut c = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        let _ = c.write_all(format!("GET /version/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", s.port).as_bytes());
        let mut out = Vec::new();
        let _ = c.read_to_end(&mut out);
        String::from_utf8_lossy(&out).into_owned()
    };
    // idle connections that send no head: each holds a place until it closes
    let idle: Vec<TcpStream> = (0..OPEN_MAX).map(|_| TcpStream::connect(("127.0.0.1", s.port)).unwrap()).collect();
    let busy = ask();
    assert!(busy.is_empty(), "closed unanswered: {busy}");
    drop(idle);
    let until = std::time::Instant::now() + Duration::from_secs(5);
    let mut again = ask();
    while !again.starts_with("HTTP/1.1 200") && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        again = ask();
    }
    assert!(again.starts_with("HTTP/1.1 200 OK\r\n") && again.ends_with("\r\n\r\n1"), "{again}");
}

/// `/version?after=N` is answered once the version is not N: at once when it already moved, else when a new file is
/// served; a version asked with no `after` is answered at once, as before.
#[test]
fn a_version_waits_for_the_file_to_change() {
    let (s, tok) = served("/srv/paper/main.pdf");
    let port = s.port;
    let ask = move |target: String| {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        out.rsplit("\r\n\r\n").next().unwrap().to_string()
    };
    assert_eq!(ask(format!("/version/{tok}")), "1");
    assert_eq!(ask(format!("/version/{tok}?after=0")), "1", "already past what the page has");
    let waiting = std::thread::spawn({
        let (ask, tok) = (ask.clone(), tok.clone());
        move || (ask(format!("/version/{tok}?after=1")), std::time::Instant::now())
    });
    std::thread::sleep(Duration::from_millis(300));
    let rebuilt = std::time::Instant::now();
    s.serve(Path::new("/srv/paper/main.pdf"), pdf("two"), (11, Some(2))).unwrap();
    let (got, at) = waiting.join().unwrap();
    assert_eq!(got, "2");
    assert!(at >= rebuilt, "answered only once the file changed");
    assert_eq!(ask("/version/0123456789abcdef0123456789abcdef?after=1".into()), "Forbidden", "refused at once");
}
