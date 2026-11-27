//! docket-core4 (paper-research N-101 p1575): where a coordd's time and memory go on a record, piece by piece, as
//! coordd does each: the load, the `violated` question at the record's length, the minute clocks (`expired`, the
//! `when` bindings), and what an input's effects do under the lock (the ticket's facts, its view, its tree).
//! `COST_COORD=DIR COST_BASE=DIR [COST_ASK=LINE] cargo test -p coordd a_coords_costs -- --ignored --nocapture`, DIR a
//! copy holding the tickets log, the registry and the project file; it writes in it only what the rounds put to the
//! machine (the delivery events they raise, and their runs).
//! COST_ASK: the record's line of a `decided violated` to ask again (default: the last one); COST_NO_ASK: none.
//! COST_FREE: the agent one `free` is timed for (default: the one with the most waiting).

use super::*;

/// This process's resident and peak resident memory, in MB.
fn rss() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb = |k: &str| s.lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1)).and_then(|n| n.parse::<u64>().ok()).unwrap_or(0) / 1024;
    (kb("VmRSS:"), kb("VmHWM:"))
}

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let at = std::time::Instant::now();
    let out = f();
    let (now, peak) = rss();
    println!("cost {what:<44} {:>9.3} s   rss {now:>6} MB  peak {peak:>6} MB", at.elapsed().as_secs_f64());
    out
}

/// INPUT put to CORE as `ticket_input` puts it, each part timed (consult-3 N-1397.p13: what a `free` run spends its
/// time on): the readers, the machine, its lines as JSON, their write, the effects. Left out, as nothing here needs
/// them: the layers' completions of an answer, the joint, and the rounds for the owner's cleaned words.
fn input_in_parts(core: &mut Core, what: &str, input: docket_machine::Input) -> Result<String, String> {
    let at = std::time::Instant::now();
    // the counted pieces (`counts::timed_sync`) start again, so that what follows is this input's
    let _ = crate::counts::take_line();
    let readers = timed(&format!("{what}: the readers"), || core.reply_readers());
    core.tickets.m.readers = Some(readers);
    let out = timed(&format!("{what}: the machine"), || core.tickets.m.apply(input, crate::core::now()));
    let lines: Vec<String> = timed(&format!("{what}: its lines as JSON"), || out.lines.iter().map(|l| serde_json::to_string(l).expect("a line is JSON")).collect());
    let kinds: Vec<String> = lines.iter().map(|l| format!("{} {} B", l.split('"').nth(3).unwrap_or("?"), l.len())).collect();
    println!("cost {what}: {} lines ({}), {} effects", lines.len(), kinds.join(", "), out.effects.len());
    timed(&format!("{what}: the lines written"), || crate::jsonl::record(&core.tickets.log, &lines)).expect("written");
    out.lines.iter().for_each(|l| core.tickets.quiet.take(l));
    // each effect alone, by its kind (a changed ticket by its id and posts: its view is built again)
    let all = std::time::Instant::now();
    let mut by: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for e in out.effects {
        let kind = match &e {
            docket_machine::Effect::Changed { t } => format!("Changed {} ({} posts)", core.tickets.m.id(*t), core.tickets.m.tickets.get(t).map_or(0, |x| x.posts.len())),
            e => {
                let d = format!("{e:?}");
                let head: String = d.chars().take_while(|c| c.is_alphanumeric()).collect();
                let q = d.split("question: ").nth(1).map(|q| q.chars().take_while(|c| c.is_alphanumeric()).collect::<String>());
                q.map_or(head.clone(), |q| format!("{head} {q}"))
            }
        };
        let at = std::time::Instant::now();
        core.ticket_effects(vec![e]);
        let x = by.entry(kind).or_default();
        x.0 += 1;
        x.1 += at.elapsed().as_secs_f64();
    }
    let mut by: Vec<(String, (usize, f64))> = by.into_iter().collect();
    by.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1));
    println!("cost {what}: its effects {:.3} s: {}", all.elapsed().as_secs_f64(), by.iter().map(|(k, (n, t))| format!("{k} x{n} {t:.3} s")).collect::<Vec<_>>().join(", "));
    timed(&format!("{what}: end_asks"), || core.end_asks());
    println!("cost {what}, whole: {:.3} s", at.elapsed().as_secs_f64());
    if let Some(line) = crate::counts::take_line() {
        println!("cost {what}, its counted pieces over 100 ms: {line}");
    }
    out.reply
}

