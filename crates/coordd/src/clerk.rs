//! The clerk's split of the owner's text (plan/edge.md S3, paper-research m709-m711): a filing's step is owed by
//! `clerk`, which coordd answers itself with one stateless call. The owner's text exists once, on the filing; a piece
//! of it is a span (offset, length) of that text, never a copy. Models are bad at offsets, so the call gives each
//! passage as words, and this module finds them in the text: a passage that is not the owner's words exactly is
//! refused (the clerk must never rewrite them), and what no passage covers is said, never dropped. An agent's turn is
//! split the same way (paper-research m898), but what is not its words, or not placed, is dropped: most of a turn is
//! for nobody.

/// Where PASSAGES are in TEXT, in order: each a (byte offset, byte length) span. They are looked for left to right,
/// each after the one before (the owner's order); a passage found only earlier still counts, so a model that lists
/// them out of order is not refused for it. Err names the first passage that is not in TEXT word for word.
pub fn spans(text: &str, passages: &[String]) -> Result<Vec<(usize, usize)>, String> {
    let mut out = Vec::new();
    let mut from = 0;
    for p in passages {
        let p = p.trim();
        if p.is_empty() {
            return Err("an empty passage".into());
        }
        let at = text[from..].find(p).map(|i| i + from).or_else(|| text.find(p)).ok_or_else(|| format!("the passage \"{}\" is not the owner's words as typed", clip(p)))?;
        out.push((at, p.len()));
        from = at + p.len();
    }
    Ok(out)
}

/// What of TEXT the SPANS leave out: each stretch with more than punctuation and spaces in it, trimmed, as its span.
pub fn left_out(text: &str, spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut covered = vec![false; text.len()];
    for &(at, len) in spans {
        covered[at..at + len].iter_mut().for_each(|c| *c = true);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if covered[i] || !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < text.len() && !covered[i] {
            i += 1;
        }
        let s = &text[start..i];
        if s.chars().any(char::is_alphanumeric) {
            let lead = s.len() - s.trim_start().len();
            out.push((start + lead, s.trim().len()));
        }
    }
    out
}

/// The open tickets as the split call reads them (its `open`): one a line, its id, kind and title, and the step it
/// owes the owner with that step's results, when it owes one.
pub fn open_lines(views: &[docket_machine::TicketView]) -> String {
    use docket_machine::{Type, ViewStatus};
    let mut out = Vec::new();
    for v in views {
        let line = match &v.status {
            ViewStatus::Awaits { agent, call, head, .. } if agent == "owner" => {
                let results = match head {
                    Type::Variant(cs) => cs.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(" | "),
                    _ => String::new(),
                };
                format!("{} {}: {} (owes the owner step {call}; its results: {results})", v.id, v.kind, v.title)
            }
            ViewStatus::Done { .. } | ViewStatus::Failed { .. } => continue,
            _ => format!("{} {}: {}", v.id, v.kind, v.title),
        };
        // a ticket under it that owes the owner a step (the decision a minder's program opened): his words may answer it
        // (paper-research m1185 B; m1194)
        // (a clarify is said apart, below: it is the ticket's own, run by a handler kept out of every list)
        let owed: Vec<String> = v.waits_on.iter().filter(|w| w.agent == "owner" && w.ticket != v.id).map(|w| format!("{} ({})", w.ticket, w.call)).collect();
        let line = if owed.is_empty() { line } else { format!("{line} (awaits the owner's answer on {})", owed.join(", ")) };
        // the owner's post a handler holds while it asks the owner whom it is for (design §13, the clarify chip): said
        // so, with why, so that words typed to an agent in answer are placed here (paper-research m1341)
        let asked: Vec<String> = v
            .handlers
            .iter()
            .filter_map(|h| match &h.status {
                ViewStatus::Awaits { agent, call, args, .. } if agent == "owner" && call == "clarify" => {
                    let n = v.posts.iter().find(|q| q.held.as_deref() == Some(h.id.as_str())).map_or(String::new(), |q| format!(" #{}", q.n));
                    Some(format!("(the owner's post{n} was given to no one; the owner is asked whom it is for: {})", args["why"].as_str().unwrap_or("").trim()))
                }
                _ => None,
            })
            .collect();
        let line = if asked.is_empty() { line } else { format!("{line} {}", asked.join(" ")) };
        // its last post, one line, so the subject is plain beside the title (m1235 f); and whether it waits on the owner
        // (the machine's fact): a short "go ahead" answers that ticket
        let line = match v.posts.last().filter(|_| v.posts.len() > 1) {
            Some(p) => {
                let words = crate::render::resolve(&p.text);
                let waits = match &v.owner {
                    Some(o) if !o.unanswered.is_empty() => format!(" (waits on the owner's answer: {})", o.unanswered.join(", ")),
                    _ => String::new(),
                };
                format!("{line}\n  last post, by {}{waits}: {}", p.by, clip(words.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()))
            }
            None => line,
        };
        out.push(line);
    }
    if out.is_empty() { "(none)".into() } else { out.join("\n") }
}

/// The open tickets agent TO takes part in, as the watcher's `sort` reads them (its `open`): opened by TO, owed by TO,
/// or with a post by or naming TO; resting threads among them, since words may go on one. Of the open top threads that
/// share one label (`title`), only the newest: when his chat moves to a new thread of its label (archive-all,
/// docket-edge8 N-1446 p2451), his words typed to nobody are no longer filed onto the old one, which stays open with
/// its posts (paper-research p2457).
pub fn open_lines_of(views: &[docket_machine::TicketView], to: &str) -> String {
    use docket_machine::ViewStatus;
    let top = |v: &docket_machine::TicketView| v.kind == "thread" && v.parent.is_none() && !v.title.is_empty() && !matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. });
    // an open top thread with a newer one of its label (opened later, or the same moment and a later number)
    let older = |v: &docket_machine::TicketView| top(v) && views.iter().any(|w| top(w) && w.title == v.title && (&w.opened_at, w.n) > (&v.opened_at, v.n));
    let mine: Vec<docket_machine::TicketView> = views
        .iter()
        .filter(|v| {
            v.opener.as_deref() == Some(to)
                || matches!(&v.status, ViewStatus::Awaits { agent, .. } if agent == to)
                || v.posts.iter().any(|p| p.by == to || p.names.iter().any(|n| n == to))
        })
        .filter(|v| !older(v))
        .cloned()
        .collect();
    open_lines(&mine)
}

/// A filing's `file` step as the clerk takes it ([`crate::core::Core::clerk_job`]): its ticket and step, the owner's
/// text, the open tickets the call reads (`open_lines`), the head type of the split call (`place`), and where the
/// policy and calls are.
#[derive(Clone)]
pub struct Job {
    /// Whom coordd answers for: `clerk` (a filing's `file`) or the watcher (a thread's `intake`).
    pub worker: String,
    /// The agent the owner wrote to (intake only).
    pub to: String,
    pub t: u64,
    pub step: u32,
    pub ticket: String,
    pub text: String,
    pub open: String,
    pub split: Option<docket_machine::Type>,
    pub places: crate::policy::Places,
    /// The watcher's `route` step (design §13), when that is the step: whom the words were typed to, the others on the
    /// ticket, its last posts (`recent_lines`). None for intake and the clerk.
    pub route: Option<Route>,
    /// The watcher's `clean` step (the `words` kind, N-487), when that is the step: the coord's agents' names, held to
    /// come back as they are (clean.rs). None for every other step.
    pub clean: Option<Vec<String>>,
    /// The watcher's step of a pass of an agent's brain (0.3, brain.kind, brain.call), when that is the step: the call
    /// it runs and the report coordd built for it (brain.rs). None for every other step.
    pub brain: Option<Brain>,
    /// The watcher's `fold` step of a delivery run (delivery.kind's `free`, fold.call), when that is the step: what its
    /// agent (TO) was on, how far its session ran on since, and what waits for it. None for every other step.
    pub fold: Option<Fold>,
}

/// What the `choose` call of fold.call reads, beside the agent's name (the job's TO): DOING, what the agent's last turn
/// was on; SINCE, how far its session ran on since that work was given (`free`'s `since`); WAITING, a line per item
/// waiting, oldest first; IDS, their ids in that order; FAILED, whether its run's `fold` (the call its delivery
/// ticket recorded) answers Failed, as agent-base's has since 76: a call that fails says so then ([`fold_fell`]).
/// `next`'s before I-1480 c (373c572a), with SINCE (paper-research N-1446 p2097; proto-mind p2094, check 1).
#[derive(Clone, Debug)]
pub struct Fold {
    pub doing: String,
    pub since: String,
    pub waiting: String,
    pub ids: Vec<String>,
    pub failed: bool,
}

/// What a step of a pass of an agent's brain reads (brain.rs `brain_job`): AGENT, whose brain it is; CALL, the call it
/// runs (`attention`, `election`); ENV, the report, the call's arguments by name, each a text built from the record;
/// BURST, the ids of the owner's posts the pass is on (a signal rests on one of them); IDS, every id the report holds,
/// which a proposal or a pick may rest on (paper-research-brain N-1445 p6, do 2: a field without one is dropped). OPEN,
/// the ids the selector's `unanswered by it` candidates rest on, as it was shown them; none for the owner model (brain.rs
/// `answered_since`). A step of an update of the project's owner model (owner_model.kind, brain.rs
/// `owner_model_env`) reads the same: AGENT `owner`, CALL `owner_whole` or `owner_delta`, IDS the posts of his it
/// reads (the journal counts them, `Core::owner_sent`), and none of BURST and OPEN.
#[derive(Clone, Debug)]
pub struct Brain {
    pub agent: String,
    pub call: String,
    pub env: serde_json::Value,
    pub burst: Vec<String>,
    pub ids: Vec<String>,
    pub open: Vec<String>,
}

