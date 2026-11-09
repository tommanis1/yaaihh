//! coordd: the one live watcher of a box. It owns the box's agents (process,
//! turn, waits, items), their mail, the project's items and rights, and the project's view sources, and tells every
//! change as a numbered event (`coord watch`). Started with the box, as root (cgroups); its socket is
//! /run/coordd.sock, and a caller is known by its process's cgroup, never by a name it gives. A project without a box
//! runs it on the host as the owner's user instead (bin/host-coordd: a systemd user unit with a delegated cgroup).
//!
//!   coordd            run (COORD: the project's coord folder; AGENT_BASE: the agent-base checkout; COORDD_SOCK: the
//!                     socket, default /run/coordd.sock; COORDD_CGROUP: the cgroup it owns, default
//!                     /sys/fs/cgroup/swarm; COORDD_LIVE: the live journal, default /run/coordd/live.json)

mod cgroup;
mod core;
mod jsonl;
mod net;
mod policy;
mod project;
mod transcript;

use crate::cgroup::Caller;
use crate::core::{err, ok, Core, HookNext, TurnEndMail, Who};
use crate::transcript::{ByModel, Ended, TranscriptUsage};
use coord_types::{Event, ModelUsage, Reply, Request, SOCKET};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::io::unix::AsyncFd;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

type Shared = Arc<Mutex<Core>>;

#[tokio::main]
async fn main() {
    let coord = std::env::var("COORD").expect("COORD is not set");
    let base = std::env::var("AGENT_BASE").expect("AGENT_BASE is not set");
    let sock = std::env::var("COORDD_SOCK").unwrap_or_else(|_| SOCKET.into());
    if let Err(e) = cgroup::setup() {
        eprintln!("coordd: cgroups at {}: {e}", cgroup::root().display());
        std::process::exit(1);
    }
    let mut core = Core::load(&coord, &base);
    match project::Project::load(&coord) {
        Ok(p) => core.project = p,
        Err(e) => eprintln!("coordd: {e}"),
    }
    let recovered = core.recover();
    core.hand_over_running(&recovered);
    let views: Vec<(String, String, String)> = core
        .project
        .as_ref()
        .map(|p| p.views.iter().filter_map(|v| v.source.clone().map(|s| (v.id.clone(), p.dir.clone(), s))).collect())
        .unwrap_or_default();
    let shared: Shared = Arc::new(Mutex::new(core));
    for (name, pid) in recovered {
        watch_exit(shared.clone(), name, pid);
    }
    {
        let s = shared.clone();
        tokio::spawn(async move {
            let mut core = s.lock().await;
            for name in core.idle_with_mail() {
                core.deliver_held(&name).await;
            }
        });
    }
    count_usage(shared.clone());
    // the question recogniser is built but not started: the owner wants to talk it through first
    let _ = recognise;
    start_nets(shared.clone()).await;
    for (id, dir, cmd) in views {
        let s = shared.clone();
        // a source that ends is started again, after a pause doubling from 5 s to 5 min while it keeps failing at
        // once: a view whose script died (KeyError after a repository's rename, 8 Nov) stayed dead until coordd's
        // next restart; its last error stays shown meanwhile
        tokio::spawn(async move {
            let rt = tokio::runtime::Handle::current();
            let mut pause = std::time::Duration::from_secs(5);
            loop {
                let began = std::time::Instant::now();
                let (s2, id2, rt2) = (s.clone(), id.clone(), rt.clone());
                project::run_source(dir.clone(), cmd.clone(), move |data, error| {
                    let (s, id) = (s2.clone(), id2.clone());
                    rt2.spawn(async move { s.lock().await.set_view(&id, data, error) });
                })
                .await;
                pause = if began.elapsed() > std::time::Duration::from_secs(60) { std::time::Duration::from_secs(5) } else { (pause * 2).min(std::time::Duration::from_secs(300)) };
                tokio::time::sleep(pause).await;
            }
        });
    }
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap_or_else(|e| panic!("coordd: {sock}: {e}"));
    let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o666));
    eprintln!("coordd: {coord} on {sock}");
    loop {
        if let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(serve(shared.clone(), stream));
        }
    }
}