#[test]
#[ignore]
fn a_coords_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let log = format!("{coord}/tickets/log.jsonl");
    let text = std::fs::read_to_string(&log).unwrap();
    println!("cost record {} lines, {} MB", text.lines().count(), text.len() / 1_000_000);
    let mut core = timed("Core::load (registry, mail, layers)", || Core::load(&coord, &base));
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    // the load as coordd's start does it, nothing written: Tickets::load replays every line
    core.tickets = timed("Tickets::load (replay of the record)", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let makes = core.tickets.errors.iter().filter(|e| e.contains("now makes")).count();
    let refused = core.tickets.errors.iter().filter(|e| e.contains("now refused")).count();
    println!("cost load: {} tickets, {} problem lines ({makes} now makes, {refused} now refused)", core.tickets.m.tickets.len(), core.tickets.errors.len());

    // the minute clocks, as `expiry` and `clocks` run them whether or not anything happened
    let owed = timed("m.awaiting", || core.tickets.m.awaiting());
    let now = chrono::Utc::now();
    let owed: Vec<(u64, u32, String, String, i64)> = owed.into_iter().map(|(t, s, a, since)| (t, s, a.clone(), core.tickets.m.id(t), core.silent_minutes(t, &a, &since, now))).collect();
    println!("cost owed steps: {}", owed.len());
    let facts = timed("norm_facts", || core.norm_facts(&[]));
    let places = core.places();
    // a policy question's two parts, as `may`, `fills`, `holds` and `expired` each do them afresh
    let norms = timed("norms::load (sources read, compiled to clingo)", || crate::policy::norms::load(&places).unwrap());
    for _ in 0..2 {
        let _ = timed("norms.solve of the team's facts", || norms.solve(&facts).map(|s| s.len()));
    }
    // a2h-speed: the same questions with only the acts they read grounded, and each act's atoms the same
    for acts in [&["expired"][..], &["post", "notify", "inform"], &["notify", "inform"], &["make-agent"]] {
        let alone = timed(&format!("norms.solve_for {acts:?}"), || norms.solve_for(acts, &facts).unwrap());
        let whole = norms.solve(&facts).unwrap();
        for a in acts {
            let p = crate::policy::compile::pred(a);
            let of = |s: &std::collections::HashSet<String>| s.iter().filter(|x| crate::policy::norms::about(x) == Some(p.as_str())).cloned().collect::<BTreeSet<_>>();
            assert_eq!(of(&alone), of(&whole), "act {a} on the record's facts");
        }
    }
    // a2h-speed: the owed steps as if each had been silent long past silent-after, so non-owner steps expire: the
    // expired question asked alone answers as the whole program does
    let mut long = facts.clone();
    for (_, _, agent, id, _) in &owed {
        if !long.iter().any(|(f, v)| f == "agent" && v[0] == *agent) {
            long.push(("agent".into(), vec![agent.clone()]));
        }
        long.push(("ticket".into(), vec![id.clone()]));
        long.push(("awaits".into(), vec![id.clone(), agent.clone()]));
        long.push(("waited".into(), vec![id.clone(), "100000".into()]));
    }
    let p = crate::policy::compile::pred("expired");
    let of = |s: &std::collections::HashSet<String>| s.iter().filter(|x| crate::policy::norms::about(x) == Some(p.as_str())).cloned().collect::<BTreeSet<_>>();
    let (alone, whole) = (of(&norms.solve_for(&["expired"], &long).unwrap()), of(&norms.solve(&long).unwrap()));
    println!("cost expired long past silent-after: {} atoms, {} enabled", alone.len(), alone.iter().filter(|a| a.starts_with("in((enabled,")).count());
    assert_eq!(alone, whole, "expired on the owed steps long silent");
    let e = timed("expired (norms load and solve, each minute)", || expired(&places, facts, &owed));
    println!("cost expired: {:?}", e.map(|v| v.len()));
    timed("clocks_due + when_asks (each minute)", || (core.tickets.m.clocks_due(&crate::core::now()), core.tickets.m.when_asks(&crate::core::now())).1.len());
    // 0.2 head 2 (consult-3, N-1397.p11, 2): one `free` run under the lock, for COST_FREE (default: the agent with
    // the most waiting): the event (the machine reads what waits once, when the run begins), with the `deliver` it runs;
    // on a record whose run is on the text before, its `next` answered with no call (I-1480, `next_given`) and the
    // `deliver` that runs
    let most = match std::env::var("COST_FREE") {
        Ok(a) => Some((core.tickets.m.waiting(&a).len(), a)),
        Err(_) => core.agents.keys().map(|a| (core.tickets.m.waiting(a).len(), a.clone())).max(),
    };
    if let Some((n, who)) = most.filter(|(n, _)| *n > 0) {
        // what coordd's start does after the replay (ticket::start): the questions in flight asked again, so that what
        // the event costs is its own
        timed("the start's resume effects", || {
            let resume = core.tickets.m.resume_effects();
            core.ticket_effects(resume)
        });
        timed(&format!("waiting of {who}, {n} items"), || core.tickets.m.waiting(&who).len());
        // two inputs that decide nothing (an `arrived` for nothing waiting): the first does what the record's last
        // input did live and its replay does not (the holes it made quiet asked); the second is what any input costs
        for k in ["first", "second"] {
            let none = docket_machine::Input::Event { name: docket_machine::syntax::ARRIVED.into(), value: serde_json::json!({"agent": who, "item": "N-0.p0", "above": []}) };
            let r = input_in_parts(&mut core, &format!("an input deciding nothing, the {k}"), none);
            println!("cost an input deciding nothing: {r:?}");
        }
        let above: Vec<String> = core.agents.keys().filter(|a| core.above(&Who::Agent((*a).clone()), &who)).cloned().collect();
        let free = docket_machine::Input::Event { name: docket_machine::syntax::FREE.into(), value: serde_json::json!({"agent": who, "doing": "", "writing": false, "above": above}) };
        let r = input_in_parts(&mut core, &format!("free for {who} with {n} waiting"), free).map(|_| ());
        println!("cost free for {who}: {r:?}");
        if let Some(d) = core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| core.tickets.m.run_of(d.t).as_deref() == Some(who.as_str())) {
            timed("  its next, answered with no call, the deliver", || core.next_given(&d));
        }
        println!("cost free for {who}: {} items handed", core.tickets.m.hands(&who).iter().map(|h| h.items.len()).sum::<usize>());
    }
    // a2h-speed (paper-research N-101 p1781): the handing round main.rs runs every second, after the transcripts
    let names: Vec<String> = core.agents.keys().cloned().collect();
    println!("cost agents registered: {}", names.len());
    for _ in 0..2 {
        timed("ticket_due of every agent (each second)", || names.iter().map(|n| core.ticket_due(n).len()).sum::<usize>());
        timed("ticket_due(minder) (each second)", || core.ticket_due(crate::shape::MINDER).len());
        timed("to_hand_over (each second)", || core.to_hand_over().len());
        timed("to_move (each second)", || core.to_move().len());
        timed("raise_delivery (each second)", || core.raise_delivery());
        timed("not_read (each second)", || core.not_read());
    }
    // the same due steps read in one pass for every agent, and the names any ticket has anything due for
    let mut every: BTreeSet<String> = names.iter().cloned().collect();
    for x in core.tickets.m.tickets.values() {
        if let docket_machine::Status::Awaits { agent, .. } = &x.status {
            every.insert(agent.clone());
        }
        every.extend(x.opener.iter().cloned());
        every.extend(x.posts.iter().flat_map(|p| p.names.iter().chain(&p.told).chain(&p.inform).cloned()));
    }
    let every: Vec<String> = every.into_iter().collect();
    let one = timed(&format!("due_all of {} names, one pass", every.len()), || core.tickets.due_all(&every));
    let each: Vec<Vec<docket_machine::Delivery>> = timed(&format!("due of {} names, one by one", every.len()), || every.iter().map(|a| core.tickets.due(a)).collect());
    let ids = |v: &Vec<docket_machine::Delivery>| v.iter().map(|d| serde_json::to_string(d).unwrap()).collect::<Vec<_>>();
    let mut owed = 0;
    for (a, d) in every.iter().zip(&each) {
        assert_eq!(ids(&one[a]), ids(d), "the due of {a}");
        owed += d.len();
    }
    println!("cost due_all equals due for each of {} names ({owed} deliveries)", every.len());
    timed("to_hand_over with due_all (each second)", || core.to_hand_over().len());
    timed("raise_delivery with due_all (each second)", || core.raise_delivery());

    // a2h-speed (paper-research N-101 p1813): what each piece holds at its peak, as resident and peak after it
    let ts: Vec<u64> = core.tickets.m.tickets.keys().copied().collect();
    let views = timed(&format!("ticket_shown of all {} tickets, held", ts.len()), || ts.iter().filter_map(|t| core.ticket_shown(*t)).collect::<Vec<_>>());
    let bytes: usize = views.iter().map(|v| serde_json::to_string(v).map(|s| s.len()).unwrap_or(0)).sum();
    println!("cost views of all tickets: {} MB as JSON", bytes / 1_000_000);
    let bodies = timed("ticket_body of each (posts hashed), held", || views.into_iter().map(|v| core.ticket_body(v)).collect::<Vec<_>>());
    drop(bodies);
    timed("after the views dropped", || ());
    // as a live coordd emits them, with a watcher subscribed: the ring keeps the latest of each ticket and the
    // broadcast channel the last 4096 events, whether the watcher has read them or not
    let rx = core.tx.subscribe();
    core.sent_posts.clear();
    timed(&format!("emit of all {} tickets whole, a watcher subscribed", ts.len()), || {
        for t in &ts {
            if let Some(v) = core.ticket_shown(*t) {
                let b = core.ticket_body(v);
                core.emit(b);
            }
        }
    });
    let ring: usize = core.ring.iter().filter_map(|(_, e)| e.as_ref()).map(|e| serde_json::to_string(e).map(|s| s.len()).unwrap_or(0)).sum();
    println!("cost ring after: {} events, {} MB as JSON", core.ring.len(), ring / 1_000_000);
    drop(rx);
    let snap = timed("snapshot, held", || core.snapshot());
    let snap_bytes = serde_json::to_string(&snap).map(|s| s.len()).unwrap_or(0);
    println!("cost snapshot: {} MB as JSON", snap_bytes / 1_000_000);
    drop(snap);
    timed("after the snapshot dropped", || ());

    // COST_NO_ASK: the cheap rows only
    if std::env::var("COST_NO_ASK").is_ok() {
        return;
    }
    // the ask to time: a `decided violated` line, its answering and its (t, step)
    let lines: Vec<&str> = text.lines().collect();
    let at = std::env::var("COST_ASK").ok().and_then(|n| n.parse::<usize>().ok()).unwrap_or_else(|| {
        lines.iter().rposition(|l| l.contains("\"ev\":\"decided\"") && l.contains("\"question\":\"violated\"")).map_or(0, |i| i + 1)
    });
    let d: serde_json::Value = serde_json::from_str(lines[at - 1]).unwrap();
    let (t, step) = (d["t"].as_u64().unwrap(), d["step"].as_u64().unwrap() as u32);
    let ans = &d["provenance"]["answering"];
    let answering = violated::Answering { agent: ans[0].as_str().unwrap().into(), call: ans[1].as_str().unwrap().into(), head: ans[2].as_str().unwrap().into() };
    println!("cost ask: line {at}, {} step {step}, {answering:?}, recorded events {}", core.tickets.m.id(t), d["provenance"]["events"]);

    // what an input's effects compute under the lock, for this ticket and its top
    let tree = timed("ticket_tree", || core.ticket_tree(t));
    let top = *tree.iter().next().unwrap();
    println!("cost tree of {}: {} tickets", core.tickets.m.id(t), tree.len());
    timed("ticket_facts (a Notify's facts)", || core.ticket_facts(Some(t)).0.len());
    timed("fills_facts", || core.fills_facts(t).len());
    timed("goal_facts", || core.goal_facts(t).len());
    for u in [t, top] {
        let v = timed(&format!("ticket_shown {}", core.tickets.m.id(u)), || core.ticket_shown(u));
        if let Some(v) = v {
            let posts = v.posts.len();
            let b = timed(&format!("ticket_body {} ({posts} posts)", core.tickets.m.id(u)), || core.ticket_body(v));
            timed(&format!("  its JSON for the app {}", core.tickets.m.id(u)), || serde_json::to_string(&b).map(|s| s.len()).unwrap_or(0));
        }
    }
    timed("Calls::load (each forming call)", || crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).is_ok());

    // the question, as run_task does it, then in its parts
    let policy = core.tickets.duties.clone().expect("the project declares duties");
    let repos = core.project.as_ref().map(|p| {
        let mut v: Vec<String> = p.repos().into_iter().map(|(_, r)| r).collect();
        v.extend(p.repo().filter(|r| !v.contains(r)));
        v
    }).unwrap_or_default();
    let record = timed("record_to_answer (read and parse the log)", || violated::record_to_answer(&log, t, step, &answering.agent));
    let p = timed("eflint Replay::load", || eflint::replay::Replay::load(std::path::Path::new(&policy)).unwrap());
    let opts = eflint::record::Options { runner: "runner".into(), lead: None, tickets: tree.iter().map(|t| *t as i64).collect(), repos: repos.clone(), ..Default::default() };
    let converted = timed("eflint record::convert", || eflint::record::convert(&record, "", &opts).unwrap());
    println!("cost events {}", converted.events.len());
    let mut p = p;
    let n = converted.events.len();
    let mut last = std::time::Instant::now();
    let start = last;
    for (i, e) in converted.events.iter().enumerate() {
        let a = p.apply(&eflint::replay::Event::from_json(e).unwrap());
        let _ = a;
        if (i + 1) % (n / 8).max(1) == 0 || i + 1 == n {
            println!("cost   events to {:>5}: {:>8.3} s for the last {}, state {} instances", i + 1, last.elapsed().as_secs_f64(), (n / 8).max(1), p.cfg.state.contents().len());
            last = std::time::Instant::now();
        }
    }
    println!("cost eflint apply of all events {:>9.3} s", start.elapsed().as_secs_f64());
    timed("  one duties() at the end", || p.duties().len());
    timed("  one enabled() at the end", || p.enabled().len());
    timed("  one state clone at the end", || p.cfg.state.clone().contents().len());
    for (ty, rounds, ms) in eflint::eval::derivation_profile().into_iter().take(8) {
        println!("cost   derived {ty:<28} {rounds:>8} rounds {ms:>10.0} ms");
    }
    let whole = timed("violated() whole, as coordd asks it", || violated::violated(&policy, &record, &tree, &repos, &answering).map(|(d, _)| d.len()));
    println!("cost answer: {whole:?}");
}

