//! The message runtime's coordd half (0.2 head 2, `runtime-design.md`; the owner, 20 Nov: "call a decision llm dont
//! hardcode the rules"). coordd raises the delivery events from what it knows of each agent, hands each message the
//! program's `deliver` makes as one message, with its digest at its foot, and answers a run's `next` on the text
//! before with no call (I-1480). What reaches an agent, when, how many at once, and whether into its turn, is the
//! program's (`agent-base/kinds/delivery.kind`): nothing here chooses.
//!
//! - `arrived`: an agent working, starting or compacting, or with a message on its way to it, has something new for
//!   it (not kept, reported, being handed, or raised already) that goes to it now (`push`, I-1480: what is pulled
//!   wakes nothing, and comes with its next message by its first line).
//! - `free`: an agent idle, with no decision open, nothing being handed and nothing given since its last turn ended,
//!   when it turned idle (its turn, a start or a compaction ended, or the owner stopped writing to it) with something
//!   waiting that goes to it now, or when something new for it that does comes while it is idle. An owner's post that
//!   comes while a run on the text before awaits its `next` raises `free` too: the machine ends that run, and his post
//!   goes first.
//! - `stopped`: an agent that cannot take anything now (not running, out of its account's limit, ended) has something
//!   new for it that goes to it now.
//!
//! The agent's state is read here as `Class` (docket-core5's `agent_state`, core5-a1, replaces it when it merges).

use super::*;
use docket_machine::{Delivery, Input, What};

/// The policy's act that says a copy of an agent answers a post written to it while it works (fresh-lead-3 N-1104
/// p1154: "the one line you can change"): actor the post's author, recipient the agent. A project declares it, with
/// its `Holds when` line, in a policy file of its own (research's by rule 6, paper-research N-1446 p3020, 1);
/// agent-base declares none, so where no project does, no copy is made.
pub const COPY_ACT: &str = "copy-answers";

/// What coordd keeps of an agent's delivery between rounds. Not journalled: after a start every agent is new here, so
/// one found idle with something waiting is raised `free` once (what was kept comes back by replay).
#[derive(Default, Debug)]
pub struct Seen {
    /// What the agent was at the last round, None before the first.
    class: Option<Class>,
    /// A `free` is owed: it turned idle, or a turn of its ended since the last round.
    owed: bool,
    /// The turn whose end was taken at the last round (`Agent::ended_turn`), None before the first.
    ended: Option<u64>,
    /// The items an `arrived` or `stopped` was raised for, or an owner's post a `free` was raised for while a decision
    /// was open: not raised again while they wait.
    raised: BTreeSet<String>,
    /// What was fresh when the last `free` while idle was raised: one is raised again only for something not in it,
    /// and in the turn after it none of it is `arrived` (it was decided at that `free`).
    freed: BTreeSet<String>,
    /// The owner was writing to it at the last round (`Request::Writing`).
    writing: bool,
    /// What its last turn was on, for `free`'s `doing` (`desk_turn_ended`).
    pub doing: String,
    /// Its last turn's work, for `free`'s `place` and `since` (`desk_turn_ended`): the delivery it began with
    /// (`N-1446.p2059`), and how far its session ran on past that delivery's `given` place by the turn's end.
    pub place: String,
    pub since: String,
    /// Its last turn's last message, where its session has it (`desk_turn_ended`'s END): what a `fork` from `place`
    /// hands the copy as its work's result (`Core::fork_now`).
    pub said: Option<docket_machine::Loc>,
    /// What an `arrived` was raised for while it worked and still waits: what goes at its next `free` came while it
    /// worked, so `free` says `worked` (paper-research N-1446 p2092; proto-mind N-1511 p17: one `fold` call for it).
    /// So did what waited at its last `free` and waits still when a turn of its ends (`listed`).
    came: BTreeSet<String>,
    /// What woke it at its last `free`: what that `free` gave went, and what it named at the foot of what it gave (his
    /// posts first and alone, its own step, or fold's `rest`) or kept waits on. Whatever of it waits still when a turn
    /// of its ends waited while it worked, and comes into `came` there (`desk_turn_ended`), so the `free` at the end of
    /// the turn that answered him says `worked` and `fold` orders what was named at his posts' foot (fresh-lead-3
    /// N-1540 p5, point 2; the owner, N-1104 p695: "a working agent gets one call at its turn's end for everything
    /// waiting"). A `free` with no turn after it (a `Rest`, his writing) leaves `came` as it was.
    listed: BTreeSet<String>,
    /// Its copy (`fork … at`, N-1104 p1154) was still answering at the last round (`Core::copy_answering`): when it
    /// stops, a `free` is owed, so what its delivery kept while the copy answered goes then.
    copy_answering: bool,
    /// What its events were last decided on: while it stays, the round passes it by (`Facts`).
    read: Option<Facts>,
    /// How many times its events were decided (what waits for it read, `raise_for`).
    pub reads: u64,
}

/// What an agent's delivery events are decided on besides what [`Seen`] keeps. While these stay as they were at its
/// last read, that read raised nothing (an event raised changes the machine), and a read now would raise nothing
/// either: the round passes it by (paper-research N-1397.p38: build 31's round read every agent's due and built what
/// waits for it, each second, and held research's lock about 170 ms of each with nothing new). Its own part of the
/// machine ([`Mine`]) is read again only once the machine has changed (its count of changes, every write to the
/// tickets, their posts and the desks), and an agent the change left as it was is passed by too (N-1397.p58: a round
/// after a change read every agent, about 5 s of the lock a minute). Who is above it is not among them: it is only
/// told in an event.
#[derive(Clone, Debug, PartialEq)]
struct Facts {
    /// The machine's count of changes when `mine` was read.
    changes: u64,
    quick: Quick,
    mine: Mine,
}

/// The facts read every round, the machine aside: its class, the owner writing to it, the cut-over's hold, a `free`
/// owed (set outside the round too: the owner's Release, `desk_released`), and what of its own is on its way to its
/// session (`Tickets::handed`: each delivery, its ticket and how far).
#[derive(Clone, Debug, PartialEq)]
struct Quick {
    class: Class,
    writing: bool,
    only_his: bool,
    owed: bool,
    handed: Vec<(String, u64, u32)>,
    /// Its copy is still answering (`Core::copy_answering`): its own turn and mail, none of this agent's machine.
    copy_answering: bool,
}

/// What of the machine is an agent's own, as its events are decided on (`raise_for`): what is due to it, each delivery
/// as what waits reads it (`waiting_held`: its id, whether it only informs it, whether it is the owner's take-back, and
/// each of its posts, [`PostFact`]); each item kept from it, with why, and a kept post as it shows now (below its read
/// mark it waits by its keep alone); what is reported; the items of its events queued or running, the events queued,
/// and whether a decision is open; the program's messages still to go, and whether one goes now (`hand_now`, which
/// reads what of it is on its way too, so it is read again when that changes as well); and what it was given since
/// its turn ended.
#[derive(Clone, Debug, PartialEq)]
struct Mine {
    due: Vec<(String, bool, bool, Vec<PostFact>)>,
    kept: Vec<(String, String, Option<PostFact>)>,
    reported: Vec<String>,
    raised: BTreeSet<String>,
    queue: Vec<String>,
    deciding: bool,
    hands: Vec<docket_machine::machine::Hand>,
    to_hand: bool,
    given: Vec<String>,
}

/// A post as what waits for an agent reads it: its number, whether it wakes the agent, whether it informs it, and
/// whether it is the owner's.
type PostFact = (u32, bool, bool, bool);

fn post_fact(p: &docket_machine::Post, agent: &str) -> PostFact {
    (p.n, p.wakes().any(|a| a == agent), p.inform.iter().any(|a| a == agent), p.by == "owner")
}

