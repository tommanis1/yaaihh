//! A REAPI server for tests: CAS, action cache, ByteStream and an Execute that streams a made-up output live.

use crate::grpc::{frame, unframe};
use crate::pb::{Enc, Msg};
use crate::reapi::{digest, read_digest};
use crate::Digest;
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// What the server holds and counts.
#[derive(Default)]
pub struct State {
    pub cas: HashMap<String, Vec<u8>>,
    pub ac: HashMap<String, Vec<u8>>,
    pub executes: usize,
    pub writes: usize,
    /// The live output was read to its end before the execution finished.
    pub followed_live: bool,
    /// Each execution's command, by its number.
    live: HashMap<usize, Vec<String>>,
}

pub type Shared = Arc<(Mutex<State>, Notify)>;

/// A server on a free loopback port; the port and its state.
pub async fn start() -> (u16, Shared) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let st: Shared = Arc::default();
    let s = st.clone();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = l.accept().await {
            let s = s.clone();
            tokio::spawn(async move {
                let mut conn = h2::server::handshake(tcp).await.unwrap();
                while let Some(Ok((req, respond))) = conn.accept().await {
                    tokio::spawn(handle(req, respond, s.clone()));
                }
            });
        }
    });
    (port, st)
}

/// The stdout an execution of `args` gives, in two pieces.
pub fn said(args: &[String]) -> [String; 2] {
    ["ran ".to_string(), format!("{}\n", args.join(" "))]
}

type Answer = Result<Vec<Vec<u8>>, (u32, String)>;

async fn handle(req: http::Request<h2::RecvStream>, mut respond: h2::server::SendResponse<Bytes>, st: Shared) {
    let path = req.uri().path().to_string();
    let mut body = req.into_body();
    let (mut buf, mut msgs) = (BytesMut::new(), Vec::new());
    while let Some(Ok(c)) = body.data().await {
        drop(body.flow_control().release_capacity(c.len()));
        buf.extend_from_slice(&c);
        while let Some(m) = unframe(&mut buf).unwrap() {
            msgs.push(m);
        }
    }
    let head = http::Response::builder().header("content-type", "application/grpc").body(()).unwrap();
    let mut send = respond.send_response(head, false).unwrap();
    let answer = if path.ends_with("/Execute") {
        execute(&msgs[0], &st, &mut send).await
    } else if path.ends_with("/Read") {
        read(&msgs[0], &st, &mut send).await
    } else {
        unary(&path, &msgs, &st)
    };
    let code = match answer {
        Ok(out) => {
            for m in out {
                send.send_data(frame(&m), false).unwrap();
            }
            (0, String::new())
        }
        Err(e) => e,
    };
    let mut t = http::HeaderMap::new();
    t.insert("grpc-status", code.0.to_string().parse().unwrap());
    t.insert("grpc-message", code.1.parse().unwrap());
    drop(send.send_trailers(t));
}

fn unary(path: &str, msgs: &[Vec<u8>], st: &Shared) -> Answer {
    let mut s = st.0.lock().unwrap();
    let m = Msg::read(&msgs[0]).unwrap();
    if path.ends_with("/FindMissingBlobs") {
        let missing = m.all(2).into_iter().map(|d| read_digest(&Msg::read(d).unwrap())).filter(|d| !s.cas.contains_key(&d.hash));
        Ok(vec![missing.fold(Enc::new(), |e, d| e.msg(2, digest(&d))).done()])
    } else if path.ends_with("/GetActionResult") {
        let key = read_digest(&m.msg(2).unwrap().unwrap());
        s.ac.get(&key.hash).cloned().map(|r| vec![r]).ok_or((5, "not in the action cache".into()))
    } else if path.ends_with("/Write") {
        let name = m.str(1);
        let data: Vec<u8> = msgs.iter().flat_map(|b| Msg::read(b).unwrap().bytes(10).unwrap_or_default().to_vec()).collect();
        let hash = name.split('/').rev().nth(1).unwrap().to_string();
        if Digest::of(&data).hash != hash {
            return Err((3, format!("{name}: not its bytes")));
        }
        s.writes += 1;
        let n = data.len();
        s.cas.insert(hash, data);
        Ok(vec![Enc::new().int(1, n as i64).done()])
    } else {
        Err((12, format!("{path}: not served")))
    }
}