/// Two `violated` asks at once, each on a thread of its own as coordd's blocking pool runs them: the resident size
/// before, at the peak and after both have ended (what the process keeps of them). Run it once as it is and once with
/// `MALLOC_ARENA_MAX=2` to see whether glibc's per-thread arenas hold the memory (docket-core4, N-101 p1662, p1664).
/// `COST_COORD=DIR COST_BASE=DIR [COST_ASK=LINE] cargo test -p coordd two_asks_at_once -- --ignored --nocapture`.
#[test]
#[ignore]
fn two_asks_at_once() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let log = format!("{coord}/tickets/log.jsonl");
    let text = std::fs::read_to_string(&log).unwrap();
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = Tickets::load(&coord, &base, project.as_deref(), 1);
    let lines: Vec<&str> = text.lines().collect();
    let at = std::env::var("COST_ASK").ok().and_then(|n| n.parse::<usize>().ok()).unwrap_or_else(|| {
        lines.iter().rposition(|l| l.contains("\"ev\":\"decided\"") && l.contains("\"question\":\"violated\"")).map_or(0, |i| i + 1)
    });
    let d: serde_json::Value = serde_json::from_str(lines[at - 1]).unwrap();
    let (t, step) = (d["t"].as_u64().unwrap(), d["step"].as_u64().unwrap() as u32);
    let ans = &d["provenance"]["answering"];
    let answering = violated::Answering { agent: ans[0].as_str().unwrap().into(), call: ans[1].as_str().unwrap().into(), head: ans[2].as_str().unwrap().into() };
    let tree = core.ticket_tree(t);
    let policy = core.tickets.duties.clone().expect("the project declares duties");
    let repos = core.project.as_ref().map(|p| {
        let mut v: Vec<String> = p.repos().into_iter().map(|(_, r)| r).collect();
        v.extend(p.repo().filter(|r| !v.contains(r)));
        v
    }).unwrap_or_default();
    drop(text);
    println!("cost two asks: {} step {step}, MALLOC_ARENA_MAX={:?}", core.tickets.m.id(t), std::env::var("MALLOC_ARENA_MAX").ok());
    timed("before the asks", || ());
    let started = std::time::Instant::now();
    let asks: Vec<_> = (0..2).map(|_| {
        let (policy, log, tree, repos, answering) = (policy.clone(), log.clone(), tree.clone(), repos.clone(), answering.clone());
        std::thread::spawn(move || violated::violated(&policy, &violated::record_to_answer(&log, t, step, &answering.agent), &tree, &repos, &answering).map(|(d, _)| d.len()))
    }).collect();
    let answers: Vec<_> = asks.into_iter().map(|h| h.join().unwrap()).collect();
    println!("cost two asks answered {answers:?} in {:.1} s", started.elapsed().as_secs_f64());
    timed("after both asks ended (kept)", || ());
}

/// The real case of an ask that outlived its ticket (Tovrin-dev, 17 Nov): A-1099511628020's answer was asked
/// `violated` at 08:22:28, and the ticket was cancelled at 08:52:28 when P-1729's step 8 was decided ("the block it
/// watched ended"); its ask ran on and its answer was refused, with nothing in the record. On a copy cut just before
/// that decision: the ask is started, the decision put in, and the ask gives up and is recorded as ended unanswered.
/// `ENDS_COORD=DIR COST_BASE=DIR cargo test -p coordd an_ask_ends_with_its_ticket -- --ignored --nocapture`, DIR a copy
/// of the coord whose log ends at line 27750, with `decided-1729-8.json` the record's line 27751. It writes in DIR.
#[test]
#[ignore]
fn an_ask_ends_with_its_ticket() {
    let (Ok(coord), Ok(base)) = (std::env::var("ENDS_COORD"), std::env::var("COST_BASE")) else { return };
    let act: u64 = 1099511628020;
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let resume = core.tickets.m.resume_effects();
    core.ticket_effects(resume);
    assert!(core.tickets.m.asks_violated(act, 1), "A-{act} asks at the cut");
    let tasks: Vec<Task> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let task = tasks.into_iter().find(|t| matches!(t, Task::Violated { t, .. } if *t == act)).expect("its ask is started again");
    let engine = crate::policy::Engine::start(core.places());
    let started = std::time::Instant::now();
    let running = std::thread::spawn(move || run_task(&engine, task));
    std::thread::sleep(std::time::Duration::from_millis(500));
    // P-1729's step 8 decided, as the record has it: the block A-…020 watched ends, and the ticket with it
    let d: Value = serde_json::from_str(&std::fs::read_to_string(format!("{coord}/decided-1729-8.json")).unwrap()).unwrap();
    let decided = Input::Decided { t: 1729, step: 8, question: "violated".into(), answer: d["answer"].clone(), provenance: d["provenance"].clone() };
    core.ticket_input(decided).unwrap();
    let ended_at = std::time::Instant::now();
    assert!(!core.tickets.m.asks_violated(act, 1));
    assert!(!core.tickets.m.tickets[&act].is_open(), "{:?}", core.tickets.m.tickets[&act].status);
    let inputs = running.join().unwrap();
    println!("ends: the ask gave up {:.2} s after its ticket closed ({:.2} s after it started)", ended_at.elapsed().as_secs_f64(), started.elapsed().as_secs_f64());
    let [Input::Decided { t, step: 1, question, answer, provenance }] = &inputs[..] else { panic!("{inputs:?}") };
    assert_eq!((*t, question.as_str()), (act, "violated"));
    assert!(answer.is_null(), "{answer}");
    println!("ends: provenance {provenance}");
    assert!(provenance["ended"].as_str().is_some_and(|e| e.starts_with("the ticket no longer asks it")), "{provenance}");
    let said = core.ticket_input(inputs.into_iter().next().unwrap()).unwrap();
    println!("ends: {said}");
    // the record says so: its last line is the ask's end, unanswered
    let log = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
    let last: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!((last["ev"].as_str(), last["t"].as_u64(), last["answer"].is_null()), (Some("decided"), Some(act), true), "{last}");
    println!("ends: line {}", log.lines().last().unwrap());
    assert!(core.tickets.asks.is_empty() || !core.tickets.asks.contains_key(&(act, 1, "violated")));
}

/// The large anonymous regions of this process (glibc's thread arenas are 64 MB each): how many, and their resident MB.
fn arenas() -> (usize, u64) {
    let s = std::fs::read_to_string("/proc/self/smaps").unwrap_or_default();
    let (mut n, mut rss, mut size, mut anon) = (0usize, 0u64, 0u64, false);
    for l in s.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.first().is_some_and(|a| a.contains('-') && a.chars().next().is_some_and(|c| c.is_ascii_hexdigit())) && f.len() >= 5 {
            anon = f.len() == 5;
        } else if f.first() == Some(&"Size:") {
            size = f[1].parse().unwrap_or(0);
        } else if f.first() == Some(&"Rss:") && anon && size >= 30 * 1024 {
            n += 1;
            rss += f[1].parse::<u64>().unwrap_or(0) / 1024;
        }
    }
    (n, rss)
}

/// Where Tovrin-dev's coordd's 2.3 GB stand (a2h-speed's reading, N-406 p4: ~25 regions of 40–64 MB beside a 297 MB
/// heap): the allocator's share, measured as coordd spreads its work. On a tokio runtime of 16 workers, COST_ROUNDS
/// rounds (default 4), each: COST_ASKS `violated` asks at once by spawn_blocking (default 8, the ask of the record's last
/// decided `violated`), and every ticket's view and body made on the workers under the core's lock. Resident, peak
/// and the large regions after each round; with COST_TRIM set, `malloc_trim(0)` after each round. Run it with and
/// without MALLOC_ARENA_MAX. `COST_COORD=DIR COST_BASE=DIR cargo test -p coordd asks_and_views_on_workers -- --ignored --nocapture`.
#[test]
#[ignore]
fn asks_and_views_on_workers() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let num = |k: &str, d: usize| std::env::var(k).ok().and_then(|n| n.parse().ok()).unwrap_or(d);
    let (rounds, nasks, trim) = (num("COST_ROUNDS", 4), num("COST_ASKS", 8), std::env::var_os("COST_TRIM").is_some());
    // COST_GIVE_BACK=each|last: each ask runs as coordd's run_task runs it, its memory given back after each ask or
    // once the last running ask ends (violated::Running)
    let give_back = std::env::var("COST_GIVE_BACK").ok();
    let every = give_back.as_deref() == Some("each");
    let log = format!("{coord}/tickets/log.jsonl");
    let text = std::fs::read_to_string(&log).unwrap();
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = Tickets::load(&coord, &base, project.as_deref(), 1);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().rposition(|l| l.contains("\"ev\":\"decided\"") && l.contains("\"question\":\"violated\"")).map_or(0, |i| i + 1);
    let d: serde_json::Value = serde_json::from_str(lines[at - 1]).unwrap();
    let (t, step) = (d["t"].as_u64().unwrap(), d["step"].as_u64().unwrap() as u32);
    let ans = &d["provenance"]["answering"];
    let answering = violated::Answering { agent: ans[0].as_str().unwrap().into(), call: ans[1].as_str().unwrap().into(), head: ans[2].as_str().unwrap().into() };
    let tree = core.ticket_tree(t);
    let policy = core.tickets.duties.clone().expect("the project declares duties");
    let repos = core.project.as_ref().map(|p| {
        let mut v: Vec<String> = p.repos().into_iter().map(|(_, r)| r).collect();
        v.extend(p.repo().filter(|r| !v.contains(r)));
        v
    }).unwrap_or_default();
    drop(lines);
    drop(text);
    let ids: Vec<u64> = core.tickets.m.tickets.keys().copied().collect();
    println!("cost workers: {} step {step}, {rounds} rounds of {nasks} asks and {} views, MALLOC_ARENA_MAX={:?}, trim {trim}, given back {give_back:?}", core.tickets.m.id(t), ids.len(), std::env::var("MALLOC_ARENA_MAX").ok());
    let report = |what: &str| {
        let (n, r) = arenas();
        let (now, peak) = rss();
        println!("cost {what:<28} rss {now:>6} MB  peak {peak:>6} MB  large regions {n:>3}, {r:>6} MB resident");
    };
    report("loaded");
    let core = std::sync::Arc::new(tokio::sync::Mutex::new(core));
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(16).enable_all().build().unwrap();
    for round in 1..=rounds {
        let started = std::time::Instant::now();
        rt.block_on(async {
            let asks: Vec<_> = (0..nasks).map(|_| {
                let (policy, log, tree, repos, answering) = (policy.clone(), log.clone(), tree.clone(), repos.clone(), answering.clone());
                let give_back = give_back.clone();
                tokio::task::spawn_blocking(move || {
                    let _running = give_back.map(|_| violated::Running::start(every));
                    violated::violated(&policy, &violated::record_to_answer(&log, t, step, &answering.agent), &tree, &repos, &answering).map(|(d, _)| d.len())
                })
            }).collect();
            let views: Vec<_> = ids.chunks(ids.len().div_ceil(16).max(1)).map(|part| {
                let (core, part) = (core.clone(), part.to_vec());
                tokio::spawn(async move {
                    let mut bytes = 0usize;
                    for u in part {
                        let mut c = core.lock().await;
                        if let Some(v) = c.ticket_shown(u) {
                            bytes += serde_json::to_string(&c.ticket_body(v)).map_or(0, |s| s.len());
                        }
                    }
                    bytes
                })
            }).collect();
            for a in asks {
                a.await.unwrap().unwrap();
            }
            let mut bytes = 0;
            for v in views {
                bytes += v.await.unwrap();
            }
            println!("cost round {round}: {:.1} s, {} MB of views", started.elapsed().as_secs_f64(), bytes >> 20);
        });
        if trim {
            unsafe { libc::malloc_trim(0) };
        }
        report(&format!("after round {round}"));
    }
}