/// What the watcher's `route` step reads (route.call's `whom`): whom the words were typed to, the others on the ticket,
/// RECENT (`recent_lines`), and the thread's report for its holder (the owner's fundamentals, N-1104 p593; consult-3
/// N-1443 p2): THREAD (`thread_line`), YOURS (`yours_lines`), and HIS, the numbers of the posts YOURS lists, the only
/// ones the answer's `joins` may name. FALLBACK: where his words go when the call fails and they were typed to nobody
/// (`route_failed`). BURST: his posts the message is, in his order, when `gather` held more than one (E1b): what a
/// Split's parts must cover; empty for one post. CHOSEN: each post of the burst that carries his own choice, with whom
/// he chose (`Machine::his_choices`; E1b hold 1, proto-lead N-1446 p325): the part holding it goes to exactly them.
#[derive(Clone, Debug, Default)]
pub struct Route {
    pub to: Vec<String>,
    pub others: Vec<String>,
    pub recent: String,
    pub thread: String,
    pub yours: String,
    pub his: Vec<u32>,
    pub fallback: Vec<String>,
    pub burst: Vec<u32>,
    pub chosen: Vec<(u32, Vec<String>)>,
}

/// One of his posts on the thread for the holder's YOURS: its number N, when (AT), whom it went to (WENT; none while a
/// handler still holds it, whom it is for not said yet), when the first of them was given it (GIVEN), and its words.
#[derive(Clone, Debug)]
pub struct Yours {
    pub n: u32,
    pub at: String,
    pub went: Option<Vec<String>>,
    pub given: Option<String>,
    pub text: String,
}

/// The holder's YOURS (route.call): his posts on the thread just before this one, since an agent last posted there,
/// oldest first, at most five, each "#N at HH:MM:SSZ, S s before this one, went to A (given HH:MM:SSZ): words" cut
/// short; "(none)" when there are none. With the numbers it lists, which the answer's `joins` may name. THIS_AT: when
/// this post was made.
pub fn yours_lines(posts: &[Yours], this_at: &str) -> (String, Vec<u32>) {
    let shown: Vec<&Yours> = posts.iter().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect();
    if shown.is_empty() {
        return ("(none)".into(), Vec::new());
    }
    let secs = |a: &str| chrono::DateTime::parse_from_rfc3339(this_at).ok().zip(chrono::DateTime::parse_from_rfc3339(a).ok()).map(|(b, a)| (b - a).num_seconds());
    let lines: Vec<String> = shown
        .iter()
        .map(|y| {
            let when = match secs(&y.at) {
                Some(s) => format!("at {}, {s} s before this one", clock(&y.at)),
                None => format!("at {}", clock(&y.at)),
            };
            let went = match &y.went {
                None => "held: whom it is for is not said yet".to_string(),
                Some(to) if to.is_empty() => "went to nobody".to_string(),
                Some(to) => match &y.given {
                    Some(g) => format!("went to {} (given {})", to.join(", "), clock(g)),
                    None => format!("went to {} (not given yet)", to.join(", ")),
                },
            };
            format!("#{} {when}, {went}: {}", y.n, cut(&y.text, 200))
        })
        .collect();
    (lines.join("\n"), shown.iter().map(|y| y.n).collect())
}

/// The holder's THREAD (route.call): ID and its LABEL, whom it was opened for (FOR, its form's `to` and @NAMEs), the
/// ticket it is under, and HOLDING: each agent on it that has been given posts there in the turn it is in and has not
/// posted there since, with their numbers (`Machine::given_since`).
pub fn thread_line(id: &str, label: &str, opened_for: &[String], under: Option<&str>, holding: &[(String, Vec<u32>)]) -> String {
    let mut s = if label.trim().is_empty() { id.to_string() } else { format!("{id} \"{}\"", cut(label, 80)) };
    if let Some(u) = under {
        s += &format!(", under {u}");
    }
    if !opened_for.is_empty() {
        s += &format!(", opened for {}", opened_for.join(", "));
    }
    s.push('.');
    for (a, ns) in holding.iter().filter(|(_, ns)| !ns.is_empty()) {
        let ns: Vec<String> = ns.iter().map(|n| format!("#{n}")).collect();
        s += &format!(" {a} has been given {} in the turn it is in and has not posted here since.", ns.join(", "));
    }
    s
}

/// HH:MM:SSZ of an RFC 3339 time, as the record writes it; the text as it is when it is not one.
fn clock(at: &str) -> String {
    match at.split_once('T') {
        Some((_, t)) => t.to_string(),
        None => at.to_string(),
    }
}

/// GOT, an answer `route_ok` took, with its `joins` kept only when it names one of R's HIS (the holder's YOURS): a
/// `joins` on Unclear, or naming another post, is left out, and the journal says so. The To stands either way.
pub fn joins_kept(mut got: serde_json::Value, r: &Route, ticket: &str) -> serde_json::Value {
    let Some(o) = got.as_object_mut() else { return got };
    let Some(j) = o.get("joins").cloned() else { return got };
    let fine = o.get("head").and_then(|h| h.as_str()) == Some("To") && j.as_u64().is_some_and(|n| r.his.iter().any(|h| u64::from(*h) == n));
    if !fine {
        if !j.is_null() {
            eprintln!("coordd: tickets: watcher on {ticket}: route's joins {j} is none of his posts before it ({:?}), left out", r.his);
        }
        o.remove("joins");
    }
    got
}

/// The head that answers JOB's `file` step, and the decision that chose the call's model. Blocking: called off the
/// lock.
///
/// The owner's words (`place`): one stateless call, its passages turned into spans of the text. Filed with the
/// pieces when every passage is the owner's words and they leave nothing out; else Unplaced with why and what is left
/// (all of the text when nothing could be placed).
pub fn answer(job: &Job) -> Answered {
    if job.worker == docket_machine::WATCHER {
        if let Some(n) = &job.fold {
            return fold(job, n);
        }
        if let Some(b) = &job.brain {
            return brain(job, b);
        }
        if let Some(names) = &job.clean {
            return clean(job, names);
        }
        return match &job.route {
            Some(r) => route(job, r),
            None => intake(job),
        };
    }
    let (head, decision) = file(job);
    (head, decision, None)
}

/// A head, the decision that chose its model, and why the model call failed when it did: for intake that is said on
/// the ticket too, as a notice (paper-research N-8); a later call of the same kind that goes through clears it.
/// For route it is the answer line's text, the reason only (the head says whom it went to), no notice.
pub type Answered = (serde_json::Value, Option<coord_types::Decision>, Option<String>);

/// The head that answers the watcher's `clean` step on the owner's words (clean.rs): Sure, Borderline or Unreadable.
/// A call that fails answers Sure with his words as they came (only the certain fixes made), and says why on the
/// ticket: the words are not held up by a model that did not answer.
fn clean(job: &Job, names: &[String]) -> Answered {
    let engine = crate::policy::Engine::start(job.places.clone());
    match crate::clean::clean(&engine, &job.text, names) {
        Ok(out) => {
            let mut v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
            if let Some(o) = v.as_object_mut() {
                o.remove("model");
                o.remove("ms");
            }
            (v, None, None)
        }
        Err(why) => {
            eprintln!("coordd: tickets: watcher on {}: clean failed, his words go as they came: {why}", job.ticket);
            (serde_json::json!({"head": "Sure", "cleaned": job.text}), None, Some(why))
        }
    }
}

/// The clerk's own: a filing's `file` step (`answer`).
fn file(job: &Job) -> (serde_json::Value, Option<coord_types::Decision>) {
    use serde_json::json;
    let text = job.text.as_str();
    let all = || json!([{"at": 0, "len": text.len()}]);
    let unplaced = |why: String, left: serde_json::Value| json!({"head": "Unplaced", "why": why, "left": left});
    let call = "place";
    let Some(split) = &job.split else { return (unplaced(format!("the calls have no {call} (filing.call)"), all()), None) };
    let places = &job.places;
    let prompt = match crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).map_err(|e| e.to_string()).and_then(|c| c.render_step(call, &json!({"text": text, "open": job.open})).ok_or_else(|| format!("filing.call has no {call}"))) {
        Ok(p) => p,
        Err(e) => return (unplaced(format!("the clerk could not read its call: {e}"), all()), None),
    };
    let check = |a: &serde_json::Value| split.conforms(a);
    let engine = crate::policy::Engine::start(places.clone());
    let (got, decision) = match engine.call_prompt("clerk", call, &prompt, &split.schema(), "clerk", &check) {
        Ok(c) => (c.answer.unwrap_or_default(), c.decision),
        Err(e) => return (unplaced(format!("the clerk's call failed: {e}"), all()), None),
    };
    let pieces = got["pieces"].as_array().cloned().unwrap_or_default();
    let passages: Vec<String> = pieces.iter().map(|p| p["passage"].as_str().unwrap_or("").to_string()).collect();
    let found = match spans(text, &passages) {
        Ok(s) => s,
        Err(e) => return (unplaced(format!("the clerk's split was refused: {e}"), all()), Some(decision)),
    };
    let left = left_out(text, &found);
    if !left.is_empty() {
        let n = left.len();
        let words: Vec<String> = left.iter().map(|&(a, l)| format!("\"{}\"", clip(&text[a..a + l]))).collect();
        let head = unplaced(
            format!("{} of what you wrote belong{} to no open ticket: {}", if n == 1 { "one part".to_string() } else { format!("{n} parts") }, if n == 1 { "s" } else { "" }, words.join(", ")),
            json!(left.iter().map(|&(a, l)| json!({"at": a, "len": l})).collect::<Vec<_>>()),
        );
        return (head, Some(decision));
    }
    let filed: Vec<serde_json::Value> = pieces.iter().zip(&found).map(|(p, &(at, len))| json!({"ticket": p["ticket"], "at": at, "len": len, "answer": p["answer"]})).collect();
    (json!({"head": "Filed", "pieces": filed}), Some(decision))
}

