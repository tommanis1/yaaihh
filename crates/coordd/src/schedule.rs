//! His messages held to go later (`Request::Schedule`): kept in `scheduled.json` in the coord folder, so a restart
//! drops none; sent as his own request when he says so, at their time, or once the thread they wait on closes
//! ([`watch`]). One that is refused when it is due stays, its refusal said on it, until he sends or drops it.

use crate::Shared;
use crate::cgroup::Caller;
use crate::core::{Who, err, ok};
use coord_types::Reply;
use coord_types::schedule::{Held, Sched, When, holdable};
use std::path::{Path, PathBuf};

/// How often [`watch`] looks for one that is due.
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// The most held at once.
pub const MOST: usize = 256;

/// One reader or writer of the file at a time.
static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn file(coord: &str) -> PathBuf {
    Path::new(coord).join("scheduled.json")
}

/// The file: the held messages; those out being sent, kept until their answer so a crash loses none; and the id the
/// next one takes, which only grows, so an id is never given twice even while one is out being sent.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Book {
    next: u64,
    held: Vec<Held>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sending: Vec<Held>,
}

/// What a message out being sent when coordd stopped says when it is found again: it may have gone.
pub const MAY_HAVE_GONE: &str = "coordd stopped while sending it, so it may have gone: Send now sends it again";

/// The book in FILE; an empty one when there is no file.
fn read(file: &Path) -> Result<Book, String> {
    match std::fs::read_to_string(file) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book::default()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// B written to FILE whole, readable by the owner only (his drafts), through a file beside it renamed over it; that
/// file removed when the write fails.
fn write(file: &Path, b: &Book) -> Result<(), String> {
    use std::io::Write;
    let part = file.with_extension("json.part");
    let body = serde_json::to_vec_pretty(b).map_err(|e| e.to_string())?;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let wrote = o.open(&part).and_then(|mut w| w.write_all(&body).and_then(|()| w.sync_all())).and_then(|()| std::fs::rename(&part, file));
    if wrote.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    wrote.map_err(|e| format!("{}: {e}", file.display()))
}

/// WHEN as kept: a time read as RFC 3339 and written in UTC as `core::now` writes it, so it compares as text; a time
/// that does not read is refused.
pub fn kept_when(when: When) -> Result<When, String> {
    match when {
        When::At { at } => chrono::DateTime::parse_from_rfc3339(at.trim())
            .map(|t| When::At { at: t.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string() })
            .map_err(|e| format!("coord schedule: {at:?} is not a time ({e})")),
        w => Ok(w),
    }
}

fn listed(held: &[Held]) -> Reply {
    ok(serde_json::to_string(held).unwrap_or_else(|_| "[]".into()))
}

/// `Request::Schedule`: the owner's. Hold one, list them, send one now or drop it; answered with the list.
pub async fn act(shared: &Shared, w: &Who, act: Sched) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord schedule: the owner's".into());
    }
    let coord = shared.lock().await.coord.clone();
    let f = file(&coord);
    let guard = ONE.lock().await;
    let mut b = match read(&f) {
        Ok(b) => b,
        Err(e) => return err(e),
    };
    match act {
        Sched::List => return listed(&b.held),
        Sched::Add { req, when, place } => {
            if !holdable(&req) {
                return err("coord schedule: only a post or a message is held".into());
            }
            if b.held.len() >= MOST {
                return err(format!("coord schedule: {MOST} are held already; send or delete some first"));
            }
            let when = match kept_when(when) {
                Ok(w) => w,
                Err(e) => return err(e),
            };
            let id = b.next.max(b.held.iter().map(|h| h.id).max().unwrap_or(0)) + 1;
            b.next = id;
            b.held.push(Held { id, made: crate::core::now(), when, place, req: *req, failed: None });
        }
        Sched::Drop { id } => b.held.retain(|h| h.id != id),
        Sched::Send { id } => {
            let Some(i) = b.held.iter().position(|h| h.id == id) else { return err(format!("coord schedule: nothing held as {id}")) };
            let h = b.held.remove(i);
            b.sending.push(h.clone());
            if let Err(e) = write(&f, &b) {
                return err(e);
            }
            drop(guard);
            let r = deliver(shared, h, &f).await;
            let _g = ONE.lock().await;
            let b = read(&f).unwrap_or_default();
            return if r.ok { listed(&b.held) } else { err(r.out) };
        }
    }
    match write(&f, &b) {
        Ok(()) => listed(&b.held),
        Err(e) => err(e),
    }
}

/// H, kept in FILE as being sent, sent as his request: gone from FILE once taken; refused, back among the held with
/// its refusal on it.
async fn deliver(shared: &Shared, mut h: Held, f: &Path) -> Reply {
    let r = Box::pin(crate::handle(shared, &Caller::Owner, std::process::id() as i32, h.req.clone())).await;
    let _g = ONE.lock().await;
    let mut b = read(f).unwrap_or_default();
    b.sending.retain(|x| x.id != h.id);
    if !r.ok {
        h.failed = Some(r.out.lines().next().unwrap_or_default().to_string());
        b.held.push(h);
        b.held.sort_by_key(|x| x.id);
    }
    let _ = write(f, &b);
    r
}

/// At coordd's start, before its socket is served: each message that was out being sent when it stopped is held again, saying it may have gone
/// ([`MAY_HAVE_GONE`]), for him to send again or delete; none is sent twice unasked.
pub async fn found_again(coord: &str) {
    let f = file(coord);
    let _g = ONE.lock().await;
    let Ok(mut b) = read(&f) else { return };
    if b.sending.is_empty() {
        return;
    }
    for mut h in std::mem::take(&mut b.sending) {
        h.failed = Some(MAY_HAVE_GONE.into());
        b.held.push(h);
    }
    b.held.sort_by_key(|x| x.id);
    let _ = write(&f, &b);
}

/// Whether H is due at NOW (as `core::now` writes it), CLOSED saying whether a thread is closed (None: no such thread).
pub fn due(h: &Held, now: &str, closed: impl Fn(&str) -> Option<bool>) -> bool {
    h.failed.is_none()
        && match &h.when {
            When::Hold => false,
            When::At { at } => at.as_str() <= now,
            When::Closes { t } => closed(t).unwrap_or(false),
        }
}

/// Every [`EVERY`], each held message that is due sent, in the order held.
pub async fn watch(shared: Shared) {
    loop {
        tokio::time::sleep(EVERY).await;
        send_due(&shared).await;
    }
}

/// The held messages due now, sent.
pub async fn send_due(shared: &Shared) {
    let coord = shared.lock().await.coord.clone();
    let f = file(&coord);
    let due_now: Vec<Held> = {
        let _g = ONE.lock().await;
        let Ok(mut b) = read(&f) else { return };
        if b.held.is_empty() {
            return;
        }
        let now = crate::core::now();
        let core = shared.lock().await;
        let closed = |t: &str| core.tickets.m.ticket_named(t).and_then(|n| core.tickets.m.tickets.get(&n)).map(|x| !x.is_open());
        let (go, stay): (Vec<Held>, Vec<Held>) = b.held.drain(..).partition(|h| due(h, &now, closed));
        drop(core);
        b.held = stay;
        b.sending.extend(go.iter().cloned());
        if go.is_empty() || write(&f, &b).is_err() {
            return;
        }
        go
    };
    for h in due_now {
        deliver(shared, h, &f).await;
    }
}