/// a2h-speed (paper-research N-459): what `coord outline` and `coord tie` wait behind on a record, piece by piece:
/// the outline itself, the tie's input, and the waits job each ticket change queues (`Job::Waits`, `refresh_waits`:
/// every idle or waiting agent's `ticket_waits` twice, each over every shown ticket's view), all under the core lock.
/// `COST_COORD=DIR COST_BASE=DIR [COST_AGENTS=9] cargo test -p coordd waits_and_outline_costs -- --ignored --nocapture`
#[test]
#[ignore]
fn waits_and_outline_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = timed("Core::load", || Core::load(&coord, &base));
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let open = core.tickets.m.tickets.values().filter(|x| x.is_open()).count();
    println!("cost tickets {} ({open} open), agents {}", core.tickets.m.tickets.len(), core.agents.len());
    for _ in 0..2 {
        let r = timed("ticket_outline (coord outline)", || core.ticket_outline());
        assert!(r.ok, "{}", r.out);
    }
    let views = timed("ticket_views (every shown ticket's view)", || core.ticket_views());
    println!("cost shown views {}", views.len());
    let n: usize = std::env::var("COST_AGENTS").ok().and_then(|n| n.parse().ok()).unwrap_or(9);
    let names: Vec<String> = core.agents.keys().take(n).cloned().collect();
    // the pre-filter keeps exactly the views the old loop kept: those whose opener is the agent
    for name in core.agents.keys() {
        let all: Vec<String> = views.iter().filter(|v| v.opener.as_deref() == Some(name.as_str())).map(|v| v.id.clone()).collect();
        let some: Vec<String> = core.ticket_views_where(|x| x.opener.as_deref().or(x.raised_by.as_deref()) == Some(name.as_str())).into_iter().filter(|v| v.opener.as_deref() == Some(name.as_str())).map(|v| v.id).collect();
        assert_eq!(all, some, "{name}");
    }
    // a2h-speed (paper-research N-590 p28): the waits read from the machine's views of the open tickets alone are
    // the ones the shown views of every kept ticket gave, for every agent
    let shown_waits = |name: &str| -> Vec<String> {
        use docket_machine::ViewStatus;
        let mut out = Vec::new();
        for v in core.ticket_views_where(|x| x.opener.as_deref().or(x.raised_by.as_deref()) == Some(name)) {
            if v.opener.as_deref() != Some(name) || matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. }) {
                continue;
            }
            let held = |w: &&docket_machine::WaitsOn| {
                docket_machine::parse_ticket(&w.ticket)
                    .and_then(|t| core.tickets.m.tickets.get(&t))
                    .is_some_and(|x| matches!(&x.status, docket_machine::Status::Awaits { owed, .. } if owed.hold))
            };
            let waits: Vec<&docket_machine::WaitsOn> = v.waits_on.iter().filter(|w| !held(w)).collect();
            if waits.is_empty() || waits.iter().any(|w| w.agent == name) {
                continue;
            }
            let on: Vec<String> = waits.iter().map(|w| format!("{}: {}, {}", w.agent, w.call, w.ticket)).collect();
            out.push(format!("{} ({})", v.id, on.join("; ")));
        }
        out
    };
    let all: Vec<String> = core.agents.keys().cloned().collect();
    let before = timed(&format!("waits from shown views, every agent ({})", all.len()), || all.iter().map(|n| shown_waits(n)).collect::<Vec<_>>());
    let after = timed(&format!("ticket_waits, every agent ({})", all.len()), || all.iter().map(|n| core.ticket_waits(n)).collect::<Vec<_>>());
    assert_eq!(before, after);
    println!("cost waits found {}", after.iter().map(Vec::len).sum::<usize>());
    let one = timed("ticket_waits of one agent", || core.ticket_waits(&names[0]));
    let _ = one;
    timed(&format!("refresh_waits' reads: {} agents x 2", names.len()), || {
        for name in &names {
            let _ = core.ticket_waits(name);
            let _ = core.ticket_waits(name);
        }
    });
    // a tie and its untie, as `coord tie T TO`, on the copy's record (it is a copy: the log is the copy's)
    let mut open_tops: Vec<String> = core.tickets.m.tickets.values().filter(|x| x.is_open() && x.kind == "thread").map(|x| core.tickets.m.id(x.t)).collect();
    open_tops.sort();
    if open_tops.len() >= 2 {
        let who = Who::Owner;
        let (t, to) = (open_tops[0].clone(), open_tops[1].clone());
        let (tn, ton) = (ticket_of(&core.tickets.m, &t).unwrap(), ticket_of(&core.tickets.m, &to).unwrap());
        let mut m = timed("second machine (for the parts)", || Tickets::load(&coord, &base, project.as_deref(), 1)).m;
        m.readers = Some(timed("reply_readers", || core.reply_readers()));
        let out = timed("machine apply Tie alone (the policy's check)", || m.apply(docket_machine::Input::Tie { t: tn, to: Some(ton), by: "owner".into() }, now()));
        println!("cost apply: {} lines, {} effects", out.lines.len(), out.effects.len());
        let v = timed("ticket_shown of T (the view emitted)", || core.ticket_shown(tn));
        if let Some(v) = v {
            let b = timed("ticket_body of T", || core.ticket_body(v));
            println!("cost body bytes {:?}", serde_json::to_string(&b).map(|s| s.len()).ok());
        }
        let r = timed(&format!("ticket_tie {t} under {to}"), || core.ticket_tie(&who, &t, &to));
        println!("cost tie: {:?} {:?}", r.ok, r.out.chars().take(200).collect::<String>());
        let r = timed(&format!("ticket_tie {t} untied"), || core.ticket_tie(&who, &t, "-"));
        println!("cost untie: {:?} {:?}", r.ok, r.out.chars().take(200).collect::<String>());
    }
}

/// `coord label` as the window asks it every 2 s when anything changed (yaaihh refs.rs `ids_of`): every shown
/// ticket, every agent, each done ticket's commits. Its share under the lock (`labels`) and its git off it (`subjects`).
#[test]
#[ignore]
fn label_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = timed("Core::load", || Core::load(&coord, &base));
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let views = core.ticket_views();
    let mut ids: Vec<String> = views.iter().map(|v| v.id.clone()).collect();
    let nt = ids.len();
    ids.extend(core.agents.keys().cloned());
    let na = ids.len() - nt;
    for v in &views {
        if let docket_machine::ViewStatus::Done { result } = &v.status {
            if let Some(c) = result.get("commit").and_then(|c| c.as_object()) {
                ids.extend(c.iter().filter_map(|(r, sha)| sha.as_str().map(|sha| format!("{r}:{sha}"))));
            }
        }
    }
    println!("cost label ids: {nt} tickets, {na} agents, {} commits", ids.len() - nt - na);
    for _ in 0..2 {
        let (out, commits, repos) = timed("labels (under the lock)", || core.labels(&ids));
        let subjects = timed("subjects (git, off the lock)", || crate::label::subjects(&commits, &repos));
        let bytes: usize = out.iter().map(|l| serde_json::to_string(l).unwrap_or_default().len() + 1).sum();
        println!("cost label answer {bytes} bytes, {} subjects", subjects.len());
        let all: Vec<String> = out.iter().map(|l| serde_json::to_string(l).unwrap_or_default()).collect();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&all, &mut h);
        println!("cost label answer digest {:016x}", std::hash::Hasher::finish(&h));
    }
    // the parts of `labels`: tickets alone, agents alone
    let agents: Vec<String> = core.agents.keys().cloned().collect();
    timed("labels of the tickets alone", || ids[..nt].iter().map(|id| crate::label::of(&core.tickets.m, &agents, id)).count());
    timed("labels of the agents alone", || ids[nt..nt + na].iter().map(|id| crate::label::of(&core.tickets.m, &agents, id)).count());
}