/// `fold`'s `doing` for an agent no turn of which has ended since coordd started: what it was on is not known.
pub(crate) const NOT_KNOWN: &str = "not known (no turn of it has ended since coordd started)";

/// The lines of WORDS that name AGENT (`docket_machine::machine::names_agent`, with or without the `@`), each as
/// `first_line` cuts it, to about 160 characters, in order: what `fold`'s line quotes of a post for its agent.
fn naming_lines(words: &str, agent: &str) -> Vec<String> {
    words.lines().filter(|l| docket_machine::machine::names_agent(l, agent, true)).map(|l| docket_machine::label::first_line(l, 160)).filter(|l| !l.is_empty()).collect()
}

/// What `fold_answered`'s journal line says of the `fold` call: who decided, or why it failed, and how long it took
/// from its start to its answer (TOOK; none when it is answered in the round, in tests). Every turn end that had an
/// arrival waits on that call before its agent gets its mail, so the journal shows what each wait cost
/// (paper-research N-1446 p2115).
pub(crate) fn fold_said(model: Option<&str>, failed: Option<&str>, took: Option<std::time::Duration>) -> String {
    let took = took.map(|d| format!(" in {:.1} s", d.as_secs_f64())).unwrap_or_default();
    match failed {
        Some(e) => format!("the call failed ({e}){took}, everything waiting goes"),
        None => format!("{} decided{took}", model.unwrap_or("the call")),
    }
}

/// `free`'s `since` (syntax.rs FREE): BYTES of an agent's session past where its work was given, as its reader reads
/// it: kilobytes, and about how many tokens that is (a session's bytes are its JSON lines, about four to a token).
fn since_text(bytes: u64) -> String {
    format!("{} KB of its session (about {}k tokens)", bytes / 1024, bytes / 4 / 1000)
}

/// An agent as the delivery events read it (runtime-design §1).
#[derive(Clone, Debug, PartialEq)]
enum Class {
    /// In a turn, or compacting.
    Working,
    /// Starting, restarting, its turn's end not yet in its transcript, an esc stopping it, moving to another view, or
    /// waiting to be handed again what its session did not take.
    Starting,
    /// A message on its way to its session.
    Handing,
    Idle,
    /// Cannot take anything now, and why.
    Down(String),
}

/// A message of the delivery program's for an agent that goes now (`Core::hand_now`).
#[derive(Debug, Default)]
pub(crate) struct Going {
    /// The deliveries it carries, as one message.
    pub items: Vec<Delivery>,
    /// What it names at its foot, waiting on (`deliver … listing REST`), by id.
    pub rest: Vec<String>,
    /// What goes with it by first lines (`deliver … digest PULL`, I-1480), by id, as its foot draws it
    /// ([`Core::digest_named`]).
    pub digest: Vec<String>,
    /// The deliveries that give DIGEST with the message, so the read mark passes what its foot lists (N-1481 p26): one
    /// a thread, up to its last post of DIGEST, as ITEMS's are made (`hand_deliveries`).
    pub given: Vec<Delivery>,
    /// A `fork … at PLACE` made it (`Hand::fork`, part 4): PLACE, the delivery whose `given` line its session is cut
    /// after before it goes ([`Core::fork_now`]).
    pub fork: Option<String>,
}

/// What a `fork` came to for an agent (`Core::forked`, part 4): PLACE, the delivery its session was to go on from, and
/// WORDS, what is handed ahead of the message, in coord's words, once its session is the cut copy; None when it was
/// not forked (not Claude Code, the place not found, the restart failed), which coordd's log says, and the message
/// goes as a `deliver`'s.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Forked {
    pub place: String,
    pub words: Option<String>,
}

/// What [`Core::fork_now`] answers: restart NAME on its cut copy first, handing nothing now; or hand the message now,
/// with these words ahead of it, if any.
#[derive(Debug, PartialEq)]
pub(crate) enum ForkNow {
    Restart,
    Hand(Option<String>),
}

impl Core {
    /// Whether the delivery program decides what A is handed: a harness's session (Claude Code, Codex or opencode,
    /// `harness`), with `kind delivery` loaded. Without the program an agent is handed everything due, out of a turn.
    pub(crate) fn runs_delivery(&self, a: &Agent) -> bool {
        harness(&a.kind) && self.tickets.m.delivery_loaded()
    }

    /// NAME's class, read from [`Core::agent_state`], the one answer of what it can take (consult-3 N-1396.p107: no
    /// second list of facts). Compacting is working (the owner: "yes just like if an agent is working"); an idle agent
    /// with a message on its way to its session is handing.
    fn desk_class(&self, name: &str) -> Class {
        match self.agent_state(name) {
            None => Class::Down(String::from("not known to coordd")),
            Some(AgentState::Stopped { why }) => Class::Down(why),
            Some(AgentState::OutOfLimit { .. }) => Class::Down(String::from("out of its account's limit")),
            Some(AgentState::Starting { .. }) => Class::Starting,
            Some(AgentState::Working | AgentState::Compacting { .. }) => Class::Working,
            Some(AgentState::Idle) if self.inflight.contains_key(name) => Class::Handing,
            Some(AgentState::Idle) => Class::Idle,
        }
    }