/// The head that answers a thread's `intake` step (agent-base thread.kind, intake.call): one stateless call, `sort`,
/// with the model policy chooses for the task `intake`. Anything that goes wrong is a thread of its own, so the words
/// reach the agent and are kept (design-two-systems §16, 3: there is no one-off). A Place on a ticket that is not open
/// is a thread too, and so is an Unsure that names fewer than two tickets, one twice, or one that is not open.
/// The call coordd runs for each task of a watcher's step it answers itself (thread.kind's `intake`, base.kind's
/// `route`, brain.kind's passes, owner_model.kind's updates), one stateless call each with the model policy gives its
/// task: what each head below runs, and what the Kinds answer says a watcher's step runs (paper-research N-1396 p235).
/// A delivery run's `next`, on the text before, runs none (I-1480: delivery.rs `next_given`); its `fold` runs `choose`
/// (delivery.kind's `free` for what came while the agent worked; paper-research N-1446 p2097); thread.kind's `asks` is
/// answered Nothing with no call (`Core::clerk_job`; paper-research N-1446 p1081).
pub(crate) const RUNS: &[(&str, &str)] = &[("intake", SORT), ("route", WHOM), ("deliver", CHOOSE), ("attend", ATTENTION), ("elect", ELECTION), ("ahead", OUTLOOK), ("consolidate", CONSOLIDATION), ("owner_whole", OWNER_WHOLE), ("owner_delta", OWNER_DELTA)];
const SORT: &str = "sort";
const CHOOSE: &str = "choose";
const WHOM: &str = "whom";
pub(crate) const ATTENTION: &str = "attention";
pub(crate) const ELECTION: &str = "election";
pub(crate) const OUTLOOK: &str = "outlook";
pub(crate) const CONSOLIDATION: &str = "consolidation";
pub(crate) const OWNER_WHOLE: &str = "owner_whole";
pub(crate) const OWNER_DELTA: &str = "owner_delta";

/// The call coordd runs for a watcher's step of CALL in TASK (`RUNS`): none for a delivery run's `next`, the text
/// before's, which `next_given` answers with no call (I-1480), so its Kinds answer and a Reach name no `choose` for it.
pub(crate) fn runs(task: &str, call: &str) -> Option<&'static str> {
    if task == "deliver" && call == "next" {
        return None;
    }
    RUNS.iter().find(|(t, _)| *t == task).map(|(_, c)| *c)
}

pub fn intake(job: &Job) -> Answered {
    use serde_json::json;
    let thread = |why: String| {
        eprintln!("coordd: tickets: watcher on {}: a thread, {why}", job.ticket);
        json!({"head": "Thread"})
    };
    let call = SORT;
    let Some(head) = &job.split else { return (thread(format!("the calls have no {call} (intake.call)")), None, None) };
    let places = &job.places;
    let prompt = match crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).map_err(|e| e.to_string()).and_then(|c| c.render_step(call, &json!({"text": job.text, "to": job.to, "open": job.open})).ok_or_else(|| format!("intake.call has no {call}"))) {
        Ok(p) => p,
        Err(e) => return (thread(format!("the call could not be read: {e}")), None, None),
    };
    let check = |a: &serde_json::Value| head.conforms(a);
    let engine = crate::policy::Engine::start(places.clone());
    match engine.call_prompt("intake", call, &prompt, &head.schema(), docket_machine::WATCHER, &check) {
        Ok(c) => {
            // no model answered (the proxy down, every model failed): the call failed, as an Err is, its why in the one
            // copy's words (policy `unanswered_words`), as route and the cleaning say it
            let Some(got) = c.answer else {
                let why = crate::policy::unanswered_words(&c.decision);
                return (thread(format!("the call failed: {why}")), Some(c.decision), Some(why));
            };
            let ok = verdict_ok(&got, &job.open);
            if ok { (got, Some(c.decision), None) } else { (thread(format!("the answer {got} is not a verdict on the open tickets")), Some(c.decision), None) }
        }
        Err(e) => (thread(format!("the call failed: {e}")), None, Some(e.to_string())),
    }
}

/// Whether GOT is a verdict on the open tickets OPEN (one a line, its id first): a Place on one of them; an Unsure
/// (m1249) on two or more of them, none twice, for the owner to pick from; a Thread.
fn verdict_ok(got: &serde_json::Value, open: &str) -> bool {
    let is_open = |t: &str| open.lines().any(|l| l.split(' ').next() == Some(t));
    match got["head"].as_str() {
        Some("Place") => got["ticket"].as_str().is_some_and(is_open),
        Some("Unsure") => got["tickets"].as_array().is_some_and(|ts| {
            let ids: Vec<&str> = ts.iter().filter_map(|t| t.as_str()).collect();
            let mut distinct = ids.clone();
            distinct.sort_unstable();
            distinct.dedup();
            ids.len() == ts.len() && distinct.len() == ids.len() && ids.len() >= 2 && ids.iter().all(|t| is_open(t))
        }),
        Some("Thread") => true,
        _ => false,
    }
}

/// The head that answers the watcher's `route` step (base.kind's owner_post handler, route.call, design §13): one
/// stateless call, `whom`, with the model policy chooses for the task `route`. Anything that goes wrong (a call that
/// fails, an answer out of shape, one that gives nobody, one naming an agent not on the ticket, or Unclear for words
/// typed to an agent) is `route_failed`: words typed to an agent go to that agent (design.md part 7).
pub fn route(job: &Job, r: &Route) -> Answered {
    use serde_json::json;
    let typed = |why: String, decision: Option<coord_types::Decision>| {
        eprintln!("coordd: tickets: watcher on {}: route failed, {why}", job.ticket);
        // the answer line's text is the reason, and ends there: the head says whom it went to (proto-lead N-1446 p240)
        (route_failed(r), decision, Some(why))
    };
    let call = WHOM;
    let Some(head) = &job.split else { return typed(format!("the routing call did not run: route.call has no {call}"), None) };
    let places = &job.places;
    let to = if r.to.is_empty() { "nobody (it was posted on the ticket itself)".to_string() } else { r.to.join(", ") };
    let agents: Vec<&str> = r.to.iter().chain(&r.others).map(String::as_str).collect();
    let env = json!({"text": job.text, "to": to, "agents": agents.join(", "), "recent": r.recent, "thread": r.thread, "yours": r.yours});
    let prompt = match crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).map_err(|e| e.to_string()).and_then(|c| c.render_step(call, &env).ok_or_else(|| format!("route.call has no {call}"))) {
        Ok(p) => p,
        Err(e) => return typed(format!("the routing call did not run: route.call could not be read ({e})"), None),
    };
    // the words route was asked, one line in coordd's log: route is rare, and its answers are read against them
    eprintln!("coordd: tickets: watcher on {}: route prompt {:?}", job.ticket, prompt);
    let check = |a: &serde_json::Value| head.conforms(a);
    let engine = crate::policy::Engine::start(places.clone());
    match engine.call_prompt("route", call, &prompt, &head.schema(), docket_machine::WATCHER, &check) {
        Ok(c) => {
            // what the holder read goes on the record with the model that read it (the `decided` line)
            let decision = coord_types::Decision { prompt: Some(prompt), ..c.decision };
            match c.answer {
                // no model answered (tonight's 429s): why, in the one copy's words
                None => typed(format!("the routing call failed: {}", crate::policy::unanswered_words(&decision)), Some(decision)),
                Some(got) if route_ok(&got, r) => (one_part(joins_kept(got, r, &job.ticket)), Some(decision), None),
                Some(got) if got["head"] == "Unclear" && !r.to.is_empty() => typed(format!("the routing call answered Unclear for words typed to {}", r.to.join(", ")), Some(decision)),
                Some(got) if !choices_kept(&got, r) => typed(format!("the routing call's answer {got} sends a post of his elsewhere than to whom he chose"), Some(decision)),
                Some(got) => typed(format!("the routing call's answer {got} names no agent on the ticket"), Some(decision)),
            }
        }
        Err(e) => typed(format!("the routing call failed: {e}"), None),
    }
}

/// The head that answers the watcher's `fold` step of a delivery run (delivery.kind's `free`, fold.call): one
/// stateless call, `choose`, with the model policy chooses for the task `deliver`. Its verdict is taken as `fold_ok`
/// reads it; a call that fails or answers out of shape is answered [`fold_fell`], and why is said in the journal and
/// nowhere else. `next`'s head before I-1480 c (373c572a), with the agent's SINCE in the call's words.
fn fold(job: &Job, n: &Fold) -> Answered {
    let failed = |why: String, decision: Option<coord_types::Decision>| {
        eprintln!("coordd: tickets: watcher on {}: fold for {} failed, {}: {why}", job.ticket, job.to, if n.failed { "answered Failed" } else { "everything waiting goes" });
        (fold_fell(n, &why), decision, Some(why))
    };
    let call = CHOOSE;
    let Some(head) = &job.split else { return failed(format!("the calls have no {call} (fold.call)"), None) };
    // the agent it decides for, by name (TO, whose delivery ticket it is): never left to the names in `doing`
    // (paper-research N-1397.p103)
    if job.to.is_empty() {
        return failed("the delivery ticket names no agent".into(), None);
    }
    let prompt = match fold_prompt(job, n) {
        Ok(p) => p,
        Err(e) => return failed(format!("the call could not be read: {e}"), None),
    };
    let check = |a: &serde_json::Value| head.conforms(a);
    let engine = crate::policy::Engine::start(job.places.clone());
    match engine.call_prompt("deliver", call, &prompt, &head.schema(), docket_machine::WATCHER, &check) {
        Ok(c) => match c.answer {
            // no model answered: the gateway refused each the policy allows, or none was left; the journal says which
            // were asked
            None => {
                let asked = c.decision.attempts.iter().map(|a| format!("{}: {}", a.model, a.failure.as_deref().unwrap_or("no answer"))).collect::<Vec<_>>();
                let why = c.decision.fault.clone().unwrap_or_else(|| "no option".into());
                failed(format!("no model answered ({why}); asked {}", if asked.is_empty() { "none".into() } else { asked.join("; ") }), Some(c.decision))
            }
            Some(got) => match fold_ok(&got, n) {
                Some(v) => (v, Some(c.decision), None),
                None => failed(format!("the answer {got} gives nothing that waits"), Some(c.decision)),
            },
        },
        Err(e) => failed(format!("the call failed: {e}"), None),
    }
}

