//! A driver over a REAPI v2 endpoint: blobs by CAS and ByteStream, a job by the action cache, else by Execute.

use crate::grpc::{Channel, Status, Tls, NOT_FOUND};
use crate::pb::{Enc, Msg};
use crate::reapi::{self, digest, read_digest, resource, SHA256};
use crate::{ComputeDriver, Digest, JobResult, JobSpec, Stream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const FIND_MISSING: &str = "/build.bazel.remote.execution.v2.ContentAddressableStorage/FindMissingBlobs";
const GET_ACTION_RESULT: &str = "/build.bazel.remote.execution.v2.ActionCache/GetActionResult";
const EXECUTE: &str = "/build.bazel.remote.execution.v2.Execution/Execute";
const READ: &str = "/google.bytestream.ByteStream/Read";
const WRITE: &str = "/google.bytestream.ByteStream/Write";
const CHUNK: usize = 1 << 20;

/// Where a REAPI server is: `grpc://host:port`, or `grpcs://` with the CA and, for mutual TLS, our certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    pub instance: String,
    pub ca: Option<PathBuf>,
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
}

impl Endpoint {
    fn parts(&self) -> Result<(String, u16, Option<Tls>), String> {
        let (tls, rest) = match self.url.split_once("://") {
            Some(("grpc", r)) => (false, r),
            Some(("grpcs", r)) => (true, r),
            _ => return Err(format!("{}: grpc:// or grpcs:// wanted", self.url)),
        };
        let (host, port) = rest.trim_end_matches('/').rsplit_once(':').ok_or_else(|| format!("{}: no port", self.url))?;
        let port = port.parse().map_err(|_| format!("{}: not a port", self.url))?;
        let read = |p: &PathBuf| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
        let tls = match (tls, &self.ca, &self.cert, &self.key) {
            (false, None, None, None) => None,
            (false, ..) => return Err(format!("{}: certificates given for a plain grpc:// endpoint", self.name)),
            (true, Some(ca), None, None) => Some(Tls { ca_pem: read(ca)?, identity_pem: None }),
            (true, Some(ca), Some(c), Some(k)) => Some(Tls { ca_pem: read(ca)?, identity_pem: Some((read(c)?, read(k)?)) }),
            (true, ..) => return Err(format!("{}: grpcs:// wants ca, and cert with key or neither", self.name)),
        };
        Ok((host.trim_matches(['[', ']']).to_string(), port, tls))
    }
}

/// A driver over one endpoint; its connection is made at the first call and kept.
pub struct RemoteDriver {
    endpoint: Endpoint,
    rt: tokio::runtime::Runtime,
    channel: Option<Channel>,
}

impl RemoteDriver {
    pub fn new(endpoint: Endpoint) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        Ok(RemoteDriver { endpoint, rt, channel: None })
    }

    fn block<T>(&mut self, f: impl AsyncFnOnce(&Channel, &str) -> Result<T, String>) -> Result<T, String> {
        let RemoteDriver { endpoint, rt, channel } = self;
        rt.block_on(async {
            if channel.is_none() {
                let (host, port, tls) = endpoint.parts()?;
                *channel = Some(Channel::connect(&host, port, tls.as_ref()).await?);
            }
            let r = f(channel.as_ref().expect("connected"), &endpoint.instance).await;
            if r.is_err() {
                *channel = None;
            }
            r
        })
    }
}

fn st(s: Status) -> String {
    s.to_string()
}

async fn upload(ch: &Channel, instance: &str, blobs: &[Vec<u8>]) -> Result<Vec<Digest>, String> {
    let digests: Vec<Digest> = blobs.iter().map(|b| Digest::of(b)).collect();
    let req = digests.iter().fold(Enc::new().str(1, instance), |e, d| e.msg(2, digest(d))).uint(4, SHA256).done();
    let resp = ch.call(FIND_MISSING, vec![req]).await.map_err(st)?.one().await.map_err(st)?;
    let missing: Vec<Digest> = Msg::read(&resp)?.all(2).into_iter().map(|b| Msg::read(b).map(|m| read_digest(&m))).collect::<Result<_, _>>()?;
    for (b, d) in blobs.iter().zip(&digests) {
        if d.size > 0 && missing.contains(d) {
            write(ch, instance, b, d).await?;
        }
    }
    Ok(digests)
}

async fn write(ch: &Channel, instance: &str, blob: &[u8], d: &Digest) -> Result<(), String> {
    let name = resource(instance, Some(&upload_id()?), d);
    let chunks: Vec<&[u8]> = blob.chunks(CHUNK).collect();
    let msgs = chunks.iter().enumerate().map(|(i, c)| {
        let e = if i == 0 { Enc::new().str(1, &name) } else { Enc::new() };
        e.int(2, (i * CHUNK) as i64).uint(3, (i + 1 == chunks.len()).into()).bytes(10, c).done()
    });
    let resp = ch.call(WRITE, msgs.collect()).await.map_err(st)?.one().await.map_err(st)?;
    let committed = Msg::read(&resp)?.int(1);
    if committed != d.size {
        return Err(format!("{}: {committed} of {} bytes written", d.hash, d.size));
    }
    Ok(())
}

/// A version-4 UUID's form, from the system's random bytes.
fn upload_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom").map_err(|e| e.to_string())?, &mut b).map_err(|e| e.to_string())?;
    b[6] = b[6] & 0x0f | 0x40;
    b[8] = b[8] & 0x3f | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]))
}