/// Count each agent's tokens from its sessions' transcripts (transcript.rs), reading on every few seconds, off the lock
/// (the first round reads every transcript whole), and keep the requests read (`Core::add_sent`). An agent's view changes when its counts do, at most once a round, so
/// a busy session does not flood the stream.
fn count_usage(shared: Shared) {
    tokio::spawn(async move {
        let mut files: HashMap<String, TranscriptUsage> = HashMap::new();
        loop {
            let sessions = shared.lock().await.sessions();
            let round = tokio::task::spawn_blocking(move || {
                let counted: Vec<(String, Vec<ModelUsage>, Option<String>, Vec<Ended>)> = sessions
                    .into_iter()
                    .map(|(name, paths)| {
                        let (mut by, mut last, mut ended) = (ByModel::new(), None, Vec::new());
                        for path in paths.iter().flat_map(|p| transcript::session_files(p)) {
                            let u = files.entry(path.clone()).or_default();
                            u.read(&path);
                            ended.append(&mut u.ended);
                            for (k, (all, long)) in &u.by {
                                let e = by.entry(k.clone()).or_default();
                                e.0 += *all;
                                e.1 += *long;
                            }
                            last = last.max(u.last.clone());
                        }
                        let usage = by
                            .into_iter()
                            .filter(|(_, (tokens, _))| tokens.total() > 0)
                            .map(|((model, tier), (tokens, long))| ModelUsage { model, tier, tokens, long })
                            .collect();
                        (name, usage, last, ended)
                    })
                    .collect();
                (files, counted)
            });
            let Ok((f, counted)) = round.await else { return };
            files = f;
            let mut core = shared.lock().await;
            for (name, usage, last, ended) in counted {
                core.set_usage(&name, usage, last);
                core.add_sent(&name, ended);
            }
            drop(core);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

/// When PID exits, agent NAME stopped.
fn watch_exit(shared: Shared, name: String, pid: i32) {
    tokio::spawn(async move {
        if let Ok(fd) = cgroup::pidfd(pid) {
            // SAFETY: the pidfd is ours alone and lives inside the AsyncFd until it is dropped here
            if let Ok(afd) = unsafe { AsyncFd::register(fd) } {
                let _ = afd.readable().await;
            }
        }
        shared.lock().await.exited(&name, pid);
    });
}

async fn serve(shared: Shared, stream: UnixStream) {
    let pid = stream.peer_cred().ok().and_then(|c| c.pid()).unwrap_or(0);
    let caller = cgroup::identify(pid);
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let Ok(Some(line)) = lines.next_line().await else { return };
    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => return send(&mut write, &err(format!("coordd: not a request: {e}"))).await,
    };
    if let Request::Watch { from } = req {
        return watch(shared, from, &mut write).await;
    }
    let reply = handle(&shared, &caller, pid, req).await;
    send(&mut write, &reply).await;
}

async fn send(w: &mut tokio::net::unix::OwnedWriteHalf, reply: &Reply) {
    let _ = w.write_all(format!("{}\n", serde_json::to_string(reply).unwrap()).as_bytes()).await;
}

/// The stream: a snapshot (or the events after FROM, when coordd still has them), then each change.
async fn watch(shared: Shared, from: Option<u64>, w: &mut tokio::net::unix::OwnedWriteHalf) {
    let (first, mut rx) = {
        let core = shared.lock().await;
        let first = from.and_then(|f| core.since(f)).unwrap_or_else(|| vec![core.snapshot()]);
        (first, core.tx.subscribe())
    };
    let mut last = 0;
    for ev in first {
        last = ev.seq;
        if write_event(w, &ev).await.is_err() {
            return;
        }
    }
    loop {
        match rx.recv().await {
            Ok(ev) if ev.seq <= last => {}
            Ok(ev) => {
                last = ev.seq;
                if write_event(w, &ev).await.is_err() {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                let snap = shared.lock().await.snapshot();
                last = snap.seq;
                if write_event(w, &snap).await.is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

async fn write_event(w: &mut tokio::net::unix::OwnedWriteHalf, ev: &Event) -> std::io::Result<()> {
    w.write_all(format!("{}\n", serde_json::to_string(ev).unwrap()).as_bytes()).await
}

fn who(caller: &Caller) -> Option<Who> {
    match caller {
        Caller::Owner => Some(Who::Owner),
        Caller::Agent(a) => Some(Who::Agent(a.clone())),
        _ => None,
    }
}

async fn handle(shared: &Shared, caller: &Caller, pid: i32, req: Request) -> Reply {
    if let Request::Run { name } = &req {
        let r = shared.lock().await.run(caller, name, pid);
        return match r {
            Ok(()) => {
                watch_exit(shared.clone(), name.clone(), pid);
                ok(String::new())
            }
            Err(e) => err(e),
        };
    }
    // the nets' page: files read and nets run off the core's lock (a try runs model calls)
    if let Request::NetGraph { file, text } = &req {
        return match net::draw::graphs(file, text) {
            Ok(g) => ok(serde_json::to_string(&g).unwrap()),
            Err(e) => err(e),
        };
    }
    if let Request::NetMark { mark } = &req {
        if !matches!(caller, Caller::Owner) {
            return err("the owner marks the nets' runs".into());
        }
        let coord = shared.lock().await.coord.clone();
        let m = mark.clone();
        return match tokio::task::spawn_blocking(move || net::mark(&coord, m)).await {
            Ok(Ok(m)) => {
                shared.lock().await.net_marked(&m);
                ok(serde_json::to_string(&m).unwrap())
            }
            Ok(Err(e)) => err(e),
            Err(e) => err(format!("the nets: {e}")),
        };
    }
    if matches!(req, Request::Nets | Request::NetTry { .. } | Request::NetSave { .. } | Request::NetReplay { .. }) {
        if !matches!(caller, Caller::Owner) {
            return err("the nets are read, tried and saved by the owner".into());
        }
        let (places, world) = {
            let core = shared.lock().await;
            (core.places(), core.net_world())
        };
        let done = tokio::task::spawn_blocking(move || match req {
            Request::Nets => Ok(serde_json::to_string(&net::view(&places)).unwrap()),
            Request::NetTry { edits, event, agent, value } => net::try_run(&places, &edits, &event, agent, value, world).map(|r| serde_json::to_string(&r).unwrap()),
            Request::NetSave { edit } => net::save(&places, &edit),
            Request::NetReplay { edits } => net::replay(&places, &edits, world).map(|r| serde_json::to_string(&r).unwrap()),
            _ => unreachable!("matched above"),
        })
        .await;
        return match done {
            Ok(Ok(out)) => ok(out),
            Ok(Err(e)) => err(e),
            Err(e) => err(format!("the nets: {e}")),
        };
    }
    if let Request::Hook { event, input } = &req {
        let Caller::Agent(name) = caller else { return ok(String::new()) }; // not an agent's session: nothing to record
        let next = shared.lock().await.hook(name, event, input);
        return match next {
            HookNext::Done(out) => ok(out),
            HookNext::TurnEnd | HookNext::TurnFailed => {
                // the turn as it ended, before held mail starts the next one; its final message from the hook's input
                let value = shared.lock().await.turn_end_value(name, input.get("last_assistant_message").and_then(|v| v.as_str()));
                // the hook answers at once (its timeout is short); the waits and the idle notice follow on their own
                let continue_ = matches!(next, HookNext::TurnEnd);
                let ended = shared.lock().await.turn_end_mail(name, continue_).await;
                match ended {
                    TurnEndMail::Continue(text) => {
                        if let Some(v) = value {
                            shared.lock().await.turn_end_to_nets(name, v, true, Vec::new());
                        }
                        ok(text)
                    }
                    TurnEndMail::Delivered => {
                        if let Some(v) = value {
                            shared.lock().await.turn_end_to_nets(name, v, true, Vec::new());
                        }
                        ok(String::new())
                    }
                    TurnEndMail::Ended(turn, bg) => {
                        tokio::spawn(turn_waits(shared.clone(), name.clone(), turn, bg, value));
                        ok(String::new())
                    }
                }
            }
            HookNext::CodexStop => {
                let (s, n) = (shared.clone(), name.clone());
                tokio::spawn(async move {
                    // wait for the Codex turn to end before held mail goes in as the next one
                    let rpc = {
                        let core = s.lock().await;
                        let a = &core.agents[&n];
                        let mut cmd = tokio::process::Command::new(format!("{}/bin/codex-rpc", core.base));
                        cmd.arg("wait-idle").arg(a.session.get("session_id").and_then(|v| v.as_str()).unwrap_or(""));
                        cmd.env("CODEX_SOCK", a.session.get("socket").and_then(|v| v.as_str()).unwrap_or(""));
                        cmd
                    };
                    let mut rpc = rpc;
                    let _ = rpc.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().await;
                    // the Codex turn has ended: its transcript holds the whole turn
                    let value = s.lock().await.turn_end_value(&n, None);
                    let ended = s.lock().await.turn_end_mail(&n, false).await;
                    match ended {
                        TurnEndMail::Ended(turn, bg) => turn_waits(s, n, turn, bg, value).await,
                        _ => {
                            if let Some(v) = value {
                                s.lock().await.turn_end_to_nets(&n, v, true, Vec::new());
                            }
                        }
                    }
                });
                ok(String::new())
            }
        };
    }
    if let Request::Pty = req {
        // a terminal holder (coord-pty) leaves its caller's cgroup; what it starts is unclaimed until `coord run`
        if *caller == Caller::Project || pid <= 0 {
            return err("coordd: only a terminal holder of a known process asks for the cgroup pty".into());
        }
        return match cgroup::place(pid, "pty") {
            Ok(()) => ok(String::new()),
            Err(e) => err(format!("coordd: cgroup pty: {e}")),
        };
    }
    if let Request::Older { id, before } = &req {
        // runs a source (seconds): off the lock
        if who(caller).is_none() {
            return err("coordd: a terminal holder's program or a project source cannot do that".into());
        }
        let Some(project) = shared.lock().await.project.clone() else { return err("coord older: no project file".into()) };
        return match project.older(id, before).await {
            Ok(out) => ok(out),
            Err(e) => err(e),
        };
    }
    let Some(w) = who(caller) else { return err("coordd: a terminal holder's program or a project source cannot do that".into()) };
    let mut core = shared.lock().await;
    match req {
        Request::Msg { to, body, esc, as_host, re, quotes } => core.msg(&w, as_host, to, &body, esc, re, quotes).await,
        Request::Ask { body, on } => core.ask(&w, &body, on),
        Request::Drop { id, why } => core.drop_question(&w, &id, why),
        Request::Unfile { id } => core.unfile(&w, &id),
        Request::Release { id } => core.release(&w, &id).await,
        Request::Nets | Request::NetTry { .. } | Request::NetSave { .. } | Request::NetReplay { .. } | Request::NetMark { .. } | Request::NetGraph { .. } => unreachable!("handled before the lock"),
        Request::Unsend { id } => core.unsend(&w, &id),
        Request::Now { id } => core.send_now(&w, &id).await,
        Request::Questions => core.questions(&w),
        Request::Mail { id } => core.mail_list(&w, id.as_deref()),
        Request::State => core.state_lines(),
        Request::Log { name, last } => core.log(&w, &name, last),
        Request::Item { action, arg, why } => core.item(&w, &action, &arg, why),
        Request::Add { name, kind, model } => core.add(&w, &name, &kind, &model),
        Request::Rename { name, to } => core.rename(&w, &name, &to),
        Request::Session { name, session } => core.session(&w, &name, &session),
        Request::Transcript { name, before, after } => core.transcript_page(&w, &name, before, after),
        Request::Image { name, session, offset, n } => core.transcript_image(&w, &name, &session, offset, n),
        Request::Requests { since } => core.requests(&w, since),
        Request::Decision { id } => match policy::decision(&core.coord, &id) {
            Ok(d) => ok(d),
            Err(e) => err(format!("coord decision: {e}")),
        },
        Request::Attach { .. } | Request::Keys { .. } | Request::Resize { .. } | Request::Detach { .. } => {
            err("coordd: terminals are reached through coordc (attach, or the bridge), not coordd".into())
        }
        Request::Run { .. } | Request::Hook { .. } | Request::Watch { .. } | Request::Pty | Request::Older { .. } => unreachable!(),
    }
}

/// A turn ended with no mail held: the agent waits (background work, the project's waits) or is idle, and then its
/// parent is told.
/// What agent NAME waits on after its turn TURN ended with background work BG: the project's waits too, then its
/// state, and the turn end VALUE to the nets with all it waits on (so the clerk takes the waits as facts).
async fn turn_waits(shared: Shared, name: String, turn: u64, bg: Vec<String>, value: Option<serde_json::Value>) {
    let project = shared.lock().await.project.clone();
    let domain = match project {
        Some(p) => p.waits().await.remove(&name).unwrap_or_default(),
        None => Vec::new(),
    };
    let mut core = shared.lock().await;
    if let Some(v) = value {
        core.turn_end_to_nets(&name, v, false, bg.iter().chain(&domain).cloned().collect());
    }
    core.turn_end_state(&name, turn, bg, domain).await;
}

/// Start the nets (`net`): the policy engine they decide with, and coordd's events sent to them. Dry: their effects
/// are logged, not taken (`net::DRY`).
async fn start_nets(shared: Shared) {
    let mut core = shared.lock().await;
    let places = core.places();
    let engine = policy::Engine::start(places.clone());
    let tx = net::start(shared.clone(), places, engine);
    core.net_runs = net::recent(&core.coord, 200).into();
    core.nets = Some(tx);
}

/// The question recogniser: each new thread an agent starts
/// with the owner is given to the typed call `recognise-question`, its model decided by the project's policy
/// (`policy::Engine`); a question it finds makes the mail a question thread, with the decision that chose the model.
/// One mail at a time, off the core's lock while the model runs.
async fn recognise(shared: Shared) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<core::Mail>();
    let engine = {
        let mut core = shared.lock().await;
        core.recognise = Some(tx);
        policy::Engine::start(core.places())
    };
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            let e = engine.clone();
            let input = serde_json::json!({"message": {"from": m.from, "text": m.body}});
            let from = m.from.clone();
            let called = tokio::task::spawn_blocking(move || e.call("recognise-question", input, &from, Vec::new())).await;
            match called {
                Ok(Ok(c)) => match (c.answer, c.decision.chosen) {
                    (Some(a), Some(model)) if !a.is_null() => {
                        let q = coord_types::Recognised {
                            question: a["question"].as_str().unwrap_or_default().into(),
                            quote: a["quote"].as_str().unwrap_or_default().into(),
                            decision: c.decision.id,
                            model,
                        };
                        shared.lock().await.record_recognised(&m.id, q);
                    }
                    (_, None) => eprintln!("coordd: recognise {}: no model ({}; {})", m.id, c.decision.id, c.decision.fault.unwrap_or_default()),
                    _ => {}
                },
                Ok(Err(e)) => eprintln!("coordd: recognise {}: {e}", m.id),
                Err(e) => eprintln!("coordd: recognise {}: {e}", m.id),
            }
        }
    });
}