/// The words `choose` is called with for `fold` N of JOB (fold.call's template): its agent, what it was on, how far its
/// session ran on since its work was given, and what waits, a line each.
pub fn fold_prompt(job: &Job, n: &Fold) -> Result<String, String> {
    let env = serde_json::json!({"agent": job.to, "doing": n.doing, "since": n.since, "waiting": n.waiting});
    crate::policy::call::Calls::load(&job.places.call_dirs(), &job.places.overlay).map_err(|e| e.to_string()).and_then(|c| c.render_step(CHOOSE, &env).ok_or_else(|| format!("fold.call has no {CHOOSE}")))
}

/// GOT as the answer to `fold` N: Give or Fork with NOW those of its ids that wait, each once, and REST every other
/// id waiting, those GOT names first; or Rest with a WHY to read. None for anything else, a Give or Fork of nothing
/// that waits among them. A message holds at most one step, since the agent's last message answers it: that is
/// `deliver`'s rule, kept once in the act (docket-machine's `act_deliver`, which names a later step of NOW first at
/// the foot), not here as well (gemini-advisor N-1446 p2400, A3; proto-mind p2476; settled on N-1540).
/// `next_ok` before I-1480 c, with Fork (fold.call: it goes on from a fork of its session where its work was given).
pub fn fold_ok(got: &serde_json::Value, n: &Fold) -> Option<serde_json::Value> {
    use serde_json::json;
    let ids = |k: &str| -> Vec<String> { got[k].as_array().into_iter().flatten().filter_map(|x| x.as_str()).filter(|i| n.ids.iter().any(|w| w.as_str() == *i)).map(String::from).collect() };
    match got["head"].as_str() {
        Some(head @ ("Give" | "Fork")) => {
            let mut now: Vec<String> = Vec::new();
            for i in ids("now") {
                if !now.contains(&i) {
                    now.push(i);
                }
            }
            if now.is_empty() {
                return None;
            }
            let mut rest: Vec<String> = Vec::new();
            for i in ids("rest").into_iter().chain(n.ids.iter().cloned()) {
                if !now.contains(&i) && !rest.contains(&i) {
                    rest.push(i);
                }
            }
            Some(json!({"head": head, "now": now, "rest": rest}))
        }
        Some("Rest") => got["why"].as_str().map(str::trim).filter(|w| !w.is_empty()).map(|w| json!({"head": "Rest", "why": w})),
        _ => None,
    }
}

/// `fold` N's answer when its call fails on a text whose fold has no Failed: everything waiting, oldest first
/// (nothing is held back on a failed call: runtime-design §5); `deliver` hands its first step, and names any later one
/// at the foot (`fold_ok`). `next_failed` before I-1480 c.
pub fn fold_failed(n: &Fold) -> serde_json::Value {
    serde_json::json!({"head": "Give", "now": n.ids, "rest": []})
}

/// `fold` N's answer when its call failed, or answered out of shape, for WHY: Failed(WHY) where its run's fold answers
/// Failed (N's FAILED: agent-base's fold.call since 76, and delivery.kind says what goes then, gemini-advisor N-1446
/// p2400; settled on N-1540), so the run's record says the call failed rather than that it gave everything; else
/// [`fold_failed`]'s Give, for a run open on a text before it, whose match has no arm for Failed.
pub fn fold_fell(n: &Fold, why: &str) -> serde_json::Value {
    if n.failed { serde_json::json!({"head": "Failed", "why": why}) } else { fold_failed(n) }
}

/// The head that answers the watcher's step of a pass of an agent's brain (brain.kind's `attend`, brain.call): one
/// stateless call, B's (`attention`), with the model the policy chooses for the pass's task, on B's report. Its form
/// is taken as `brain_ok` reads it; a call that fails or answers out of shape is answered `brain_failed`, the pass
/// says nothing to the agent, and why is in the journal only (N-1444 p2, 6: never a notice to the owner).
fn brain(job: &Job, b: &Brain) -> Answered {
    let failed = |why: String, decision: Option<coord_types::Decision>| {
        eprintln!("coordd: tickets: watcher on {}: {} said nothing, its call failed: {why}", job.ticket, brain_whose(b));
        (brain_failed(&why), decision, Some(why))
    };
    let call = b.call.as_str();
    let task = RUNS.iter().find(|(_, c)| *c == call).map_or(call, |(t, _)| *t);
    let Some(head) = &job.split else { return failed(format!("the calls have no {call} (brain.call)"), None) };
    let places = &job.places;
    let prompt = match brain_prompt(places, b) {
        Ok(p) => p,
        Err(e) => return failed(format!("the call could not be read: {e}"), None),
    };
    let check = |a: &serde_json::Value| head.conforms(a);
    let engine = crate::policy::Engine::start(places.clone());
    match engine.call_prompt(task, call, &prompt, &head.schema(), docket_machine::WATCHER, &check) {
        Ok(c) => match c.answer {
            None => {
                let why = c.decision.fault.clone().unwrap_or_else(|| "no option".into());
                failed(format!("no model answered ({why})"), Some(c.decision))
            }
            Some(got) => match brain_ok(&got, b) {
                Some(v) => (v, Some(c.decision), None),
                None => failed(format!("the answer {got} is not a form of the pass"), Some(c.decision)),
            },
        },
        Err(e) => failed(format!("the call failed: {e}"), None),
    }
}

/// Whose call B is, as the journal names it, the same on each of its lines: "the owner model" for an update's
/// (`owner_whole`, `owner_delta`), else "the brain of AGENT".
pub fn brain_whose(b: &Brain) -> String {
    if [OWNER_WHOLE, OWNER_DELTA].contains(&b.call.as_str()) { "the owner model".to_string() } else { format!("the brain of {}", b.agent) }
}

/// The words B's call sends, as the call files under PLACES render its ENV (brain.call, owner_model.call); why not, when
/// they cannot be read or have no such call.
pub fn brain_prompt(places: &crate::policy::Places, b: &Brain) -> Result<String, String> {
    let call = b.call.as_str();
    crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).map_err(|e| e.to_string()).and_then(|c| c.render_step(call, &b.env).ok_or_else(|| format!("the calls have no {call}")))
}

/// The kinds of the owner's error signals (his p627: "4-5 signal maybe owner due diligence epxectiosn of quality").
pub const SIGNALS: &[&str] = &["repeat", "correction", "shallow", "diligence", "expectation"];

/// How many of his earlier posts the owner model may name in EARLIER (brain.call), whose words go to the agent.
pub const NAMED: usize = 5;

/// How long the base step's THOUGHT may be, in characters, and how many lines its NOTES keep (brain.call's `outlook`).
pub const THOUGHT_CHARS: usize = 150;
pub const NOTES_LINES: usize = 10;

/// How many lines consolidation's TEXT keeps, and how long each may be, in characters (brain.call's `consolidation`).
pub const KEPT_LINES: usize = 8;
pub const KEPT_CHARS: usize = 300;