/// The window's snapshot (`watch`) on the copy: its time under the lock and its size, by part (the copy has no mail log,
/// so its mail part is empty here).
#[test]
#[ignore]
fn snapshot_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = timed("Core::load", || Core::load(&coord, &base));
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let snap = timed("snapshot (under the lock)", || core.snapshot());
    println!("cost snapshot bytes {}", serde_json::to_string(&snap).map(|s| s.len()).unwrap_or(0));
    let views = timed("its tickets part (ticket_views)", || core.ticket_views());
    println!("cost tickets part bytes {}", serde_json::to_string(&views).map(|s| s.len()).unwrap_or(0));
    let posts: usize = views.iter().map(|v| v.posts.len()).sum();
    let pbytes: usize = views.iter().map(|v| serde_json::to_string(&v.posts).map(|s| s.len()).unwrap_or(0)).sum();
    println!("cost tickets part: {} views, {posts} posts, posts {pbytes} bytes", views.len());
    // (B)'s asks on every view (proto-reader N-1479 p145, small 2): the tickets part twice more, as one sample is
    // noise on a shared machine; his posts' asks alone are timed below
    for _ in 0..2 {
        timed("its tickets part again", || core.ticket_views().len());
    }
    // 0.3's `ran` (N-1446 p45): what the handler runs under the shown tickets add, and the time their words take (the
    // walk over the tickets that finds them was there before, for `handlers`)
    let runs: usize = views.iter().map(|v| v.ran.len()).sum();
    let open: usize = views.iter().map(|v| v.handlers.len()).sum();
    let rbytes: usize = views.iter().filter(|v| !v.ran.is_empty()).map(|v| serde_json::to_string(&v.ran).map(|s| s.len() + ",\"ran\":".len()).unwrap_or(0)).sum();
    println!("cost tickets part: ran {runs} runs ({open} open, in handlers too), {rbytes} bytes");
    let m = &core.tickets.m;
    let all: Vec<&docket_machine::Ticket> = m.tickets.values().filter(|y| y.handling.is_some()).collect();
    let said = timed("said, of every handler run", || all.iter().filter_map(|y| docket_machine::view::said(m, y)).count());
    println!("cost said: {said} of {} handler runs say something", all.len());
    // (B): his posts' asks alone (`Machine::asked`, for each of his posts), the part of the tickets part the ledger adds
    let his: Vec<(u64, u32)> = m.tickets.iter().flat_map(|(t, x)| x.posts.iter().filter(|p| p.by == "owner").map(move |p| (*t, p.n))).collect();
    let asks = timed("his posts' asks alone (Machine::asked)", || his.iter().map(|(t, n)| m.asked(*t, *n).len()).sum::<usize>());
    println!("cost asks: {} of his posts, {asks} agents asked", his.len());
    let mut most: Vec<(usize, usize, String)> = views.iter().map(|v| (serde_json::to_string(&v.ran).map(|s| s.len()).unwrap_or(0), v.ran.len(), v.id.clone())).collect();
    most.sort();
    most.reverse();
    println!("cost most ran (bytes, runs, ticket) {:?}", &most[..most.len().min(5)]);
    let mut big: Vec<(usize, String)> = views.iter().map(|v| (serde_json::to_string(v).map(|s| s.len()).unwrap_or(0), v.id.clone())).collect();
    big.sort();
    big.reverse();
    println!("cost biggest views {:?}", &big[..big.len().min(5)]);
}

/// The follow round's work under the lock when no transcript has grown (main.rs `follow`), piece by piece, on the copy
/// with the agents prod runs (COST_RUNNING, names comma-separated) marked running. Jobs are sent to a channel nobody
/// reads; whatever a piece writes goes to the copy's folder.
#[test]
#[ignore]
fn follow_round_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = timed("Core::load", || Core::load(&coord, &base));
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let running: Vec<String> = std::env::var("COST_RUNNING").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    // COST_WORKING: of them, those in a turn (the others idle), as prod's agents were when it was read
    let working: Vec<String> = std::env::var("COST_WORKING").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    for n in &running {
        if let Some(a) = core.agents.get_mut(n) {
            a.live.pid = Some(1);
            a.live.turn = 1;
            a.live.state = if working.contains(n) { "working" } else { "idle" }.into();
        }
    }
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    core.jobs = Some(tx);
    println!("cost running {} of {} agents, boxed {:?}", core.agents.values().filter(|a| a.running()).count(), core.agents.len(), core.project.as_ref().map(|p| p.project.boxed));
    // build 31 (paper-research N-1397.p38): the delivery round by part, each read as `raise_for` makes it, summed over
    // the agents the program decides for, and the agents that cost most; the first round raises what the record left
    // owed, the later ones have nothing new
    for round in 0..4 {
        let names: Vec<String> = core.agents.values().filter(|a| core.runs_delivery(a) && !(crate::core::is_fresh(&a.name) && !a.running())).map(|a| a.name.clone()).collect();
        println!("cost delivery round {round}: {} agents decided for", names.len());
        let t0 = std::time::Instant::now();
        let _ = core.tickets.due_all(&names);
        let all = t0.elapsed().as_secs_f64();
        let mut parts: BTreeMap<&str, f64> = BTreeMap::new();
        let mut each: Vec<(f64, String, usize)> = Vec::new();
        for n in &names {
            let mut part = |k: &'static str, at: std::time::Instant| *parts.entry(k).or_default() += at.elapsed().as_secs_f64();
            let a0 = std::time::Instant::now();
            let at = std::time::Instant::now();
            let _ = core.agent_state(n);
            part("agent_state", at);
            let at = std::time::Instant::now();
            let due = core.tickets.due_whole(n);
            part("due_whole", at);
            let at = std::time::Instant::now();
            let waiting = core.tickets.m.waiting_of(n, due);
            part("waiting_of", at);
            let at = std::time::Instant::now();
            let _ = (core.tickets.m.kept(n), core.tickets.m.reported(n), core.tickets.m.raised_items(n));
            part("kept, reported, raised_items", at);
            let at = std::time::Instant::now();
            let _ = core.hand_now(n, false);
            part("hand_now", at);
            let at = std::time::Instant::now();
            let _: Vec<&String> = core.agents.keys().filter(|a| core.above(&Who::Agent((*a).clone()), n)).collect();
            part("above", at);
            let at = std::time::Instant::now();
            let _ = (core.tickets.m.deciding(n), core.tickets.m.delivery_queue(n), core.tickets.m.given_since(n));
            part("deciding, queue, given_since", at);
            each.push((a0.elapsed().as_secs_f64(), n.clone(), waiting.len()));
        }
        println!("cost   due_all of them, one pass {all:.4} s; by part over them: {}", parts.iter().map(|(k, s)| format!("{k} {s:.4} s")).collect::<Vec<_>>().join(", "));
        each.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("cost   most: {}", each.iter().take(8).map(|(s, n, w)| format!("{n} {s:.4} s ({w} waiting)")).collect::<Vec<_>>().join(", "));
        timed("  raise_delivery", || core.raise_delivery());
        let names = timed("  to_hand_over (its raise_delivery again)", || core.to_hand_over());
        timed(&format!("  hand_over x{}", names.len()), || for n in &names { core.hand_over(n) });
    }
    // paper-research N-1397.p58: rounds after one change, a thread COST_POST_BY opens for one agent (each of
    // COST_POST_TO in turn), as most of prod's rounds after a change are: the round (`to_hand_over` raises first), the
    // agents it reads, its passes over the machine; then the round after, which reads what the first one raised
    let by = std::env::var("COST_POST_BY").unwrap_or_else(|_| String::from("paper-research"));
    for to in std::env::var("COST_POST_TO").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
        let form = serde_json::json!({"to": by, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null});
        let text = Some(docket_machine::Text::New(format!("a post for {to}")));
        let open = docket_machine::Input::Open { kind: "thread".into(), under: None, opener: by.clone(), form, text, names: vec![to.to_string()], told: vec![] };
        if let Err(e) = core.ticket_input(open) {
            println!("cost a post for {to}: {e}");
            continue;
        }
        for round in ["the change round", "the round after"] {
            let (r0, p0) = (core.desk_reads(), core.tickets.due_passes());
            let names = timed(&format!("a post for {to}, {round}: to_hand_over"), || core.to_hand_over());
            let read: Vec<String> = core.desk_reads().into_iter().filter(|(a, n)| r0.get(a) != Some(n)).map(|(a, _)| a).collect();
            println!("cost   read {} agents ({}), {} passes over the machine", read.len(), read.iter().take(8).cloned().collect::<Vec<_>>().join(", "), core.tickets.due_passes() - p0);
            timed(&format!("  hand_over x{}", names.len()), || for n in &names { core.hand_over(n) });
        }
    }
    // hand_far's due, one pass for all far names, is what each name's own pass gives
    let far: Vec<String> = timed("far_names", || core.far_names()).into_iter().collect();
    let each: Vec<Vec<String>> = timed(&format!("due_now of {} far names, one pass each", far.len()), || far.iter().map(|f| core.due_now(f).into_iter().map(|d| d.id).collect()).collect());
    let mut all = timed("due_all of the far names, one pass", || core.tickets.due_all(&far));
    let once: Vec<Vec<String>> = far.iter().map(|f| core.due_now_of(all.remove(f).unwrap_or_default()).into_iter().map(|d| d.id).collect()).collect();
    assert_eq!(each, once);
    println!("cost far deliveries due {}", each.iter().map(Vec::len).sum::<usize>());
    for round in 0..5 {
        // a2h-speed (N-560 p37): rounds 3 and 4 with what is due forgotten first, as every round read it before
        if round >= 3 {
            core.tickets.forget_due();
        }
        println!("cost round {round}{}", if round >= 3 { " (nothing kept)" } else { "" });
        let t0 = std::time::Instant::now();
        let c0 = core.tickets.m.changes();
        timed("  line_spec", || { let _ = core.line_spec(); });
        timed("  not_read", || core.not_read());
        timed("  to_restart", || { let _ = core.to_restart(); });
        timed("  send_moves", || core.send_moves());
        timed("  to_start_owing", || { let _ = core.to_start_owing(); });
        timed("  raise_delivery", || core.raise_delivery());
        let names = timed("  to_hand_over", || core.to_hand_over());
        timed(&format!("  hand_over x{}", names.len()), || for n in &names { core.hand_over(n) });
        timed("  hand_clerk", || core.hand_clerk());
        timed("  hand_minder", || core.hand_minder());
        timed("  hand_hosts", || core.hand_hosts());
        timed("  hand_far", || core.hand_far());
        // the owner's steps that go on by themselves (lapse.rs, N-487): the kept set only, never every ticket
        timed(&format!("  lapse_round of {} kept", core.tickets.m.lapsing.len()), || core.lapse_round(chrono::Utc::now()));
        println!("cost round {round} total {:.3} s, the machine's changes counted up {} in it", t0.elapsed().as_secs_f64(), core.tickets.m.changes() - c0);
    }
    // docket-core8 (paper-research N-412 p1506): the round's part for each agent under the lock. Each tool list that
    // changed and each turn begun sends the agent's whole view (`Core::changed`), and a view reads every ticket for
    // the agent's label (`agent_on`) and items (`items_of`). COORDD_LIVE should name a file in the copy (`journal`).
    let names: Vec<String> = core.agents.values().filter(|a| a.running()).map(|a| a.name.clone()).collect();
    for round in 0..3 {
        println!("cost views, round {round}");
        timed(&format!("  view of {} running agents", names.len()), || for n in &names { let _ = core.view(n); });
        timed("  agent_on of each", || for n in &names { let _ = docket_machine::label::agent_on(&core.tickets.m, n, &crate::label::read); });
        timed("  items_of of each", || for n in &names { let _ = core.items_of(n); });
        timed("  parts() once each", || for _ in &names { let _ = core.parts(); });
        timed("  moves::stand of each", || for n in &names { let _ = crate::moves::stand(&core.coord, n); });
        timed("  changed of each (view, emit, journal)", || for n in &names { core.changed(n) });
    }
}

