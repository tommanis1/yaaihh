//! An agent's brain, coordd's half (0.3; the owner, N-1104 p606-p628, N-1445 p9-p11; fundamentals structure.md §9; the
//! plan, research N-1444 p16 and proto-mind N-1446 p12, p35). A pass of the brain is a `brain` ticket
//! (agent-base/kinds/brain.kind); its steps, owed by the watcher, are answered here as the delivery's `next` is
//! (delivery.rs): one stateless call each (brain.call), on the agent's own queue beside its `next`, so a pass holds up
//! neither the clerk nor his posts, on a report built from the record alone. The report is never the agent's
//! transcript, its tool calls or its files (the owner, p612: "maybe not the toolcalls ... lets think economics";
//! paper-research N-1444 p4): it is the owner's posts to the agent and the agent's own posts, as the tickets hold them.
//!
//! - `attend`, the owner model: BURST, his posts the pass is on, whole; HIS, his last posts to the agent before them;
//!   OPEN, his posts to it that it has not answered (the ledger, `Machine::asked`), from the policy's
//!   `unanswered-within` before the burst (none read without it); EARLIER, his earlier posts as an index, first lines,
//!   on the burst's thread, the thread it hangs from and the threads hanging from it, and to the agent, from the
//!   policy's `earlier-within` before the burst (none read without it); ITS, its own last posts, first lines; DOING,
//!   its turn as the record has it: the one running, what it was given and what went into it, else the last that ended.
//!   It answers, beside its reading, the ids of his earlier posts whose words on his topic the agent must have before
//!   it answers.
//! - `elect`, the selector: the burst again, the owner model's topic and reading, and the candidates: the owner model's
//!   proposals, `unanswered by it` (each of his posts in OPEN, "answer it", with no model), `his earlier words` (the
//!   posts the owner model named), and nothing. Its pick goes to the agent with HIS, those posts' words as the record
//!   holds them, which coordd adds here (`from_record`): a model never quotes him (paper-research-brain, N-1450 p3). A
//!   pick of the `unanswered by it` candidate on his post, which the agent answered while the call ran, is Nothing
//!   (`answered_since`).
//! - `ahead`, the base step (the owner, N-1104 p606, p610; N-1511 p56, p59): at every Nth turn end of the agent, one
//!   call on its own record since its last look, `outlook_env`: its role, its own posts, what it owes by the ledger,
//!   what waits for it, its turn, and the last look's decision and notes, the brain's memory (`last_look`). Next's
//!   thought is posted naming the agent, so delivery gives it as a post that asks it to act: at once to an idle agent,
//!   at a busy one's turn end (brain.kind). The ids a Next may rest on are the report's (`clerk::brain_ok`).
//!
//! OPEN is symbolic and says only that: his post to the agent that no post of the agent's answers, by the ledger's one
//! rule (`Machine::asked`, I-1455 (B)): no post of the agent's written after it with it handed that names him (typed,
//! or filled in by the reply rule), nor the reply tool's answer to it, nor a result. A post that only passes his on to
//! another, typing only that one's name, does not answer it (consult-3's N-1479 p84 case, paper-research p94); under
//! the rule before, it did (N-1445 #4 after #3, proto-reader N-1446 p112).
//!
//! The project's owner model (agent-base/kinds/owner_model.kind; the brain's part 1, paper-research N-1446 p1108, its
//! question N-1445 p29's word for word): what he wants in general, as a model reads all his posts. One ticket of that
//! kind stands for the project; coordd opens an update under it in its round (`update_owner_model`, an `owner_update`
//! ticket, as a pass is a `brain` ticket) and answers its step here, as a pass's (`owner_model_env`, on the queue
//! `brain owner`): `whole` reads his posts from the policy's `owner-model-within` before the newest of the update's,
//! `delta` the last model and his posts since. Attention and the selector read the newest model under the words "the
//! model's reading of him, not his words" (`model_line`), and so does any other part (`owner_model`): his own words
//! outrank it.

use super::*;
use crate::clerk::{Brain, ATTENTION, CONSOLIDATION, ELECTION, OUTLOOK, OWNER_DELTA, OWNER_WHOLE};
use coord_types::{OWNER_MODEL, OWNER_UPDATE};
use docket_machine::{Delivery, What};

/// How many of his posts to the agent before the burst the owner model reads (paper-research N-1444 p10: "his last 3
/// posts to the agent").
const HIS: usize = 3;
/// How many of those posts of his it reads, newest; each is also a candidate, `unanswered by it`. How far back they
/// reach is the policy's (`unanswered-within`, policy/tickets.eflint; research's 12 h: unbounded, his p614 burst read
/// seven from 14-19 Nov and none from tonight, proto-reader N-1446 p98, proto-lead p138).
const OPEN: usize = 8;
/// How many of the agent's own posts it reads, as first lines (N-1444 p10: "the agent's last 8 posts").
const ITS: usize = 8;
/// How much of each of his posts in the burst it reads (his posts are short; a pasted log is cut).
const BURST_CHARS: usize = 3000;
/// How many of his earlier posts the index lists, newest, when the policy gives no `earlier-lines`, and how much of each
/// one's first line: 40 lines are about 2,000 tokens of `attend`'s input. How far back they reach is the policy's
/// (`earlier-within`).
const EARLIER: usize = 40;
const EARLIER_CHARS: usize = 160;
/// How much of the words of each of his earlier posts the owner model named (at most `clerk::NAMED`) the agent is given:
/// a longer post is cut, marked "…"; what is given is the record's bytes up to there.
const QUOTE_CHARS: usize = 1500;
/// How many of the agent's own posts since its last look the base step reads, newest, each whole up to BURST_CHARS, and
/// how many of the asks it owes, newest (brain.call's `outlook`: "at most 12").
const SINCE: usize = 12;
const OWED: usize = 12;

/// A post as the report reads it: when, its ticket, its number.
type At = (String, u64, u32);

/// The name the steps' jobs of an update of the project's owner model go by (`Brain::agent`: the journal's, and their
/// queue's, `brain owner`). The kinds' names, the standing ticket's and an update's, are coord-types' `OWNER_MODEL` and
/// `OWNER_UPDATE`, which the window's Threads leaves out.
const OWNER: &str = "owner";

/// What attention and the selector are given for the model when none stands: before the first is made, and once the
/// lead has stopped the node (consult-3 N-1446 p1176: "none yet" would be false then).
const NO_MODEL: &str = "none (no model of him stands)";