/// The bytes of ByteStream resource `name`, from `offset`, each chunk handed to `got` as it comes.
async fn read(ch: &Channel, name: &str, offset: i64, got: &mut (dyn FnMut(&[u8]) + Send)) -> Result<(), Status> {
    let mut a = ch.call(READ, vec![Enc::new().str(1, name).int(2, offset).done()]).await?;
    while let Some(m) = a.next().await? {
        let m = Msg::read(&m).map_err(|message| Status { code: 13, message })?;
        got(m.bytes(10).unwrap_or_default());
    }
    Ok(())
}

async fn download(ch: &Channel, instance: &str, d: &Digest) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(d.size.max(0) as usize);
    if d.size > 0 {
        read(ch, &resource(instance, None, d), 0, &mut |b| out.extend_from_slice(b)).await.map_err(st)?;
    }
    if Digest::of(&out) != *d {
        return Err(format!("{}: the bytes read are not this blob", d.hash));
    }
    Ok(out)
}

/// The result in `b`, with `out/` as its root directory: a `Tree` given instead is unpacked into the store.
async fn result(ch: &Channel, instance: &str, b: &[u8]) -> Result<JobResult, String> {
    let (mut r, tree) = reapi::result_and_tree(b)?;
    if let Some(t) = tree {
        let t = download(ch, instance, &t).await?;
        let t = Msg::read(&t)?;
        let root = t.bytes(1).ok_or("a Tree with no root")?.to_vec();
        let mut dirs: Vec<Vec<u8>> = t.all(2).into_iter().map(<[u8]>::to_vec).collect();
        r.output_tree_digest = Some(Digest::of(&root));
        dirs.push(root);
        upload(ch, instance, &dirs).await?;
    }
    Ok(r)
}

async fn run(ch: &Channel, instance: &str, job: &JobSpec, out: &mut dyn FnMut(Stream, &[u8]), stop: &AtomicBool) -> Result<JobResult, String> {
    let start = Instant::now();
    let (command, action, key) = crate::action(job);
    upload(ch, instance, &[command, action]).await?;
    let ask = Enc::new().str(1, instance).msg(2, digest(&key)).uint(6, SHA256).done();
    match ch.call(GET_ACTION_RESULT, vec![ask]).await.map_err(st)?.one().await {
        Ok(b) => {
            let mut r = result(ch, instance, &b).await?;
            (r.cached, r.wall_ms) = (true, start.elapsed().as_millis() as u64);
            return Ok(r);
        }
        Err(s) if s.code == NOT_FOUND => {}
        Err(s) => return Err(st(s)),
    }
    let mut ops = ch.call(EXECUTE, vec![Enc::new().str(1, instance).msg(6, digest(&key)).uint(9, SHA256).done()]).await.map_err(st)?;
    // each live stream followed on its own task, its chunks handed over here, in the caller's order
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(Stream, Vec<u8>)>();
    let mut tx = Some(tx);
    let (mut followed, mut seen, mut done) = ([false; 2], [0i64; 2], None);
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            // stopped: the Execute stream is dropped; whether the server then cancels the action is the server's
            _ = tick.tick(), if done.is_none() => if stop.load(Ordering::Relaxed) {
                return Err("stopped".into());
            },
            Some((s, b)) = rx.recv() => {
                seen[s as usize] += b.len() as i64;
                out(s, &b);
            }
            op = ops.next(), if done.is_none() => {
                let p = reapi::operation(&op.map_err(st)?.ok_or("the execution ended with no result")?)?;
                for (s, name) in [(Stream::Stdout, p.stdout_stream), (Stream::Stderr, p.stderr_stream)] {
                    if let (false, Some(tx)) = (name.is_empty() || followed[s as usize], &tx) {
                        followed[s as usize] = true;
                        let (ch, tx) = (ch.clone(), tx.clone());
                        tokio::spawn(async move { drop(read(&ch, &name, 0, &mut |b| drop(tx.send((s, b.to_vec())))).await) });
                    }
                }
                if let Some(d) = p.done {
                    done = Some(d?);
                    tx = None;
                }
            }
            else => break,
        }
    }
    let (done_result, cached) = done.ok_or("the execution ended with no result")?;
    let mut r = result(ch, instance, &done_result).await?;
    // what live streaming did not bring, read whole from the store
    for (s, d) in [(Stream::Stdout, &r.stdout_digest), (Stream::Stderr, &r.stderr_digest)] {
        if seen[s as usize] < d.size {
            let all = download(ch, instance, d).await?;
            out(s, &all[seen[s as usize] as usize..]);
        }
    }
    (r.cached, r.wall_ms) = (cached, start.elapsed().as_millis() as u64);
    Ok(r)
}

impl ComputeDriver for RemoteDriver {
    fn upload(&mut self, blobs: &[Vec<u8>]) -> Result<Vec<Digest>, String> {
        self.block(async |ch, i| upload(ch, i, blobs).await)
    }

    fn download(&mut self, d: &Digest) -> Result<Vec<u8>, String> {
        self.block(async |ch, i| download(ch, i, d).await)
    }

    fn run(&mut self, job: &JobSpec, out: &mut dyn FnMut(Stream, &[u8]), stop: &AtomicBool) -> Result<JobResult, String> {
        self.block(async |ch, i| run(ch, i, job, out, stop).await)
    }
}