fn op(name: &str, meta: Option<Enc>, response: Option<Vec<u8>>) -> Vec<u8> {
    let any = |url: &str, v: Vec<u8>| Enc::new().str(1, url).bytes(2, &v);
    let mut e = Enc::new().str(1, name);
    if let Some(m) = meta {
        e = e.msg(2, any("type.googleapis.com/build.bazel.remote.execution.v2.ExecuteOperationMetadata", m.done()));
    }
    if let Some(r) = response {
        e = e.uint(3, 1).msg(5, any("type.googleapis.com/build.bazel.remote.execution.v2.ExecuteResponse", r));
    }
    e.done()
}

async fn execute(req: &[u8], st: &Shared, send: &mut h2::SendStream<Bytes>) -> Answer {
    let key = read_digest(&Msg::read(req).unwrap().msg(6).unwrap().unwrap());
    let (args, n) = {
        let mut s = st.0.lock().unwrap();
        let action = s.cas.get(&key.hash).cloned().ok_or((9, "the action is not in the store".to_string()))?;
        let cmd = read_digest(&Msg::read(&action).unwrap().msg(1).unwrap().unwrap());
        let command = s.cas.get(&cmd.hash).cloned().ok_or((9, "the command is not in the store".to_string()))?;
        s.executes += 1;
        let args: Vec<String> = Msg::read(&command).unwrap().all(1).iter().map(|a| String::from_utf8_lossy(a).into_owned()).collect();
        let n = s.executes;
        s.live.insert(n, args.clone());
        (args, n)
    };
    let log = format!("logs/{n}/stdout");
    send.send_data(frame(&op("op", Some(Enc::new().uint(1, 3).msg(2, digest(&key)).str(3, &log)), None)), false).unwrap();
    // finished only once the live output was read whole, or after a while when nobody follows it
    let followed = tokio::time::timeout(std::time::Duration::from_secs(5), st.1.notified()).await.is_ok();
    let out = said(&args).concat().into_bytes();
    let mut s = st.0.lock().unwrap();
    s.followed_live = followed;
    let d = Digest::of(&out);
    s.cas.insert(d.hash.clone(), out);
    // out/ given back as a Tree only, holding one file, as NativeLink gives it
    let root = Enc::new().msg(1, Enc::new().str(1, "said").msg(2, digest(&d))).done();
    let tree = Enc::new().bytes(1, &root).done();
    let t = Digest::of(&tree);
    s.cas.insert(t.hash.clone(), tree);
    let result = Enc::new().msg(3, Enc::new().str(1, "out").msg(3, digest(&t))).int(4, 0).msg(6, digest(&d)).done();
    s.ac.insert(key.hash, result.clone());
    Ok(vec![op("op", None, Some(Enc::new().bytes(1, &result).done()))])
}

async fn read(req: &[u8], st: &Shared, send: &mut h2::SendStream<Bytes>) -> Answer {
    let name = Msg::read(req).unwrap().str(1);
    if name.starts_with("logs/") {
        // the live output: what the running command has printed so far, piece by piece
        let n: usize = name.split('/').nth(1).unwrap().parse().unwrap();
        let args = st.0.lock().unwrap().live.get(&n).cloned().ok_or((5, format!("{name}: no such execution")))?;
        for p in said(&args) {
            send.send_data(frame(&Enc::new().bytes(10, p.as_bytes()).done()), false).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        st.1.notify_one();
        return Ok(Vec::new());
    }
    let hash = name.split('/').rev().nth(1).unwrap();
    let b = st.0.lock().unwrap().cas.get(hash).cloned().ok_or((5, format!("{name}: not in the store")))?;
    Ok(vec![Enc::new().bytes(10, &b).done()])
}