/// The project's owner model as the record holds it ([`Core::owner_model_read`]).
struct OwnerModel {
    /// Its node: the newest open `owner_model` ticket.
    node: u64,
    /// Whether an update of it is open.
    running: bool,
    /// The open update whose step was refused, and why: none of this build's answers is (`brain_ok`), but an older
    /// build's watcher takes a step it has no call for as an intake and answers Thread (build 64, proto-mind's
    /// wb64-step-run5.log), and a refused step is handed no more.
    refused: Option<(u64, String)>,
    /// The newest model: the newest update that made one, by its ticket, its text, and the newest of his posts that
    /// update was sent (an update is sent only posts no model had read).
    model: Option<(u64, String, At)>,
    /// The posts the updates that made a model were sent: those a model has read. A whole is sent every one of his
    /// that none had read, from the policy's `within` before then (and reads the rest of that week besides), so no time
    /// is compared: the record's is to the second, and a post he wrote while an update was open, in the same second as
    /// the newest it was sent, is not one it read.
    sent: std::collections::BTreeSet<(u64, u32)>,
    /// The last update, when it made no model (its call failed, or answered out of shape), and the posts it was sent:
    /// they are not sent again until one of his it was not sent comes.
    failed: Option<(u64, Vec<(u64, u32)>)>,
}

/// What the brain reads of the record for AGENT on a burst of his posts: his posts in it, his posts to the agent before
/// them, his open asks, his earlier posts near the burst or to the agent, and the agent's own posts, each oldest first.
struct Reading {
    burst: Vec<At>,
    his: Vec<At>,
    open: Vec<At>,
    earlier: Vec<At>,
    its: Vec<At>,
    /// The policy's `unanswered-within`, in seconds, OPEN was read by.
    within: Option<u64>,
    /// The policy's `earlier-within`, in seconds, EARLIER was read by.
    earlier_within: Option<u64>,
    /// The agent's turn as the record had it when the pass read it (`Core::turn_on`), attention's DOING.
    doing: String,
    /// The model of him the pass read: the update that made it, by its ticket, and the newest of his posts that update
    /// was sent; none, there was none. Its text is that update's result on the record, which never changes once
    /// written, so a render of the pass gives the model it read (`Core::model_line`; docket-core2 N-1446 p1155).
    model: Option<(u64, At)>,
}

impl Core {
    /// The agent whose brain's pass D's step is: a watcher's step of a `brain` ticket that coordd answers here; `owner`
    /// for a watcher's step of an update of the owner model (`owner_run`), which coordd answers here too.
    pub(crate) fn brain_of(&self, d: &Delivery) -> Option<String> {
        let What::Step { call, .. } = &d.what else { return None };
        if d.to != docket_machine::WATCHER {
            return None;
        }
        if ["whole", "delta"].contains(&call.as_str()) && self.owner_run(d.t) {
            return Some(OWNER.into());
        }
        if !["attend", "elect", "ahead", "consolidate"].contains(&call.as_str()) {
            return None;
        }
        let x = self.tickets.m.tickets.get(&d.t)?;
        (x.kind == "brain").then(|| x.form.get("agent").and_then(Value::as_str).map(String::from)).flatten()
    }

    /// Whether T is an update of the owner model: an `owner_update` ticket under an `owner_model` one (one opened
    /// anywhere else is not coordd's to answer).
    fn owner_run(&self, t: u64) -> bool {
        let m = &self.tickets.m;
        m.tickets.get(&t).is_some_and(|x| x.kind == OWNER_UPDATE && x.under.and_then(|u| m.tickets.get(&u)).is_some_and(|u| u.kind == OWNER_MODEL))
    }

    /// The brain's step D as its call's job takes it: the step given first (the log reads given, then answered, as the
    /// clerk's), and the report. None for any other step.
    pub(crate) fn brain_job(&mut self, d: &Delivery) -> Option<crate::clerk::Job> {
        let agent = self.brain_of(d)?;
        let What::Step { step, call, args, .. } = &d.what else { return None };
        let (step, call) = (*step, call.clone());
        let args = crate::render::resolved(args);
        let posts: Vec<String> = args["posts"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(String::from)).collect();
        self.ticket_given(&d.to, d);
        let (run, env, burst, ids, open) = if self.owner_run(d.t) {
            let (run, env, read) = self.owner_model_env(&call, &args);
            (run, env, Vec::new(), read, Vec::new())
        } else if call == "ahead" || call == "consolidate" {
            // consolidation reads the base step's report (brain.kind's Compacting)
            let (env, ids) = self.outlook_env(&agent, d.t);
            (if call == "ahead" { OUTLOOK } else { CONSOLIDATION }, env, Vec::new(), ids, Vec::new())
        } else {
            let r = self.reading(&agent, &posts);
            let burst: Vec<String> = r.burst.iter().map(|a| self.post_name(a)).collect();
            if call == "attend" {
                let ids: Vec<String> = [&r.burst, &r.his, &r.open, &r.earlier, &r.its].into_iter().flatten().map(|a| self.post_name(a)).collect();
                (ATTENTION, self.attention_env(&agent, &r), burst, ids, Vec::new())
            } else {
                let (env, ids, open) = self.election_env(&agent, &r, &args);
                (ELECTION, env, burst, ids, open)
            }
        };
        let m = &self.tickets.m;
        Some(crate::clerk::Job {
            worker: d.to.clone(),
            to: agent.clone(),
            t: d.t,
            step,
            ticket: d.ticket.clone(),
            text: String::new(),
            open: String::new(),
            split: m.defs.calls.get(run).map(|c| c.head.clone()),
            places: self.places(),
            route: None,
            clean: None,
            brain: Some(Brain { agent, call: run.into(), env, burst, ids, open }),
            fold: None,
        })
    }

    /// The brain's answer HEAD to JOB (its form, or Failed when the call failed: FAILED, said in the journal only), with
    /// the DECISION that chose its model: the decision first, then the answer, as the clerk's.
    pub(crate) fn brain_answered(&mut self, job: &crate::clerk::Job, d: &Delivery, head: Value, decision: Option<coord_types::Decision>, failed: Option<String>) {
        self.tickets.handed.remove(&(d.to.clone(), d.id.clone()));
        let head = self.from_record(job, d, head);
        if let Some(dec) = &decision {
            self.ticket_decision(job.t, job.step, dec);
        }
        let said = match &failed {
            Some(e) => format!("the call failed ({e}), nothing is said"),
            None => format!("{} answered", decision.as_ref().and_then(|d| d.chosen.as_deref()).unwrap_or("the call")),
        };
        let whose = job.brain.as_ref().map_or_else(|| format!("the brain of {}", job.to), crate::clerk::brain_whose);
        let said = match self.owner_sent(job) {
            Some(sent) => format!("{said} ({sent})"),
            None => said,
        };
        match self.ticket_answer(&job.worker, job.t, head.clone(), docket_machine::Text::New(String::new())) {
            Ok(_) => eprintln!("coordd: tickets: {whose} ({}): {said}: {head}", job.ticket),
            Err(e) => eprintln!("coordd: tickets: {whose} ({}): {said}, not taken: {e}", job.ticket),
        }
    }