/// GOT as the answer of B's call, or None (out of shape: the pass says nothing).
/// - `attention`, the owner model: Read with a topic and whether the agent is on it; SIGNALS of the five kinds, each on
///   a post of B's burst with his words, once per post and kind; PROPOSALS, each an act resting on an id of B's report,
///   at most three; EARLIER, ids of B's report outside the burst, each once, at most NAMED (brain.rs then keeps
///   his own posts, `from_record`). A signal, a proposal or an id out of shape is left out: the form stands without it.
/// - `election`, the selector: Pick with an act, the id it rests on (one of B's), the case against, and the thought the
///   agent reads, or Nothing; SCORES, each criterion held to 0..2, a candidate's rank as given.
/// - `outlook`, the base step: its four perspectives and its NOTES (their first NOTES_LINES lines), then Next with an
///   act, the id it rests on (one of B's) and the thought, its first line cut at THOUGHT_CHARS; or Same. A Next that
///   rests on an id B's report did not hold, or lacks its act or its thought, is Same with the same perspectives and
///   notes (brain.call: "answered Same, as a pick is dropped"; N-1445 p6, do 2). Either way, its STOP (inhibition,
///   paper-research N-1104 p622, 3), its first line cut at THOUGHT_CHARS, when that rests on B's report (`rests`, as
///   consolidation's lines do); else it has none, as an answer from before the field, and the rest stands.
/// - `consolidation`, at a compaction: Kept with its TEXT's lines that each rest on B's report (`rests`: they name a
///   post of it and none it did not hold; `ticket::mentions::ids_in`; N-1445 p6, do 2), the first KEPT_LINES of them,
///   each cut at KEPT_CHARS. With none left it is out of shape, and the pass says nothing.
/// - `owner_whole`, `owner_delta`, the owner model: Model with its text, not empty.
pub fn brain_ok(got: &serde_json::Value, b: &Brain) -> Option<serde_json::Value> {
    use serde_json::json;
    let said = |v: &serde_json::Value, k: &str| v[k].as_str().map(str::trim).filter(|w| !w.is_empty()).map(String::from);
    let known = |id: &str| b.ids.iter().any(|i| i == id);
    // a line that names a post, and only posts B's report held
    let rests = |line: &str| {
        use crate::ticket::mentions::{ids_in, Id};
        let posts: Vec<String> = ids_in(line).into_iter().filter_map(|i| if let Id::Post(t, n) = i { Some(format!("{t}.p{n}")) } else { None }).collect();
        !posts.is_empty() && posts.iter().all(|p| known(p))
    };
    match (b.call.as_str(), got["head"].as_str()) {
        (ATTENTION, Some("Read")) => {
            let mut signals: Vec<serde_json::Value> = Vec::new();
            for s in got["signals"].as_array().into_iter().flatten() {
                let (Some(kind), Some(post), Some(words)) = (said(s, "kind"), said(s, "post"), said(s, "words")) else { continue };
                if !SIGNALS.contains(&kind.as_str()) || !b.burst.contains(&post) || signals.iter().any(|x| x["kind"] == kind && x["post"] == post) {
                    continue;
                }
                signals.push(json!({"kind": kind, "post": post, "words": words}));
            }
            let proposals: Vec<serde_json::Value> = got["proposals"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some((said(p, "act")?, said(p, "rests_on")?)))
                .filter(|(_, on)| known(on))
                .take(3)
                .map(|(act, on)| json!({"act": act, "rests_on": on}))
                .collect();
            let mut earlier: Vec<String> = Vec::new();
            for id in got["earlier"].as_array().into_iter().flatten().filter_map(|i| i.as_str()).map(str::trim) {
                if known(id) && !b.burst.iter().any(|p| p == id) && !earlier.iter().any(|e| e == id) && earlier.len() < NAMED {
                    earlier.push(id.to_string());
                }
            }
            Some(json!({"head": "Read", "topic": said(got, "topic")?, "on_topic": got["on_topic"].as_bool()?, "signals": signals, "proposals": proposals, "earlier": earlier}))
        }
        (ELECTION, Some(head @ ("Pick" | "Nothing"))) => {
            let n = |s: &serde_json::Value, k: &str| s[k].as_i64().unwrap_or(0).clamp(0, 2);
            let scores: Vec<serde_json::Value> = got["scores"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| Some(json!({"act": said(s, "act")?, "newest": n(s, "newest"), "rules": n(s, "rules"), "owed": n(s, "owed"), "sees": n(s, "sees"), "cost": n(s, "cost"), "rank": s["rank"].as_i64().unwrap_or(0).max(0)})))
                .collect();
            if head == "Nothing" {
                return Some(json!({"head": "Nothing", "scores": scores}));
            }
            let on = said(got, "rests_on").filter(|on| known(on))?;
            Some(json!({"head": "Pick", "act": said(got, "act")?, "rests_on": on, "against": said(got, "against").unwrap_or_default(), "thought": said(got, "thought")?, "scores": scores}))
        }
        (OUTLOOK, Some(head @ ("Next" | "Same"))) => {
            let text = |k: &str| got[k].as_str().map(|w| w.trim().to_string());
            let notes = text("notes")?.lines().take(NOTES_LINES).collect::<Vec<_>>().join("\n");
            let mut v = json!({"head": "Same", "goal": text("goal")?, "critic": text("critic")?, "memory": text("memory")?, "attention": text("attention")?, "notes": notes});
            match said(got, "stop").map(|s| docket_machine::label::first_line(&s, THOUGHT_CHARS)).filter(|s| !s.is_empty()) {
                Some(s) if rests(&s) => v["stop"] = json!(s),
                Some(s) => eprintln!("coordd: tickets: {}: its stop names no post its report held, or one it did not, so it is left out: {s}", brain_whose(b)),
                None => {}
            }
            if head == "Same" {
                return Some(v);
            }
            let on = said(got, "rests_on");
            match (said(got, "act"), on.clone().filter(|on| known(on)), said(got, "thought")) {
                (Some(act), Some(on), Some(thought)) => {
                    v["head"] = json!("Next");
                    v["act"] = json!(act);
                    v["rests_on"] = json!(on);
                    v["thought"] = json!(docket_machine::label::first_line(&thought, THOUGHT_CHARS));
                }
                _ => {
                    let why = match on {
                        Some(on) if !known(&on) => format!("rests on {on}, which its report did not hold"),
                        _ => "lacks its act, its id or its thought".to_string(),
                    };
                    eprintln!("coordd: tickets: {}: its Next {why}, so it is Same", brain_whose(b));
                }
            }
            Some(v)
        }
        (CONSOLIDATION, Some("Kept")) => {
            let text = said(got, "text")?;
            let kept: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty() && rests(l)).take(KEPT_LINES).map(|l| docket_machine::label::first_line(l, KEPT_CHARS)).collect();
            (!kept.is_empty()).then(|| json!({"head": "Kept", "text": kept.join("\n")}))
        }
        (OWNER_WHOLE | OWNER_DELTA, Some("Model")) => Some(json!({"head": "Model", "text": said(got, "text")?})),
        _ => None,
    }
}

/// A pass's answer when its call fails: Failed, WHY said; the pass says nothing to the agent.
pub fn brain_failed(why: &str) -> serde_json::Value {
    serde_json::json!({"head": "Failed", "why": why})
}

/// Route R's answer when its call fails (paper-research m1341): To whom the words were typed to. Typed to nobody (the
/// owner's own post on the ticket): To R's FALLBACK, the agent the thread is for (its form's `to`), else where his
/// last post on it went, so his post goes and no press comes back to him (proto-lead N-1446 p57.2: a call that fails
/// is the handler's to answer, not his); Unclear only when there is neither, so he picks and nothing sits unheard.
/// A burst with his own choices in it (CHOSEN) goes by them: each post he chose for to whom he chose, the others where
/// the words would go, posts going to the same agents in one part (a Split; of one part, its To), so a post of his
/// never goes against his own @NAME (E1b hold 1, proto-lead N-1446 p325). When the others have nowhere to go it is
/// Unclear, and base.kind sends those he chose for where he chose before he is asked about the rest (proto-reader
/// p445); when every post carries a choice, they go by them alone (consult-3 p446).
pub fn route_failed(r: &Route) -> serde_json::Value {
    if !r.chosen.is_empty() {
        let mut rest = Vec::new();
        if r.burst.iter().any(|n| !r.chosen.iter().any(|(c, _)| c == n)) {
            let plain = route_failed(&Route { chosen: Vec::new(), ..r.clone() });
            let Some(agents) = plain["agents"].as_array().map(|xs| xs.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<String>>()) else { return plain };
            rest = agents;
        }
        let mut parts: Vec<(Vec<u32>, Vec<String>)> = Vec::new();
        for n in &r.burst {
            let to = r.chosen.iter().find(|(c, _)| c == n).map_or(&rest, |(_, to)| to);
            match parts.iter_mut().find(|(_, t)| same_agents(t, to)) {
                Some((ns, _)) => ns.push(*n),
                None => parts.push((vec![*n], to.clone())),
            }
        }
        let parts: Vec<serde_json::Value> = parts.into_iter().map(|(ns, to)| serde_json::json!({"posts": ns, "to": to})).collect();
        return one_part(serde_json::json!({"head": "Split", "parts": parts}));
    }
    if !r.to.is_empty() {
        serde_json::json!({"head": "To", "agents": r.to})
    } else if !r.fallback.is_empty() {
        serde_json::json!({"head": "To", "agents": r.fallback})
    } else {
        serde_json::json!({"head": "Unclear", "why": "the call that says whom it is for failed"})
    }
}

/// Whether GOT answers route R: To one or more agents, each on the ticket (typed to or among the others), none twice;
/// or Unclear with a reason the owner can read, only for words typed to nobody (design.md part 7: words typed to an
/// agent are never put back to the owner).
pub(crate) fn route_ok(got: &serde_json::Value, r: &Route) -> bool {
    match got["head"].as_str() {
        Some("To") => agents_ok(&got["agents"], r) && choices_kept(got, r),
        Some("Unclear") => r.to.is_empty() && got["why"].as_str().is_some_and(|w| !w.trim().is_empty()),
        Some("Split") => split_ok(got, r) && choices_kept(got, r),
        _ => false,
    }
}

/// Whether XS are agents for R: one or more, each on the ticket (typed to or among the others), none twice.
fn agents_ok(xs: &serde_json::Value, r: &Route) -> bool {
    xs.as_array().is_some_and(|xs| {
        let names: Vec<&str> = xs.iter().filter_map(|x| x.as_str()).collect();
        let mut distinct = names.clone();
        distinct.sort_unstable();
        distinct.dedup();
        !names.is_empty() && names.len() == xs.len() && distinct.len() == names.len() && names.iter().all(|n| r.to.iter().chain(&r.others).any(|a| a == n))
    })
}

/// Whether agents A and B are the same, in any order.
fn same_agents(a: &[String], b: &[String]) -> bool {
    let set = |x: &[String]| x.iter().cloned().collect::<std::collections::BTreeSet<String>>();
    set(a) == set(b)
}

/// Whether GOT, To or a Split, sends each post of R's burst that carries his own choice (R's CHOSEN) to exactly whom he
/// chose (proto-reader N-1446 p307, proto-lead p325: equality, not "leaves none out"): a To, every post of the burst
/// to its agents; a Split, each part to its own. So two posts with different choices never share a part, and a post
/// without one may join a chosen post's part. True with nothing chosen, and for Unclear: base.kind then sends each
/// such post where he chose before he is asked about the rest (proto-reader p445); an answer of no such shape is
/// `route_ok`'s.
fn choices_kept(got: &serde_json::Value, r: &Route) -> bool {
    let agents = |v: &serde_json::Value| v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<String>>();
    let parts: Vec<(Vec<u32>, Vec<String>)> = match got["head"].as_str() {
        Some("To") => vec![(r.burst.clone(), agents(&got["agents"]))],
        Some("Split") => got["parts"].as_array().into_iter().flatten().map(|p| (p["posts"].as_array().into_iter().flatten().filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok())).collect(), agents(&p["to"]))).collect(),
        _ => return true,
    };
    r.chosen.iter().all(|(n, to)| parts.iter().filter(|(ns, _)| ns.contains(n)).all(|(_, a)| same_agents(a, to)))
}

