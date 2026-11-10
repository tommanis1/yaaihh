//! coordd: the one live watcher of a box (vault designs/yaaihh-2026-11-07.md §7). It owns the box's agents (process,
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
mod follow;
mod jsonl;
mod net;
mod policy;
mod project;
mod transcript;

use crate::cgroup::Caller;
use crate::core::{err, ok, Core, HookNext, Job, Who};
use crate::transcript::{ByModel, Ended, TranscriptUsage};
use coord_types::{Event, ModelUsage, Reply, Request, SOCKET};
use serde_json::Value;
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
    jsonl::start_writer();
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
    let events = core.project.as_ref().and_then(|p| Some((p.dir.clone(), p.project.events.clone()?)));
    let shared: Shared = Arc::new(Mutex::new(core));
    for (name, pid) in recovered {
        watch_exit(shared.clone(), name, pid);
    }
    shared.lock().await.jobs = Some(senders(shared.clone()));
    follow(shared.clone());
    count_usage(shared.clone());
    read_models(shared.clone());
    // the question recogniser is built but not started: the owner wants to talk it through first (8 Nov, m93)
    let _ = recognise;
    start_nets(shared.clone()).await;
    for (id, dir, cmd) in views {
        let (s, rt) = (shared.clone(), tokio::runtime::Handle::current());
        keep_source(dir, cmd, move |data, error| {
            let (s, id) = (s.clone(), id.clone());
            rt.spawn(async move { s.lock().await.set_view(&id, data, error) });
        });
    }
    if let Some((dir, cmd)) = events {
        let (s, rt) = (shared.clone(), tokio::runtime::Handle::current());
        keep_source(dir, cmd, move |data, error| {
            let s = s.clone();
            rt.spawn(project_event(s, data, error));
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

/// Run a project source (`project::run_source`) for as long as coordd runs: one that ends is started again, after a
/// pause doubling from 5 s to 5 min while it keeps failing at once. A view whose script died (KeyError after a
/// repository's rename, 8 Nov) stayed dead until coordd's next restart; its last error stays shown meanwhile.
fn keep_source(dir: String, cmd: String, send: impl Fn(Option<Value>, Option<String>) + Clone + Send + Sync + 'static) {
    tokio::spawn(async move {
        let mut pause = std::time::Duration::from_secs(5);
        loop {
            let began = std::time::Instant::now();
            project::run_source(dir.clone(), cmd.clone(), send.clone()).await;
            pause = if began.elapsed() > std::time::Duration::from_secs(60) { std::time::Duration::from_secs(5) } else { (pause * 2).min(std::time::Duration::from_secs(300)) };
            tokio::time::sleep(pause).await;
        }
    });
}

/// A line of the project's event source (`Core::project_event`). The runner seen at start, going up or down, or a check or job
/// ending, changes what agents wait on: the project's waits are read again, off the lock, and restated
/// (`Core::refresh_waits`), so no agent stays "waiting on" a check no living runner holds.
async fn project_event(shared: Shared, data: Option<Value>, error: Option<String>) {
    let Some(v) = data else { return eprintln!("coordd: project events: {}", error.unwrap_or_default()) };
    let refresh = matches!(v.get("event").and_then(Value::as_str), Some("runner_seen" | "runner_up" | "runner_down" | "check_end" | "job_end"));
    shared.lock().await.project_event(v).await;
    if !refresh {
        return;
    }
    let project = shared.lock().await.project.clone();
    if let Some(domain) = match project {
        Some(p) => p.read_waits().await,
        None => None,
    } {
        shared.lock().await.refresh_waits(domain).await;
    }
}

/// Count each agent's tokens from its sessions' transcripts (transcript.rs), reading on every few seconds, off the lock
/// (the first round reads every transcript whole), and keep the requests read (`Core::add_sent`). An agent's view changes when its counts do, at most once a round, so
/// a busy session does not flood the stream.
/// The proxy's models and their families for the norms (`Core::set_models`), read off the lock once a minute.
fn read_models(shared: Shared) {
    tokio::spawn(async move {
        loop {
            let coord = shared.lock().await.coord.clone();
            let read = tokio::task::spawn_blocking(move || policy::facts::models(&policy::facts::Proxy::of(&coord))).await;
            if let Ok(Ok(facts)) = read {
                let families = facts
                    .into_iter()
                    .filter(|(f, _)| f == "family")
                    .filter_map(|(_, a)| Some((a.first()?.as_str()?.to_string(), a.get(1)?.as_str()?.to_string())))
                    .collect();
                shared.lock().await.set_models(families);
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

fn count_usage(shared: Shared) {
    tokio::spawn(async move {
        let mut files: HashMap<String, TranscriptUsage> = HashMap::new();
        loop {
            let (sessions, running) = {
                let core = shared.lock().await;
                (core.sessions(), core.running_names())
            };
            let round = tokio::task::spawn_blocking(move || {
                let cgroups: Vec<(String, (Option<u64>, Option<f64>))> = running.into_iter().map(|n| { let u = cgroup::usage(&n); (n, u) }).collect();
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
                (files, counted, cgroups)
            });
            let Ok((f, counted, cgroups)) = round.await else { return };
            files = f;
            let mut core = shared.lock().await;
            for (name, (mem, cpu)) in cgroups {
                core.set_cgroup(&name, mem, cpu);
            }
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
                    let ended = s.lock().await.end_turn(&n);
                    if let Some((turn, bg, snap)) = ended {
                        turn_waits(s, n, turn, bg, snap).await;
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
    if let Request::Busy { name, busy } = &req {
        // a terminal holder's (or anyone's but a project source's): a Claude Code agent's state is what its screen shows
        if *caller == Caller::Project {
            return err("coordd: a project source does not report screens".into());
        }
        shared.lock().await.busy(name, *busy);
        return ok(String::new());
    }
    if let Request::Popup { name, title, screen } = &req {
        // a terminal holder's report (or anyone's but a project source's): told to the owner once per title
        if *caller == Caller::Project {
            return err("coordd: a project source does not report dialogs".into());
        }
        shared.lock().await.popup(name, title, screen);
        return ok(String::new());
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
        Request::Msg { to, body, esc, soon, as_host, re, quotes } => core.msg(&w, as_host, to, &body, esc, soon, re, quotes).await,
        Request::Ask { body, on } => core.ask(&w, &body, on).await,
        Request::Drop { id, why } => core.drop_question(&w, &id, why),
        Request::Unfile { id } => core.unfile(&w, &id),
        Request::Release { id } => core.release(&w, &id).await,
        Request::Nets | Request::NetTry { .. } | Request::NetSave { .. } | Request::NetReplay { .. } | Request::NetMark { .. } | Request::NetGraph { .. } => unreachable!("handled before the lock"),
        Request::Unsend { id } => core.unsend(&w, &id),
        Request::Now { id, soon } => core.send_now(&w, &id, soon).await,
        Request::Questions => core.questions(&w),
        Request::Mail { id } => core.mail_list(&w, id.as_deref()),
        Request::State => core.state_lines(),
        Request::Log { name, last } => core.log(&w, &name, last),
        Request::Item { action, arg, why } => core.item(&w, &action, &arg, why),
        Request::Add { name, kind, model } => core.add(&w, &name, &kind, &model),
        Request::Rename { name, to } => core.rename(&w, &name, &to),
        Request::Role { name, role } => core.set_role(&w, &name, &role),
        Request::TeamAsk { text } => core.team_ask(&w, &text).await,
        Request::TeamGrant { name, granted } => core.team_grant(&w, &name, granted).await,
        Request::Norm { act, args, thread } => core.norm(&act, &args, &thread),
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
        Request::Run { .. } | Request::Hook { .. } | Request::Watch { .. } | Request::Pty | Request::Popup { .. } | Request::Busy { .. } | Request::Older { .. } => unreachable!(),
    }
}

/// A turn ended with no mail held: the agent waits (background work, the project's waits) or is idle, and then its
/// parent is told.
/// What agent NAME waits on after its turn TURN ended with background work BG: the project's waits too, then its
/// state, and the turn end VALUE to the nets with all it waits on (so the clerk takes the waits as facts).
/// The `turn_end` event's value from SNAP: the turn's text read off the lock, then the value under it.
async fn turn_value(shared: &Shared, snap: Option<core::TurnSnap>) -> Option<serde_json::Value> {
    let snap = snap?;
    let (snap, text) = tokio::task::spawn_blocking(move || {
        let text = snap.text();
        (snap, text)
    })
    .await
    .ok()?;
    shared.lock().await.turn_end_value(snap, text)
}


async fn turn_waits(shared: Shared, name: String, turn: u64, bg: Vec<String>, snap: Option<core::TurnSnap>) {
    let value = turn_value(&shared, snap).await;
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

/// Follow the Claude Code agents' transcripts (`follow.rs`), every second and at once when a hook or new mail pokes:
/// the screen (its holder's `Request::Busy`), else the transcript, decides whether a turn runs; the transcript which tool calls run, a turn end found is taken once
/// (`Core::end_turn`, then `turn_waits`), and held mail is handed to every agent not in a turn (design
/// coordd-core-2026-11-10 §3–§4). Files are read off the lock; only new bytes after the first read.
fn follow(shared: Shared) {
    tokio::spawn(async move {
        let poke = shared.lock().await.poke.clone();
        let mut followers: HashMap<String, follow::Follower> = HashMap::new();
        loop {
            let paths: Vec<(String, String)> = {
                let core = shared.lock().await;
                core.agents.values().filter(|a| a.running() && a.kind == "claude").filter_map(|a| Some((a.name.clone(), a.transcript()?))).collect()
            };
            followers.retain(|n, _| paths.iter().any(|(p, _)| p == n));
            let mut fs = std::mem::take(&mut followers);
            let Ok((fs, reads)) = tokio::task::spawn_blocking(move || {
                let mut reads = Vec::new();
                for (name, path) in paths {
                    let f = fs.entry(name.clone()).or_insert_with(|| follow::Follower::new(&path));
                    if f.path != path {
                        *f = follow::Follower::new(&path); // a new session, or its file found elsewhere
                    }
                    let r = f.read();
                    let tools = r.tools.then(|| f.tools.values().cloned().collect::<Vec<_>>());
                    reads.push((name, r.seen, f.ended, f.running_since.clone(), tools, f.model.clone()));
                }
                (fs, reads)
            })
            .await
            else {
                return;
            };
            followers = fs;
            let mut ends = Vec::new();
            {
                let mut core = shared.lock().await;
                for (name, seen, ended, since, tools, model) in reads {
                    if !core.agents.get(&name).is_some_and(|a| a.running()) {
                        continue;
                    }
                    // a pick made in this process only: one from before (a fork's copied history, a window read again
                    // after coordd restarts) does not undo a model set since
                    if let Some((m, _)) = model.filter(|(_, at)| at.get(..19) >= core.agents[&name].live.started.get(..19)) {
                        core.set_model(&name, &m);
                    }
                    // the screen says whether a turn runs, when its holder tells it (`Core::busy`); else the transcript.
                    // A turn the transcript began before this process started does not run in it: its entry (a prompt,
                    // a background task's notice) was left unanswered when the old process went (10 Nov: MEANING
                    // shown working for hours on a notice from 06:39, restarted 08:46, after coordd restarts forgot
                    // its screen)
                    let a = &core.agents[&name];
                    let stale = !ended && !since.is_empty() && since.get(..19) < a.live.started.get(..19);
                    let ended = a.live.screen_busy.map_or(ended || stale, |b| !b);
                    if !seen.is_empty() {
                        core.seen(&name, &seen);
                    }
                    let working = core.agents[&name].live.state == "working";
                    if !ended && !working {
                        core.begin_turn(&name);
                    } else if ended && working {
                        if let Some(e) = core.end_turn(&name) {
                            ends.push((name.clone(), e));
                        }
                    }
                    match tools {
                        Some(t) => core.set_tools(&name, t),
                        None if !ended && !working => core.changed(&name),
                        None => {}
                    }
                }
                for name in core.to_hand_over() {
                    core.hand_over(&name);
                }
            }
            for (name, (turn, bg, snap)) in ends {
                tokio::spawn(turn_waits(shared.clone(), name, turn, bg, snap));
            }
            tokio::select! {
                _ = poke.notified() => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            }
        }
    });
}

/// The agents' senders: each agent's jobs (`core::Job`) in order on a task of its own, off the core's lock, their
/// outcome told back (`Core::handed`).
fn senders(shared: Shared) -> tokio::sync::mpsc::UnboundedSender<Job> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    tokio::spawn(async move {
        let mut per: HashMap<String, tokio::sync::mpsc::UnboundedSender<Job>> = HashMap::new();
        while let Some(job) = rx.recv().await {
            let w = per.entry(job.name().to_string()).or_insert_with(|| {
                let (wtx, mut wrx) = tokio::sync::mpsc::unbounded_channel::<Job>();
                let s = shared.clone();
                tokio::spawn(async move {
                    while let Some(j) = wrx.recv().await {
                        run_job(&s, j).await;
                    }
                });
                wtx
            });
            let _ = w.send(job);
        }
    });
    tx
}

async fn run_job(shared: &Shared, job: Job) {
    match job {
        Job::Channel { name, ids, sock, line } => {
            let talk = async {
                use tokio::io::AsyncReadExt;
                let mut c = UnixStream::connect(&sock).await.ok()?;
                c.write_all(line.as_bytes()).await.ok()?;
                let mut buf = [0u8; 16];
                let n = c.read(&mut buf).await.ok()?;
                buf[..n].starts_with(b"ok").then_some(())
            };
            let took = tokio::time::timeout(std::time::Duration::from_secs(10), talk).await.ok().flatten().is_some();
            shared.lock().await.handed(&name, &ids, took, "channel");
        }
        Job::Codex { name, ids, base, session, socket, text } => {
            let took = core::codex_rpc(&base, &session, &socket, &["send", "later", &text]).await;
            shared.lock().await.handed(&name, &ids, took, "codex");
        }
        Job::Interrupt { name, pty, codex } => {
            match codex {
                Some((base, session, socket)) => {
                    core::codex_rpc(&base, &session, &socket, &["interrupt"]).await;
                }
                None => {
                    if let Ok(mut c) = UnixStream::connect(&pty).await {
                        let _ = c.write_all(&coord_types::pty::frame(coord_types::pty::KEYS, b"\x1b")).await;
                    }
                }
            }
            shared.lock().await.interrupted(&name);
        }
    }
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

/// The question recogniser (vault designs/decisions-policies-forks-2026-11-08.md §8): each new thread an agent starts
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