    /// What the brain reads of the record for AGENT on his posts BURST (their ids), as far back as the policy says.
    fn reading(&self, agent: &str, burst: &[String]) -> Reading {
        let lines = self.earlier_lines().map_or(EARLIER, |n| usize::try_from(n).unwrap_or(usize::MAX));
        self.reading_within(agent, burst, self.unanswered_within(), self.earlier_within(), lines)
    }

    /// [`Core::reading`], OPEN read from WITHIN seconds before the burst and EARLIER from EARLIER_WITHIN, its newest CAP.
    fn reading_within(&self, agent: &str, burst: &[String], within: Option<u64>, earlier_within: Option<u64>, cap: usize) -> Reading {
        let m = &self.tickets.m;
        let in_burst = |t: u64, n: u32| burst.iter().any(|b| docket_machine::post_id(b) == Some((t, n)));
        // near the burst: its threads, the threads they hang from, and the threads hanging from them (the outline's tie,
        // else under; `Machine::outline_parent`)
        let threads: std::collections::BTreeSet<u64> = burst.iter().filter_map(|b| docket_machine::post_id(b)).map(|(t, _)| t).collect();
        let hangs = |t: u64| m.outline_parent(t).map(|(p, _)| p);
        let near = |t: u64| threads.contains(&t) || threads.iter().any(|b| hangs(*b) == Some(t)) || hangs(t).is_some_and(|p| threads.contains(&p));
        // his posts to it (those that wake it), his near the burst or to it, and its own, each by when. His are his
        // words (`Machine::his_words`): an agent's opener that thread.kind before ls-opener posted again as his is not
        // (N-1445 #2; proto-lead N-1446 p552)
        let (mut his, mut near_his, mut its, mut mine): (Vec<At>, Vec<At>, Vec<At>, Vec<At>) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (t, x) in m.tickets.range(..docket_machine::HIDDEN) {
            let close = near(*t);
            for p in x.posts.iter().filter(|p| p.visible() && p.moved.is_none()) {
                if m.his_words(*t, p.n) {
                    let a = (p.at.clone(), *t, p.n);
                    if in_burst(*t, p.n) {
                        mine.push(a);
                        continue;
                    }
                    let to_it = p.wakes().any(|w| w == agent);
                    if to_it || close {
                        near_his.push(a.clone());
                    }
                    if to_it {
                        his.push(a);
                    }
                } else if p.by == agent {
                    its.push((p.at.clone(), *t, p.n));
                }
            }
        }
        mine.sort();
        his.sort();
        near_his.sort();
        its.sort();
        // what came before the burst's first post is "before" (the record's time is to the second: within one, by
        // ticket and post); an ask of his is open while the agent has not answered it (the ledger's one rule,
        // `Machine::asked`, I-1455 (B))
        let first = mine.first().cloned();
        let is_before = |a: &At| first.as_ref().is_none_or(|f| a < f);
        let before: Vec<At> = his.iter().filter(|a| is_before(a)).cloned().collect();
        let anchor = first.as_ref().map_or_else(crate::core::now, |f| f.0.clone());
        let back = |w: Option<u64>| w.and_then(|w| secs_before(&anchor, w));
        let open: Vec<At> = match back(within) {
            Some(s) => his.iter().filter(|(at, t, n)| *at >= s && m.asked(*t, *n).iter().any(|(a, answer)| a == agent && answer.is_none())).cloned().collect(),
            None => Vec::new(),
        };
        let tail = |xs: Vec<At>, k: usize| xs[xs.len().saturating_sub(k)..].to_vec();
        let (his, open) = (tail(before, HIS), tail(open, OPEN));
        // EARLIER: his posts near the burst or to it from `earlier-within` before it, but those HIS and OPEN show
        let earlier: Vec<At> = match back(earlier_within) {
            Some(s) => near_his.into_iter().filter(|a| a.0 >= s && is_before(a) && !his.contains(a) && !open.contains(a)).collect(),
            None => Vec::new(),
        };
        let model = self.owner_model_read().and_then(|o| o.model).map(|(u, _, upto)| (u, upto));
        Reading { burst: mine, his, open, earlier: tail(earlier, cap), its: tail(its, ITS), within, earlier_within, doing: self.turn_on(agent), model }
    }

    /// The owner model's report (brain.call's `attention`), by argument: from R and nothing the machine holds now but the
    /// words of the posts R names and their threads' labels (proto-lead N-1446 p1153, docket-core2 p1155).
    fn attention_env(&self, agent: &str, r: &Reading) -> Value {
        json!({
            "agent": agent,
            "model": self.model_line(r.model.as_ref()),
            "burst": self.report_lines(&r.burst, BURST_CHARS, false, false),
            "his": self.report_lines(&r.his, 400, true, false),
            "open": self.report_lines(&r.open, 200, true, false),
            "within": r.within.map_or_else(|| String::from("none read (the policy gives no unanswered-within)"), |w| format!("from {} before these", span(w))),
            "earlier": self.report_lines(&r.earlier, EARLIER_CHARS, true, true),
            "earlier_within": r.earlier_within.map_or_else(|| String::from("none read (the policy gives no earlier-within)"), |w| format!("from {} before these", span(w))),
            "its": self.report_lines(&r.its, 160, true, false),
            "doing": r.doing,
        })
    }