/// Whether GOT, a Split, splits R's burst (E1b, route.call's SPLIT): by whole posts, each post of the burst in exactly
/// one part, and each part to one or more agents on the ticket, none twice. A burst of one post is never split.
fn split_ok(got: &serde_json::Value, r: &Route) -> bool {
    let Some(parts) = got["parts"].as_array().filter(|ps| !ps.is_empty()) else { return false };
    let mut seen: Vec<u32> = Vec::new();
    for p in parts {
        let posts: Vec<u32> = p["posts"].as_array().into_iter().flatten().filter_map(|n| n.as_u64()).filter_map(|n| u32::try_from(n).ok()).collect();
        if posts.is_empty() || posts.len() != p["posts"].as_array().map_or(0, Vec::len) || !agents_ok(&p["to"], r) {
            return false;
        }
        for n in posts {
            if seen.contains(&n) || !r.burst.contains(&n) {
                return false;
            }
            seen.push(n);
        }
    }
    r.burst.len() > 1 && seen.len() == r.burst.len()
}

/// A Split of one part is that part's To (its agents); anything else as it is.
pub fn one_part(got: serde_json::Value) -> serde_json::Value {
    match got["parts"].as_array() {
        Some(ps) if got["head"] == "Split" && ps.len() == 1 => serde_json::json!({"head": "To", "agents": ps[0]["to"]}),
        _ => got,
    }
}

/// A ticket's last posts for route's RECENT, oldest first: up to three before post BEFORE, held and dropped ones left
/// out, each "#N by: its first 200 characters".
pub fn recent_lines(posts: &[(u32, String, String)], before: u32) -> String {
    let shown: Vec<String> = posts.iter().filter(|(n, _, _)| *n < before).rev().take(3).collect::<Vec<_>>().into_iter().rev().map(|(n, by, text)| format!("#{n} {by}: {}", cut(text, 200))).collect();
    if shown.is_empty() { "(none)".into() } else { shown.join("\n") }
}

fn cut(s: &str, n: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= n { one } else { format!("{}…", one.chars().take(n - 1).collect::<String>()) }
}

