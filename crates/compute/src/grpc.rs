//! gRPC over HTTP/2: a call sends its messages and reads the answers as they come, on TCP or TLS.

use bytes::{Buf, Bytes, BytesMut};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::rustls;

pub const NOT_FOUND: u32 = 5;

/// A call's failure: a gRPC status code and its message; transport failures are 14, unavailable.
#[derive(Debug)]
pub struct Status {
    pub code: u32,
    pub message: String,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "gRPC status {}: {}", self.code, self.message)
    }
}

fn unavailable(e: impl std::fmt::Display) -> Status {
    Status { code: 14, message: e.to_string() }
}

/// The certificates a TLS connection checks the server by, and ours when the server asks for one.
pub struct Tls {
    pub ca_pem: Vec<u8>,
    pub identity_pem: Option<(Vec<u8>, Vec<u8>)>,
}

/// One HTTP/2 connection; clones share it.
#[derive(Clone)]
pub struct Channel {
    send: h2::client::SendRequest<Bytes>,
    base: String,
}

impl Channel {
    pub async fn connect(host: &str, port: u16, tls: Option<&Tls>) -> Result<Self, String> {
        let tcp = TcpStream::connect((host, port)).await.map_err(|e| format!("{host}:{port}: {e}"))?;
        tcp.set_nodelay(true).map_err(|e| e.to_string())?;
        let send = match tls {
            None => handshake(tcp).await?,
            Some(t) => {
                let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
                let s = tokio_rustls::TlsConnector::from(Arc::new(config(t)?)).connect(name, tcp).await.map_err(|e| format!("{host}:{port}: {e}"))?;
                handshake(s).await?
            }
        };
        let scheme = if tls.is_some() { "https" } else { "http" };
        Ok(Channel { send, base: format!("{scheme}://{host}:{port}") })
    }

    /// Calls method `path` with `msgs`, sent as one stream.
    pub async fn call(&self, path: &str, msgs: Vec<Vec<u8>>) -> Result<Answers, Status> {
        let mut send = self.send.clone().ready().await.map_err(unavailable)?;
        let req = http::Request::post(format!("{}{path}", self.base))
            .header("content-type", "application/grpc")
            .header("te", "trailers")
            .body(())
            .map_err(unavailable)?;
        let (resp, mut body) = send.send_request(req, msgs.is_empty()).map_err(unavailable)?;
        let n = msgs.len();
        for (i, m) in msgs.into_iter().enumerate() {
            body.send_data(frame(&m), i + 1 == n).map_err(unavailable)?;
        }
        let resp = resp.await.map_err(unavailable)?;
        if resp.status() != http::StatusCode::OK {
            return Err(unavailable(format!("HTTP {}", resp.status())));
        }
        // a status in the headers is a trailers-only answer: no messages follow
        let early = status(resp.headers());
        Ok(Answers { body: resp.into_body(), buf: BytesMut::new(), ended: early.is_some(), early })
    }
}

async fn handshake<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>(io: T) -> Result<h2::client::SendRequest<Bytes>, String> {
    let (send, conn) = h2::client::handshake(io).await.map_err(|e| e.to_string())?;
    tokio::spawn(async move { drop(conn.await) });
    Ok(send)
}

fn config(t: &Tls) -> Result<rustls::ClientConfig, String> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let mut roots = rustls::RootCertStore::empty();
    for c in CertificateDer::pem_slice_iter(&t.ca_pem) {
        roots.add(c.map_err(|e| format!("ca: {e}"))?).map_err(|e| format!("ca: {e}"))?;
    }
    let b = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots);
    let mut c = match &t.identity_pem {
        None => b.with_no_client_auth(),
        Some((cert, key)) => {
            let certs = CertificateDer::pem_slice_iter(cert).collect::<Result<Vec<_>, _>>().map_err(|e| format!("cert: {e}"))?;
            let key = PrivateKeyDer::from_pem_slice(key).map_err(|e| format!("key: {e}"))?;
            b.with_client_auth_cert(certs, key).map_err(|e| e.to_string())?
        }
    };
    c.alpn_protocols = vec![b"h2".to_vec()];
    Ok(c)
}

/// A message as gRPC frames it: uncompressed, its length, itself.
pub fn frame(m: &[u8]) -> Bytes {
    let mut b = Vec::with_capacity(5 + m.len());
    b.push(0);
    b.extend_from_slice(&(m.len() as u32).to_be_bytes());
    b.extend_from_slice(m);
    b.into()
}

/// The next whole message in `buf`, taken out of it.
pub fn unframe(buf: &mut BytesMut) -> Result<Option<Vec<u8>>, Status> {
    if buf.len() < 5 {
        return Ok(None);
    }
    if buf[0] != 0 {
        return Err(Status { code: 12, message: "a compressed message".into() });
    }
    let n = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    if buf.len() < 5 + n {
        return Ok(None);
    }
    buf.advance(5);
    Ok(Some(buf.split_to(n).to_vec()))
}

fn status(h: &http::HeaderMap) -> Option<Status> {
    let code = h.get("grpc-status")?.to_str().ok()?.parse().ok()?;
    let message = h.get("grpc-message").and_then(|m| m.to_str().ok()).unwrap_or_default().to_string();
    Some(Status { code, message })
}

/// A call's answers, read as they arrive; its end checks the status.
pub struct Answers {
    body: h2::RecvStream,
    buf: BytesMut,
    ended: bool,
    early: Option<Status>,
}

impl Answers {
    pub async fn next(&mut self) -> Result<Option<Vec<u8>>, Status> {
        loop {
            if let Some(m) = unframe(&mut self.buf)? {
                return Ok(Some(m));
            }
            if self.ended {
                return match self.early.take() {
                    Some(s) if s.code != 0 => Err(s),
                    _ if !self.buf.is_empty() => Err(unavailable("a message cut short")),
                    _ => Ok(None),
                };
            }
            match self.body.data().await {
                Some(Ok(chunk)) => {
                    drop(self.body.flow_control().release_capacity(chunk.len()));
                    self.buf.extend_from_slice(&chunk);
                }
                Some(Err(e)) => return Err(unavailable(e)),
                None => {
                    self.ended = true;
                    let t = self.body.trailers().await.map_err(unavailable)?;
                    self.early = Some(t.as_ref().and_then(status).ok_or_else(|| unavailable("no grpc-status"))?);
                }
            }
        }
    }

    /// The one answer of a unary call.
    pub async fn one(mut self) -> Result<Vec<u8>, Status> {
        let m = self.next().await?.ok_or_else(|| unavailable("no answer"))?;
        while self.next().await?.is_some() {}
        Ok(m)
    }
}