    /// The base step's report (brain.call's `outlook`), by argument, for AGENT's pass T, and the ids a Next may rest on:
    /// the posts ITS, OWED and WAITING show. From the record alone, as `attention_env` is: ROLE, the registry's role and
    /// lead; TURNS, the pass's N and since when, its last look's opening (`last_look`; none before the first), since when
    /// alone for a Compacting pass, which reads this report too (brain.call's `consolidation`); ITS, its own posts since
    /// then, whole up to BURST_CHARS, the newest SINCE; OWED, the asks of its ledger with no answer (`Machine::ledger`,
    /// I-1455 (B)), his and every other asker's but its own brain's thoughts, from the policy's `unanswered-within`
    /// before now, the newest OWED (none read without it, as attention's OPEN); WAITING, what waits for it
    /// (`Machine::waiting`), by first lines; DOING, its turn (`turn_on`); LAST, the last look's decision and NOTES, which
    /// the agent never reads.
    fn outlook_env(&self, agent: &str, t: u64) -> (Value, Vec<String>) {
        let m = &self.tickets.m;
        let turns = m.tickets.get(&t).and_then(|x| x.form["pass"]["turns"].as_i64()).unwrap_or(0);
        let compacting = m.tickets.get(&t).is_some_and(|x| x.form["pass"]["head"] == "Compacting");
        let last = self.last_look(agent, t);
        let since = last.map(|x| x.opened_at.clone());
        let mut its: Vec<At> = Vec::new();
        for (u, x) in m.tickets.range(..docket_machine::HIDDEN) {
            let after = |p: &docket_machine::Post| since.as_ref().is_none_or(|s| p.at >= *s);
            its.extend(x.posts.iter().filter(|p| p.by == agent && p.visible() && p.moved.is_none() && after(p)).map(|p| (p.at.clone(), *u, p.n)));
        }
        its.sort();
        let its = its[its.len().saturating_sub(SINCE)..].to_vec();
        let within = self.unanswered_within();
        // its own brain's earlier thoughts are no asks it owes: each Next posts one naming it (brain.kind), which the
        // ledger holds until the agent posts an answer, and LAST already carries the last with its notes. Read back as
        // owed, a pass took its own thought for an ask and said it again (paper-research N-1446 p2781: B-1547.p1, four
        // passes running)
        let own = |c: &docket_machine::Commitment| {
            let x = self.post_at(&c.post).and_then(|(_, u, _)| Some((u, m.tickets.get(&u)?)));
            x.is_some_and(|(u, x)| x.kind == "brain" && x.form["agent"] == agent && c.by == m.id(u))
        };
        let owed: Vec<docket_machine::Commitment> = match within.and_then(|w| secs_before(&crate::core::now(), w)) {
            Some(from) => m.ledger(agent).into_iter().filter(|c| c.answer.is_none() && c.at >= from && !own(c)).collect(),
            None => Vec::new(),
        };
        let owed = &owed[owed.len().saturating_sub(OWED)..];
        let waiting = m.waiting(agent);
        let mut ids: Vec<String> = its.iter().map(|a| self.post_name(a)).collect();
        let mut owed_lines = Vec::new();
        for c in owed {
            let words = self.post_at(&c.post).map(|a| docket_machine::label::first_line(&self.post_words(&a), EARLIER_CHARS)).unwrap_or_default();
            owed_lines.push(format!("{} · {} · {} · {} · {words}", c.post, c.by, c.why, day_time(&c.at)));
            ids.push(c.post.clone());
        }
        let mut waiting_lines = Vec::new();
        for i in &waiting {
            let id = i["id"].as_str().unwrap_or_default();
            match self.post_at(id) {
                Some(a) => waiting_lines.push(self.report_line(&a, EARLIER_CHARS, true, true)),
                None => waiting_lines.push(format!("{id} · {} · {}", i["what"].as_str().unwrap_or_default(), i["label"].as_str().unwrap_or_default())),
            }
            if !id.is_empty() && !ids.iter().any(|x| x == id) {
                ids.push(id.to_string());
            }
        }
        let lines = |ls: Vec<String>| if ls.is_empty() { "(none)".to_string() } else { ls.join("\n") };
        let role = self.agents.get(agent).map_or_else(
            || "not in the registry".to_string(),
            |a| format!("{}, under {}", if a.role.is_empty() { "a member" } else { a.role.as_str() }, a.parent),
        );
        let model = self.owner_model_read().and_then(|o| o.model).map(|(u, _, upto)| (u, upto));
        let env = json!({
            "agent": agent,
            "model": self.model_line(model.as_ref()),
            "role": role,
            "turns": match (&since, compacting) {
                (Some(s), false) => format!("its last {turns} turns, since your last look at {} UTC", day_time(s)),
                (None, false) => format!("its last {turns} turns; this is your first look, so its posts are its newest {SINCE}"),
                (Some(s), true) => format!("since your last look at {} UTC", day_time(s)),
                (None, true) => format!("its newest {SINCE}, as this is your first look"),
            },
            "its": self.report_lines(&its, BURST_CHARS, false, true),
            "owed": lines(owed_lines),
            "within": within.map_or_else(|| String::from("none read (the policy gives no unanswered-within)"), |w| format!("from {} before now", span(w))),
            "waiting": lines(waiting_lines),
            "doing": self.turn_on(agent),
            "last": last.map_or_else(|| String::from("none: this is your first look"), look_line),
        });
        (env, ids)
    }

    /// AGENT's last look before pass T: its newest `brain` pass below T on the Ahead pass that closed with Next, Same or
    /// Stop. A failed one is passed over: it decided nothing and wrote no notes; so is a Compacting pass, which writes
    /// none.
    fn last_look(&self, agent: &str, t: u64) -> Option<&docket_machine::Ticket> {
        let decided = |x: &docket_machine::Ticket| matches!(&x.status, docket_machine::Status::Done(v) if ["Next", "Same", "Stop"].contains(&v["head"].as_str().unwrap_or_default()));
        self.tickets.m.tickets.range(..t).rev().map(|(_, x)| x).find(|x| x.kind == "brain" && x.form["agent"] == agent && x.form["pass"]["head"] == "Ahead" && decided(x))
    }