/// The follow round's moves (moves.rs `to_move`, every second under the lock; docket-edge5, N-1446 p2899, p2903) on a
/// copy of a record: one walk of the machine as each agent's `move_due` made three of them (the active layers; one
/// agent's owed steps, then its held ones), the reads now made once for every agent (`moves::Reads`), and `to_move`
/// with the agents COST_RUNNING names running and out of their turn. Each needs a session kept to be asked about: the
/// run makes `agents/NAME/session.json` in the copy. Times in ms, each the mean of 20.
#[test]
#[ignore]
fn move_reads_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = timed("Core::load", || Core::load(&coord, &base));
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let running: Vec<String> = std::env::var("COST_RUNNING").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    for n in &running {
        if let Some(a) = core.agents.get_mut(n) {
            (a.live.pid, a.live.turn, a.live.state) = (Some(1), 1, "idle".into());
        }
    }
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    core.jobs = Some(tx);
    let ms = |f: &mut dyn FnMut() -> usize| {
        let at = std::time::Instant::now();
        let n: usize = (0..20).map(|_| f()).sum();
        (at.elapsed().as_secs_f64() * 1000.0 / 20.0, n / 20)
    };
    let m = &core.tickets.m;
    let asked: Vec<&String> = running.iter().filter(|n| core.agents.get(*n).is_some_and(|a| matches!(a.kind.as_str(), "claude" | "codex"))).collect();
    println!("cost moves: {} tickets, {} steps owed or held, {} layers active; {} agents asked about", m.tickets.len(), m.steps_awaited().count(), m.active_layers().len(), asked.len());
    let (layers, _) = ms(&mut || m.active_layers().len());
    let (owed, _) = ms(&mut || asked.iter().map(|n| m.owes(n).len() + m.holds(n).len()).sum());
    let (once, _) = ms(&mut || {
        let _ = crate::moves::Reads::of(m);
        1
    });
    let each = owed / asked.len().max(1) as f64;
    println!("cost moves: active_layers {layers:.2} ms; owes and holds {each:.2} ms an agent; so move_due's reads {:.2} ms an agent, and the round's for {} agents {:.2} ms", layers + each, asked.len(), layers * (1 + asked.len()) as f64 + owed);
    println!("cost moves: Reads::of, once for every agent, {once:.2} ms");
    // the first round marks the moves it finds as running (`moving`): the rounds after are what a quiet second costs
    let first = timed("to_move, the first round", || core.to_move().len());
    let (round, _) = ms(&mut || core.to_move().len());
    println!("cost moves: to_move {round:.2} ms a round after the first, which found {first} moves");
}

/// Mentions on a copy of a record (paper-research N-412 p169): what reading every post's words costs at start, what
/// they add to the views the snapshot carries, the most named ticket's view, and one landing post (its read and the
/// views of what it names, sent again). `COST_COORD=DIR COST_BASE=DIR cargo test -p coordd mentions_costs -- --ignored
/// --nocapture`; it writes nothing.
#[test]
#[ignore]
fn mentions_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load (replay of the record)", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let m = &core.tickets.m;
    let shown = m.tickets.range(..docket_machine::HIDDEN).count();
    let posts: usize = m.tickets.range(..docket_machine::HIDDEN).map(|(_, x)| x.posts.len()).sum();
    let placed = m.tickets.range(..docket_machine::HIDDEN).flat_map(|(_, x)| &x.posts).filter(|p| !matches!(m.text(&p.text).unwrap_or_else(|| p.text.clone()), Text::New(_))).count();
    println!("cost record: {shown} shown tickets, {posts} posts, {placed} of them read from a session file");
    let views = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.is_open() || x.kind == "item").filter(|x| x.t < docket_machine::HIDDEN).map(|x| x.t).collect::<Vec<_>>();
    let ts = views(&core);
    let size = |core: &Core| ts.iter().filter_map(|t| core.ticket_shown(*t)).map(|v| serde_json::to_string(&v).unwrap().len()).sum::<usize>();
    let before = timed(&format!("ticket_shown of {} open tickets, no mentions", ts.len()), || size(&core));
    {
        let Tickets { m, mentions, .. } = &mut core.tickets;
        timed("mentions: every post read (the start)", || mentions.read_all(m, &crate::label::read));
    }
    let (holding, ids) = core.tickets.mentions.size();
    println!("cost mentions: {holding} posts hold {ids} ids");
    let after = timed(&format!("ticket_shown of {} open tickets, mentions", ts.len()), || size(&core));
    println!("cost views' JSON: {before} bytes without, {after} with (+{})", after - before);
    // the most named ticket
    let agent = |n: &str| core.agents.contains_key(n);
    let label = |u: u64| docket_machine::label::ticket(&core.tickets.m, u, &crate::label::read);
    let most = core.tickets.m.tickets.range(..docket_machine::HIDDEN).map(|(t, _)| *t).max_by_key(|t| core.tickets.mentions.lines(&core.tickets.m, &agent, &label, &core.tickets.m.id(*t)).len()).unwrap();
    let lines = timed(&format!("lines back of {}", core.tickets.m.id(most)), || core.tickets.mentions.lines(&core.tickets.m, &agent, &label, &core.tickets.m.id(most)));
    let v = timed(&format!("ticket_shown {}", core.tickets.m.id(most)), || core.ticket_shown(most).unwrap());
    println!("cost {}: {} lines back, {} bytes of them, {} posts' ids {} bytes", v.id, lines.len(), serde_json::to_string(&v.mentioned).unwrap().len(), v.mentions.len(), serde_json::to_string(&v.mentions).unwrap().len());
    // one landing post: the newest post that names something, read again, then the views of what it names
    let (t, n) = core.tickets.mentions.newest(&core.tickets.m).unwrap();
    core.tickets.mentions.forget(t, n);
    let named = {
        let Tickets { m, mentions, .. } = &mut core.tickets;
        timed(&format!("a landing post's read ({}.p{n})", m.id(t)), || mentions.read_posts(m, &crate::label::read, t))
    };
    let us: Vec<u64> = named.iter().filter_map(|id| docket_machine::parse_ticket(id).filter(|u| *u != t && core.tickets.m.tickets.contains_key(u) && core.tickets.m.id(*u) == *id)).collect();
    let agent = |n: &str| core.agents.contains_key(n);
    let label = |u: u64| docket_machine::label::ticket(&core.tickets.m, u, &crate::label::read);
    timed(&format!("its {} named tickets' new lines back ({named:?})", us.len()), || us.iter().map(|u| core.tickets.mentions.lines_new(&core.tickets.m, &agent, &label, &core.tickets.m.id(*u), t, n - 1).len()).sum::<usize>());
    // the most named ticket's new line for its newest naming post (`Body::Mentioned`), against its view rebuilt
    let id = core.tickets.m.id(most);
    let last = lines.last().unwrap();
    let lt = docket_machine::parse_ticket(&last.by).unwrap();
    let new = timed(&format!("{id}'s new line back for {}.p{}", last.by, last.post), || core.tickets.mentions.lines_new(&core.tickets.m, &agent, &label, &id, lt, last.post - 1));
    println!("cost {id}: Body::Mentioned of {} lines, {} bytes, against the view's {} bytes", new.len(), serde_json::to_string(&new).unwrap().len(), serde_json::to_string(&v).unwrap().len());
}