fn clip(s: &str) -> String {
    if s.chars().count() <= 60 { s.to_string() } else { format!("{}…", s.chars().take(59).collect::<String>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// His chat moved (archive-all, docket-edge8 N-1446 p2451; paper-research p2457): of two open top threads with one
    /// label, his words typed to nobody (`open_lines_of` for the owner) are offered only the newer; another label stays,
    /// and so does the older once the newer is closed. A thread under another is no top thread: it stays.
    #[test]
    fn of_two_open_threads_with_one_label_his_words_are_offered_only_the_newer() {
        use serde_json::json;
        let thread = |id: &str, n: u64, title: &str, opened: &str, status: serde_json::Value, parent: Option<&str>| -> docket_machine::TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "title": title, "form": {"to": "paper-research"},
                "status": status, "children": [], "bound": 3, "spent": 1, "opened_at": opened, "waits_on": [], "given": [], "program": "", "parent": parent,
                "posts": [{"n": 1, "at": opened, "by": "owner", "text": "x", "names": ["paper-research"]}]}))
            .unwrap()
        };
        let rest = || json!({"is": "resting"});
        let old = thread("N-1104", 1104, "Your one group chat", "2026-11-19T15:15:00Z", rest(), None);
        let new = thread("N-1600", 1600, "Your one group chat", "2026-11-24T10:00:00Z", rest(), None);
        let other = thread("N-1446", 1446, "The engine", "2026-11-20T09:00:00Z", rest(), None);
        let under = thread("N-1601", 1601, "Your one group chat", "2026-11-24T10:05:00Z", rest(), Some("N-1446"));
        let ids = |o: String| o.lines().filter_map(|l| l.split(' ').next().filter(|w| w.starts_with("N-")).map(String::from)).collect::<Vec<_>>();
        assert_eq!(ids(open_lines_of(&[old.clone(), other.clone()], "owner")), ["N-1104", "N-1446"], "one chat: offered");
        assert_eq!(ids(open_lines_of(&[old.clone(), other.clone(), new.clone(), under.clone()], "owner")), ["N-1446", "N-1600", "N-1601"], "the newer only, and a thread under another stays");
        assert_eq!(ids(open_lines_of(&[old.clone(), new.clone()], "paper-research")), ["N-1600"], "for the agent too");
        let closed = thread("N-1600", 1600, "Your one group chat", "2026-11-24T10:00:00Z", json!({"is": "done", "result": null}), None);
        assert_eq!(ids(open_lines_of(&[old, closed, other], "owner")), ["N-1104", "N-1446"], "the newer closed: the older is offered again");
    }

    /// whom's prompt for words typed to lead, with lead and fixer on the ticket: TO and AGENTS written out
    /// (WHOM_PROMPT=FILE writes the prompt there, for reading).
    #[test]
    fn whoms_prompt_says_whom_the_words_were_typed_to() {
        use serde_json::json;
        let root = env!("CARGO_MANIFEST_DIR");
        let calls = crate::policy::call::Calls::load(&[format!("{root}/../../agent-base/calls")], &Default::default()).unwrap();
        let posts = vec![(1, "owner".to_string(), "I want to add mode(xs) to calc.".to_string()), (2, "lead".to_string(), "Design: mode(xs) ... @fixer, please review.".to_string()), (3, "fixer".to_string(), "Opinion: the tie rule is sound.".to_string())];
        let r = Route { to: vec!["lead".into()], others: vec!["fixer".into()], recent: recent_lines(&posts, 4), ..Default::default() };
        let to = if r.to.is_empty() { "nobody (it was posted on the ticket itself)".to_string() } else { r.to.join(", ") };
        let agents: Vec<&str> = r.to.iter().chain(&r.others).map(String::as_str).collect();
        let p = calls.render_step("whom", &json!({"text": "looks good, go ahead", "to": to, "agents": agents.join(", "), "recent": r.recent, "thread": r.thread, "yours": r.yours})).unwrap();
        if let Ok(f) = std::env::var("WHOM_PROMPT") { std::fs::write(f, &p).unwrap(); }
        assert!(p.contains("It was typed to: lead\n"), "{p}");
        assert!(p.contains("The agents on the ticket: lead, fixer\n"), "{p}");
    }

    /// paper-research m1341: a thread whose owner's post waits on the clarify chip says so in intake's OPEN, with why,
    /// and names itself once.
    #[test]
    fn a_thread_waiting_on_clarify_says_whom_the_owner_is_asked_about() {
        use serde_json::json;
        let handler = json!({"id": "O-1099511627779", "n": 1099511627779u64, "kind": "owner_post", "ctx": {}, "form": {}, "title": "",
            "status": {"is": "awaits", "agent": "owner", "call": "clarify", "step": 2, "since": "2026-11-12T08:46:35Z", "taken": false, "refusals": 0, "forming": false,
                "head": {"Variant": [["Send", [["to", {"List": "Agent"}]]], ["Drop", []]]}, "args": {"text": "hmm", "why": "It is unclear what \"this\" refers to."}},
            "posts": [], "children": [], "program": "", "bound": 3, "spent": 1, "given": [], "opened_at": "2026-11-12T08:46:31Z", "waits_on": []});
        let v: docket_machine::TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "add mode(xs)",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "given": [], "opened_at": "2026-11-12T08:39:03Z",
            "waits_on": [{"ticket": "N-1", "agent": "owner", "call": "clarify"}],
            "posts": [{"n": 1, "at": "2026-11-12T08:39:03Z", "by": "owner", "text": "add mode(xs)", "names": ["lead"]},
                      {"n": 11, "at": "2026-11-12T08:46:31Z", "by": "owner", "text": "hmm, I am not sure about this anymore", "held": "O-1099511627779"}],
            "handlers": [handler]}))
        .unwrap();
        let open = open_lines(&[v]);
        let first = open.lines().next().unwrap();
        assert_eq!(first, "N-1 thread: add mode(xs) (the owner's post #11 was given to no one; the owner is asked whom it is for: It is unclear what \"this\" refers to.)");
    }

    #[test]
    fn route_answers_name_agents_on_the_ticket_or_say_why() {
        use serde_json::json;
        let r = Route { to: vec!["lead".into()], others: vec!["fixer".into()], ..Default::default() };
        assert!(route_ok(&json!({"head": "To", "agents": ["fixer"]}), &r));
        assert!(route_ok(&json!({"head": "To", "agents": ["lead", "fixer"]}), &r));
        assert!(!route_ok(&json!({"head": "To", "agents": []}), &r), "nobody is not an answer");
        assert!(!route_ok(&json!({"head": "To", "agents": ["runner"]}), &r), "runner is not on the ticket");
        assert!(!route_ok(&json!({"head": "To", "agents": ["lead", "lead"]}), &r));
        // typed to lead: Unclear is refused, and the words go to lead
        assert!(!route_ok(&json!({"head": "Unclear", "why": "which change?"}), &r));
        assert_eq!(route_failed(&r), json!({"head": "To", "agents": ["lead"]}));
        // typed to nobody: Unclear with a reason stands
        let nobody = Route { to: vec![], others: vec!["lead".into(), "fixer".into()], ..Default::default() };
        assert!(route_ok(&json!({"head": "Unclear", "why": "which change?"}), &nobody));
        assert!(!route_ok(&json!({"head": "Unclear", "why": " "}), &nobody));
        let posts = vec![(1, "owner".to_string(), "add median".to_string()), (2, "lead".into(), "Design:\n sorted copy".into()), (3, "fixer".into(), "x".repeat(300)), (4, "lead".into(), "ok".into()), (5, "owner".into(), "go ahead".into())];
        let recent = recent_lines(&posts, 5);
        assert!(recent.starts_with("#2 lead: Design: sorted copy\n#3 fixer: "), "{recent}");
        assert!(recent.ends_with("#4 lead: ok") && !recent.contains("go ahead"));
        assert_eq!(recent_lines(&posts, 1), "(none)");
    }

    /// A call that fails on words typed to nobody (proto-lead N-1446 p57.2): his post goes to the agent the thread is
    /// for, else where his last post went; the press comes back only when there is neither. Typed to an agent, it goes
    /// to that agent as before.
    #[test]
    fn a_failed_call_sends_his_post_where_the_thread_is_for_and_asks_him_only_when_nothing_says() {
        use serde_json::json;
        let others: Vec<String> = N1104.iter().map(|a| a.to_string()).collect();
        // N-1104 is for paper-research (its form's `to`): p587's call failing sends it there, with no press
        let for_it = Route { to: vec![], others: others.clone(), fallback: vec!["paper-research".into()], ..Default::default() };
        assert_eq!(route_failed(&for_it), json!({"head": "To", "agents": ["paper-research"]}));
        // a thread whose `to` is not on it: where his last post went (computed so in route_job), here consult-5
        let last = Route { to: vec![], others: others.clone(), fallback: vec!["consult-5".into()], ..Default::default() };
        assert_eq!(route_failed(&last), json!({"head": "To", "agents": ["consult-5"]}));
        // neither: today's Unclear, and he picks
        let neither = Route { to: vec![], others, ..Default::default() };
        assert_eq!(route_failed(&neither), json!({"head": "Unclear", "why": "the call that says whom it is for failed"}));
        // typed to an agent: that agent, whatever the thread is for
        let typed = Route { to: vec!["here".into()], fallback: vec!["paper-research".into()], ..Default::default() };
        assert_eq!(route_failed(&typed), json!({"head": "To", "agents": ["here"]}));
    }

    /// A burst's Split (E1b; proto-lead N-1446 p61.5, by whole posts): each post of the burst in exactly one part,
    /// each part to agents on the ticket; a Split of one post, or one that leaves a post out, names one twice, or one
    /// not in the burst, is no answer. A Split of one part is its To. The burst is p590+p591 (20 Nov, 9 s apart).
    #[test]
    fn a_split_covers_his_burst_by_whole_posts_once_each() {
        use serde_json::json;
        let others: Vec<String> = N1104.iter().map(|a| a.to_string()).collect();
        let r = Route { others: others.clone(), burst: vec![590, 591], ..Default::default() };
        let part = |posts: serde_json::Value, to: serde_json::Value| json!({"posts": posts, "to": to});
        let split = |parts: Vec<serde_json::Value>| json!({"head": "Split", "parts": parts});
        assert!(route_ok(&split(vec![part(json!([590]), json!(["docket-core"])), part(json!([591]), json!(["paper-research"]))]), &r));
        assert!(route_ok(&split(vec![part(json!([590, 591]), json!(["docket-core", "paper-research"]))]), &r));
        assert!(!route_ok(&split(vec![part(json!([590]), json!(["docket-core"]))]), &r), "591 left out");
        assert!(!route_ok(&split(vec![part(json!([590, 591]), json!(["docket-core"])), part(json!([591]), json!(["here"]))]), &r), "591 twice");
        assert!(!route_ok(&split(vec![part(json!([590, 592]), json!(["docket-core"])), part(json!([591]), json!(["here"]))]), &r), "592 is not his burst");
        assert!(!route_ok(&split(vec![part(json!([590]), json!(["runner"])), part(json!([591]), json!(["here"]))]), &r), "runner is not on the ticket");
        assert!(!route_ok(&split(vec![part(json!([]), json!(["here"])), part(json!([590, 591]), json!(["here"]))]), &r), "a part with no post");
        assert!(!route_ok(&split(vec![part(json!(["590"]), json!(["here"])), part(json!([591]), json!(["here"]))]), &r), "posts are numbers");
        assert!(!route_ok(&split(vec![]), &r));
        let one = Route { others, burst: vec![], ..Default::default() };
        assert!(!route_ok(&split(vec![part(json!([590]), json!(["here"]))]), &one), "one post is never split");
        // one part: its To, as the engine's `tell as p` takes it
        assert_eq!(one_part(split(vec![part(json!([590, 591]), json!(["docket-core"]))])), json!({"head": "To", "agents": ["docket-core"]}));
        let two = split(vec![part(json!([590]), json!(["docket-core"])), part(json!([591]), json!(["here"]))]);
        assert_eq!(one_part(two.clone()), two);
        assert_eq!(one_part(json!({"head": "To", "agents": ["here"], "joins": 589})), json!({"head": "To", "agents": ["here"], "joins": 589}));
    }

    /// His own choice in a burst is kept exactly (E1b hold 1; proto-reader N-1446 p307, proto-lead p325, made up on
    /// p590+p591): p590 names nobody, p591 is "@docket-core …". Only the Split that sends 591 to docket-core alone is
    /// taken; a To for both, to paper-research or to both agents, gives 591 to an agent he did not choose. p590 may
    /// join 591's part. The fallback splits by his choice, and posts going to the same agents stay one part.
    #[test]
    fn his_own_choice_in_a_burst_is_kept_exactly() {
        use serde_json::json;
        let others: Vec<String> = N1104.iter().map(|a| a.to_string()).collect();
        let r = Route { others: others.clone(), burst: vec![590, 591], chosen: vec![(591, vec!["docket-core".into()])], fallback: vec!["paper-research".into()], ..Default::default() };
        let part = |posts: serde_json::Value, to: serde_json::Value| json!({"posts": posts, "to": to});
        let split = |parts: Vec<serde_json::Value>| json!({"head": "Split", "parts": parts});
        let his = split(vec![part(json!([590]), json!(["paper-research"])), part(json!([591]), json!(["docket-core"]))]);
        assert!(route_ok(&his, &r));
        assert!(!route_ok(&json!({"head": "To", "agents": ["paper-research"]}), &r), "591 is not paper-research's");
        assert!(!route_ok(&json!({"head": "To", "agents": ["paper-research", "docket-core"]}), &r), "paper-research is given 591, which he chose docket-core for");
        assert!(!route_ok(&split(vec![part(json!([590]), json!(["paper-research"])), part(json!([591]), json!(["docket-core", "paper-research"]))]), &r));
        assert!(route_ok(&json!({"head": "Unclear", "why": "which work is 590?"}), &r), "the holder's doubt about 590 reaches him; 591 goes where he chose first (base.kind)");
        assert!(route_ok(&split(vec![part(json!([590, 591]), json!(["docket-core"]))]), &r), "a post with no choice may join a chosen post's part");
        assert_eq!(route_failed(&r), his);
        // two choices of his: never one part; a fallback whose agents are a chosen post's keeps them in one
        let two = Route { others: others.clone(), burst: vec![1, 2, 3], chosen: vec![(1, vec!["docket-core".into()]), (3, vec!["here".into()])], fallback: vec!["docket-core".into()], ..Default::default() };
        assert!(!route_ok(&json!({"head": "To", "agents": ["docket-core", "here"]}), &two));
        assert_eq!(route_failed(&two), split(vec![part(json!([1, 2]), json!(["docket-core"])), part(json!([3]), json!(["here"]))]));
        // the posts with no choice have nowhere to go: Unclear (base.kind tells those he chose for first); every post
        // with a choice and no fallback: they go by his choices (consult-3 N-1446 p446)
        let nowhere = Route { others: others.clone(), burst: vec![1, 2], chosen: vec![(2, vec!["here".into()])], ..Default::default() };
        assert_eq!(route_failed(&nowhere)["head"], "Unclear");
        let all = Route { others: others.clone(), burst: vec![1, 2], chosen: vec![(1, vec!["docket-core".into()]), (2, vec!["here".into()])], ..Default::default() };
        assert_eq!(route_failed(&all), split(vec![part(json!([1]), json!(["docket-core"])), part(json!([2]), json!(["here"]))]));
        // nothing chosen: as before
        let plain = Route { others, burst: vec![590, 591], fallback: vec!["paper-research".into()], ..Default::default() };
        assert!(route_ok(&json!({"head": "To", "agents": ["paper-research", "docket-core"]}), &plain));
        assert_eq!(route_failed(&plain), json!({"head": "To", "agents": ["paper-research"]}));
    }

    /// The agents on N-1104 when his p586 and p587 came (research's record, line 101197's `others`).
    const N1104: [&str; 11] = ["paper-research", "consult-lead", "here", "consult-5", "here-1", "accounts-lead", "consult-6", "docket-core", "a2h-speed", "docket-core3", "tool-scan"];
    /// His p586 (record line 101194), told to paper-research at 18:23:00 (101200) and given it at 18:23:01 (101206).
    const P586: &str = "eh yeah but wait lets analyze where we need special actions vs not . like launching a special action every time i refresh limits is bad.";
    const P587: &str = "like ofc for reset apply can be special action open a claude ocde put in the ocmmands etc";

    /// The thread's holder for his p587 (20 Nov, N-1104 p586/p587, 22 s apart; the owner's fundamentals, N-1104 p593):
    /// its report says where p586 went and that paper-research holds it in the turn it is in, which p587's `whom` did
    /// not have (consult-3 N-1443 p2, 2a); the form back joins p587 to p586, To paper-research, and is one decision
    /// about both. WHOM_PROMPT=FILE writes the prompt there, for the still.
    #[test]
    fn the_holder_of_n1104_sees_p586_where_it_went_when_p587_comes() {
        use serde_json::json;
        let p586 = Yours { n: 586, at: "2026-11-20T18:22:55Z".into(), went: Some(vec!["paper-research".into()]), given: Some("2026-11-20T18:23:01Z".into()), text: P586.into() };
        let (yours, his) = yours_lines(&[p586], "2026-11-20T18:23:17Z");
        assert_eq!(his, vec![586]);
        assert_eq!(yours, format!("#586 at 18:22:55Z, 22 s before this one, went to paper-research (given 18:23:01Z): {P586}"));
        let thread = thread_line("N-1104", "please onboard the new guys or fok existing staff", &["paper-research".into()], None, &[("paper-research".into(), vec![586]), ("here".into(), vec![])]);
        assert_eq!(thread, "N-1104 \"please onboard the new guys or fok existing staff\", opened for paper-research. paper-research has been given #586 in the turn it is in and has not posted here since.");
        let r = Route { to: vec![], others: N1104.iter().map(|a| a.to_string()).collect(), recent: "#584 owner, to paper-research: > One question for you…\n#585 paper-research, to owner: Yes…".into(), thread, yours, his, fallback: vec!["paper-research".into()], burst: vec![], chosen: vec![] };
        let root = env!("CARGO_MANIFEST_DIR");
        let calls = crate::policy::call::Calls::load(&[format!("{root}/../../agent-base/calls")], &Default::default()).unwrap();
        let p = calls.render_step("whom", &json!({"text": P587, "to": "nobody (it was posted on the ticket itself)", "agents": N1104.join(", "), "recent": r.recent, "thread": r.thread, "yours": r.yours})).unwrap();
        if let Ok(f) = std::env::var("WHOM_PROMPT") {
            std::fs::write(f, &p).unwrap();
        }
        assert!(p.contains("\nThe ticket: N-1104 \"please onboard the new guys or fok existing staff\", opened for paper-research. paper-research has been given #586"), "{p}");
        assert!(p.contains("with where each went:\n#586 at 18:22:55Z, 22 s before this one, went to paper-research (given 18:23:01Z): eh yeah"), "{p}");
        // the form back (made up: no model is called in a test), checked as coordd checks it: To paper-research, joins 586
        let got = json!({"head": "To", "agents": ["paper-research"], "joins": 586});
        assert!(route_ok(&got, &r));
        assert_eq!(joins_kept(got.clone(), &r, "O-1"), got);
        // the head's shape takes it, and an answer with no joins, as every answer before it
        let sigs = docket_machine::syntax::parse_calls("route.call", include_str!("../../../agent-base/calls/route.call"), &mut Default::default()).unwrap();
        for call in ["whom", "route"] {
            let head = sigs.iter().find(|c| c.name == call).unwrap().head.clone();
            assert!(head.conforms(&got).is_ok() && head.conforms(&json!({"head": "To", "agents": ["paper-research"]})).is_ok(), "{call}");
        }
        // joins naming a post that is not one of his just before, or on Unclear, is left out; the To stands
        assert_eq!(joins_kept(json!({"head": "To", "agents": ["paper-research"], "joins": 585}), &r, "O-1"), json!({"head": "To", "agents": ["paper-research"]}));
        assert_eq!(joins_kept(json!({"head": "To", "agents": ["paper-research"], "joins": null}), &r, "O-1"), json!({"head": "To", "agents": ["paper-research"]}));
        assert_eq!(joins_kept(json!({"head": "Unclear", "why": "?", "joins": 586}), &r, "O-1"), json!({"head": "Unclear", "why": "?"}));
    }

    /// N-1445 "Brain fundamentals" (record lines 103218-103418): opened under N-1104 for paper-research-brain, its #2
    /// told to it; his #3, typed to nobody, went to paper-research, who opened it. The holder's report says whom the
    /// thread is for and where #2 went.
    #[test]
    fn the_holder_of_n1445_knows_whom_the_thread_is_for() {
        let thread = thread_line("N-1445", "Brain fundamentals", &["paper-research-brain".into()], Some("N-1104"), &[("paper-research-brain".into(), vec![]), ("paper-research".into(), vec![])]);
        assert_eq!(thread, "N-1445 \"Brain fundamentals\", under N-1104, opened for paper-research-brain.");
        // #2 "by owner" is paper-research's opener posted again as his (thread.kind's Named open; proto-reader N-1446
        // p71): route_job leaves it out (ticket::the_holder_leaves_out_an_agents_opener…), so #3's YOURS has nothing
        // and `joins: 2` is none of his; should the call fail, #3 goes to the thread's `to`, paper-research-brain
        let (yours, his) = yours_lines(&[], "2026-11-20T19:09:17Z");
        assert_eq!((yours.as_str(), his), ("(none)", vec![]));
        let r = Route { to: vec![], others: vec!["paper-research-brain".into(), "paper-research".into()], thread, yours, fallback: vec!["paper-research-brain".into()], ..Default::default() };
        let joined = serde_json::json!({"head": "To", "agents": ["paper-research-brain"], "joins": 2});
        assert_eq!(joins_kept(joined, &r, "N-1445"), serde_json::json!({"head": "To", "agents": ["paper-research-brain"]}));
        assert_eq!(route_failed(&r), serde_json::json!({"head": "To", "agents": ["paper-research-brain"]}));
        // a post of his given before the job: the time of its `given` line as the record has it when the job is made
        // (19:09:14Z; the `delivered` event's own 19:09:19Z came after: proto-reader N-1446 p71, 2)
        let p = Yours { n: 4, at: "2026-11-20T19:07:16Z".into(), went: Some(vec!["paper-research-brain".into()]), given: Some("2026-11-20T19:09:14Z".into()), text: "a post of his".into() };
        let (yours, his) = yours_lines(&[p], "2026-11-20T19:09:17Z");
        assert_eq!(his, vec![4]);
        assert_eq!(yours, "#4 at 19:07:16Z, 121 s before this one, went to paper-research-brain (given 19:09:14Z): a post of his");
        // a post a handler still holds says so; none before: (none)
        let held = Yours { n: 7, at: "x".into(), went: None, given: None, text: "a".into() };
        assert_eq!(yours_lines(&[held], "y").0, "#7 at x, held: whom it is for is not said yet: a");
        assert_eq!(yours_lines(&[], "y"), ("(none)".to_string(), vec![]));
    }

    #[test]
    fn an_unsure_verdict_names_two_or_more_open_tickets_once_each() {
        use serde_json::json;
        let open = "N-1 thread: add median\nN-4 thread: mean([]) crashes\nD-9 decision: go ahead?";
        assert!(verdict_ok(&json!({"head": "Unsure", "tickets": ["N-1", "N-4"]}), open));
        assert!(!verdict_ok(&json!({"head": "Unsure", "tickets": ["N-1"]}), open), "one ticket is a Place, not a pick");
        assert!(!verdict_ok(&json!({"head": "Unsure", "tickets": ["N-1", "N-1"]}), open));
        assert!(!verdict_ok(&json!({"head": "Unsure", "tickets": ["N-1", "N-7"]}), open), "N-7 is not open");
        assert!(!verdict_ok(&json!({"head": "Unsure"}), open));
        assert!(verdict_ok(&json!({"head": "Place", "ticket": "N-4"}), open));
        assert!(!verdict_ok(&json!({"head": "Place", "ticket": "N-7"}), open));
        assert!(verdict_ok(&json!({"head": "Thread"}), open));
        assert!(!verdict_ok(&json!({"head": "OneOff"}), open), "no one-off since §16");
    }

    const TEXT: &str = "yes go ahead with the fix. and on the median question: not now, keep calc small. also ping the lead";

    #[test]
    fn passages_become_spans_of_the_owners_text() {
        let ps = vec!["yes go ahead with the fix.".to_string(), "not now, keep calc small.".to_string()];
        let s = spans(TEXT, &ps).unwrap();
        assert_eq!(&TEXT[s[0].0..s[0].0 + s[0].1], "yes go ahead with the fix.");
        assert_eq!(&TEXT[s[1].0..s[1].0 + s[1].1], "not now, keep calc small.");
        // what is left: the lead-in to the second passage and the last sentence, each said
        let left: Vec<&str> = left_out(TEXT, &s).iter().map(|&(a, l)| &TEXT[a..a + l]).collect();
        assert_eq!(left, vec!["and on the median question:", "also ping the lead"]);
    }

    #[test]
    fn a_rewritten_passage_is_refused() {
        let e = spans(TEXT, &["Yes, go ahead with the fix.".to_string()]).unwrap_err();
        assert!(e.contains("not the owner's words"), "{e}");
    }

    #[test]
    fn full_cover_leaves_nothing_out() {
        let s = spans("a b. c d.", &["a b.".into(), "c d.".into()]).unwrap();
        assert!(left_out("a b. c d.", &s).is_empty());
    }

    /// The gateway refuses gpt-6.2-sol, the one model the policy allows for `clean` (decisions.eflint): no second model
    /// is asked, and the watcher's step answers Sure with his words as he typed them, nothing held back and nothing
    /// lost, and says why on the ticket (paper-research N-1397.p27, p30).
    #[test]
    fn a_clean_no_model_answers_gives_his_words_as_he_typed_them() {
        use serde_json::json;
        let seen = crate::clean::tests::stand_in(|_| serde_json::Value::Null);
        let typed = "se N-487 now, then `cargo test`  twice.\n\n> his quote, as it was\n\nand teh last line ";
        let job = super::Job {
            worker: docket_machine::WATCHER.into(),
            to: "w2".into(),
            t: 0,
            step: 1,
            ticket: "N-1".into(),
            text: typed.into(),
            open: String::new(),
            split: None,
            places: crate::clean::tests::places(),
            route: None,
            clean: Some(vec![]),
            brain: None,
            fold: None,
        };
        let (head, decision, why) = super::clean(&job, &[]);
        assert_eq!(head, json!({"head": "Sure", "cleaned": typed}), "letter for letter");
        assert!(decision.is_none());
        let why = why.expect("why is said on the ticket");
        assert!(why.starts_with("paragraph 1 of 2: "), "{why}");
        let asked: Vec<String> = seen.lock().unwrap().iter().map(|r| r["model"].as_str().unwrap_or_default().to_string()).collect();
        assert!(!asked.is_empty() && asked.iter().all(|m| m == "gpt-6.2-sol"), "no second model: {asked:?}");
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }
}