    /// AGENT's turn as the record has it, for DOING (proto-reader's A3 report at his p614, N-1446 p98): the turn running,
    /// what it was given and when, and what went into it since (`given_rows`, `into_since`), as "the turn given
    /// N-1396.p327 + N-1397.p99 at 18:59:04, running; N-1104.p614 went into it at 18:59:09"; else the last that ended,
    /// the givings read at its end.
    fn turn_on(&self, agent: &str) -> String {
        let m = &self.tickets.m;
        let hms = |at: &str| at.get(11..19).unwrap_or(at).to_string();
        let names = |gs: &[(String, String)]| gs.iter().map(|(_, d)| d.as_str()).collect::<Vec<_>>().join(" + ");
        let mut rows: Vec<(String, String)> = m.given_rows(agent).into_iter().map(|g| (g.at, g.delivery)).collect();
        if !rows.is_empty() {
            rows.sort();
            let into = m.into_since(agent);
            let (put, own): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(_, d)| into.contains(d));
            let mut s = match own.first() {
                Some((at, _)) => format!("the turn given {} at {}, running", names(&own), hms(at)),
                None => "a turn, running".to_string(),
            };
            for (at, d) in &put {
                s.push_str(&format!("; {d} went into it at {}", hms(at)));
            }
            return s;
        }
        let read: Vec<(String, String, String)> = m.tickets.values().flat_map(|x| &x.given).filter(|g| g.to == agent).filter_map(|g| g.read.clone().map(|r| (r, g.at.clone(), g.delivery.clone()))).collect();
        let Some(end) = read.iter().map(|(r, ..)| r.clone()).max() else { return format!("no turn of {agent}'s in the record") };
        let mut last: Vec<(String, String)> = read.into_iter().filter(|(r, ..)| *r == end).map(|(_, at, d)| (at, d)).collect();
        last.sort();
        let mut once = std::collections::BTreeSet::new();
        last.retain(|(_, d)| once.insert(d.clone()));
        format!("no turn running; its last, given {} at {}, ended at {}", names(&last), hms(&last[0].0), hms(&end))
    }

    /// The selector's report (brain.call's `election`), by argument, from the step's ARGS (the owner model's topic,
    /// reading, proposals and EARLIER), and the ids a pick may rest on: the candidates', each numbered, the owner model's
    /// first, then `unanswered by it` (his posts in OPEN), then `his earlier words` (the posts EARLIER named, each by its
    /// first line; the pick carries them whole, `from_record`), then nothing. With them, the ids the `unanswered by it`
    /// candidates rest on, as `Brain::open`. From R and ARGS, as `attention_env` is.
    fn election_env(&self, agent: &str, r: &Reading, args: &Value) -> (Value, Vec<String>, Vec<String>) {
        let mut lines: Vec<String> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        let mut open: Vec<String> = Vec::new();
        for p in args["proposals"].as_array().into_iter().flatten() {
            let (Some(act), Some(on)) = (p["act"].as_str(), p["rests_on"].as_str()) else { continue };
            lines.push(format!("{} · the owner model · rests on {on} · {act}", lines.len() + 1));
            ids.push(on.to_string());
        }
        for a in &r.open {
            let id = self.post_name(a);
            if ids.contains(&id) {
                continue;
            }
            let act = format!("{}which {agent} has not answered: \"{}\"", unanswered_act(&id), docket_machine::label::first_line(&self.post_words(a), 120));
            lines.push(format!("{} · unanswered by it · rests on {id} · {act}", lines.len() + 1));
            ids.push(id.clone());
            open.push(id);
        }
        let named = self.named(args);
        if let Some(last) = named.last() {
            let said: Vec<String> = named.iter().map(|a| format!("{} \"{}\"", self.post_name(a), docket_machine::label::first_line(&self.post_words(a), 80))).collect();
            lines.push(format!("{} · his earlier words · rests on {} · give {agent} his earlier words on this before it answers: {}", lines.len() + 1, self.post_name(last), said.join(" · ")));
            let new: Vec<String> = named.iter().map(|a| self.post_name(a)).filter(|id| !ids.contains(id)).collect();
            ids.extend(new);
        }
        lines.push(format!("{} · always · rests on nothing · nothing: {agent} goes on as it is", lines.len() + 1));
        ids.extend(r.burst.iter().map(|a| self.post_name(a)));
        let env = json!({
            "agent": agent,
            "model": self.model_line(r.model.as_ref()),
            "burst": self.report_lines(&r.burst, BURST_CHARS, false, false),
            "topic": args["topic"].as_str().unwrap_or_default(),
            "on_topic": if args["on_topic"] == json!(true) { "yes" } else { "no" },
            "candidates": lines.join("\n"),
        });
        (env, ids, open)
    }

    /// POSTS as the lines of a report, each [`Core::report_line`]; "(none)" for none.
    fn report_lines(&self, posts: &[At], chars: usize, first: bool, day: bool) -> String {
        if posts.is_empty() {
            return "(none)".to_string();
        }
        posts.iter().map(|a| self.report_line(a, chars, first, day)).collect::<Vec<_>>().join("\n")
    }

    /// Post A as one line of a report: its id, its thread's label, when (UTC; DAY, with its day, as 11-20 12:14), and
    /// its words cut at CHARS; FIRST, its first line only.
    fn report_line(&self, a: &At, chars: usize, first: bool, day: bool) -> String {
        let words = self.post_words(a);
        let words = if first { docket_machine::label::first_line(&words, chars) } else { crate::transcript::clip(&words, chars) };
        let when = if day { day_time(&a.0) } else { a.0.get(11..19).unwrap_or(&a.0).to_string() };
        format!("{} · {} · {when} · {words}", self.post_name(a), self.ticket_head(a.1))
    }

    /// The ids ARGS' EARLIER names (the owner model's, as its check left them) that are his posts in the record, oldest
    /// first.
    fn named(&self, args: &Value) -> Vec<At> {
        let mut xs: Vec<At> = args["earlier"].as_array().into_iter().flatten().filter_map(Value::as_str).filter_map(|id| self.his_post(id)).collect();
        xs.sort();
        xs.dedup();
        xs
    }

    /// The owner's post ID, if the record holds it, visible, and his words (`Machine::his_words`).
    fn his_post(&self, id: &str) -> Option<At> {
        let (t, n) = docket_machine::post_id(id)?;
        let p = self.tickets.m.tickets.get(&t)?.posts.iter().find(|p| p.n == n && p.visible() && self.tickets.m.his_words(t, n))?;
        Some((p.at.clone(), t, n))
    }

    /// HEAD, as the call's check left it, with what only the record can give (brain.call: a model never quotes him):
    /// `attend`'s EARLIER kept to his own posts; a pick's HIS, the words of the posts its step's EARLIER named, as he wrote
    /// them, each under its id and when, after a blank line (the post is `{{ thought }}{{ his }}`); "" when it named none.
    /// A pick of the `unanswered by it` candidate on his post, which the agent has answered since (`answered_since`), is
    /// Nothing with its scores: the pass says nothing, and the journal says why.
    fn from_record(&self, job: &crate::clerk::Job, d: &Delivery, mut head: Value) -> Value {
        let What::Step { args, .. } = &d.what else { return head };
        match (job.brain.as_ref().map(|b| b.call.as_str()), head["head"].as_str()) {
            (Some(ATTENTION), Some("Read")) => {
                let kept: Vec<Value> = head["earlier"].as_array().into_iter().flatten().filter(|id| id.as_str().and_then(|id| self.his_post(id)).is_some()).cloned().collect();
                head["earlier"] = json!(kept);
            }
            (Some(ELECTION), Some("Pick")) => {
                if let Some(answer) = job.brain.as_ref().and_then(|b| self.answered_since(b, &head)) {
                    eprintln!("coordd: tickets: the brain of {} ({}): its pick rests on {}, which {} answered with {answer} while the selector ran: nothing is said", job.to, job.ticket, head["rests_on"].as_str().unwrap_or_default(), job.to);
                    return json!({"head": "Nothing", "scores": head["scores"].clone()});
                }
                let named = self.named(&crate::render::resolved(args));
                let mut his = String::new();
                if !named.is_empty() {
                    his.push_str("\n\nHis earlier words on this, as he wrote them:");
                }
                for a in &named {
                    let words = self.post_words(a);
                    let words = match words.char_indices().nth(QUOTE_CHARS) {
                        Some((i, _)) => format!("{}…", &words[..i]),
                        None => words,
                    };
                    his.push_str(&format!("\n{} ({} UTC): \"{words}\"", self.post_name(a), day_time(&a.0)));
                }
                head["his"] = json!(his);
            }
            _ => {}
        }
        head
    }

    /// The post of B's agent that answered his post the pick HEAD rests on, when HEAD is the selector's `unanswered by
    /// it` candidate on that post (it rests on a post B showed as one, `Brain::open`, and its act begins with coordd's
    /// own words for it, `unanswered_act`), and the ledger now holds the agent's answer to it (`Machine::asked`, I-1455
    /// (B)): the agent answered it while the call ran, so the pick is stale (the read before the act, consult-3's rule
    /// for each head). Only coordd's part of the act is read, so an echo of his quoted first line with a quote mark or an
    /// ellipsis changed is that candidate too and says nothing (proto-lead N-1479 p187). Another candidate on the same
    /// post, as `his earlier words` resting on it, is not (consult-3's p136 point 4).
    fn answered_since(&self, b: &Brain, head: &Value) -> Option<String> {
        let (on, act) = (head["rests_on"].as_str()?, head["act"].as_str()?);
        (b.open.iter().any(|id| id == on) && act.starts_with(&unanswered_act(on))).then_some(())?;
        let (t, n) = docket_machine::post_id(on)?;
        self.tickets.m.asked(t, n).into_iter().find(|(a, _)| *a == b.agent).and_then(|(_, answer)| answer)
    }

    /// The project's owner model as the record holds it: its node, the newest open `owner_model` ticket (none, there is
    /// no model), and its updates read oldest first. An update's posts are its form's; it made a model when it closed
    /// with Model and a text.
    fn owner_model_read(&self) -> Option<OwnerModel> {
        let m = &self.tickets.m;
        let node = m.tickets.range(..docket_machine::HIDDEN).rev().find(|(_, x)| x.kind == OWNER_MODEL && x.is_open()).map(|(t, _)| *t)?;
        let mut o = OwnerModel { node, running: false, refused: None, model: None, sent: Default::default(), failed: None };
        for (u, x) in m.tickets.range(node..docket_machine::HIDDEN).filter(|(_, x)| x.under == Some(node) && x.kind == OWNER_UPDATE) {
            if x.is_open() {
                o.running = true;
                if let docket_machine::Status::Awaits { refused: Some((why, _)), .. } = &x.status {
                    o.refused = Some((*u, why.clone()));
                }
                continue;
            }
            let posts: Vec<At> = x.form["posts"].as_array().into_iter().flatten().filter_map(Value::as_str).filter_map(|id| self.post_at(id)).collect();
            let (Some(text), Some(newest)) = (model_of(x), posts.iter().max().cloned()) else {
                o.failed = Some((*u, posts.iter().map(|a| (a.1, a.2)).collect()));
                continue;
            };
            o.sent.extend(posts.iter().map(|a| (a.1, a.2)));
            o.model = Some((*u, text.to_string(), newest));
            o.failed = None;
        }
        Some(o)
    }

    /// The owner model as his window draws it (`coord_types::OwnerModelView`, the Map's node; proto-reader N-1446 p1212,
    /// B): what `owner_model_read` reads, by its tickets' ids and the times they closed, so the window holds no rule of
    /// its own and says the same at any age of the record. None where no node stands.
    pub(crate) fn owner_model_view(&self) -> Option<coord_types::OwnerModelView> {
        let o = self.owner_model_read()?;
        let m = &self.tickets.m;
        let closed = |u: u64| m.tickets.get(&u).and_then(|x| x.closed_at.clone()).unwrap_or_default();
        Some(coord_types::OwnerModelView {
            node: m.id(o.node),
            running: o.running,
            model: o.model.as_ref().map(|(u, text, upto)| coord_types::ModelMade { update: m.id(*u), upto: self.post_name(upto), at: closed(*u), first_line: first_line(text) }),
            failed: o.failed.as_ref().map(|(u, _)| coord_types::UpdateEnded { update: m.id(*u), at: closed(*u) }),
        })
    }

    /// His window told of the owner model when it changed (`Body::OwnerModel`, only to an app at
    /// `OWNER_MODEL_PROTOCOL`; the snapshot carries it whole): read again once the machine has changed, and told when
    /// it differs from what was last told, the first round's told in any case. A whole snapshot is not sent for it, as
    /// for the parts (`parts_round`): an update opens and closes at each burst of his.
    pub(crate) fn owner_model_round(&mut self) {
        let now = self.tickets.m.changes();
        if self.owner_model_told.as_ref().is_some_and(|(at, _)| *at == now) {
            return;
        }
        let view = self.owner_model_view();
        let changed = self.owner_model_told.as_ref().is_none_or(|(_, told)| *told != view);
        self.owner_model_told = Some((now, view.clone()));
        if changed {
            self.emit(coord_types::Body::OwnerModel(view));
        }
    }

    /// The newest model of him, for part 2 and any other reader (docket-core2's a2o check, paper-research N-1446
    /// p1108): its node's id, the model's text, and the id of the newest post its update was sent. It is a model's
    /// reading of him, never his words: whoever shows it or reads it with his says so. None while there is none, and
    /// once the node is closed.
    pub(crate) fn owner_model(&self) -> Option<(String, String, String)> {
        let o = self.owner_model_read()?;
        let (_, text, upto) = o.model?;
        Some((self.tickets.m.id(o.node), text, self.post_name(&upto)))
    }

    /// The model a pass READ (`Reading::model`), as attention and the selector are given it (brain.call's `model`): marked
    /// as the model's reading, its text the update's result on the record, its node that update's.
    fn model_line(&self, read: Option<&(u64, At)>) -> String {
        let m = &self.tickets.m;
        let made = read.and_then(|(u, upto)| {
            let x = m.tickets.get(u)?;
            Some((m.id(x.under?), model_of(x)?.to_string(), self.post_name(upto)))
        });
        match made {
            Some((node, text, upto)) => format!("the model's reading of him, not his words (owner model {node}, from his posts up to {upto}):\n{text}"),
            None => NO_MODEL.to_string(),
        }
    }

    /// An update of the owner model, opened under its node in coordd's round (`to_hand_over`, after the delivery
    /// events) by coordd, when the node is open, no update of it is, he is writing nowhere (in no thread's box,
    /// `Machine::writing`, and to no agent, `Core::writing`: the posts he is writing go in the same update), and posts
    /// of his that have reached whom they wake (`Ticket::his_shown`, his words: `Machine::his_words`), from the
    /// policy's `owner-model-within` before now, are ones no model has read; when the last update made no model, only
    /// once one comes that it was not sent. Its POSTS are those, oldest first; BUILT, whether a model was made; MISSES,
    /// 0 until the brain's predictions are checked (part 4); REBUILD_AFTER and WITHIN, the policy's. Without both of
    /// the policy's numbers none is opened. Looked for again only once the machine, those numbers, or his writing to an
    /// agent have changed (`owner_model_seen`).
    pub(crate) fn update_owner_model(&mut self) {
        if self.holding {
            return;
        }
        let (Some(within), Some(rebuild_after)) = (self.owner_model_within(), self.owner_model_rebuild_after()) else { return };
        let seen = (self.tickets.m.changes(), within, rebuild_after, self.writing.is_empty());
        if self.owner_model_seen == Some(seen) {
            return;
        }
        self.owner_model_seen = Some(seen);
        let Some(o) = self.owner_model_read() else { return };
        // it would stay open and hold every later one: it fails, and his next post opens the next (proto-reader N-1446
        // p1184, 1)
        if let Some((u, why)) = o.refused {
            let failed = json!({"head": "Failed", "why": format!("its step was answered out of shape: {why}")});
            if let Err(e) = self.ticket_answer(docket_machine::WATCHER, u, failed, docket_machine::Text::New(String::new())) {
                eprintln!("coordd: tickets: the owner model's update {} refused, not closed: {e}", self.tickets.m.id(u));
            }
            return;
        }
        if o.running || !self.tickets.m.writing.is_empty() || !self.writing.is_empty() {
            return;
        }
        // the clock's time is one the record can read; were it not, nothing would be looked for
        let Some(from) = secs_before(&crate::core::now(), within) else { return };
        let new: Vec<At> = self.his_reached(&from, |a| !o.sent.contains(&(a.1, a.2)));
        if new.is_empty() || o.failed.as_ref().is_some_and(|(_, f)| new.iter().all(|a| f.contains(&(a.1, a.2)))) {
            return;
        }
        let posts: Vec<String> = new.iter().map(|a| self.post_name(a)).collect();
        let form = json!({"posts": posts, "built": o.model.is_some(), "misses": 0, "rebuild_after": rebuild_after, "within": within});
        let open = docket_machine::Input::Open { kind: OWNER_UPDATE.into(), under: Some(o.node), opener: "coordd".into(), form, text: None, names: vec![], told: vec![] };
        if let Err(e) = self.ticket_input(open) {
            eprintln!("coordd: tickets: the owner model's update ({} of his posts) not opened: {e}", posts.len());
        }
    }

    /// His posts that have reached whom they wake, from FROM on, oldest first: shown (`Ticket::his_shown`), his words,
    /// not moved, on a ticket of the outline's (below HIDDEN). A ticket's posts are in the order they were written, so
    /// one whose last post is from before FROM is passed over unread (most of a week's record, read in every round
    /// that the machine changed in while no update is open).
    ///
    /// Each once, as his window shows it (paper-research N-1446 p1124, 1): none on a `words` ticket of his, his words
    /// being cleaned, which reach his thread as the post they were cleaned into (`Post::cleaned_from`) and which his
    /// Threads never lists (yaaihh threads.rs `listed`); what he typed is that ticket's form, never a post. Nor one with
    /// no words: his answer to a cleaning's `look` (Go, Use) is such a post (research's record holds 39, none with
    /// words), and it would open an update that reads nothing.
    ///
    /// Only those WANTED, the caller's own cut, made before a post's words are read (a `Text::Loc` post's from its
    /// file): a round passes over the posts a model has read, so it reads the words of his new posts alone, not the
    /// week's (proto-reader N-1446 p1184: on research, 655 posts and 233 KB, 105 of them files, in every round).
    fn his_reached(&self, from: &str, wanted: impl Fn(&At) -> bool) -> Vec<At> {
        let m = &self.tickets.m;
        let mut out: Vec<At> = Vec::new();
        for (t, x) in m.tickets.range(..docket_machine::HIDDEN).filter(|(_, x)| x.kind != "words" && x.posts.last().is_some_and(|p| p.at.as_str() >= from)) {
            let shown = x.his_shown();
            for p in x.posts.iter().filter(|p| shown.contains(&p.n) && p.moved.is_none() && p.at.as_str() >= from && m.his_words(*t, p.n)) {
                let a = (p.at.clone(), *t, p.n);
                if wanted(&a) && !self.post_words(&a).trim().is_empty() {
                    out.push(a);
                }
            }
        }
        out.sort();
        out
    }

    /// What the step of an update of the owner model JOB sent, for the journal beside its answer (proto-lead N-1446
    /// p1118: the cost of every rebuild can be read): "sent 2 posts, 1876 bytes", its posts of his and its words as
    /// rendered, the bytes the call was given (a call refused for length is Failed, and says why there too). None for
    /// any other job.
    pub(crate) fn owner_sent(&self, job: &crate::clerk::Job) -> Option<String> {
        let b = job.brain.as_ref().filter(|_| self.owner_run(job.t))?;
        let bytes = crate::clerk::brain_prompt(&job.places, b).map_or_else(|e| format!("its words not read: {e}"), |p| format!("{} bytes", p.len()));
        let n = b.ids.len();
        Some(format!("sent {n} {}, {bytes}", if n == 1 { "post" } else { "posts" }))
    }

    /// What a step of an update of the owner model reads (owner_model.call), for CALL with the step's ARGS: the call
    /// coordd runs, its arguments by name, and the posts of his it reads, by id. `whole`: every post of his that has reached whom it wakes from the step's
    /// WITHIN before the newest of its POSTS up to that one's second; `delta`: the newest model, the newest post it
    /// read, and his POSTS still shown. His posts in his words as the record holds them, each under its day, time, id
    /// and thread (`owner_posts`); FROM and TO, the first's and the last's day and time.
    fn owner_model_env(&self, call: &str, args: &Value) -> (&'static str, Value, Vec<String>) {
        let mut posts: Vec<At> = args["posts"].as_array().into_iter().flatten().filter_map(Value::as_str).filter_map(|id| self.his_post(id)).collect();
        posts.sort();
        posts.dedup();
        let span = |xs: &[At]| (xs.first().map_or_else(String::new, |a| week_time(&a.0)), xs.last().map_or_else(String::new, |a| week_time(&a.0)));
        if call == "whole" {
            // the newest post's time is the record's own; were it unreadable, the build reads only its POSTS
            let read: Vec<At> = match posts.last() {
                Some(newest) => match secs_before(&newest.0, args["within"].as_u64().unwrap_or(0)) {
                    Some(from) => self.his_reached(&from, |a| a.0 <= newest.0),
                    None => posts.clone(),
                },
                None => Vec::new(),
            };
            let (from, to) = span(&read);
            let ids = read.iter().map(|a| self.post_name(a)).collect();
            return (OWNER_WHOLE, json!({"from": from, "to": to, "posts": self.owner_posts(&read)}), ids);
        }
        let (model, upto) = self.owner_model_read().and_then(|o| o.model).map_or_else(|| (String::from("(none)"), String::from("(none)")), |(_, text, u)| (text, self.post_name(&u)));
        let (from, to) = span(&posts);
        let ids = posts.iter().map(|a| self.post_name(a)).collect();
        (OWNER_DELTA, json!({"upto": upto, "model": model, "from": from, "to": to, "posts": self.owner_posts(&posts)}), ids)
    }

    /// POSTS as the owner model reads them (as paper-research-brain's run of 21 Nov, fundamentals/brain-run-21nov
    /// build.py, did): each `## Sat 21 Nov 11:50, N-1104.p716 (its thread)`, then his words whole, the lines starting
    /// with > the passages he answered, a blank line between posts; one with no words is left out; "(none)" for none.
    fn owner_posts(&self, posts: &[At]) -> String {
        let blocks: Vec<String> = posts
            .iter()
            .filter_map(|a| {
                let words = self.post_words(a);
                let words = words.trim();
                (!words.is_empty()).then(|| format!("## {}, {} ({})\n{words}\n", week_time(&a.0), self.post_name(a), self.ticket_head(a.1)))
            })
            .collect();
        if blocks.is_empty() { "(none)".to_string() } else { blocks.join("\n") }
    }

    /// Post ID's when, ticket and number, if the record holds it.
    fn post_at(&self, id: &str) -> Option<At> {
        let (t, n) = docket_machine::post_id(id)?;
        let p = self.tickets.m.tickets.get(&t)?.posts.iter().find(|p| p.n == n)?;
        Some((p.at.clone(), t, n))
    }

    /// Post A's id, as N-1104.p586.
    fn post_name(&self, a: &At) -> String {
        format!("{}.p{}", self.tickets.m.id(a.1), a.2)
    }

    /// Post A's words, as the record holds them.
    fn post_words(&self, a: &At) -> String {
        self.tickets.m.tickets.get(&a.1).and_then(|x| x.posts.iter().find(|p| p.n == a.2)).and_then(|p| crate::label::read(&p.text)).unwrap_or_default()
    }
}