/// `AgentView.on` on a copy of a record (paper-research N-447 p49): what it adds to each agent's view, built under the
/// lock. `COST_COORD=DIR COST_BASE=DIR cargo test -p coordd agent_on_costs -- --ignored --nocapture`; it writes nothing.
#[test]
#[ignore]
fn agent_on_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load (replay of the record)", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let names: Vec<String> = core.agents.keys().cloned().collect();
    for _ in 0..2 {
        timed(&format!("view of {} agents (on included)", names.len()), || names.iter().map(|n| core.view(n)).count());
        timed(&format!("agent_on of {} agents alone", names.len()), || names.iter().filter_map(|n| docket_machine::label::agent_on(&core.tickets.m, n, &crate::label::read)).count());
    }
    for n in &names {
        let on = core.view(n).on;
        println!("cost {n}: on {on:?}, label {:?}", docket_machine::label::agent(&core.tickets.m, n, &crate::label::read));
    }
}

/// `AgentView.acted` on a copy of a record (paper-research N-1396 p247: nothing measurable added under the lock;
/// consult-3 p250: the bytes it adds to every snapshot): each agent's view timed with its last acts and their read
/// alone, over ROUNDS (COST_ROUNDS, default 1000), and the snapshot's agents part as JSON with and without them.
/// `COST_COORD=DIR COST_BASE=DIR cargo test -p coordd agent_acted_costs -- --ignored --nocapture`; it writes nothing.
#[test]
#[ignore]
fn agent_acted_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let rounds: usize = std::env::var("COST_ROUNDS").ok().and_then(|n| n.parse().ok()).unwrap_or(1000);
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load (replay of the record)", || Tickets::load(&coord, &base, project.as_deref(), 1));
    // the machine replayed from the record keeps, for each agent, the last ACTS_KEPT of its `delivery_acted` lines
    // there, the newest first (consult-3 N-1396 p250)
    let log = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
    let mut written: BTreeMap<String, Vec<docket_machine::ViewActed>> = BTreeMap::new();
    for l in log.lines().filter(|l| l.contains(docket_machine::syntax::ACTED)) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else { continue };
        if v["name"] != docket_machine::syntax::ACTED {
            continue;
        }
        let ids = |k: &str| -> Vec<String> { v["value"][k].as_array().map(|xs| xs.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default() };
        let x = docket_machine::ViewActed::of(v["value"]["act"].as_str().unwrap_or_default(), &v["at"].as_str().unwrap_or_default().to_string(), &ids("items"), &ids("rest"), v["value"]["why"].as_str());
        written.entry(v["value"]["agent"].as_str().unwrap_or_default().into()).or_default().push(x);
    }
    let mut same = 0;
    for (agent, all) in &written {
        let last: Vec<docket_machine::ViewActed> = all.iter().rev().take(docket_machine::machine::ACTS_KEPT).cloned().collect();
        let kept = core.tickets.m.last_acts(agent);
        if kept == last {
            same += 1;
            continue;
        }
        // where they part, by field only (no ids, no words)
        let field = |a: &docket_machine::ViewActed, b: &docket_machine::ViewActed| -> Vec<&str> {
            [("act", a.act != b.act), ("at", a.at != b.at), ("items", (&a.items, a.more_items) != (&b.items, b.more_items)), ("rest", (&a.rest, a.more_rest) != (&b.rest, b.more_rest)), ("why", a.why != b.why)].into_iter().filter(|(_, d)| *d).map(|(f, _)| f).collect()
        };
        let parts: Vec<String> = kept.iter().zip(&last).enumerate().filter(|(_, (a, b))| a != b).map(|(i, (a, b))| format!("#{i} {} {} {:?} (replayed items {} rest {} why {}; written items {} rest {} why {})", b.act, b.at, field(a, b), a.items.len() + a.more_items, a.rest.len() + a.more_rest, a.why.as_ref().map_or(0, |w| w.len()), b.items.len() + b.more_items, b.rest.len() + b.more_rest, b.why.as_ref().map_or(0, |w| w.len()))).collect();
        println!("cost acted differs {agent}: replayed {} written {} ({} lines in all): {}", kept.len(), last.len(), all.len(), parts.join("; "));
    }
    println!("cost acted: the record's {} acted lines, of {} agents: each agent's last {} replayed as written for {same} of them", written.values().map(Vec::len).sum::<usize>(), written.len(), docket_machine::machine::ACTS_KEPT);
    let names: Vec<String> = core.agents.keys().cloned().collect();
    for _ in 0..2 {
        let at = std::time::Instant::now();
        for _ in 0..rounds {
            for n in &names {
                std::hint::black_box(core.view(n));
            }
        }
        let view = at.elapsed().as_secs_f64() * 1e6 / (rounds * names.len()) as f64;
        let at = std::time::Instant::now();
        for _ in 0..rounds {
            for n in &names {
                std::hint::black_box(core.tickets.m.last_acts(n));
            }
        }
        let acts = at.elapsed().as_secs_f64() * 1e6 / (rounds * names.len()) as f64;
        // what an act adds under the lock (ticket_effects: `changed` once per agent that acted): its view made and
        // written as the event line the window reads; `changed` itself is not called, as its journal writes a file
        let at = std::time::Instant::now();
        for _ in 0..rounds {
            for n in &names {
                std::hint::black_box(serde_json::to_string(&coord_types::Event { seq: 1, body: coord_types::Body::Agent(core.view(n)) }).unwrap().len());
            }
        }
        let sent = at.elapsed().as_secs_f64() * 1e6 / (rounds * names.len()) as f64;
        println!("cost acted: {rounds} rounds of {} agents' views: {view:.2} µs a view with acted, of it {acts:.3} µs its last acts ({:.2}%); {sent:.2} µs a view made and written as its event (one more for each agent that acts)", names.len(), 100.0 * acts / view);
    }
    let views: Vec<coord_types::AgentView> = names.iter().map(|n| core.view(n)).collect();
    let bytes = |vs: &[coord_types::AgentView]| serde_json::to_string(vs).map(|s| s.len()).unwrap_or(0);
    let bare: Vec<coord_types::AgentView> = views.iter().map(|v| coord_types::AgentView { acted: Vec::new(), ..v.clone() }).collect();
    let (with, without) = (bytes(&views), bytes(&bare));
    let snap = serde_json::to_string(&core.snapshot()).map(|s| s.len()).unwrap_or(0);
    println!("cost acted: {} agents with acts, {} acts; the snapshot's agents {without} B without, {with} B with (+{} B); the whole snapshot {snap} B", views.iter().filter(|v| !v.acted.is_empty()).count(), views.iter().map(|v| v.acted.len()).sum::<usize>(), with - without);
    for v in views.iter().filter(|v| !v.acted.is_empty()) {
        let by: Vec<String> = ["deliver", "keep", "report"].iter().map(|a| format!("{a} {}", v.acted.iter().filter(|x| x.act == *a).count())).collect();
        println!("cost acted {}: {} ({}), newest {}", v.name, v.acted.len(), by.join(", "), v.acted[0].at);
    }
}

/// What part 4's whereabouts cost in the window's snapshot (N-818 posts 33 and 35): `owner_unhanded` made once (what
/// `ahead` counts; kept until a ticket changes), the field for every ticket the snapshot carries (`ticket_views`: open
/// ones, and closed in the last day), its bytes there, and for every ticket of the record at all, the most it could be.
/// `COST_COORD=DIR COST_BASE=DIR cargo test -p coordd whereabouts_costs -- --ignored --nocapture`; it writes nothing.
#[test]
#[ignore]
fn whereabouts_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load (replay of the record)", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let views = timed("ticket_views (the snapshot's tickets, with the field)", || core.ticket_views());
    let m = &core.tickets.m;
    let list = timed("owner_unhanded, made once", || docket_machine::view::owner_unhanded(m));
    println!("cost unhanded: {} agents, {} posts of his not handed to them", list.len(), list.values().map(Vec::len).sum::<usize>());
    let ts: Vec<u64> = views.iter().filter_map(|v| docket_machine::parse_ticket(&v.id).filter(|t| m.tickets.contains_key(t) && m.id(*t) == v.id)).collect();
    let field = timed(&format!("whereabouts of the snapshot's {} tickets", ts.len()), || ts.iter().map(|t| docket_machine::view::whereabouts(m, &m.tickets[t]).len()).sum::<usize>());
    let bytes: usize = views.iter().map(|v| serde_json::to_string(&v.whereabouts).unwrap().len()).sum();
    let waiting = views.iter().flat_map(|v| &v.whereabouts).flat_map(|p| &p.agents).filter(|w| w.waiting.is_some()).count();
    println!("cost whereabouts: {field} posts of his on {} of the snapshot's tickets, {waiting} waiting, {bytes} bytes of JSON", views.iter().filter(|v| !v.whereabouts.is_empty()).count());
    let all = timed(&format!("whereabouts of every ticket ({})", m.tickets.len()), || m.tickets.values().map(|x| docket_machine::view::whereabouts(m, x).len()).sum::<usize>());
    println!("cost whereabouts: {all} posts of his on every ticket");
}