    /// Raise the delivery events owed now, for every agent the program decides for (the hand-over round, before
    /// `to_hand_over`). Nothing while the owner holds every hand-over (`coord hold`); while `$COORD/hold` is there,
    /// only for the owner's posts (`due_now`: the cut-over lets his through and nothing else).
    pub fn raise_delivery(&mut self) {
        self.threads_writing_ended();
        if self.holding || !self.tickets.m.delivery_loaded() {
            return;
        }
        let only_his = self.hold().is_some();
        self.writing.retain(|_, p| alive(*p));
        // a step's short-lived agent that has stopped is none of the program's: what is due to it is dropped
        // (`ticket_drop_ended`)
        let names: Vec<String> = self.agents.values().filter(|a| self.runs_delivery(a) && !(is_fresh(&a.name) && !a.running())).map(|a| a.name.clone()).collect();
        // every agent's due read in one pass, kept until an event changes the machine (`Tickets::machine_due`); not
        // while the machine is as it was when each agent's own part of it was read (`Facts`)
        let changes = self.tickets.m.changes();
        if names.iter().any(|n| self.desk_seen.get(n).and_then(|s| s.read.as_ref()).is_none_or(|f| f.changes != changes)) {
            self.tickets.with_due(&names, |_| ());
        }
        for name in names {
            let class = self.desk_class(&name);
            let writing = self.writing.contains_key(&name);
            let ended = self.agents[&name].ended_turn;
            let mut handed: Vec<(String, u64, u32)> = self.tickets.handed.iter().filter(|((a, _), _)| *a == name).map(|((_, d), (t, n, _))| (d.clone(), *t, *n)).collect();
            handed.sort();
            let copy_answering = self.copy_answering(&name);
            let seen = self.desk_seen.entry(name.clone()).or_default();
            // a turn's end taken since the last round owes a `free` as its turning idle does, also for a turn that
            // began and ended between two rounds, which no round saw working (one `follow` pass; proto-engine N-1481
            // p162, proto-lead p164); a new process's none taken yet (u64::MAX) is no end
            if seen.ended.is_some_and(|e| e != ended) && ended != u64::MAX {
                seen.owed = true;
            }
            seen.ended = Some(ended);
            if seen.class.as_ref() != Some(&class) {
                match &class {
                    // a message taken that has not yet begun its turn is no end of one
                    Class::Idle if seen.class != Some(Class::Handing) => seen.owed = true,
                    // what waited at its last `free` stays decided there: no `arrived` for it in the turn (`raise_for`)
                    Class::Working => seen.owed = false,
                    _ => {}
                }
                seen.class = Some(class.clone());
            }
            if seen.writing && !writing {
                seen.owed = true;
            }
            seen.writing = writing;
            // its copy done answering: what its delivery kept meanwhile goes now (N-1104 p1154's "it finishes first")
            if seen.copy_answering && !copy_answering {
                seen.owed = true;
            }
            seen.copy_answering = copy_answering;
            let quick = Quick { class: class.clone(), writing, only_his, owed: seen.owed, handed, copy_answering };
            let last = seen.read.take();
            // its own part of the machine, read again only once the machine has changed (an event raised for an agent
            // before it in this round changes it too) or what of it is on its way has (`Mine::to_hand`)
            let changes = self.tickets.m.changes();
            let (same, mine) = match last {
                Some(f) if f.changes == changes && f.quick.handed == quick.handed => (f.quick == quick, f.mine),
                last => {
                    let mine = self.desk_mine(&name);
                    (last.is_some_and(|f| f.quick == quick && f.mine == mine), mine)
                }
            };
            let seen = self.desk_seen.get_mut(&name).expect("seen this round");
            seen.read = Some(Facts { changes, quick, mine });
            if same {
                continue;
            }
            seen.reads += 1;
            self.raise_for(&name, class, only_his, writing);
        }
        #[cfg(test)]
        self.fold_inline();
        // a run on the text before that awaits the watcher's `next` (one raised now, or open at the install): answered
        // in this round, with no call (`next_given`)
        if self.agents.keys().any(|a| self.tickets.m.deciding(a)) {
            let runs: Vec<Delivery> = self.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| self.tickets.m.run_of(d.t).is_some()).collect();
            for d in runs {
                self.next_given(&d);
            }
        }
    }

    /// In tests: how many times each agent's events were decided (`Seen::reads`), for the cost harness's change rounds.
    #[cfg(test)]
    pub(crate) fn desk_reads(&self) -> BTreeMap<String, u64> {
        self.desk_seen.iter().map(|(a, s)| (a.clone(), s.reads)).collect()
    }

    /// NAME's own part of the machine now ([`Mine`]), its due read where `Tickets::machine_due` keeps it.
    fn desk_mine(&self, name: &str) -> Mine {
        let m = &self.tickets.m;
        let of = |d: &Delivery| {
            let posts: Vec<PostFact> = match &d.what {
                What::Posts { unread } => unread.iter().map(|p| post_fact(p, name)).collect(),
                _ => Vec::new(),
            };
            (d.id.clone(), d.inform, matches!(&d.what, What::TakenBack { by, .. } if by == "owner"), posts)
        };
        let due = self.tickets.with_due(&[name.to_string()], |all| all.get(name).into_iter().flatten().map(of).collect());
        let shown = |id: &str| {
            let (t, n) = docket_machine::post_id(id)?;
            m.tickets.get(&t)?.posts.iter().find(|p| p.n == n && p.visible() && p.moved.is_none()).map(|p| post_fact(p, name))
        };
        let kept = m.kept(name).into_iter().map(|(i, why)| {
            let post = shown(&i);
            (i, why, post)
        });
        Mine {
            due,
            kept: kept.collect(),
            reported: m.reported(name).into_keys().collect(),
            raised: m.raised_items(name),
            queue: m.delivery_queue(name),
            deciding: m.deciding(name),
            hands: m.hands(name),
            to_hand: !self.hand_now(name, false).items.is_empty(),
            given: m.given_since(name),
        }
    }

    /// The events owed for NAME, of CLASS, now.
    fn raise_for(&mut self, name: &str, class: Class, only_his: bool, writing: bool) {
        let m = &self.tickets.m;
        // what is due read from the round's one pass over the tickets (`Tickets::due_all`), not once more per agent
        let waiting = m.waiting_of(name, self.tickets.due_whole(name));
        let (kept, reported, raised) = (m.kept(name), m.reported(name), m.raised_items(name));
        let id = |i: &Value| i.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        let flag = |i: &Value, f: &str| i.get(f) == Some(&json!(true));
        // what wakes it: what goes to it now (`push`, I-1480 rule 1: his post, a step, a notice, words given, an ask,
        // read or blocker for it, an `@` its writer typed, a hold on an item it works on, an answer to its ask, a result
        // it waits on), never what is pulled, given at its next message by its first line (rule 2): a wake for that
        // alone would run `free` for nothing (proto-engine N-1481 p94, 1); under the cut-over's hold, only the owner's
        let wakes = |i: &Value| flag(i, "push") && (!only_his || flag(i, "owner"));
        // its brain's line (`into_turn`, a program's `post … informing`; `free`'s `told`): never into a running turn,
        // whole at the next message; to an idle agent it goes at once, so its `free` is raised as for what wakes it
        // (fresh-lead-3 N-1540 p5, p18: the line steers the next turn, and would wait for a message that may not come)
        let told = |i: &Value| !only_his && flag(i, "into_turn") && !flag(i, "push");
        let ids: BTreeSet<String> = waiting.iter().map(id).collect();
        // a message of the program's still to go; one whose items are none of them due any more holds nothing back
        let to_hand = !self.hand_now(name, false).items.is_empty();
        // the agents above it in the team's tree (`Core::above`, as the policy's `above` facts are drawn): each event
        // names them, and a post of theirs is a lead's to the program (`lead`; consult-3 N-1397.p13, 1)
        let above: Vec<String> = self.agents.keys().filter(|a| self.above(&Who::Agent((*a).clone()), name)).cloned().collect();
        // fold's Fork can be made at its work's place now (`fork_place`), for `free`'s `forkable`
        let forkable = class == Class::Idle && self.desk_seen.get(name).is_some_and(|s| self.fork_place(name, &s.place).is_ok());
        // the copy's facts (syntax.rs ARRIVED and FREE, N-1104 p1154): in a turn, where it began and whether a copy can
        // be cut there now, and of his posts that wake it, those the policy says a copy answers; at a `free`, whether
        // its copy still answers
        // only while its turn runs: where it began is its `turn_began` line's (the machine's `began`)
        let working = class == Class::Working;
        let began = if working { self.tickets.m.began(name).unwrap_or_default() } else { String::new() };
        let cuttable = working && !began.is_empty() && self.fork_place(name, &began).is_ok();
        // the policy asked once a post, as its `arrived` is raised: not again for one raised before
        let before = self.desk_seen.get(name).map(|s| s.raised.clone()).unwrap_or_default();
        let his_new = |i: &&Value| wakes(*i) && flag(*i, "owner") && i.get("what").and_then(Value::as_str) == Some("post") && !before.contains(&id(*i));
        // a copy only of the one agent his post is written to (N-1104 p1206: twelve copies at 06:57, one of every busy
        // agent a post of his reached; the owner's N-1794: "fork only the specific target agent"): a post that names
        // this agent and no other, not one that reaches it as one on its thread or names several
        let to_it_alone = |i: &&Value| i.get("id").and_then(Value::as_str).is_some_and(|i| self.written_to_alone(i, name));
        let copied: BTreeSet<String> = if working { waiting.iter().filter(his_new).filter(to_it_alone).filter(|i| self.copy_answers(name, i.get("from").and_then(Value::as_str).unwrap_or_default())).map(id).collect() } else { BTreeSet::new() };
        let answering = self.copy_answering(name);
        let seen = self.desk_seen.get_mut(name).expect("seen this round");
        // a kept item is decided by the keep: once a keep is lifted while it still waits (the owner's Release, line
        // 327), it is new again, and raised as anything new is
        seen.raised.retain(|i| ids.contains(i) && !kept.contains_key(i));
        seen.freed.retain(|i| ids.contains(i) && !kept.contains_key(i));
        seen.came.retain(|i| ids.contains(i));
        let new_of = |go: &dyn Fn(&Value) -> bool| -> Vec<String> { waiting.iter().filter(|i| go(i)).map(id).filter(|i| !kept.contains_key(i) && !reported.contains_key(i) && !raised.contains(i)).collect() };
        // while it works only what wakes it arrives: an item that only informs it waits for `free`, its brain's line
        // too, though its post asks into the turn and is about what the turn holds (76; on the text before, `arrived`
        // put that in), and one that policy only informs it of, as in build 36 (proto-lead N-1446 p173)
        let fresh = new_of(&wakes);
        // what an idle agent is given at once: what wakes it, and its brain's line
        let goes = |i: &Value| wakes(i) || told(i);
        let fresh_idle = new_of(&goes);
        let mut events: Vec<(&str, Value)> = Vec::new();
        match &class {
            Class::Working | Class::Starting | Class::Handing => {
                // what waited at its last `free` was decided there (given, or named at the foot of what was given):
                // it did not come while it works, and a lead's post among it does not go into the turn after all
                let new: Vec<String> = fresh.iter().filter(|i| !seen.raised.contains(*i) && !seen.freed.contains(*i)).cloned().collect();
                for i in new {
                    events.push((docket_machine::syntax::ARRIVED, json!({"agent": name, "item": i, "above": above, "forkable": cuttable, "copy_answers": copied.contains(&i)})));
                    seen.came.insert(i.clone());
                    seen.raised.insert(i);
                }
            }
            Class::Down(why) => {
                let new: Vec<String> = fresh.iter().filter(|i| !seen.raised.contains(*i)).cloned().collect();
                for i in new {
                    events.push((docket_machine::syntax::STOPPED, json!({"agent": name, "item": i, "why": why, "above": above})));
                    seen.raised.insert(i);
                }
            }
            Class::Idle => {
                // fold's facts (syntax.rs FREE): what goes now came while it worked; its work's place, and how far its
                // session ran on past it. A `free` leaves `came` as it was: it loses only what waits no more (above), so
                // what a `free` kept with no turn after it (a `Rest`, his writing) still came while it worked at the
                // next, and one call decides it (docket-core N-1446 p2534, 2; on 75 that `free` cleared it, and the pile
                // went with no call)
                let worked = waiting.iter().any(|i| wakes(i) && seen.came.contains(&id(i)));
                let value = json!({"agent": name, "doing": seen.doing, "writing": writing, "above": above, "worked": worked, "since": seen.since, "place": seen.place, "forkable": forkable, "answering": answering});
                if m.deciding(name) || !m.delivery_queue(name).is_empty() {
                    // his post does not wait behind a decision: the machine ends the run it was not offered to
                    let his: Vec<String> = waiting.iter().filter(|i| flag(i, "owner") && wakes(i)).map(id).filter(|i| fresh.contains(i) && !seen.raised.contains(i)).collect();
                    if !his.is_empty() {
                        seen.raised.extend(his);
                        seen.listed = waiting.iter().filter(|i| wakes(i)).map(id).collect();
                        events.push((docket_machine::syntax::FREE, value));
                    }
                } else if !to_hand && m.given_since(name).is_empty() {
                    if !waiting.iter().any(goes) {
                        seen.owed = false;
                    } else if std::mem::take(&mut seen.owed) || fresh_idle.iter().any(|i| !seen.freed.contains(i)) {
                        seen.freed = fresh_idle.iter().cloned().collect();
                        seen.listed = waiting.iter().filter(|i| wakes(i)).map(id).collect();
                        events.push((docket_machine::syntax::FREE, value));
                    }
                }
            }
        }
        for (event, value) in events {
            if let Err(e) = self.ticket_input(Input::Event { name: event.to_string(), value }) {
                eprintln!("coordd: tickets: {event} for {name}: {e}");
            }
        }
    }

    /// NAME's turn ended at END (its last message's place in its session), given GIVEN: what it was on, for its next
    /// `free`'s `doing`; and its work, the delivery the turn began with as its end line names it (the machine's
    /// `turn_delivery`: its step, else its `turn_began` line's), else the first of GIVEN, for `place`, with how far the
    /// session ran on past where that work was given (its `given` line's place) to END, for `since`: "-" when the
    /// session is not the one it was given in, or it was given nowhere coordd saw. Not the first of GIVEN alone, which
    /// is sorted as text: a digest's thread numbered lower came first, given nowhere in the session, so `since` was "-"
    /// and `forkable` false at the next `free` (112 of the 516 `free` lines with a place in research's record since
    /// `forkable` came, each a place never given in the session; L10, paper-research N-1446 p3273).
    pub(crate) fn desk_turn_ended(&mut self, name: &str, given: &[String], end: &docket_machine::Loc) {
        if !self.tickets.m.delivery_loaded() {
            return;
        }
        let place = self.tickets.m.turn_delivery(name, given).or_else(|| given.first().cloned()).unwrap_or_default();
        let at = self.given_place(name, &place);
        let since = match at {
            Some(w) if w.file == end.file && end.offset + end.len >= w.offset + w.len => since_text(end.offset + end.len - (w.offset + w.len)),
            _ => String::from("-"),
        };
        let seen = self.desk_seen.entry(name.to_string()).or_default();
        (seen.place, seen.since, seen.said) = (place, since, Some(end.clone()));
        // what its last `free` left waiting waited while it worked (`Seen::listed`); `raise_for` drops what has gone
        let listed = std::mem::take(&mut seen.listed);
        seen.came.extend(listed);
        let mut threads: Vec<u64> = Vec::new();
        for (t, _) in given.iter().filter_map(|d| docket_machine::parse_delivery(d)) {
            if !threads.contains(&t) {
                threads.push(t);
            }
        }
        let on: Vec<String> = threads.into_iter().map(|t| format!("#{} · {}", self.tickets.m.id(t), self.ticket_head(t))).collect();
        self.desk_seen.entry(name.to_string()).or_default().doing = if on.is_empty() { String::from("nothing from a thread") } else { on.join("; ") };
    }

    /// The owner's Release (line 327): what the hold kept, by agent, before Release lifts it (each item kept with WHY,
    /// `MailHold::why`). Read before the `mail_hold` line, for [`Core::desk_released`].
    pub(crate) fn desk_held(&self, why: &str) -> Vec<(String, Vec<String>)> {
        self.agents
            .keys()
            .map(|a| (a.clone(), self.tickets.m.kept(a).into_iter().filter(|(_, w)| w == why).map(|(i, _)| i).collect::<Vec<String>>()))
            .filter(|(_, items)| !items.is_empty())
            .collect()
    }

    /// The owner's Release (line 327), HELD as [`Core::desk_held`] read it before: an agent one of whose items is
    /// still kept after it is owed a `free`, as when it turned idle. That is a post below its read mark, which waits
    /// only by its keep and is kept now for `MailHold::ended` (`Machine::hold_lifted`): decided at once, not at some
    /// later turn's end. No other agent is owed one (consult-3 N-1396 p220, 1): what the Release let go is new, and
    /// raised as anything new is (`raise_for`), and an agent the hold never touched is not asked `next` again for what
    /// a `Rest` kept. An agent this coordd has not yet seen is owed its first round's `free` anyway.
    pub(crate) fn desk_released(&mut self, held: &[(String, Vec<String>)]) {
        for (agent, items) in held {
            let kept = self.tickets.m.kept(agent);
            if items.iter().any(|i| kept.contains_key(i)) {
                if let Some(seen) = self.desk_seen.get_mut(agent) {
                    seen.owed = true;
                }
            }
        }
    }

    /// The program's messages for NAME still to go, as one message ([`Going`]); none while one of them is on its way.
    /// Two `deliver`s not yet handed are one message (the 0.2 list, A3: "several of his waiting become one";
    /// runtime-design §8: his five posts during a compaction, "not yet delivered, so joined"), its rest what theirs
    /// name and it does not carry, its digest what theirs give by first lines and it does not carry: never alone,
    /// since a digest starts no turn (I-1480). IN_TURN, NAME's turn running: one made at its `free` (`Hand::idle`) is
    /// not among them, so a turn begun while fold's call was out (its own job's end begins one) gets it at its end and
    /// never in it (gemini-advisor N-1446 p2365, 5b; proto-mind p2370 (a)); counted with IN_TURN false, it still stands
    /// for a message to go, so its turn's end raises no `free` again (`raise_for`'s `to_hand`).
    pub(crate) fn hand_now(&self, name: &str, in_turn: bool) -> Going {
        let hands: Vec<docket_machine::machine::Hand> = self.tickets.m.hands(name).into_iter().filter(|h| !(in_turn && h.idle)).collect();
        let mut items: Vec<String> = Vec::new();
        for i in hands.iter().flat_map(|h| &h.items) {
            if !items.contains(i) {
                items.push(i.clone());
            }
        }
        if items.is_empty() {
            return Going::default();
        }
        // what of it is no longer due (its thread moved or closed meanwhile) is passed
        let all = self.tickets.due_whole(name);
        let going = self.hand_deliveries(name, &items, &all);
        if going.is_empty() || going.iter().any(|d| self.tickets.handed.contains_key(&(name.to_string(), d.id.clone()))) {
            return Going::default();
        }
        let mut rest: Vec<String> = Vec::new();
        for i in hands.iter().flat_map(|h| &h.rest) {
            if !items.contains(i) && !rest.contains(i) {
                rest.push(i.clone());
            }
        }
        let mut digest: Vec<String> = Vec::new();
        for i in hands.iter().flat_map(|h| &h.digest) {
            if !items.contains(i) && !digest.contains(i) {
                digest.push(i.clone());
            }
        }
        // given as ITEMS are, a delivery a thread up to its last post of it: but for what a delivery of the message
        // passes on its thread already (a post of it before a handed one: docket-machine's `act_deliver`)
        let passed = |i: &String| docket_machine::post_id(i).is_some_and(|(t, n)| going.iter().any(|d| d.t == t && matches!(d.what, What::Posts { .. }) && n <= d.upto));
        let own: Vec<String> = digest.iter().filter(|i| !passed(i)).cloned().collect();
        let given = self.hand_deliveries(name, &own, &all);
        let fork = hands.iter().find_map(|h| h.fork.clone());
        Going { items: going, rest, digest, given, fork }
    }

    /// NAME's message is a `fork … at PLACE` (part 4, docket-core N-1446 p2096): whether its session is to be cut first.
    /// A Claude Code agent's is, once: its `fork_due` is set ([`ForkCut`], `Core::to_restart` restarts it with roster's
    /// `--cut`), and when its new session runs, the message goes with coord's words ahead of it: where it goes on
    /// from, and what its work ended with ([`Seen::said`]). Not forked, and said in coordd's log, the message going as
    /// a `deliver`'s: a Codex, opencode or agy agent (not yet: p2096's "says so"); a PLACE its last `given` line does not
    /// place, or placed in a session that is no longer the one it runs (`fork_cut`). Decided once a PLACE
    /// ([`Forked`]): a message handed again goes as decided.
    pub(crate) fn fork_now(&mut self, name: &str, place: &str) -> ForkNow {
        if let Some(f) = self.forked.get(name).filter(|f| f.place == place) {
            return ForkNow::Hand(f.words.clone());
        }
        let not = |core: &mut Core, why: String| {
            eprintln!("coordd: {name}: fork at {place} delivered as deliver: {why}");
            core.forked.insert(name.to_string(), Forked { place: place.to_string(), words: None });
            ForkNow::Hand(None)
        };
        let w = match self.fork_place(name, place) {
            Ok(w) => w,
            Err(why) => return not(self, why),
        };
        let Some(upto) = crate::follow::fork_cut(&w.file, w.offset, place) else { return not(self, format!("no entry of {} after {place}'s given line holds its message", w.file)) };
        let said = self.desk_seen.get(name).and_then(|s| s.said.clone()).filter(|l| l.file == w.file).and_then(|l| crate::follow::text_at(&l.file, l.offset, l.len)).filter(|t| !t.trim().is_empty());
        let session = w.file.rsplit('/').next().unwrap_or_default().trim_end_matches(".jsonl").to_string();
        let words = match said {
            Some(t) => format!("You go on from a copy of your session, cut where {place} was given to you: what you did after it is not in this copy, and your session {session} is kept as it was. That work ended with your message:\n\n{t}\n\nWhat came while you worked follows."),
            None => format!("You go on from a copy of your session, cut where {place} was given to you: what you did after it is not in this copy, and your session {session} is kept as it was. What came while you worked follows."),
        };
        eprintln!("coordd: {name}: fork at {place}: restarted on {} cut after {upto} bytes", w.file);
        self.agents.get_mut(name).unwrap().live.fork_due = Some(ForkCut { place: place.to_string(), file: w.file, upto });
        self.forked.insert(name.to_string(), Forked { place: place.to_string(), words: Some(words) });
        self.changed(name);
        self.journal();
        ForkNow::Restart
    }

    /// Where NAME's session would be cut for a fork at PLACE, as far as coordd reads it without the session's file: the
    /// place of PLACE's last `given` line, in the session NAME runs now; or why it cannot fork there: not a Claude Code
    /// agent (a Codex, opencode or agy session is not forked yet), no `given` line places PLACE, or it was given in
    /// another session. `free`'s `forkable` (syntax.rs FREE: the program says what goes when it cannot, gemini-advisor
    /// N-1446 p2400, A4; settled on N-1540), and the first of [`Core::fork_now`]'s checks, which stays the net for
    /// what changed between the two (its session restarted meanwhile) and for a run on the text before, which forks
    /// with no such guard.
    pub(crate) fn fork_place(&self, name: &str, place: &str) -> Result<docket_machine::Loc, String> {
        let a = self.agents.get(name).ok_or_else(|| format!("{name} is not known to coordd"))?;
        if a.kind != "claude" {
            return Err(format!("a {} session is not forked yet", a.kind));
        }
        let w = self.given_place(name, place).ok_or_else(|| format!("no given line places {place}"))?;
        if a.transcript().as_deref() != Some(w.file.as_str()) {
            return Err(format!("{place} was given in {}, not in the session it runs", w.file));
        }
        Ok(w)
    }

    /// Where DELIVERY was given to NAME: its last `given` line's place in NAME's session, None when coordd saw it
    /// nowhere.
    pub(crate) fn given_place(&self, name: &str, delivery: &str) -> Option<docket_machine::Loc> {
        docket_machine::parse_delivery(delivery).and_then(|(t, _)| self.tickets.m.tickets.get(&t)).and_then(|x| x.given.iter().rev().find(|g| g.to == name && g.delivery == delivery)).and_then(|g| g.place.clone())
    }

    /// The delivery NAME's turn began with, for its `turn_began` line (`begin_turn`; the machine's TURN_BEGAN, which
    /// `arrived`'s `began` reads, N-1104 p1154): of what it was given since its last turn ended, not put into the turn
    /// as it ran, the one given first in the session it runs (its `given` line's place); empty when coordd saw none
    /// given there.
    pub(crate) fn turn_began(&self, name: &str) -> String {
        let m = &self.tickets.m;
        let into = m.into_since(name);
        let file = self.agents.get(name).and_then(|a| a.transcript());
        m.given_since(name).into_iter().filter(|d| !into.contains(d)).filter_map(|d| self.given_place(name, &d).filter(|w| Some(&w.file) == file.as_ref()).map(|w| (w.offset, d))).min().map(|(_, d)| d).unwrap_or_default()
    }

    /// Whether NAME's copy (`fork … at`, the machine's `copy_of`) is still answering: in a turn or about to be, or
    /// with a post of his it was handed still to take, while it has not stopped (`free`'s `answering`). A copy coordd
    /// does not know yet is being started; one that stopped answers nothing.
    pub(crate) fn copy_answering(&self, name: &str) -> bool {
        let m = &self.tickets.m;
        let Some(c) = m.copy_of(name) else { return false };
        match self.agent_state(&c) {
            Some(AgentState::Working | AgentState::Compacting { .. } | AgentState::Starting { .. }) => true,
            Some(AgentState::Idle) => self.inflight.contains_key(&c) || m.copy_owed(&c),
            None => m.copy_owed(&c),
            Some(_) => false,
        }
    }

    /// Whether post ID (`N-1104.p12`) is written to agent NAME alone: it names NAME and no other agent. What a copy of a
    /// busy agent answers (`copy_answers`, the owner's N-1794): his post to one agent, never one that reaches many.
    pub(crate) fn written_to_alone(&self, id: &str, name: &str) -> bool {
        let m = &self.tickets.m;
        docket_machine::post_id(id).and_then(|(t, n)| m.tickets.get(&t)?.posts.iter().find(|p| p.n == n).map(|p| p.names.iter().all(|x| x == name) && p.names.iter().any(|x| x == name))).unwrap_or(false)
    }

    /// Whether the policy says a copy of NAME answers a post FROM wrote it while it works (fresh-lead-3 N-1104 p1154;
    /// paper-research N-1446 p3020, 1): the policy's act COPY_ACT, actor FROM, recipient NAME, in
    /// the team's situation (`norm_facts`), which holds the record's facts, `working` among them (agent-base's
    /// working.eflint: made by NAME's `turn_began` line, ended by its turn's end; ticket/effects.rs). A project's policy
    /// declares the act, as p1154's line `Holds when actor == "owner" and working(recipient)`; agent-base declares none.
    /// No act, or a policy that cannot be read or solved: no, said in coordd's log.
    pub(crate) fn copy_answers(&self, name: &str, from: &str) -> bool {
        // a step's short-lived agent, a copy among them, is copied by nobody: a copy's own copy would answer for it
        if crate::core::is_fresh(name) {
            return false;
        }
        let norms = match crate::policy::interp::acts(&self.places()) {
            Ok(n) if n.acts.contains_key(COPY_ACT) => n,
            Ok(_) => return false,
            Err(e) => {
                eprintln!("coordd: {name}: {COPY_ACT}: {e}; no copy");
                return false;
            }
        };
        let mut facts = self.norm_facts(&[]);
        if !facts.iter().any(|(f, v)| f == "agent" && v[0] == "owner") {
            facts.push(("agent".into(), vec!["owner".into()]));
        }
        let args: Vec<&str> = norms.acts[COPY_ACT].fields.iter().map(|(n, _)| if n == "actor" { from } else { name }).collect();
        match norms.solve_for(&[COPY_ACT], &facts).and_then(|shown| norms.verdict(&shown, COPY_ACT, &args)) {
            Ok(v) => v.is_some_and(|v| v.enabled),
            Err(e) => {
                eprintln!("coordd: {name}: {COPY_ACT}: {e}; no copy");
                false
            }
        }
    }

    /// The deliveries that carry ITEMS to NAME, made from ALL (the machine's due, on their way or not): on a thread,
    /// its posts up to the last one handed, without those that wait for NAME and are not handed (kept, or left for
    /// later), as one delivery up to that post, `T.pN` (a post kept below NAME's read mark is found on its ticket);
    /// a step, a notice or words whole, by their id. In ITEMS' order, each thread where its first item stands: the
    /// program's (his posts first; fold's most important first, fold.call), not the threads' by number, which put a
    /// newer thread's post after an older one's whatever the program said (76; fresh-lead-3 N-1540 p18: the brain's
    /// line after his posts).
    fn hand_deliveries(&self, name: &str, items: &[String], all: &[Delivery]) -> Vec<Delivery> {
        let m = &self.tickets.m;
        let post = |i: &String| docket_machine::post_id(i);
        // his post told late (`N-1.p5.told`): a delivery of its own, handed whole by its id (proto-lead N-1446 p569)
        let told = |d: &&Delivery| docket_machine::told_post(&d.id).is_some();
        let mut out: Vec<Delivery> = all.iter().filter(|d| !matches!(d.what, What::Posts { .. }) && items.contains(&d.id)).cloned().collect();
        let threads: BTreeSet<u64> = items.iter().filter_map(post).map(|(t, _)| t).collect();
        for t in threads {
            let handed: Vec<u32> = items.iter().filter_map(post).filter(|(u, _)| *u == t).map(|(_, n)| n).collect();
            let top = *handed.iter().max().unwrap();
            let Some(x) = m.tickets.get(&t) else { continue };
            let waits_for = |p: &docket_machine::Post| p.wakes().chain(p.inform.iter()).any(|a| a == name);
            let d = all.iter().find(|d| d.t == t && matches!(d.what, What::Posts { .. }) && !told(d));
            let mut unread: Vec<docket_machine::Post> = match d.map(|d| &d.what) {
                Some(What::Posts { unread }) => unread.iter().filter(|p| p.n <= top && (handed.contains(&p.n) || !waits_for(p))).cloned().collect(),
                _ => Vec::new(),
            };
            let below: Vec<docket_machine::Post> = x.posts.iter().filter(|p| handed.contains(&p.n) && p.visible() && p.moved.is_none() && !unread.iter().any(|q| q.n == p.n)).cloned().collect();
            unread.extend(m.shown(below));
            unread.sort_by_key(|p| p.n);
            if unread.is_empty() {
                continue;
            }
            // what only informs NAME starts no turn of its own (`Delivery::inform`)
            let inform = !unread.iter().any(|p| p.wakes().any(|a| a == name));
            let ticket = m.id(t);
            out.push(Delivery { id: format!("{ticket}.p{top}"), to: name.to_string(), t, ticket, kind: x.kind.clone(), upto: top, what: What::Posts { unread }, inform });
        }
        out.extend(all.iter().filter(told).filter(|d| items.contains(&d.id)).cloned());
        let at = |d: &Delivery| items.iter().position(|i| *i == d.id || (matches!(d.what, What::Posts { .. }) && !told(&d) && post(i).is_some_and(|(t, _)| t == d.t))).unwrap_or(usize::MAX);
        out.sort_by_key(|d| (at(d), d.t));
        out
    }

    /// REST, what a message of the delivery program names at its foot, as `rest.tmpl` reads it: the first ten, each
    /// its id and who it is from (a post's author; a step or a notice is coordd's), one after another.
    pub(crate) fn rest_named(&self, rest: &[String]) -> String {
        let m = &self.tickets.m;
        let named: Vec<String> = rest
            .iter()
            .take(10)
            .map(|id| {
                let post = docket_machine::delivered_post(id);
                let from = post.and_then(|(t, n)| m.tickets.get(&t)?.posts.iter().find(|p| p.n == n).map(|p| p.by.clone()));
                format!("{id} from {}", from.unwrap_or_else(|| String::from("coordd")))
            })
            .collect();
        named.join(" · ")
    }

    /// DIGEST, what goes with a message of the delivery program by first lines (`deliver … digest PULL`, I-1480), as
    /// `rest.tmpl` reads it (paper-research N-1481 p25, hs-ls p28): a line each thread, those NAME opened or works on
    /// first (`own_of`), then the rest, each the newest change first; each its id and title, its count of changes in
    /// words ("1 change", "20 changes"), its newest three, each "ID, writer: first line", and how many more (none:
    /// null). Every one is given with the message (`Going::given`), so its next digest lists only what came since.
    pub(crate) fn digest_named(&self, name: &str, digest: &[String]) -> Vec<Value> {
        let m = &self.tickets.m;
        // each change on its thread: its number there (none for a notice), its line, when it was posted
        let mut threads: Vec<(u64, Vec<(Option<u32>, String, String)>)> = Vec::new();
        for id in digest {
            let Some((t, _)) = docket_machine::parse_delivery(id) else { continue };
            let post = docket_machine::delivered_post(id).and_then(|(t, n)| m.tickets.get(&t)?.posts.iter().find(|p| p.n == n));
            let change = match post {
                Some(p) => {
                    let words = m.text(&p.text).map(|w| crate::render::resolve(&w)).unwrap_or_default();
                    (Some(p.n), format!("{id}, {}: {}", p.by, docket_machine::label::first_line(&words, 160)), p.at.clone())
                }
                None => (None, format!("{id}, coordd"), String::new()),
            };
            match threads.iter_mut().find(|(u, _)| *u == t) {
                Some((_, cs)) => cs.push(change),
                None => threads.push((t, vec![change])),
            }
        }
        for (_, cs) in &mut threads {
            cs.sort_by(|a, b| b.0.cmp(&a.0));
        }
        let own = |t: u64| m.tickets.get(&t).is_some_and(|x| docket_machine::machine::own_of(x, name));
        let newest = |cs: &[(Option<u32>, String, String)]| cs.iter().map(|c| c.2.clone()).max().unwrap_or_default();
        threads.sort_by(|(a, ca), (b, cb)| own(*b).cmp(&own(*a)).then_with(|| newest(cb).cmp(&newest(ca))));
        threads
            .into_iter()
            .map(|(t, cs)| {
                let label = m.tickets.get(&t).map(|x| docket_machine::label::title(m, x)).unwrap_or_default();
                let changes = if cs.len() == 1 { String::from("1 change") } else { format!("{} changes", cs.len()) };
                let shown: Vec<&str> = cs.iter().take(3).map(|c| c.1.as_str()).collect();
                let more = Some(cs.len().saturating_sub(3)).filter(|n| *n > 0);
                json!({"id": m.id(t), "label": label, "changes": changes, "newest": shown.join("; "), "more": more})
            })
            .collect()
    }

    /// A delivery run's `next` step D on the text before, answered at once with no call (I-1480: c's `free` delivers
    /// what pushes and names the rest; no `choose` call decides it). A run open at the install still awaits it, and
    /// `delivery_ticket` moves its agent to the text now at its next run. Its answer is what a failed call's was
    /// (runtime-design §5): everything waiting, oldest first, the first step only, any later one named at the foot.
    /// The journal says so. Nothing for any other step.
    pub(crate) fn next_given(&mut self, d: &Delivery) {
        let What::Step { call, args, .. } = &d.what else { return };
        let Some(agent) = self.tickets.m.run_of(d.t).filter(|_| call == "next") else { return };
        let (mut now, mut rest, mut step) = (Vec::new(), Vec::new(), false);
        for i in crate::render::resolved(args)["waiting"].as_array().into_iter().flatten() {
            let Some(id) = i.get("id").and_then(Value::as_str) else { continue };
            let is_step = i.get("what").and_then(Value::as_str) == Some("step");
            if is_step && step {
                rest.push(id.to_string());
                continue;
            }
            step |= is_step;
            now.push(id.to_string());
        }
        let head = json!({"head": "Give", "now": now, "rest": rest});
        match self.ticket_answer(docket_machine::WATCHER, d.t, head.clone(), docket_machine::Text::New(String::new())) {
            Ok(_) => eprintln!("coordd: tickets: next for {agent} ({}), on the text before: no call, everything waiting goes: {head}", d.ticket),
            Err(e) => eprintln!("coordd: tickets: next for {agent} ({}), on the text before: {head} not taken: {e}", d.ticket),
        }
    }

    /// The column "the post it answers" of `fold`'s line for item I waiting for AGENT (fold.call's `choose`): the post
    /// it names (the reply tool's `to`); else, for an agent's post, of the posts it answers that were new to its writer
    /// (since its last post there written with less in hand, as far back as its givings since then held, up to what it
    /// had been handed when it wrote it, `Ticket::answered_upto`), the latest that is AGENT's or names it, else the
    /// last; read from the latest back. Then whose that post is: "its own" when AGENT's, with its first line; when
    /// another's that names AGENT, its lines that do ([`naming_lines`]), else its first. "-" when it answers none. A
    /// post that brings what the agent was asked to read is its work, and the call sees that it was asked
    /// (paper-research N-1397.p169). `next`'s before I-1480 c (373c572a).
    fn answered_column(&self, i: &Value, agent: &str) -> String {
        let m = &self.tickets.m;
        let words = |q: &docket_machine::Post| m.text(&q.text).map(|w| crate::render::resolve(&w)).unwrap_or_default();
        let named = i.get("answers").and_then(Value::as_str).and_then(docket_machine::post_id);
        let handed = || {
            let (t, n) = i.get("id").and_then(Value::as_str).and_then(docket_machine::post_id)?;
            let x = m.tickets.get(&t)?;
            let p = x.posts.iter().find(|p| p.n == n && p.by != "owner")?;
            let upto = *x.answered_upto.get(&n)?;
            // where its last post there written with less in hand left off: posts with the same mark share one window
            let since = x.posts.iter().filter(|q| q.by == p.by && q.n < n).map(|q| x.answers_upto(q)).filter(|u| *u < upto).max().unwrap_or(0);
            // and what its givings since then held: from the read mark the first of them raised (`GivenRec::before`)
            let from = x.given.iter().filter(|g| g.to == p.by && g.upto > since && g.upto <= upto).map(|g| g.before).min().map_or(since, |b| b.max(since));
            let new: Vec<&docket_machine::Post> = x.posts.iter().filter(|q| q.n > from && q.n <= upto && q.by != p.by && q.visible() && x.answers(p, q.n)).collect();
            let theirs = |q: &&&docket_machine::Post| q.by == agent || q.names.iter().any(|a| a == agent) || !naming_lines(&words(**q), agent).is_empty();
            new.iter().rev().find(theirs).or(new.last()).map(|q| (t, q.n))
        };
        let Some((t, n)) = named.or_else(handed) else { return String::from("-") };
        let at = format!("{}.p{n}", m.id(t));
        let Some(q) = m.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n && p.visible())) else { return at };
        let words = words(q);
        if q.by == agent {
            return format!("{at}, its own: \"{}\"", docket_machine::label::first_line(&words, 160));
        }
        let by = if q.by == "owner" { "the owner" } else { q.by.as_str() };
        let lines = naming_lines(&words, agent);
        if lines.is_empty() && !q.names.iter().any(|a| a == agent) {
            return format!("{at} by {by}");
        }
        let said = if lines.is_empty() { docket_machine::label::first_line(&words, 160) } else { lines.into_iter().take(3).collect::<Vec<_>>().join(" … ") };
        format!("{at} by {by}, which names {agent}: \"{said}\"")
    }

    /// The watcher's `fold` step D of a delivery run, as the clerk's `deliver` job takes it (fold.call's `choose`):
    /// what its agent was on, how far its session ran on since, and what waits for it, a line each, oldest first. None
    /// for any other step. `next_job` before I-1480 c (373c572a), with SINCE (paper-research N-1446 p2097).
    pub(crate) fn fold_job(&self, d: &Delivery) -> Option<crate::clerk::Job> {
        let What::Step { step, call, args, .. } = &d.what else { return None };
        if d.to != docket_machine::WATCHER || call != "fold" || self.tickets.m.run_of(d.t).is_none() {
            return None;
        }
        let m = &self.tickets.m;
        let agent = m.run_of(d.t).unwrap_or_default();
        let args = crate::render::resolved(args);
        let mut lines: Vec<String> = Vec::new();
        let mut ids = Vec::new();
        for i in args["waiting"].as_array().into_iter().flatten() {
            let f = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let text = match i.get("text") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => docket_machine::Text::of(v).and_then(|t| m.text(&t)).map(|t| crate::render::resolve(&t)).unwrap_or_default(),
                None => String::new(),
            };
            // its lines further down that name the agent, after its first: an ask below a first line that does not
            // say it (proto-mind N-1397.p173)
            let first = docket_machine::label::first_line(&text, 160);
            let further: Vec<String> = naming_lines(&text, &agent).into_iter().filter(|l| *l != first).take(3).collect();
            let line = if further.is_empty() { first } else { format!("{first} [further down, naming {agent}: \"{}\"]", further.join(" … ")) };
            // a post from above it says so: the call puts one from its lead that names it before other agents' posts
            // (paper-research N-1397.p69)
            let from = match (i.get("owner") == Some(&json!(true)), i.get("lead") == Some(&json!(true))) {
                (true, _) => String::from("the owner"),
                (false, true) => format!("{} (its lead)", f("from")),
                (false, false) => f("from"),
            };
            let names = if i.get("names") == Some(&json!(true)) { "names it" } else { "-" };
            let answers = self.answered_column(i, &agent);
            let mut row = format!("{} · #{} {} · {from} · {names} · {answers} · {} · {line}", f("id"), f("thread"), f("label"), f("what"));
            if let Some(k) = i.get("kept").and_then(Value::as_str) {
                row.push_str(&format!(" (kept: {k})"));
            }
            lines.push(row);
            ids.push(f("id"));
        }
        // empty: no turn of it has ended since coordd started (`doing` is kept in memory, `desk_turn_ended`), and the
        // call is not told it was on nothing (consult-3, N-1397.p34, 2)
        let doing = args["doing"].as_str().filter(|s| !s.is_empty()).unwrap_or(NOT_KNOWN).to_string();
        // its run's fold, as its delivery ticket recorded the call: Failed is an answer to it, or not (a run open on a
        // text from before fold had it)
        let failed = m.call_of(d.t, "fold").is_some_and(|c| matches!(&c.head, docket_machine::Type::Variant(vs) if vs.iter().any(|(v, _)| v == "Failed")));
        let since = args["since"].as_str().filter(|s| !s.is_empty()).unwrap_or("-").to_string();
        Some(crate::clerk::Job {
            worker: d.to.clone(),
            to: agent,
            t: d.t,
            step: *step,
            ticket: d.ticket.clone(),
            text: String::new(),
            open: String::new(),
            split: m.defs.calls.get("choose").map(|c| c.head.clone()),
            places: self.places(),
            route: None,
            clean: None,
            brain: None,
            fold: Some(crate::clerk::Fold { doing, since, waiting: lines.join("\n"), ids, failed }),
        })
    }

    /// In tests: each `fold` due answered at once by `fold_by`, its verdict read as the clerk's job reads the call's
    /// (`clerk::fold_ok`, else the fallback).
    #[cfg(test)]
    fn fold_inline(&mut self) {
        let Some(by) = self.fold_by else { return };
        let due: Vec<Delivery> = self.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| self.tickets.m.run_of(d.t).is_some()).collect();
        for d in due {
            if let Some(job) = self.fold_job(&d) {
                let n = job.fold.as_ref().expect("a fold job");
                let head = crate::clerk::fold_ok(&by(n), n).unwrap_or_else(|| crate::clerk::fold_fell(n, "the answer gives nothing that waits"));
                self.fold_answered(&job, &d, head, None, None, None);
            }
        }
    }

    /// The clerk's answer HEAD to the `fold` JOB (Give, Fork or Rest, or when the call failed `clerk::fold_fell`'s
    /// Failed, or Give of everything on a text before it: FAILED, said in the journal), TOOK from the call's start to
    /// its answer. The run takes it as its
    /// step's answer, a DECIDED line; the journal says who decided and how long the agent's mail waited on it
    /// ([`fold_said`]).
    pub(crate) fn fold_answered(&mut self, job: &crate::clerk::Job, d: &Delivery, head: Value, model: Option<String>, failed: Option<String>, took: Option<std::time::Duration>) {
        self.tickets.handed.remove(&(d.to.clone(), d.id.clone()));
        let said = fold_said(model.as_deref(), failed.as_deref(), took);
        match self.ticket_answer(&job.worker, job.t, head.clone(), docket_machine::Text::New(String::new())) {
            Ok(_) => eprintln!("coordd: tickets: fold for {} ({}): {said}: {head}", job.to, job.ticket),
            Err(e) => eprintln!("coordd: tickets: fold for {} ({}): {said}, not taken: {e}", job.to, job.ticket),
        }
    }

    /// The watcher's steps DUE: a pass of an agent's brain (brain.rs), and a delivery run's `fold` (fold.call), each on
    /// a queue of its agent's own, one job each (`Job::Deliver`): one agent's call holds up no other's, nor the clerk,
    /// which routes the owner's posts (structure.md §8b: "his post is never delayed by a holder"); on its way until
    /// answered, since a run writes nothing but the runtime's `event` lines. A delivery run's `next` on the text
    /// before is answered in the round that raised it (`raise_delivery`, `next_given`), never by the clerk. What is
    /// left is the clerk's.
    pub(crate) fn hand_passes(&mut self, due: Vec<Delivery>) -> Vec<Delivery> {
        let Some(jobs) = self.jobs.clone() else { return due };
        let is_fold = |d: &Delivery| matches!(&d.what, What::Step { call, .. } if call == "fold");
        let (passes, rest): (Vec<Delivery>, Vec<Delivery>) = due.into_iter().filter(|d| self.tickets.m.run_of(d.t).is_none() || is_fold(d)).partition(|d| self.brain_of(d).is_some() || is_fold(d));
        for d in passes {
            // an agent in Step (N-1788): no model chooses what goes; everything waiting goes, as it came, as one
            // message, the turn staged for the owner exactly (`clerk::fold_failed`'s Give of all, decided here)
            if is_fold(&d) && self.tickets.m.run_of(d.t).is_some_and(|a| self.agents.get(&a).is_some_and(|x| x.control == coord_types::AgentMode::Step)) {
                if let Some(job) = self.fold_job(&d) {
                    let head = crate::clerk::fold_failed(job.fold.as_ref().expect("a fold job"));
                    self.fold_answered(&job, &d, head, Some("Step (no fold call)".into()), None, None);
                    continue;
                }
            }
            let queue = match self.tickets.m.run_of(d.t) {
                Some(agent) => format!("deliver {agent}"),
                None => format!("brain {}", self.brain_of(&d).unwrap_or_default()),
            };
            let h = crate::ticket::Handing::of(&self.tickets.m, &d.to, &d);
            self.tickets.handed.insert((d.to.clone(), d.id.clone()), (d.t, d.upto, h));
            let _ = jobs.send(Job::Deliver { queue, delivery: d });
        }
        rest
    }
}

#[cfg(test)]
#[path = "tests/delivery.rs"]
pub(crate) mod tests;