/// Look X, the last (`Core::last_look`), as the base step's LAST reads it: when, what it decided, what it told the
/// agent to stop (a Next's or a Stop's `stop`; none on a look from before it), and its notes.
fn look_line(x: &docket_machine::Ticket) -> String {
    let docket_machine::Status::Done(v) = &x.status else { return String::new() };
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let notes = if s("notes").is_empty() { "(none)".to_string() } else { s("notes") };
    let stop = if s("stop").is_empty() { String::new() } else { format!("; and to stop: \"{}\"", s("stop")) };
    let decided = match v["head"].as_str() {
        Some("Next") => format!("you had it act: {} (resting on {}), and told it: \"{}\"{stop}", s("act"), s("rests_on"), s("thought")),
        Some("Stop") => format!("you had it go on as it was, and told it only to stop: \"{}\"", s("stop")),
        _ => "you said nothing: it went on as it was".to_string(),
    };
    format!("at {} UTC {decided}.\nYour notes then:\n{notes}", day_time(&x.opened_at))
}

/// The start of the act of the `unanswered by it` candidate on his post ID: coordd's own words, before his quoted first
/// line (`election_env`), which a pick may echo inexactly (`answered_since`).
fn unanswered_act(id: &str) -> String {
    format!("answer his {id}, ")
}