/// Research's tickets on 20 Nov 12:35 by their posts, counted on a copy of its record here (numbers only; the record
/// does not go to the tower, consult-3 N-412 p2169): (posts, tickets with about that many). 1198 tickets shown, 11423
/// posts; N-412 and N-101 the two longest. Its typed texts: median 894 characters, largest 25343.
const RESEARCH_SHAPE: &[(usize, usize)] = &[(1, 274), (2, 296), (3, 275), (4, 98), (5, 40), (7, 98), (14, 57), (32, 35), (76, 14), (128, 8), (396, 1), (1883, 1), (2169, 1)];

/// The owner's search by words (the app's `Request::Find`: not starred, not wide) on a made-up record of research's
/// shape ([`RESEARCH_SHAPE`]; texts of its median length): its time under the lock and off it, and under the lock each
/// ticket's title from its whole view, as `ticket_find` took it before, and from `label::title` alone, the call the
/// view makes for it and what `ticket_find` takes now (docket-edge6, paper-research N-412 p2161: his typing held
/// research's lock 31 s of a minute, `request find 19x … held 31418 ms`). Every title is the same both ways, and so
/// is the answer, given either; its digest is printed, to set beside another binary's.
/// `cargo test -p coordd find_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn find_costs() {
    let (mut core, coord) = super::edit_tests::core("find-costs");
    // no word a search below looks for, but where each post's number marks it
    let filler = "the window draws his thread and its posts while the engine answers each word he types; ".repeat(11);
    let (mut n, at) = (0usize, now());
    let text = |n: usize| {
        let marks = [(10, "search"), (50, "deep"), (100, "titles only")];
        let marked: Vec<&str> = marks.iter().filter(|(k, _)| n % k == 0).map(|(_, w)| *w).collect();
        format!("{} post {n} {}", &filler[..880], marked.join(" "))
    };
    timed("the made-up record", || {
        for &(posts, tickets) in RESEARCH_SHAPE {
            for _ in 0..tickets {
                n += 1;
                let first = text(n);
                let form = json!({"to": "w2", "typed": [], "names": ["w2"], "text": first, "addressed": {"head": "Named"}});
                let _ = core.tickets.m.apply(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New(first)), names: vec!["w2".into()], told: vec![] }, at.clone());
                let t = *core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
                for k in 1..posts {
                    n += 1;
                    let by = if k % 2 == 1 { "w2" } else { "owner" };
                    let _ = core.tickets.m.apply(Input::Post { t, by: by.into(), text: Text::New(text(n)), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }, at.clone());
                }
            }
        }
    });
    let m = &core.tickets.m;
    let shown: Vec<&docket_machine::Ticket> = m.tickets.values().filter(|x| x.t < docket_machine::HIDDEN).collect();
    println!("cost find: {} tickets shown, {} posts", shown.len(), shown.iter().map(|x| x.posts.len()).sum::<usize>());
    let viewed: HashMap<u64, String> = timed("titles from each whole view (before)", || shown.iter().map(|x| (x.t, m.view(x.t).map(|v| v.title).unwrap_or_default())).collect());
    let titled: HashMap<u64, String> = timed("titles from label::title (now)", || shown.iter().map(|x| (x.t, docket_machine::label::title(m, x))).collect());
    assert_eq!(viewed, titled);
    timed("ids, once for each post (before)", || shown.iter().flat_map(|x| x.posts.iter().map(|_| m.id(x.t))).count());
    timed("texts of each post", || shown.iter().flat_map(|x| x.posts.iter().map(|p| m.text(&p.text).unwrap_or_else(|| p.text.clone()))).count());
    for q in ["search", "titles only", "deep", "zz-no-such-word"] {
        for _ in 0..2 {
            let Err(f) = timed(&format!("ticket_find {q:?} (under the lock)"), || core.ticket_find(q, false, false)) else { panic!("{q:?} is a search by words") };
            let posts = f.posts.len();
            let r = timed(&format!("find_words {q:?} (off the lock)"), || find_words(f));
            let Err(mut before) = core.ticket_find(q, false, false) else { unreachable!() };
            before.titles = viewed.clone();
            assert_eq!(find_words(before).out, r.out, "{q:?}: the answer with each view's title");
            let mut h = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&r.out, &mut h);
            println!("cost find {q:?}: {posts} posts searched, {} hits, digest {:016x}", r.out.lines().filter(|l| !l.is_empty()).count(), std::hash::Hasher::finish(&h));
        }
    }
    let _ = std::fs::remove_dir_all(&coord);
}

/// The owner's Reach on a record (consult-3 N-1396 p306, 3; docket-edge5 p307): for each draft, the read off the lock
/// (`Tickets::read_defs`: the files and the policy compiled, as Reload reads them) and the check and plan under it
/// (`Core::ticket_reach`), timed apart over COST_ROUNDS (default 10), with the open tickets it plans over. The drafts,
/// each in the folder a reload reads it from: claim.call's `repair` words, fix.kind's rounds, delivery.kind's keep
/// reason, next.call's words. It writes nothing: a reach records and counts nothing, and `Core::changed` is not called.
/// `COST_COORD=DIR COST_BASE=DIR cargo test -p coordd reach_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn reach_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("COST_COORD"), std::env::var("COST_BASE")) else { return };
    let rounds: u32 = std::env::var("COST_ROUNDS").ok().and_then(|n| n.parse().ok()).unwrap_or(10);
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    core.tickets = timed("Tickets::load (replay of the record)", || Tickets::load(&coord, &base, project.as_deref(), 1));
    let m = &core.tickets.m;
    let open = |k: Option<&str>| m.tickets.values().filter(|x| x.is_open() && k.is_none_or(|k| x.kind == k)).count();
    println!("cost reach: open tickets {} (fix {}, delivery {}), handed now {}", open(None), open(Some("fix")), open(Some("delivery")), core.tickets.handed.len());
    let places = core.places();
    let (kind_dirs, call_dirs) = defs_dirs(&folders(&places.coord, &places.base, places.project.as_deref()));
    let drafts = [
        (&call_dirs, "claim.call", "This defect was accepted for fixing, and you are to fix it:", "Fix this accepted defect:"),
        (&kind_dirs, "fix.kind", "iterate c = first up to 3", "iterate c = first up to 4"),
        (&kind_dirs, "delivery.kind", "keep [i] \"comes when its turn ends\"", "keep [i] \"waits for its turn to end\""),
        (&call_dirs, "next.call", ", of a software team. It is idle", ", of the team. It is idle"),
    ];
    for (dirs, name, was, now) in drafts {
        // the file a reload reads: the last folder that has it
        let Some(path) = dirs.iter().rev().map(|d| format!("{d}/{name}")).find(|p| std::path::Path::new(p).exists()) else {
            println!("cost reach {name}: in no folder read");
            continue;
        };
        let text = std::fs::read_to_string(&path).unwrap();
        let new = text.replacen(was, now, 1);
        if new == text {
            println!("cost reach {name}: the draft changes nothing in {path}");
            continue;
        }
        let (places, first) = core.ticket_reach_from(&crate::core::Who::Owner, &path, &new).unwrap_or_else(|r| panic!("{path}: {}", r.out));
        let (mut off, mut under, mut checked) = (std::time::Duration::ZERO, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let written = || std::fs::metadata(format!("{coord}/tickets/log.jsonl")).map(|f| f.len()).unwrap_or(0);
        let (bytes, changes) = (written(), core.tickets.m.changes());
        let mut out = String::new();
        for _ in 0..rounds {
            let at = std::time::Instant::now();
            let read = Tickets::read_defs(&places, first);
            off += at.elapsed();
            let at = std::time::Instant::now();
            let r = core.ticket_reach(read);
            under += at.elapsed();
            out = r.out;
            // of it, the checks alone (`reload_checked`: Reload's refusals and the calls changed)
            let read = Tickets::read_defs(&places, first);
            let at = std::time::Instant::now();
            std::hint::black_box(core.reload_checked(read).is_ok());
            checked += at.elapsed();
        }
        // a changed call's check reads every ticket in the log for one run as the files have it (`runs_today`)
        let calls: Vec<String> = serde_json::from_str::<serde_json::Value>(&out).ok().and_then(|v| v["calls"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())).unwrap_or_default();
        let at = std::time::Instant::now();
        for _ in 0..rounds {
            for c in &calls {
                std::hint::black_box(core.tickets.m.runs_today(c).len());
            }
        }
        let today = at.elapsed();
        assert_eq!((written(), core.tickets.m.changes()), (bytes, changes), "a reach records nothing and counts nothing");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
        let mut cases: BTreeMap<String, usize> = BTreeMap::new();
        for x in v["tickets"].as_array().into_iter().flatten() {
            *cases.entry(x["reach"].as_str().unwrap_or("?").to_string()).or_default() += 1;
        }
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3 / f64::from(rounds);
        let refused = if v.is_null() { format!("; refused: {out}") } else { String::new() };
        println!("cost reach {name}: read off the lock {:.1} ms, checked and planned under it {:.1} ms (of it the checks {:.1} ms, `runs_today` {:.1} ms) (mean of {rounds}); kinds {} calls {}; reached {cases:?}{refused}", ms(off), ms(under), ms(checked), ms(today), v["kinds"], v["calls"]);
    }
}