/// The model update X made, as the record holds it: its text, when it closed Model with a text not blank. The one rule
/// for what an update made, read by the round (`Core::owner_model_read`) and by a render (`Core::model_line`), so a
/// render from a pass's `decided` line takes a model as the round did (consult-3 N-1446 p1179).
fn model_of(x: &docket_machine::Ticket) -> Option<&str> {
    match &x.status {
        docket_machine::Status::Done(v) if v["head"] == "Model" => v["text"].as_str().filter(|w| !w.trim().is_empty()),
        _ => None,
    }
}

/// The first line of a model's TEXT that says something, as his window's inspector shows it after "a model's reading
/// of you, not your words" (paper-research N-1446 p1209; proto-window p1227: the call asks for free text, which may
/// open with a heading or a blank line): blank lines and headings passed over, a list's marker and bold's taken off; the
/// first line with words when every one is a heading. The window only cuts it to fit.
fn first_line(text: &str) -> String {
    let lines = || text.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines().find(|l| !l.starts_with('#')).or_else(|| lines().next()).unwrap_or_default();
    let line = line.trim_start_matches('#').trim_start();
    let line = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).unwrap_or(line);
    line.replace("**", "").trim().to_string()
}

/// AT (the record's `2026-11-21T11:50:03Z`) as the owner model reads a time: `Sat 21 Nov 11:50` (UTC).
fn week_time(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at).map_or_else(|_| at.to_string(), |t| t.to_utc().format("%a %d %b %H:%M").to_string())
}

/// The record's time SECS seconds before AT; none when AT cannot be read, and each caller says what that means (the
/// reading's OPEN and EARLIER read nothing; an update is not looked for; a whole build reads only its posts).
fn secs_before(at: &str, secs: u64) -> Option<String> {
    crate::transcript::epoch_ms(at).map(|ms| crate::core::at_secs(ms / 1000 - secs as i64))
}

/// AT (the record's `2026-11-20T12:14:03Z`) as the report says a time with its day: `11-20 12:14`.
fn day_time(at: &str) -> String {
    at.get(5..16).map_or_else(|| at.to_string(), |s| s.replacen('T', " ", 1))
}

/// S seconds as the report says them: "the 7 days", "the 12 hours", "the 90 minutes", "the hour".
fn span(s: u64) -> String {
    let (n, unit) = if s > 0 && s % 86400 == 0 { (s / 86400, "day") } else if s > 0 && s % 3600 == 0 { (s / 3600, "hour") } else if s > 0 && s % 60 == 0 { (s / 60, "minute") } else { (s, "second") };
    if n == 1 { format!("the {unit}") } else { format!("the {n} {unit}s") }
}

#[cfg(test)]
#[path = "tests/brain.rs"]
pub(crate) mod tests;
