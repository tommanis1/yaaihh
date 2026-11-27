//! Following a Claude Code session's transcript: the one record coordd takes a Claude agent's turns, running tool calls
//! and seen mail from (design coordd-core-2026-11-10 §3). Hooks only say "look now"; a hook that comes late or never
//! costs a poll, not a wrong state. Delivery itself is the channel's acknowledgement (§4); the transcript showing a
//! mail adds nothing but a delivery the channel did not report (a session from before).
//!
//! A turn runs from a user or assistant entry to its end: the session's stop as the transcript records it (its Stop
//! hook's `stop_hook_summary`, Claude Code's `turn_duration` record), an interrupt (`[Request interrupted by user]`)
//! or an API error message. An assistant message with `stop_reason: end_turn` ends nothing by itself: the session
//! may write on with no prompt (consult-3, N-807.p63), and its final output is the text since the last user entry
//! when the stop comes. A subagent's entries in the same file (`isSidechain`) are none of the main turn's. A tool
//! call runs from its `tool_use` block to the `tool_result` that answers it; a turn's end ends any left. Meta
//! entries other than channel messages, compaction summaries and slash-command echoes start no turn. Mail arrives as a channel entry
//! (`<channel source="coord" ... id="m12">`) or, in sessions from before, inside a hook's "Mail that arrived for you"
//! text (`[m12 · from X]`). A channel message the session cannot take now (a menu open, a turn running) is queued
//! (`queue-operation` enqueue) and taken later: the session has it, so it counts as delivered then (9 Nov: orch's
//! open model menu queued 34 mails, which went again twice while coordd waited for them as entries).

use coord_types::Tool;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

/// How much of a transcript a new follower reads first: enough for the last turn's state and the mail it took.
const WINDOW: u64 = 8 << 20;

/// Where a transcript stands, and the mail ids it showed since the last read.
pub struct Follower {
    pub path: String,
    offset: u64,
    rest: Vec<u8>,
    /// Whether no turn runs (the last turn-marking entry ended one, or there is none).
    pub ended: bool,
    /// When the entry that began the turn running was written (its `timestamp`), when one runs.
    pub running_since: String,
    /// The tool calls running: begun (`tool_use`) and not answered (`tool_result`) in the turn running.
    pub tools: BTreeMap<String, Tool>,
    /// The turn running began on a prompt (a message, a background task's notice; not a tool's result) and nothing of
    /// the model's has followed it: [`Follower::unanswered`].
    prompt_unanswered: bool,
    /// When the transcript last grew.
    pub grew: Instant,
    /// The last model picked with `/model` in the session, and when (the entry's `timestamp`).
    pub model: Option<(String, String)>,
    /// The last compaction point set with `/autocompact` in the session, and when ([`Typed`]).
    pub compact: Option<(Typed, String)>,
    /// The first read starts inside a line (a window that does not begin at the file's start).
    skip_partial: bool,
    /// Where the assistant's text since the last user entry began (its first entry's offset), while it goes on.
    said_from: Option<u64>,
    /// Where the last of that text ends (past its entry).
    said_to: u64,
    /// The file's length when this follower began: a final output within it is from a turn that ended before (the
    /// window read when coordd starts, or when an agent comes back), never a new turn's end. A position, not a flag
    /// set on the first take: a window ending with no output once cost the next turn's end (w1 on docket-live, 12:59),
    /// and a new session's file is all new.
    born: u64,
    /// The turn running (or the last one run) began before `born`: its first running entry is in the old window.
    old_turn: bool,
    /// The span (offset, length) of the last ended turn's final output: the assistant entries after its last user
    /// entry. Taken once by [`Follower::take_output`] at the turn's end (a ticket step's answer, owner m613).
    output: Option<(u64, u64)>,
    /// The final outputs of turns that ended, the next one begun, before they were taken: a delivery Claude Code
    /// queued in a turn is its next prompt at once, and in one read the old turn's end and the new turn's first entry
    /// both show; its output, wiped by that entry, was posted nowhere (consult-3, N-807.p61).
    done: Vec<(u64, u64)>,
    /// The final output of the turn the window read at this follower's start ended with, nothing after it: kept by
    /// [`Follower::take_output`] for [`Follower::take_start_end`] (paper-research N-1397 p43).
    start_end: Option<(u64, u64)>,
    /// The session's queue of coord messages: each one Claude Code wrote as `queue-operation enqueue` and has not yet
    /// put into the model's input, as (offset, length, text) of its enqueue line, oldest first. Given, not delivered
    /// (0.2 A1, N-1397 post 5): the text enters at its `remove` (into the running turn) or as a prompt (a turn begun).
    queue: Vec<(u64, u64, String)>,
    /// What the turn running did so far (insight (1)): its own tool calls by name, counted from its begin, and the
    /// model that answered last (an assistant message's `model`, a Codex rollout's `turn_context`).
    turn: docket_machine::Worked,
    /// What each turn that ended since the last take did, by where its final output ends ([`Follower::take_worked`]);
    /// none for a turn begun before this follower began, whose calls it did not all read.
    worked: Vec<(u64, docket_machine::Worked)>,
    /// When the last user entry was written (a prompt, a tool result, a channel message): the input the next reply
    /// answers, asked then.
    asked_at: Option<String>,
    /// The replies the API gave since the last take ([`Follower::take_answers`]): the model that answered each, and
    /// when the input it answers was written. A Claude Code session's only: not its own `<synthetic>` words, not an
    /// API error.
    answers: Vec<(String, String)>,
}

/// A delivery that entered the model's input (0.2 A1: delivered is the text in the model's input, not queued; the
/// owner's "read is wrong though you mean delivered").
#[derive(Clone, Debug, PartialEq)]
pub struct Entered {
    /// The delivery id (`N-412.p991`); empty for the owner's words alone (render's `words`, no id), whose delivery is
    /// the giving placed at `place` (`Core::entry_is`).
    pub id: String,
    /// Where it was given: its own enqueue line, paired by its text, as (offset, length); the entry's own place when
    /// no enqueue was read (a Codex rollout's user message, a hook's mail). The giving's `where` holds the same place,
    /// so a fact never goes to another giving of the same id (consult-3, N-1397 post 6, 1).
    pub place: (u64, u64),
    /// The entry it entered by (the `remove`, the user entry): its offset.
    pub entry: u64,
    /// When, as the session wrote it (the entry's `timestamp`, to the second, `Z`).
    pub at: String,
    /// It entered as the prompt that began a turn (not by `remove` into the running one), written since this follower
    /// began: that turn holds it, whatever a cut before it forgot (an esc with it queued, N-1397 post 6, 2).
    pub began: bool,
}

/// What one read found.
#[derive(Default)]
pub struct Seen {
    /// The mail ids of the entries read, in order.
    pub seen: Vec<String>,
    pub grew: bool,
    /// Whether the running tool calls changed.
    pub tools: bool,
    /// The entries that carried ids, with their place: (id, offset, length). A ticket delivery's place is its `Loc`.
    pub at: Vec<(String, u64, u64)>,
    /// The coord channel entries with no id (a delivery rendered without one: the owner's words to an agent that holds
    /// only them, design-two-systems §4), as (body, offset, length): `Core::bare_seen` takes each as the delivery
    /// handed before it, by position, its body the check.
    pub bare: Vec<(String, u64, u64)>,
    /// Where the last turn cut short in this read was cut: the offset of its '[Request interrupted by user' entry. That
    /// turn has no final output, so its end is a cut (`Core::ticket_turn_cut`), taken after what was seen before it
    /// and before what was seen after it (docket-edge m1153: an esc's words waited a minute behind the cut turn's).
    pub cut: Option<u64>,
    /// The prompts typed by hand into the session (its terminal), not given through coord, as (offset, length):
    /// a user entry with text that is no coord delivery (no channel tag, no `[ID · from ...]` head, no Stop hook's
    /// mail), no tool result, no command, no reminder, no compaction summary; written after this follower began
    /// (`Core::typed`, paper-research N-101 p1107).
    pub typed: Vec<(u64, u64)>,
    /// Where a turn began after one had ended, in this read: the offset of its first user entry (a prompt, a queued
    /// delivery, a background task's notice), which no prompt hook may announce (`Core::transcript_read`).
    pub begins: Vec<u64>,
    /// The turn the transcript shows running at the end of this read began before this follower did (in the window a
    /// start reads): coordd's count already holds it, so its first entry is seen and no begin of it is in `begins`
    /// (consult-3, N-412.p1690).
    pub running_old: bool,
    /// The deliveries that entered the model's input in this read, in order ([`Entered`]).
    pub entered: Vec<Entered>,
    /// At each turn's end in this read (where its final output ends, as `Core::transcript_read` marks it), the places
    /// of the coord messages still queued then, as (offset, length): given, not in its input, so the next turn's.
    pub queued: Vec<(u64, Vec<(u64, u64)>)>,
    /// A compaction's boundary was written since this follower began: the compaction is over, whether or not its hook
    /// said so (`Core::compaction_over`).
    pub compacted: bool,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    subtype: String,
    #[serde(rename = "isMeta", default)]
    meta: bool,
    #[serde(rename = "isCompactSummary", default)]
    compact: bool,
    #[serde(rename = "isApiErrorMessage", default)]
    api_error: bool,
    #[serde(rename = "isSidechain", default)]
    sidechain: bool,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    timestamp: String,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: serde_json::Value,
    #[serde(default)]
    stop_reason: Option<String>,
    /// The model that answered (an assistant message's); `<synthetic>` for a message Claude Code wrote itself.
    #[serde(default)]
    model: Option<String>,
}

enum Mark {
    Running,
    Ended,
}

/// What one entry says: of the turn (None: nothing), the mail ids it carries, the tool calls it begins and the ones
/// it answers.
#[derive(Default)]
struct Said {
    mark: Option<Mark>,
    ids: Vec<String>,
    began: Vec<Tool>,
    answered: Vec<String>,
    /// The model a `/model` pick set, and when.
    model: Option<(String, String)>,
    /// The compaction point an `/autocompact` set, and when.
    compact: Option<(Typed, String)>,
    /// A user entry (a prompt, a tool result, a channel message): the assistant's text after it is a new output.
    user: bool,
    /// An assistant entry with text in it.
    text: bool,
    /// A coord channel entry with no id: its body.
    bare: Option<String>,
    /// The turn was interrupted ('[Request interrupted by user'): it ended with no output.
    cut: bool,
    /// A prompt typed by hand (`Seen::typed`).
    typed: bool,
    /// A turn's end that closes the text said since the last user entry as its final output (Codex's `task_complete`,
    /// a line of its own after the assistant's message; Claude Code's final message carries its own end).
    closes: bool,
    /// What the entry does to the session's queue of coord messages.
    queue: Queue,
    /// A compaction's boundary (`compact_boundary`, written when it is over).
    compacted: bool,
    /// The tools it calls, by name (an assistant entry's `tool_use` blocks, a Codex rollout's function calls): what
    /// the turn did, counted for insight (1) ([`Follower::turn`]).
    calls: Vec<String>,
    /// The model that answered it: an assistant entry's own, not Claude Code's (`<synthetic>`); a Codex rollout's
    /// `turn_context`.
    answered_by: Option<String>,
    /// A Claude Code assistant entry the API gave (`answered_by` its model): a reply that came through.
    answer: bool,
    /// When a Claude Code user or assistant entry was written (its `timestamp`).
    at: String,
}

/// What an entry does to the session's queue of coord messages ([`Follower::queue`]).
#[derive(Default)]
enum Queue {
    #[default]
    None,
    /// A coord message enqueued (`queue-operation enqueue`): its text.
    In(String),
    /// A coord message taken into the model's input: its TEXT, the delivery IDS it carries, whether it is a PROMPT
    /// (a user entry that is no tool result, a Codex user message) or went into the running turn (`remove`, whose
    /// `queued_command` attachment carries the enqueue's time, not its own), and WHEN the entry was written.
    Took { text: String, ids: Vec<String>, prompt: bool, when: String },
}

impl Follower {
    /// Follow PATH from its last [`WINDOW`] bytes (the start of a line within them).
    pub fn new(path: &str) -> Follower {
        let len = std::fs::metadata(path).map_or(0, |m| m.len());
        let mut f = Follower { path: path.into(), offset: len.saturating_sub(WINDOW), rest: Vec::new(), ended: true, running_since: String::new(), tools: BTreeMap::new(), prompt_unanswered: false, grew: Instant::now(), model: None, compact: None, skip_partial: false, said_from: None, said_to: 0, output: None, done: Vec::new(), start_end: None, born: len, old_turn: false, queue: Vec::new(), turn: Default::default(), worked: Vec::new(), asked_at: None, answers: Vec::new() };
        f.skip_partial = f.offset > 0;
        f
    }

    /// Read what was written since the last read: the state it leaves and the mail ids it shows.
    pub fn read(&mut self) -> Seen {
        let mut out = Seen::default();
        let Ok(mut file) = std::fs::File::open(&self.path) else { return out };
        let len = file.metadata().map_or(0, |m| m.len());
        if len < self.offset {
            // the file was replaced or cut: read it again from its window
            *self = Follower::new(&self.path);
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return out;
        }
        let mut buf = Vec::new();
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return out;
        }
        crate::counts::read("follow (each second, running agents)", buf.len() as u64);
        // where DATA (the unfinished line kept from the last read, then BUF) begins in the file
        let start = self.offset - self.rest.len() as u64;
        self.offset += buf.len() as u64;
        out.grew = true;
        self.grew = Instant::now();
        let mut data = std::mem::take(&mut self.rest);
        data.extend_from_slice(&buf);
        let complete = data.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        self.rest = data.split_off(complete);
        let mut pos = start;
        let mut lines = data.split(|b| *b == b'\n').map(|l| {
            let at = pos;
            pos += l.len() as u64 + 1;
            (at, l)
        });
        if std::mem::take(&mut self.skip_partial) {
            lines.next(); // the window began inside a line
        }
        // an interrupt entry from before this follower began (its window, read after a start): a cut only when no turn
        // began after it. An old esc read again at every coordd start cut the turn running then, and its closing
        // message posted nowhere (consult-3, N-807.p100: 13 of the record's 44 cuts on 19 Nov, 11 to 40 s after starts)
        let mut old_cut: Option<u64> = None;
        for (at, line) in lines.filter(|(_, l)| !l.is_empty()) {
            let Some(said) = classify(line) else { continue };
            // a reply the API gave, and when the input it answers was written: the model served then (`Core::answered`)
            if said.user && !said.at.is_empty() {
                self.asked_at = Some(said.at.clone());
            }
            if let (true, Some(m), Some(asked)) = (said.answer, &said.answered_by, &self.asked_at) {
                self.answers.push((m.clone(), asked.clone()));
            }
            let len = line.len() as u64;
            out.compacted |= said.compacted && at >= self.born;
            // what the turn running did (insight (1)): its calls counted from its begin; the model, the last to answer
            // (both start empty at the turn before's end, `keep_worked`)
            if self.ended && matches!(said.mark, Some(Mark::Running)) {
                self.turn.tools.clear();
            }
            for c in &said.calls {
                *self.turn.tools.entry(c.clone()).or_default() += 1;
            }
            if said.answered_by.is_some() {
                self.turn.model.clone_from(&said.answered_by);
            }
            match &said.queue {
                Queue::None => {}
                Queue::In(text) => self.queue.push((at, len, text.clone())),
                Queue::Took { text, ids, prompt, when } => {
                    // its own enqueue, by its text, first in first out among the same; one queued before it that a
                    // prompt passes was lost with an old process (a dequeue takes the oldest), so it is forgotten
                    let place = match self.queue.iter().position(|(_, _, t)| t == text) {
                        Some(k) => {
                            let (o, l, _) = self.queue.remove(k);
                            if *prompt {
                                self.queue.drain(..k);
                            }
                            (o, l)
                        }
                        None => (at, len),
                    };
                    let at_s = when.get(..19).map(|t| format!("{t}Z")).unwrap_or_default();
                    // the owner's words alone carry no id: one entry with none, whose delivery is the giving at PLACE
                    // (`Core::entry_is`, the one answer for both)
                    let none = [String::new()];
                    for id in if ids.is_empty() { &none[..] } else { &ids[..] } {
                        out.entered.push(Entered { id: id.clone(), place, entry: at, at: at_s.clone(), began: *prompt && at >= self.born });
                    }
                }
            }
            for id in &said.ids {
                out.at.push((id.clone(), at, len));
            }
            if let Some(b) = &said.bare {
                out.bare.push((b.clone(), at, len));
            }
            out.seen.extend(said.ids);
            if said.cut && at >= self.born {
                out.cut = Some(at);
                old_cut = None;
            } else if said.cut {
                old_cut = Some(at);
            } else if said.user || said.text || matches!(said.mark, Some(Mark::Running)) {
                // a turn going on after an old interrupt (a prompt, words, or a tool call or thinking alone, as after a
                // meta entry `classify` drops: consult-3, N-807.p102): that interrupt cut an earlier turn, not this one
                old_cut = None;
            }
            if said.typed && at >= self.born {
                out.typed.push((at, len));
            }
            if said.user {
                self.said_from = None;
            } else if said.text {
                self.said_from.get_or_insert(at);
                self.said_to = at + len;
            }
            if said.user && !matches!(said.mark, Some(Mark::Ended)) {
                // a user entry after a turn's end begins the next turn: the ended turn's output is kept for its take
                if let Some(o) = self.output.take() {
                    self.done.push(o);
                }
            }
            if said.text && matches!(said.mark, Some(Mark::Ended)) {
                let from = self.said_from.unwrap_or(at);
                self.output = Some((from, at + len - from));
                self.keep_worked();
                if !self.queue.is_empty() {
                    out.queued.push((at + len, self.queue.iter().map(|(o, l, _)| (*o, *l)).collect()));
                }
            }
            if said.closes && let Some(from) = self.said_from.take() {
                self.output = Some((from, self.said_to - from));
                self.keep_worked();
                if !self.queue.is_empty() {
                    out.queued.push((self.said_to, self.queue.iter().map(|(o, l, _)| (*o, *l)).collect()));
                }
            }
            if said.model.is_some() {
                self.model = said.model;
            }
            if said.compact.is_some() {
                self.compact = said.compact;
            }
            for t in said.began {
                self.tools.insert(t.id.clone(), t);
                out.tools = true;
            }
            let prompt = said.user && said.answered.is_empty();
            for id in said.answered {
                out.tools |= self.tools.remove(&id).is_some();
            }
            match said.mark {
                Some(Mark::Running) => {
                    // a begin in the window read at a start is an old turn's, never one beginning now: each would end and
                    // begin a turn in coordd's count (consult-3, N-412.p1690)
                    if self.ended {
                        self.old_turn = at < self.born;
                        if said.user && at >= self.born {
                            out.begins.push(at);
                        }
                    }
                    #[derive(Deserialize)]
                    struct At {
                        #[serde(default)]
                        timestamp: String,
                    }
                    self.ended = false;
                    self.running_since = serde_json::from_slice::<At>(line).map(|a| a.timestamp).unwrap_or_default();
                    // a user entry with no tool result begins on a prompt; the model's own entry answers it
                    self.prompt_unanswered = prompt;
                }
                Some(Mark::Ended) => {
                    self.ended = true;
                    if !self.tools.is_empty() {
                        self.tools.clear();
                        out.tools = true;
                    }
                }
                None => {}
            }
        }
        // an interrupt written while coordd was down, with nothing after it: the turn it cut ended so
        if out.cut.is_none() {
            out.cut = old_cut;
        }
        out.running_old = !self.ended && self.old_turn;
        out
    }
}

impl Follower {
    /// Whether the transcript ends in a turn begun on a prompt that nothing of the model's has followed: no request
    /// sent, or none come back. A session resumed onto a background task's notice from its last process wrote it and
    /// then sat at its prompt (research, 25 Nov 21:32 CET: docket-core and docket-edge5, 75 and 87 min working).
    pub fn unanswered(&self) -> bool {
        !self.ended && self.prompt_unanswered
    }

    /// The last ended turn's final output as (offset, length) in [`Follower::path`], once.
    /// An output that ends within what the file held when this follower began is old: not taken. The window's last,
    /// its turn ended with nothing after it, is kept for [`Follower::take_start_end`].
    pub fn take_output(&mut self) -> Option<(u64, u64)> {
        let out = self.output.take()?;
        if out.0 + out.1 <= self.born && self.ended {
            self.start_end = Some(out);
        }
        // whether the turn was given a ticket delivery is the machine's count (`Core::ticket_turn_ended`), not a flag here
        (out.0 + out.1 > self.born).then_some(out)
    }

    /// The final output of the turn the window read at this follower's start ended with, nothing after it (no later
    /// prompt), as (offset, length), once: a turn that ended while coordd was stopped, its Stop hook with no coordd to
    /// reach (paper-research N-1397 p43). Whether a post is owed for it is the record's count (`Core::start_output`).
    pub fn take_start_end(&mut self) -> Option<(u64, u64)> {
        self.start_end.take()
    }

    /// The text said since the last user entry, as (offset, length), when there is any written since this follower
    /// began: a turn's final output when its end is taken without the transcript's stop (`main`: the hooks, the
    /// screen).
    pub fn said_span(&self) -> Option<(u64, u64)> {
        self.said_from.map(|f| (f, self.said_to - f)).filter(|o| o.0 + o.1 > self.born)
    }

    /// The coord messages the session holds in its queue now: enqueued, not yet taken into the model's input, as
    /// (offset, length) of each enqueue (0.2 A4: rule 19 lets these wait).
    pub fn queued_now(&self) -> Vec<(u64, u64)> {
        self.queue.iter().map(|(o, l, _)| (*o, *l)).collect()
    }

    /// Every turn's final output since the last take, in order: those of turns a next turn followed in the same read
    /// (`done`), then the last ended turn's ([`Follower::take_output`]). Old ones (within what the file held when this
    /// follower began) are not taken.
    pub fn take_outputs(&mut self) -> Vec<(u64, u64)> {
        let born = self.born;
        let mut out: Vec<(u64, u64)> = std::mem::take(&mut self.done).into_iter().filter(|o| o.0 + o.1 > born).collect();
        out.extend(self.take_output());
        out
    }

    /// What each turn that ended since the last take did, by where its final output ends, in order (insight (1)): what
    /// coordd writes on that turn's `turn_ended` line when it takes its end (`Core::ticket_turn_ended_queued`).
    pub fn take_worked(&mut self) -> Vec<(u64, docket_machine::Worked)> {
        std::mem::take(&mut self.worked)
    }

    /// The replies the API gave since the last take, in order: each one's model, and when the input it answers was
    /// written (RFC 3339), for the agents held on a rate limit of that model (`Core::answered`).
    pub fn take_answers(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.answers)
    }

    /// The turn whose final output was just set ended: what it did is kept by where that output ends, unless it began
    /// before this follower did (its calls before the window read are not counted, so it would count too few). The next
    /// turn's tally starts empty here, not at its begin: a model named before its begin (Codex's `turn_context`) is its
    /// own, and a turn whose transcript names none is not known, never the turn before's model (consult-3 N-1446 p541, 2).
    fn keep_worked(&mut self) {
        let turn = std::mem::take(&mut self.turn);
        if let (Some((o, l)), false) = (self.output, self.old_turn) {
            self.worked.push((o + l, turn));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    /// Where a fork cuts a session (part 4, docket-core N-1446 p2096): after the entry that took its place's message,
    /// from its given line on. Past a queued message's lines to the user entry two below, which holds it; a given line
    /// that is the entry; one id of a message that joins several, its text in blocks, an assistant's mention passed; a
    /// message queued in a turn, its attachment. None when no entry in reach names it, or the line there is not whole.
    #[test]
    fn a_fork_cuts_after_the_entry_that_took_its_places_message() {
        use serde_json::json;
        let tag = |ids: &str| format!("<channel source=\"coord\" from=\"lead\" id=\"{ids}\" kind=\"thread\">\nthe hotfix\n</channel>");
        let lines = [
            json!({"type": "queue-operation", "operation": "enqueue", "content": tag("N-1.p1")}).to_string(),
            json!({"type": "queue-operation", "operation": "dequeue"}).to_string(),
            json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": tag("N-1.p1")}, "uuid": "u1"}).to_string(),
            json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": tag("N-1.p3")}]}, "uuid": "u2"}).to_string(),
            json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": tag("N-1.p2 N-1.p3")}]}, "uuid": "u3"}).to_string(),
            json!({"type": "attachment", "attachment": {"type": "queued_command", "prompt": tag("N-1.p4")}, "uuid": "u4"}).to_string(),
        ];
        let end = |n: usize| lines[..=n].iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        let p = std::env::temp_dir().join(format!("follow-fork-{}.jsonl", std::process::id()));
        let f = p.to_str().unwrap().to_string();
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        assert_eq!(super::fork_cut(&f, 0, "N-1.p1"), Some(end(2)), "past the queue lines, after the user entry");
        assert_eq!(super::fork_cut(&f, end(1), "N-1.p1"), Some(end(2)), "the given line is the entry");
        assert_eq!(super::fork_cut(&f, end(2), "N-1.p3"), Some(end(4)), "one of its ids, in blocks; the assistant's passed");
        assert_eq!(super::fork_cut(&f, end(4), "N-1.p4"), Some(end(5)), "queued in a turn: its attachment");
        assert_eq!(super::fork_cut(&f, end(2), "N-1.p1"), None, "none after it names it");
        // the entry's line not whole yet
        std::fs::write(&p, lines[..3].join("\n")).unwrap();
        assert_eq!(super::fork_cut(&f, 0, "N-1.p1"), None);
        // past the lines read
        let filler = json!({"type": "progress"}).to_string() + "\n";
        std::fs::write(&p, filler.repeat(super::FORK_SCAN) + &lines[2] + "\n").unwrap();
        assert_eq!(super::fork_cut(&f, 0, "N-1.p1"), None);
        std::fs::write(&p, filler.repeat(super::FORK_SCAN - 1) + &lines[2] + "\n").unwrap();
        assert_eq!(super::fork_cut(&f, 0, "N-1.p1"), Some((filler.len() * (super::FORK_SCAN - 1) + lines[2].len() + 1) as u64));
        std::fs::remove_file(&p).ok();
    }

    /// The cut where the tag carries no id (paper-research N-1446 p3336, p3339; proto-mind's copy proof on engine 81,
    /// p3335): the owner's words alone, `from` only (render.rs:168), in the session of that run's shape. Two messages
    /// queued, dequeued, a meta entry of the harness's between, each message's entry with its text; then the request's
    /// lines. Each giving's enqueue is cut after the entry holding its own words, the second's past the first's; a
    /// given line that is the entry, its own; one queued in a turn, its attachment. None for words no entry in reach
    /// holds, and for a line that is no coord message.
    #[test]
    fn a_fork_cuts_after_the_entry_holding_its_givings_words_when_its_tag_carries_no_id() {
        use serde_json::json;
        let tag = |words: &str| format!("<channel source=\"coord\" from=\"owner\">\n{words}\n</channel>");
        let lines = [
            json!({"type": "queue-operation", "operation": "enqueue", "content": tag("what is 7 times 6?")}).to_string(),
            json!({"type": "queue-operation", "operation": "enqueue", "content": tag("and 8 times 9?")}).to_string(),
            json!({"type": "queue-operation", "operation": "dequeue"}).to_string(),
            json!({"type": "queue-operation", "operation": "dequeue"}).to_string(),
            json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": "<system-reminder>a harness note</system-reminder>"}, "uuid": "u0"}).to_string(),
            json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": tag("what is 7 times 6?") + "\n"}, "uuid": "u1"}).to_string(),
            json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": tag("and 8 times 9?")}, "uuid": "u2"}).to_string(),
            json!({"type": "api-request-blob", "blob": tag("what is 7 times 6?")}).to_string(),
            json!({"type": "queue-operation", "operation": "enqueue", "content": tag("one more")}).to_string(),
            json!({"type": "attachment", "attachment": {"type": "queued_command", "prompt": tag("one more")}, "uuid": "u3"}).to_string(),
        ];
        let end = |n: usize| lines[..=n].iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        let p = std::env::temp_dir().join(format!("follow-fork-words-{}.jsonl", std::process::id()));
        let f = p.to_str().unwrap().to_string();
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        assert_eq!(super::fork_cut(&f, 0, "N-1.p1"), Some(end(5)), "past the queue lines and the meta entry, after the entry with its words");
        assert_eq!(super::fork_cut(&f, end(0), "N-1.p2"), Some(end(6)), "the second's words: past the first's entry");
        assert_eq!(super::fork_cut(&f, end(4), "N-1.p1"), Some(end(5)), "the given line is the entry");
        assert_eq!(super::fork_cut(&f, end(7), "N-1.p3"), Some(end(9)), "queued in a turn: its attachment");
        assert_eq!(super::fork_cut(&f, end(3), "N-1.p1"), None, "the given line is no coord message: no words to find its entry by");
        // words no entry in reach holds: the enqueue alone
        std::fs::write(&p, lines[..4].join("\n") + "\n").unwrap();
        assert_eq!(super::fork_cut(&f, 0, "N-1.p1"), None);
        std::fs::remove_file(&p).ok();
    }

    /// One agent's side (paper-research N-1446 p1376 point 4): each `instructions` attachment's files as they stood,
    /// the first user message that is no meta entry, and the Read calls, each at its line. A request line
    /// (`api-request`) is read for its own fields only, never as an attachment, though read as JSON it would be an
    /// instructions attachment, and one with no id is no request; a subagent's Read is none of the agent's.
    #[test]
    fn an_agents_side_is_what_its_session_loaded_was_first_told_and_read() {
        let lines = [
            // read as JSON, its second `type` would make it an instructions attachment
            r#"{"type":"api-request","attachment":{"type":"instructions","files":[{"path":"/request","type":"AutoMem","content":"a prompt"}]},"type":"attachment","timestamp":"2026-11-18T10:14:21Z"}"#.to_string(),
            serde_json::json!({"attachment": {"type": "instructions", "files": [{"path": "/m/MEMORY.md", "type": "AutoMem", "content": "- one"}]}, "type": "attachment", "timestamp": "2026-11-18T10:14:22Z"}).to_string(),
            serde_json::json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": "<system-reminder>meta</system-reminder>"}, "timestamp": "2026-11-18T10:14:23Z"}).to_string(),
            serde_json::json!({"type": "user", "isCompactSummary": true, "message": {"role": "user", "content": [{"type": "text", "text": "This session is being continued"}]}, "timestamp": "2026-11-18T10:14:24Z"}).to_string(),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/w/src/a.rs"}}, {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "cat /w/b"}}]}, "timestamp": "2026-11-18T10:15:00Z"}).to_string(),
            serde_json::json!({"type": "assistant", "isSidechain": true, "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "t3", "name": "Read", "input": {"file_path": "/w/side.rs"}}]}, "timestamp": "2026-11-18T10:15:01Z"}).to_string(),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": "a later message"}, "timestamp": "2026-11-18T10:16:00Z"}).to_string(),
            serde_json::json!({"attachment": {"type": "instructions", "files": [{"path": "/m/MEMORY.md", "type": "AutoMem", "content": "- one\n- two"}, {"path": "/w/CLAUDE.md", "type": "Project", "content": "rules"}]}, "type": "attachment", "timestamp": "2026-11-18T11:00:00Z"}).to_string(),
        ];
        let p = std::env::temp_dir().join(format!("follow-side-{}.jsonl", std::process::id()));
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let side = super::side(p.to_str().unwrap(), &[]).unwrap();
        std::fs::remove_file(&p).ok();
        let loaded = |line, at: &str, kind: &str, path: &str, text: &str| super::Loaded { line, at: at.into(), kind: kind.into(), path: path.into(), text: text.into() };
        assert_eq!(side.loaded, vec![loaded(2, "2026-11-18T10:14:22Z", "AutoMem", "/m/MEMORY.md", "- one"), loaded(8, "2026-11-18T11:00:00Z", "AutoMem", "/m/MEMORY.md", "- one\n- two"), loaded(8, "2026-11-18T11:00:00Z", "Project", "/w/CLAUDE.md", "rules")]);
        assert_eq!(side.brief, Some(super::Brief { line: 4, at: "2026-11-18T10:14:24Z".into(), text: "This session is being continued".into(), compact: true }));
        assert_eq!(side.read, vec![super::Reading { line: 5, at: "2026-11-18T10:15:00Z".into(), path: "/w/src/a.rs".into() }]);
        assert!(side.requests.is_empty(), "a request line with no id is none");
        assert_eq!(super::side("/nonexistent/session.jsonl", &[]), None);
    }

    /// A session line of type T and REST's fields, its type first as Claude Code writes it (`json!` sorts its keys).
    pub(crate) fn typed(t: &str, rest: serde_json::Value) -> String {
        format!("{{\"type\":\"{t}\",{}", &rest.to_string()[1..])
    }

    /// A session's requests and where each `given`'s step began (paper-research N-1446 p1398 5): a request's fields
    /// and its shape, the first line of a shape kept; a message given at once begins its step at the next request, by
    /// order; a queued one at the request whose tail names a blob that holds its text, though its meta entry is written
    /// after (by hash), else at the first request after the entry that holds it (by order); one with no request after
    /// begins none. Two givens at one place share its step.
    #[test]
    fn a_sessions_requests_and_where_each_given_began() {
        use serde_json::json;
        let shape = |h: &str, system: serde_json::Value, tools: serde_json::Value| typed("api-request-shape", json!({"shapeHash": h, "shape": {"system": system, "tools": tools}}));
        let request = |id: &str, at: &str, shape: &str, tail: &[&str]| typed("api-request", json!({"id": id, "timestamp": at, "querySource": "repl_main_thread", "params": {"model": "claude-opus-5-5", "output_config": {"effort": "xhigh"}}, "shapeHash": shape, "tail": tail}));
        let queued = |text: &str| typed("queue-operation", json!({"operation": "enqueue", "content": text, "timestamp": "2026-11-19T09:00:02Z"}));
        let tools = json!([{"name": "Read", "input_schema": {"type": "object", "properties": {}}}]);
        let lines = [
            shape("s1", json!([{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "standing, first"}]), tools.clone()),
            json!({"type": "user", "message": {"role": "user", "content": "<channel id=\"N-1.p1\">the first post</channel>"}, "timestamp": "2026-11-19T09:00:00Z"}).to_string(),
            request("r1", "2026-11-19T09:00:01Z", "s1", &["h1"]),
            queued("<channel id=\"N-1.p2\">the second post</channel>"),
            typed("api-request-blob", json!({"hash": "b2", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}, {"type": "text", "text": "<channel id=\"N-1.p2\">the second post</channel>"}]}})),
            request("r2", "2026-11-19T09:00:03Z", "s1", &["h2", "b2"]),
            json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": "<channel id=\"N-1.p2\">the second post</channel>"}, "timestamp": "2026-11-19T09:00:02Z"}).to_string(),
            queued("<channel id=\"N-1.p3\">the third post</channel>"),
            request("r3", "2026-11-19T09:00:05Z", "s1", &["h3"]),
            json!({"type": "attachment", "attachment": {"type": "queued_command", "prompt": "<channel id=\"N-1.p3\">the third post</channel>"}, "timestamp": "2026-11-19T09:00:06Z"}).to_string(),
            shape("s2", json!([{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "standing, second"}]), tools.clone()),
            shape("s1", json!([{"type": "text", "text": "another"}]), json!([])),
            request("r4", "2026-11-19T09:00:07Z", "s2", &["h4"]),
            queued("<channel id=\"N-1.p4\">the fourth post</channel>"),
        ];
        let start = |n: usize| lines[..n - 1].iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        let p = std::env::temp_dir().join(format!("follow-requests-{}.jsonl", std::process::id()));
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let side = super::side(p.to_str().unwrap(), &[start(2), start(4), start(4), start(8), start(14)]).unwrap();
        std::fs::remove_file(&p).ok();
        let req = |line, id: &str, at: &str, shape: &str| super::Request { line, at: at.into(), id: id.into(), model: "claude-opus-5-5".into(), effort: "xhigh".into(), shape: shape.into(), tokens: None };
        assert_eq!(side.requests, vec![req(3, "r1", "2026-11-19T09:00:01Z", "s1"), req(6, "r2", "2026-11-19T09:00:03Z", "s1"), req(9, "r3", "2026-11-19T09:00:05Z", "s1"), req(13, "r4", "2026-11-19T09:00:07Z", "s2")]);
        let json_tools = r#"[{"input_schema":{"properties":{},"type":"object"},"name":"Read"}]"#;
        let shaped = |line, second: &str| super::Shape { line, at: start(line), system: vec!["You are Claude Code.".into(), second.into()], tools: super::tools_id(json_tools), tool_count: 1, tools_bytes: json_tools.len() };
        assert_eq!(side.shapes.get("s1"), Some(&shaped(1, "standing, first")), "a shape as its first line holds it, its tools' keys sorted");
        assert_eq!(side.shapes.get("s2"), Some(&shaped(11, "standing, second")));
        let began = |request, by_hash| Some(super::Began { request, by_hash });
        assert_eq!(side.began, vec![began(0, false), began(1, true), began(1, true), began(3, false), None]);
    }

    /// A system prompt block's text (paper-research N-1446 p1642, 2): the reader keeps where each shape's line starts,
    /// and the block is read from that line alone, served only when the line there is that shape's and one of its
    /// blocks is the node; a shape whose system is one string is one block. A text a start gave Claude Code (fix 4),
    /// joined onto its last block after a blank line, is that block's part after one of its blank lines, read as such
    /// only when asked for a tail; a text that begins with a blank line too.
    #[test]
    fn a_blocks_text_is_read_from_its_shapes_line_only_when_one_of_its_blocks_is_the_node() {
        use serde_json::json;
        let shape = |h: &str, system: serde_json::Value| typed("api-request-shape", json!({"shapeHash": h, "shape": {"system": system, "tools": []}}));
        let joined = json!([{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "Its own words.\n\nBe brief.\n\nAnd kind.\n"}]);
        let led = json!([{"type": "text", "text": "Own.\n\n\nLed by a blank line."}]);
        let lines = [shape("s1", json!([{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "standing"}])), shape("s2", json!("one string")), shape("s3", joined), shape("s4", led)];
        let p = std::env::temp_dir().join(format!("follow-blocks-{}.jsonl", std::process::id()));
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let f = p.to_str().unwrap();
        let side = super::side(f, &[]).unwrap();
        let (s1, s2, s3, s4) = (side.shapes["s1"].at, side.shapes["s2"].at, side.shapes["s3"].at, side.shapes["s4"].at);
        assert_eq!((s1, s2), (0, lines[0].len() as u64 + 1), "where each shape's line starts");
        let id = super::text_id;
        assert_eq!(super::block_text(f, s1, "s1", &id("standing"), false), Ok("standing".to_string()));
        assert_eq!(super::block_text(f, s2, "s2", &id("one string"), false), Ok("one string".to_string()), "one string, one block");
        assert_eq!(super::block_text(f, s1, "s2", &id("one string"), false), Err(format!("the line at byte {s1} of its session's file is another shape's")));
        assert_eq!(super::block_text(f, s1, "s1", &id("one string"), false), Err(format!("no block of that shape is {}", id("one string"))));
        assert_eq!(super::block_text(f, 5, "s1", &id("standing"), false), Err("no shape's line starts at byte 5 of its session's file".to_string()));
        // a text joined onto the last block: its tail after the blank line it was joined at, its blank line kept
        let his = id("Be brief.\n\nAnd kind.");
        assert_eq!(super::block_text(f, s3, "s3", &his, true), Ok("Be brief.\n\nAnd kind.".to_string()), "the tail, its newline at the end dropped");
        assert_eq!(super::block_text(f, s3, "s3", &his, false), Err(format!("no block of that shape is {his}")), "a tail only when asked for");
        assert_eq!(super::block_text(f, s3, "s3", &id("And kind."), true), Ok("And kind.".to_string()), "any blank line's tail");
        assert_eq!(super::block_text(f, s3, "s3", &id("Its own words."), true), Err(format!("no block of that shape ends with {}", id("Its own words."))));
        assert_eq!(super::block_text(f, s1, "s1", &id("standing"), true), Ok("standing".to_string()), "a whole block still");
        // a text that begins with a blank line, joined after one: the tail after the first two newlines (p2587, 3)
        let led = id("\nLed by a blank line.");
        assert_eq!(super::block_text(f, s4, "s4", &led, true), Ok("\nLed by a blank line.".to_string()), "its first blank line kept");
        assert_eq!(super::block_text(f, s4, "s4", &id("Led by a blank line."), true), Ok("Led by a blank line.".to_string()));
        std::fs::remove_file(&p).ok();
    }

    /// A request's tokens (p1429 2): its response's usage by `requestRef`, at the response's latest line, as coordd's
    /// usage reader counts a message; a subagent's response is none of its own; a request with no response has none,
    /// not a zero; a response that names no request line before it is said once, at its first line.
    #[test]
    fn a_requests_tokens_come_from_its_responses_usage() {
        use docket_machine::graph::Tokens;
        use serde_json::json;
        let request = |id: &str| typed("api-request", json!({"id": id, "timestamp": "2026-11-19T09:00:01Z", "params": {"model": "claude-opus-5-5", "output_config": {"effort": "xhigh"}}, "shapeHash": "s1", "tail": []}));
        let usage = |input: u64, output: u64, thinking: u64| json!({"input_tokens": input, "output_tokens": output, "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 100, "output_tokens_details": {"thinking_tokens": thinking}, "service_tier": "standard"});
        let response = |r: &str, sidechain: bool, u: serde_json::Value| json!({"type": "assistant", "requestRef": r, "isSidechain": sidechain, "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "ok"}], "usage": u}, "timestamp": "2026-11-19T09:00:02Z"}).to_string();
        let lines = [
            response("r0", false, usage(1, 1, 0)),
            response("r0", false, usage(1, 1, 0)),
            request("r1"),
            response("r1", false, usage(10, 5, 2)),
            response("r1", false, usage(10, 7, 2)),
            response("r1", true, usage(10, 999, 0)),
            request("r2"),
            request("r3"),
            response("r3", false, usage(20, 9, 0)),
            response("r9", false, usage(1, 1, 0)),
        ];
        let p = std::env::temp_dir().join(format!("follow-tokens-{}.jsonl", std::process::id()));
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let side = super::side(p.to_str().unwrap(), &[]).unwrap();
        std::fs::remove_file(&p).ok();
        let tokens = |input, output, reasoning| Some(Tokens { input, output, cache_read: 1000, cache_write: 100, reasoning });
        let got: Vec<(&str, Option<Tokens>)> = side.requests.iter().map(|q| (q.id.as_str(), q.tokens)).collect();
        assert_eq!(got, vec![("r1", tokens(10, 7, 2)), ("r2", None), ("r3", tokens(20, 9, 0))], "each request at its response's latest line, a subagent's apart; none where no response came");
        assert_eq!(side.unrequested, vec![1, 10], "each response naming no request line before it, once, at its first line");
    }

    /// A text's next version in words (p1458 point 3): the lines that differ, then their words, as git's
    /// `--word-diff=plain` writes them, with the words that stay around each change and `…` for the rest; at most 400
    /// words removed and 400 added, the cut said; a run too long to compare written as removed then added, and said; a
    /// change of spacing alone said.
    #[test]
    fn a_texts_next_version_is_the_words_that_changed() {
        use super::difference;
        assert_eq!(difference("a b c\nd e f\ng", "a b c\nd x f\ng"), "1 word removed, 1 added: a b c d [-e-] {+x+} f g");
        assert_eq!(difference("- one", "- one\n- two"), "2 words added: - one {+- two+}");
        let around = format!("{} a {}", (0..20).map(|i| format!("s{i}")).collect::<Vec<_>>().join(" "), (20..40).map(|i| format!("s{i}")).collect::<Vec<_>>().join(" "));
        assert_eq!(difference(&around, &around.replace(" a ", " b ")), "1 word removed, 1 added: … s14 s15 s16 s17 s18 s19 [-a-] {+b+} s20 s21 s22 s23 s24 s25 …");
        assert_eq!(difference("a  b\nc", "a b c"), "the same words: only their spacing or line breaks changed");
        let words = |w: &str, n: usize| (0..n).map(|i| format!("{w}{i}")).collect::<Vec<_>>().join(" ");
        let cut = difference(&words("w", 500), &words("v", 450));
        assert!(cut.starts_with("500 words removed, 450 added: [-w0 w1 w2 ") && cut.contains(" w399 …-] {+v0 v1 ") && cut.contains(" v399 …+} …"), "{cut}");
        assert!(!cut.contains("w400") && !cut.contains("v400") && cut.ends_with(" (cut there: at most 400 words removed and 400 added are written)"), "{cut}");
        let long = difference(&format!("p {} r", words("q", 2100)), &format!("s {} t", words("q", 2100)));
        assert!(long.starts_with("2102 words removed, 2102 added (too many to compare word by word: written as removed, then added): [-p q0 q1 "), "{}", long.chars().take(200).collect::<String>());
    }

    /// One agent's side in the graph (p1376 point 4): only a session a `given` line names for an agent picked, the
    /// session → its agent at the first such line; its loads, brief and reads at the session's own lines; a text that
    /// stood the same twice is one node with one `version_of`, and the request after it ran with it; a session that
    /// does not open is said.
    #[test]
    fn an_agents_side_goes_in_the_graph_from_the_sessions_its_given_lines_name() {
        use docket_machine::graph::{EdgeKind, Graph, NodeKind, Source};
        let p = std::env::temp_dir().join(format!("follow-side-graph-{}.jsonl", std::process::id()));
        let load = |at: &str| serde_json::json!({"attachment": {"type": "instructions", "files": [{"path": "/m/MEMORY.md", "type": "AutoMem", "content": "- one"}]}, "type": "attachment", "timestamp": at}).to_string();
        let session = [
            load("2026-11-18T10:14:22Z"),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": "the brief"}, "timestamp": "2026-11-18T10:14:24Z"}).to_string(),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/w/a.rs"}}]}, "timestamp": "2026-11-18T10:15:00Z"}).to_string(),
            load("2026-11-18T11:00:00Z"),
            typed("api-request-shape", serde_json::json!({"shapeHash": "s1", "shape": {"system": [{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "standing"}], "tools": []}})),
            typed("api-request", serde_json::json!({"id": "r1", "timestamp": "2026-11-18T11:00:01Z", "params": {"model": "claude-opus-5-5", "output_config": {"effort": "xhigh"}}, "shapeHash": "s1", "tail": ["h1"]})),
            serde_json::json!({"type": "assistant", "requestRef": "r1", "message": {"role": "assistant", "content": [{"type": "text", "text": "done"}], "usage": {"input_tokens": 3, "output_tokens": 40, "cache_read_input_tokens": 900, "cache_creation_input_tokens": 60}}, "timestamp": "2026-11-18T11:00:09Z"}).to_string(),
        ];
        std::fs::write(&p, session.join("\n") + "\n").unwrap();
        let f = p.to_str().unwrap().to_string();
        let brief_at = session[0].len() + 1;
        let given = |to: &str, file: &str, upto: u32| serde_json::from_value::<docket_machine::Line>(serde_json::json!({"ev": "given", "at": "2026-11-18T12:55:51Z", "t": 412, "to": to, "delivery": format!("N-412.p{upto}"), "upto": upto, "where": {"file": file, "offset": brief_at, "len": 10}})).unwrap();
        let lines = vec![given("b", "/elsewhere/b.jsonl", 120), given("a", &f, 120), given("a", &f, 121), given("a", "/nonexistent/c.jsonl", 122)];
        let mut g = Graph::default();
        let unopened = super::side_into(&mut g, &lines, &|a| a == "a", &|t, n| format!("N-{t}.p{n}"));
        std::fs::remove_file(&p).ok();
        assert!(g.get("session:b").is_none(), "a session given to an agent not picked is not read");
        assert_eq!(unopened, vec!["/nonexistent/c.jsonl".to_string()]);
        let id = p.file_stem().unwrap().to_str().unwrap().to_string();
        let (s, one, brief) = (format!("session:{id}"), super::text_id("- one"), super::text_id("the brief"));
        let at = |n| Source::Session(id.clone(), n);
        let edges: Vec<(String, String, EdgeKind, Source)> = g.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.source.clone())).collect();
        assert_eq!(edges, vec![
            (s.clone(), "agent:a".into(), EdgeKind::SessionOf, Source::Line(2)),
            (s.clone(), one.clone(), EdgeKind::Loaded, at(1)),
            (one.clone(), "file:/m/MEMORY.md".into(), EdgeKind::VersionOf, at(1)),
            (s.clone(), one.clone(), EdgeKind::Loaded, at(4)),
            (s.clone(), brief.clone(), EdgeKind::Briefed, at(2)),
            (s.clone(), "file:/w/a.rs".into(), EdgeKind::Read, at(3)),
            (s.clone(), "request:r1".into(), EdgeKind::Sent, at(6)),
            ("request:r1".into(), super::text_id("You are Claude Code."), EdgeKind::Carried, at(6)),
            ("request:r1".into(), super::text_id("standing"), EdgeKind::Carried, at(6)),
            ("request:r1".into(), super::tools_id("[]"), EdgeKind::Carried, at(6)),
            ("request:r1".into(), "N-412.p120".into(), EdgeKind::For, at(6)),
            ("request:r1".into(), "N-412.p121".into(), EdgeKind::For, at(6)),
            ("request:r1".into(), one.clone(), EdgeKind::RanWith, at(6)),
            ("session:c".into(), "agent:a".into(), EdgeKind::SessionOf, Source::Line(4)),
        ]);
        let text = |id: &str| g.get(id).map(|n| (n.kind, n.what.clone(), n.when.clone()));
        assert_eq!(text(&one), Some((NodeKind::Text, "AutoMem /m/MEMORY.md, 5 bytes".into(), Some("2026-11-18T10:14:22Z".into()))));
        assert_eq!(text(&brief), Some((NodeKind::Text, "the first user message, 9 bytes".into(), Some("2026-11-18T10:14:24Z".into()))));
        assert_eq!(one, "text:3a98d5a78bbc", "sha256's first 12 hex");
        assert_eq!(text("request:r1"), Some((NodeKind::Request, "claude-opus-5-5 xhigh".into(), Some("2026-11-18T11:00:01Z".into()))));
        assert_eq!(text(&super::text_id("standing")), Some((NodeKind::Text, "a system prompt block, 8 bytes".into(), Some("2026-11-18T11:00:01Z".into()))));
        assert_eq!(text(&super::tools_id("[]")), Some((NodeKind::Tools, "0 tools, 2 bytes".into(), Some("2026-11-18T11:00:01Z".into()))));
        let tokens = docket_machine::graph::Tokens { input: 3, output: 40, cache_read: 900, cache_write: 60, reasoning: 0 };
        assert_eq!(g.get("request:r1").and_then(|n| n.tokens), Some(tokens), "a request's node holds its response's tokens");
        assert_eq!(g.get(&one).and_then(|n| n.tokens), None, "a text has none");
    }

    /// A round put session by session (`SideRound::each`, as coordd's clock puts it, p1474) makes the graph one put
    /// makes, and each session's reader comes back for the next round.
    #[test]
    fn a_round_put_session_by_session_is_the_round_put_whole() {
        use docket_machine::graph::Graph;
        let dir = std::env::temp_dir().join(format!("follow-each-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |n: &str| {
            let p = dir.join(format!("{n}.jsonl"));
            std::fs::write(&p, typed("api-request", serde_json::json!({"id": format!("r-{n}"), "timestamp": "2026-11-22T10:00:00Z", "params": {}, "shapeHash": "", "tail": []})) + "\n").unwrap();
            p.display().to_string()
        };
        let given = |to: &str, file: &str| serde_json::from_value::<docket_machine::Line>(serde_json::json!({"ev": "given", "at": "2026-11-22T10:00:01Z", "t": 412, "to": to, "delivery": "N-412.p1", "upto": 1, "where": {"file": file, "offset": 0, "len": 10}})).unwrap();
        let lines = vec![given("a", &file("a")), given("b", &file("b"))];
        let post = |t: u64, n: u32| format!("N-{t}.p{n}");
        let (mut whole, mut each) = (Graph::default(), Graph::default());
        assert!(super::side_into(&mut whole, &lines, &|_| true, &post).is_empty());
        let mut kept = super::KeptSide::default();
        kept.take(&lines, 0, &post);
        let all: std::collections::BTreeSet<String> = kept.agents().map(String::from).collect();
        for one in kept.round(&all).read().each() {
            kept.put(one, &mut each);
        }
        std::fs::remove_dir_all(&dir).ok();
        let sorted = |mut v: Vec<String>| {
            v.sort();
            v
        };
        assert_eq!(sorted(each.nodes.iter().map(|n| format!("{n:?}")).collect()), sorted(whole.nodes.iter().map(|n| format!("{n:?}")).collect()));
        assert_eq!(sorted(each.edges.iter().map(|e| format!("{e:?}")).collect()), sorted(whole.edges.iter().map(|e| format!("{e:?}")).collect()));
        assert!(each.get("request:r-a").is_some() && each.get("request:r-b").is_some(), "both sessions' requests");
        assert_eq!(kept.round(&all).reads.len(), 2, "each session's reader came back");
    }

    /// A session put in parts of a few edges each (`KeptSide::put_part`; coordd's clock puts each part under a hold of
    /// its own, paper-research N-1446 p1922 (a)) makes the graph one put makes, node for node and edge for edge in its
    /// order: each part takes up where the last left off, each load, read and request whole in one part, and the
    /// session's reader comes back with its last part. Made up: two loads of one file (a next version), a brief, two
    /// reads and five requests, each with its response.
    #[test]
    fn a_session_put_in_parts_is_the_session_put_whole() {
        use docket_machine::graph::Graph;
        let p = std::env::temp_dir().join(format!("follow-parts-{}.jsonl", std::process::id()));
        let load = |at: &str, content: &str| serde_json::json!({"attachment": {"type": "instructions", "files": [{"path": "/m/MEMORY.md", "type": "AutoMem", "content": content}]}, "type": "attachment", "timestamp": at}).to_string();
        let read = |at: &str, id: &str, file: &str| serde_json::json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Read", "input": {"file_path": file}}]}, "timestamp": at}).to_string();
        let mut session = vec![
            load("2026-11-18T10:14:22Z", "- one"),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": "the brief"}, "timestamp": "2026-11-18T10:14:24Z"}).to_string(),
            read("2026-11-18T10:15:00Z", "t1", "/w/a.rs"),
            read("2026-11-18T10:15:01Z", "t2", "/w/b.rs"),
            load("2026-11-18T11:00:00Z", "- one\n- two"),
            typed("api-request-shape", serde_json::json!({"shapeHash": "s1", "shape": {"system": [{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "standing"}], "tools": []}})),
        ];
        for i in 1..=5 {
            let at = format!("2026-11-18T11:00:0{i}Z");
            session.push(typed("api-request", serde_json::json!({"id": format!("r{i}"), "timestamp": at, "params": {"model": "claude-opus-5-5", "output_config": {"effort": "xhigh"}}, "shapeHash": "s1", "tail": ["h1"]})));
            session.push(serde_json::json!({"type": "assistant", "requestRef": format!("r{i}"), "message": {"role": "assistant", "content": [{"type": "text", "text": "done"}], "usage": {"input_tokens": 3, "output_tokens": 40, "cache_read_input_tokens": 900, "cache_creation_input_tokens": 60}}, "timestamp": at}).to_string());
        }
        std::fs::write(&p, session.join("\n") + "\n").unwrap();
        let f = p.display().to_string();
        let brief_at = session[0].len() + 1;
        let given = |upto: u32| serde_json::from_value::<docket_machine::Line>(serde_json::json!({"ev": "given", "at": "2026-11-18T12:55:51Z", "t": 412, "to": "a", "delivery": format!("N-412.p{upto}"), "upto": upto, "where": {"file": f, "offset": brief_at, "len": 10}})).unwrap();
        // and the agent's turn cut after its five requests: each request's `sent_in` to it
        let cut = serde_json::from_value::<docket_machine::Line>(serde_json::json!({"ev": "turn_cut", "at": "2026-11-18T12:00:00Z", "agent": "a"})).unwrap();
        let lines = vec![given(120), given(121), cut];
        let post = |t: u64, n: u32| format!("N-{t}.p{n}");
        let mut whole = Graph::default();
        assert!(super::side_into(&mut whole, &lines, &|_| true, &post).is_empty());
        let mut kept = super::KeptSide::default();
        kept.take(&lines, 0, &post);
        let all: std::collections::BTreeSet<String> = kept.agents().map(String::from).collect();
        let mut round = kept.round(&all).read();
        let (mut parts, mut g) = (Vec::new(), Graph::default());
        let mut ranges = Vec::new();
        loop {
            let before = g.edges.len();
            let done = kept.put_part(&mut round, &mut g, 2);
            parts.push(g.edges.len() - before);
            ranges.push(before..g.edges.len());
            if done {
                break;
            }
            assert!(kept.round(&all).reads.is_empty(), "no round has the session while its parts go in");
        }
        std::fs::remove_file(&p).ok();
        let shown = |g: &Graph| (g.nodes.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>(), g.edges.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>());
        assert_eq!(shown(&g), shown(&whole), "the parts make the graph one put makes, in its order");
        assert!(parts.len() > 5 && parts.iter().all(|n| *n <= 9), "parts of about two edges, each request whole in one: {parts:?}");
        let sent_in: Vec<usize> = ranges.iter().map(|r| g.edges[r.clone()].iter().filter(|e| e.kind == docket_machine::graph::EdgeKind::SentIn).count()).collect();
        assert_eq!(sent_in.iter().sum::<usize>(), 5, "each request in the turn: {sent_in:?}");
        assert!(sent_in.iter().all(|n| *n <= 2), "the requests' `sent_in` about two a part, not all in one: {sent_in:?}");
        assert_eq!(kept.round(&all).reads.len(), 1, "its reader came back with its last part");
    }

    /// Another project's agent (PROJECT/NAME) is a name here with nothing read behind it (paper-research N-1446 p1828):
    /// a record whose `given` hands one a post with a session's place opens no session of it, neither on coordd's side
    /// (`KeptSide`: the agents it lists for `Core::side_round` to pick, a round even when asked for theirs) nor on a
    /// record's copy (`side_into` with every agent, COST_SIDES `*`); the same given to an agent here is read.
    #[test]
    fn a_given_to_another_projects_agent_opens_no_session_of_it() {
        use docket_machine::graph::{Graph, Source};
        let dir = std::env::temp_dir().join(format!("follow-far-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |n: &str| {
            let p = dir.join(format!("{n}.jsonl"));
            std::fs::write(&p, typed("api-request", serde_json::json!({"id": format!("r-{n}"), "timestamp": "2026-11-23T10:00:00Z", "params": {}, "shapeHash": "", "tail": []})) + "\n").unwrap();
            p.display().to_string()
        };
        let given = |to: &str, file: &str| serde_json::from_value::<docket_machine::Line>(serde_json::json!({"ev": "given", "at": "2026-11-23T10:00:01Z", "t": 412, "to": to, "delivery": "N-412.p1", "upto": 1, "where": {"file": file, "offset": 0, "len": 10}})).unwrap();
        let (here, far, far2) = (file("here"), file("far"), file("far2"));
        let lines = vec![given("a", &here), given("agda-dev/CONSULT", &far), given("tovrin-dev/DESIGN", &far2)];
        let post = |t: u64, n: u32| format!("N-{t}.p{n}");
        let mut g = Graph::default();
        let unopened = super::side_into(&mut g, &lines, &|_| true, &post);
        let mut kept = super::KeptSide::default();
        kept.take(&lines, 0, &post);
        let listed: Vec<String> = kept.agents().map(String::from).collect();
        let all: std::collections::BTreeSet<String> = ["a", "agda-dev/CONSULT", "tovrin-dev/DESIGN"].map(String::from).into();
        let opened: Vec<String> = kept.round(&all).reads.iter().map(|r| r.file.clone()).collect();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(listed, vec!["a".to_string()], "the side lists no other project's agent for the core to pick");
        assert_eq!(opened, vec![here], "a round opens the session here alone, though asked for theirs too");
        assert!(unopened.is_empty(), "none of theirs was tried: {unopened:?}");
        assert!(g.get("session:here").is_some() && g.get("request:r-here").is_some(), "the session here is read");
        for x in ["far", "far2"] {
            assert!(g.get(&format!("session:{x}")).is_none() && g.get(&format!("request:r-{x}")).is_none(), "no session of {x}'s, no request of it");
            assert!(!g.edges.iter().any(|e| matches!(&e.source, Source::Session(s, _) if s == x)), "no edge from a line of {x}");
        }
    }

    /// A session the givens name at two places is one session, its lines put once (docket-core N-1446 p1838, p1904:
    /// research's cbc70c62, its file copied under an account's folder while it ran on that account and named there
    /// once, was put twice, 88,379 edges): read from the place whose given reaches furthest into it, here home, where
    /// it went on. A state saved with the two kept apart (before p1838) comes back as that one session.
    #[test]
    fn a_session_named_at_two_places_is_put_once() {
        use docket_machine::graph::Graph;
        let dir = std::env::temp_dir().join(format!("follow-two-places-{}", std::process::id()));
        let (home, account) = (dir.join("home"), dir.join("account"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        let req = |n: u32| typed("api-request", serde_json::json!({"id": format!("r{n}"), "timestamp": format!("2026-11-22T10:00:0{n}Z"), "params": {}, "shapeHash": "", "tail": []})) + "\n";
        // its first two requests, then its file copied under the account's folder; its third at home, once it came back
        let (at_home, copy) = (home.join("s1.jsonl").display().to_string(), account.join("s1.jsonl").display().to_string());
        std::fs::write(&copy, req(1) + &req(2)).unwrap();
        std::fs::write(&at_home, req(1) + &req(2) + &req(3)).unwrap();
        let on = (req(1).len() + req(2).len()) as u64;
        let given = |file: &str, offset: u64, n: u32| serde_json::from_value::<docket_machine::Line>(serde_json::json!({"ev": "given", "at": "2026-11-22T10:00:09Z", "t": 412, "to": "a", "delivery": format!("N-412.p{n}"), "upto": n, "where": {"file": file, "offset": offset, "len": 10}})).unwrap();
        // named at home, then at the account's copy at the same place, then at home further on
        let lines = vec![given(&at_home, 0, 1), given(&copy, 0, 2), given(&at_home, on, 3)];
        let post = |t: u64, n: u32| format!("N-{t}.p{n}");
        let put = |side: &mut super::KeptSide| {
            let mut g = Graph::default();
            let all: std::collections::BTreeSet<String> = side.agents().map(String::from).collect();
            for one in side.round(&all).read().each() {
                side.put(one, &mut g);
            }
            let mut edges: Vec<String> = g.edges.iter().map(|e| format!("{e:?}")).collect();
            edges.sort();
            (g, edges)
        };
        let mut kept = super::KeptSide::default();
        kept.take(&lines, 0, &post);
        let (g, edges) = put(&mut kept);
        let mut once = edges.clone();
        once.dedup();
        assert_eq!(edges.len(), once.len(), "no edge twice: {edges:#?}");
        assert_eq!(kept.sessions.len(), 1, "one session");
        assert!(["request:r1", "request:r2", "request:r3"].iter().all(|r| g.get(r).is_some()), "read at home, where it went on: {:?}", g.nodes);
        let saved = |file: &str, line: usize, given: &[(u64, u32)]| super::SideSaved { agent: "a".into(), file: file.into(), line, given: given.iter().map(|(at, n)| (*at, format!("N-412.p{n}"))).collect() };
        let mut back = super::KeptSide::from_saved(vec![saved(&at_home, 1, &[(0, 1), (on, 3)]), saved(&copy, 2, &[(0, 2)])]);
        assert_eq!(back.sessions.len(), 1, "a state's two places of it, one session");
        assert_eq!(put(&mut back).1, edges, "and the same graph as the record's lines make");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A tool call's own words are its `about` (N-516.p3): a Bash call's description, its first line; none for a call
    /// that gives none, or only blanks.
    #[test]
    fn a_tool_calls_description_is_what_it_is_about() {
        let line = serde_json::json!({"type": "assistant", "timestamp": "2026-11-18T15:40:01.123Z", "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cargo test -p coordd", "description": "Run coordd's tests\nand more"}},
            {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/x/y.rs"}},
            {"type": "tool_use", "id": "t3", "name": "Bash", "input": {"command": "ls", "description": "  "}},
        ]}}).to_string();
        let said = super::classify(line.as_bytes()).unwrap();
        let got: Vec<(&str, &str, Option<&str>)> = said.began.iter().map(|t| (t.tool.as_str(), t.what.as_str(), t.about.as_deref())).collect();
        assert_eq!(got, vec![("Bash", "cargo test -p coordd", Some("Run coordd's tests")), ("Read", "/x/y.rs", None), ("Bash", "ls", None)]);
        assert!(serde_json::to_value(&said.began[1]).unwrap().get("about").is_none(), "none: not sent");
        // its cost per call, beside the entry's whole read (printed with --nocapture)
        let input = serde_json::json!({"command": "cargo test", "description": "Run coordd's tests with a description of usual length"});
        let at = std::time::Instant::now();
        let n = (0..100_000).filter_map(|_| super::about(std::hint::black_box(&input))).count();
        let one = at.elapsed().as_nanos() / 100_000;
        let at = std::time::Instant::now();
        let k = (0..10_000).filter_map(|_| super::classify(std::hint::black_box(line.as_bytes()))).count();
        println!("cost about: {one} ns a call ({n}); the entry's classify {} ns ({k})", at.elapsed().as_nanos() / 10_000);
    }

    /// Words typed into the terminal are a plain user entry written after the follower began; a channel delivery, a
    /// tool result, a command, a reminder, the Stop hook's mail, and an entry from before it began are not
    /// (paper-research N-101 p1107).
    #[test]
    fn a_typed_prompt_is_told_from_what_coord_delivers() {
        let p = std::env::temp_dir().join(format!("follow-typed-{}.jsonl", std::process::id()));
        let user = |c: serde_json::Value| serde_json::json!({"type": "user", "message": {"role": "user", "content": c}}).to_string() + "\n";
        let old = user("typed before coordd began".into());
        std::fs::write(&p, &old).unwrap();
        let mut f = super::Follower::new(&p.display().to_string());
        let typed = user("is the build green?".into());
        let lines = [
            user("<channel source=\"coord\" from=\"owner\" id=\"N-3.p1\" kind=\"note\" ticket=\"N-3\">\nhi\n</channel>".into()),
            user(serde_json::json!([{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}])),
            user("<command-name>/model</command-name>".into()),
            user("<system-reminder>x</system-reminder>".into()),
            user("Stop hook feedback:\nMail that arrived for you (coord): m1".into()),
            typed.clone(),
        ];
        std::fs::write(&p, format!("{old}{}", lines.concat())).unwrap();
        let r = f.read();
        let _ = std::fs::remove_file(&p);
        let at = (old.len() + lines[..5].iter().map(String::len).sum::<usize>()) as u64;
        assert_eq!(r.typed, vec![(at, typed.len() as u64 - 1)]);
    }

    /// An id-less coord channel entry is read as such, with its body and place; one with an id is not.
    #[test]
    fn an_id_less_coord_entry_is_read_with_its_body() {
        let p = std::env::temp_dir().join(format!("follow-bare-{}.jsonl", std::process::id()));
        std::fs::write(&p, "").unwrap();
        let mut f = super::Follower::new(&p.display().to_string());
        let bare = serde_json::json!({"type": "user", "isMeta": true, "message": {"content": "<channel source=\"coord\" from=\"owner\">\nwhat is 5+5\n</channel>"}}).to_string();
        let tagged = serde_json::json!({"type": "user", "isMeta": true, "message": {"content": "<channel source=\"coord\" from=\"owner\" id=\"N-3.p1\" kind=\"note\" ticket=\"N-3\">\nhi\n</channel>"}}).to_string();
        std::fs::write(&p, format!("{tagged}\n{bare}\n")).unwrap();
        let r = f.read();
        assert_eq!(r.at.iter().map(|(id, ..)| id.as_str()).collect::<Vec<_>>(), vec!["N-3.p1"]);
        assert_eq!(r.bare, vec![("what is 5+5".to_string(), tagged.len() as u64 + 1, bare.len() as u64)]);
        let _ = std::fs::remove_file(&p);
    }

    use super::Follower;
    use std::io::Write;

    /// A Codex rollout (a boxed Codex agent's, docket-core5's moves try run 5): the deliveries its user message carries,
    /// with their place, and the turn's final output, the assistant's message after its last tool output, taken at
    /// `task_complete` and read back by `text_at`; the owner's words to it alone are an id-less entry.
    #[test]
    fn a_codex_rollout_shows_its_deliveries_and_its_turns_final_output() {
        let p = std::env::temp_dir().join(format!("follow-codex-{}.jsonl", std::process::id()));
        let path = p.display().to_string();
        // in Codex's order: `timestamp` first (json! sorts its keys)
        let line = |kind: &str, payload: serde_json::Value| format!("{{\"timestamp\":\"2026-11-16T12:12:52.909Z\",\"type\":\"{kind}\",\"payload\":{payload}}}\n");
        let ev = |t: &str| line("event_msg", serde_json::json!({"type": t}));
        let msg = |role: &str, block: &str, text: &str| line("response_item", serde_json::json!({"type": "message", "role": role, "content": [{"type": block, "text": text}]}));
        let item = |t: &str| line("response_item", serde_json::json!({"type": t, "name": "exec", "output": "ok"}));
        let old = ev("task_started") + &msg("user", "input_text", "This is a test agent.") + &msg("assistant", "output_text", "Nothing") + &ev("task_complete");
        std::fs::write(&p, &old).unwrap();
        let mut f = Follower::new(&path);
        f.read();
        assert_eq!(f.take_output(), None, "a turn ended before the follower began");
        let given = msg("user", "input_text", "[B-4.s1 · from coord · ticket B-4 built step 1]\nCommit a file.\n\n[m12 · from lead]\nAlso this.");
        let said = msg("assistant", "output_text", "Commit 9724acb");
        let body = ev("task_started") + &msg("user", "input_text", "<environment_context>x</environment_context>") + &given + &item("custom_tool_call") + &msg("assistant", "output_text", "Looking.") + &item("custom_tool_call_output") + &said + &ev("token_count") + &ev("task_complete");
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(body.as_bytes()).unwrap();
        let r = f.read();
        let at_given = (old.len() + ev("task_started").len() + msg("user", "input_text", "<environment_context>x</environment_context>").len()) as u64;
        assert_eq!(r.at, vec![("B-4.s1".to_string(), at_given, given.len() as u64 - 1), ("m12".to_string(), at_given, given.len() as u64 - 1)]);
        let (o, l) = f.take_output().expect("the turn's final output");
        assert_eq!(super::text_at(&path, o, l).as_deref(), Some("Commit 9724acb"));
        // a place past the file's end has no text, and nothing is made for it (consult-3 N-1396 p386): a LEN sent wrong,
        // one byte too many, one that runs past u64
        let held = std::fs::metadata(&p).unwrap().len();
        assert_eq!(super::text_at(&path, o, u64::MAX / 2), None, "a wrong length is refused, not allocated");
        assert_eq!(super::text_at(&path, o, held - o + 1), None);
        assert_eq!(super::text_at(&path, u64::MAX, 2), None);
        assert_eq!(super::text_at(&path, at_given, given.len() as u64 - 1).unwrap(), "[B-4.s1 · from coord · ticket B-4 built step 1]\nCommit a file.\n\n[m12 · from lead]\nAlso this.");
        assert_eq!(f.take_output(), None, "taken once");
        // the owner's words alone, then an interrupt: an id-less entry and a cut, no output
        let bare = msg("user", "input_text", "[from owner]\nwhat is 5+5\n");
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all((ev("task_started") + &bare + &ev("turn_aborted")).as_bytes()).unwrap();
        let r = f.read();
        assert_eq!(r.bare.iter().map(|(b, ..)| b.as_str()).collect::<Vec<_>>(), vec!["what is 5+5"]);
        assert!(r.cut.is_some() && r.at.is_empty());
        assert_eq!(f.take_output(), None);
        let _ = std::fs::remove_file(&p);
    }

    /// An interrupt entry is a cut, with its place: what was seen before it and after it are told apart by offset.
    #[test]
    fn an_interrupted_turn_is_a_cut_at_its_place() {
        let p = std::env::temp_dir().join(format!("follow-cut-{}.jsonl", std::process::id()));
        std::fs::write(&p, "").unwrap();
        let mut f = Follower::new(&p.display().to_string());
        let first = channel("N-1.w1", "2026-11-11T19:04:27Z");
        let cut = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"[Request interrupted by user for tool use]\"}]},\"timestamp\":\"2026-11-11T19:06:06Z\"}\n";
        let second = channel("N-2.w1", "2026-11-11T19:06:07Z");
        std::fs::write(&p, format!("{first}{cut}{second}")).unwrap();
        let r = f.read();
        let c = r.cut.expect("a cut");
        assert_eq!(c, first.len() as u64);
        let before: Vec<&str> = r.at.iter().filter(|(_, o, _)| *o < c).map(|(id, ..)| id.as_str()).collect();
        let after: Vec<&str> = r.at.iter().filter(|(_, o, _)| *o > c).map(|(id, ..)| id.as_str()).collect();
        assert_eq!((before, after), (vec!["N-1.w1"], vec!["N-2.w1"]));
        assert!(f.take_output().is_none(), "a cut turn has no output");
        let _ = std::fs::remove_file(&p);
    }

    /// The replies the API gave, each with when the input it answers was written (paper-research N-1446 p873: the 200s
    /// that free the agents held on a rate limit of that model): a prompt at 10:06:30Z answered on claude-opus-5-5 at
    /// 10:06:33Z is one; Claude Code's own words on a 429 (`<synthetic>`, an API error) are none; a later prompt's reply
    /// is one again, asked at that prompt.
    #[test]
    fn the_replies_that_came_through_are_taken_with_when_they_were_asked() {
        let p = std::env::temp_dir().join(format!("follow-answers-{}.jsonl", std::process::id()));
        std::fs::write(&p, "").unwrap();
        let mut f = Follower::new(&p.display().to_string());
        let prompt = |at: &str| format!("{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"what changed?\"}},\"timestamp\":\"{at}\"}}\n");
        let reply = |at: &str| format!("{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"content\":[{{\"type\":\"text\",\"text\":\"this\"}}]}},\"timestamp\":\"{at}\"}}\n");
        let refused = "{\"type\":\"assistant\",\"isApiErrorMessage\":true,\"message\":{\"role\":\"assistant\",\"model\":\"<synthetic>\",\"content\":[{\"type\":\"text\",\"text\":\"API Error: Request rejected (429)\"}]},\"timestamp\":\"2026-11-21T10:06:41Z\"}\n";
        std::fs::write(&p, format!("{}{}{}{refused}", prompt("2026-11-21T10:06:30Z"), reply("2026-11-21T10:06:33Z"), prompt("2026-11-21T10:06:40Z"))).unwrap();
        f.read();
        assert_eq!(f.take_answers(), vec![("claude-opus-5-5".to_string(), "2026-11-21T10:06:30Z".to_string())], "the 429's words are no reply");
        assert!(f.take_answers().is_empty(), "taken");
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(format!("{}{}", prompt("2026-11-21T10:06:48Z"), reply("2026-11-21T10:06:49Z")).as_bytes()).unwrap();
        f.read();
        let answers = f.take_answers();
        let _ = std::fs::remove_file(&p);
        assert_eq!(answers, vec![("claude-opus-5-5".to_string(), "2026-11-21T10:06:48Z".to_string())]);
    }

    /// An interrupt entry already in the transcript when the follower begins (a coordd start reads its window) is the
    /// cut of an earlier turn when a turn came after it: the turn running now is not cut. One written while coordd was
    /// down, with nothing after it, still is; one written after the follower began always is (consult-3, N-807.p100).
    #[test]
    fn an_old_interrupt_cuts_the_running_turn_only_when_nothing_came_after_it() {
        let p = std::env::temp_dir().join(format!("follow-old-cut-{}.jsonl", std::process::id()));
        let cut = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"[Request interrupted by user]\"}]},\"timestamp\":\"2026-11-19T09:00:00Z\"}\n";
        let working = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"reading the record\"}]},\"timestamp\":\"2026-11-19T10:09:30Z\"}\n";
        // an esc hours before, then the turn running when coordd starts: no cut
        std::fs::write(&p, format!("{}{cut}{}{working}", channel("N-1.p1", "2026-11-19T08:59:00Z"), channel("N-871.p8", "2026-11-19T10:09:24Z"))).unwrap();
        let mut f = Follower::new(&p.display().to_string());
        let running = f.read();
        assert_eq!(running.cut, None, "the old esc does not cut the turn running now");
        assert_eq!(running.at.iter().map(|(id, ..)| id.as_str()).collect::<Vec<_>>(), vec!["N-1.p1", "N-871.p8"]);
        // an esc after the follower began: a cut, at its place
        let at = std::fs::metadata(&p).unwrap().len();
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(cut.as_bytes()).unwrap();
        assert_eq!(f.read().cut, Some(at), "an esc now cuts");
        // an esc hours before, then a turn whose first entry kept is a tool call alone (begun by a meta entry): no cut
        let tool = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Bash\",\"input\":{\"command\":\"true\"}}]},\"timestamp\":\"2026-11-19T10:09:30Z\"}\n";
        std::fs::write(&p, format!("{}{cut}{tool}", channel("N-1.p1", "2026-11-19T08:59:00Z"))).unwrap();
        assert_eq!(Follower::new(&p.display().to_string()).read().cut, None, "a tool call after the old esc is a turn going on");
        // an esc written while coordd was down, nothing after it: the turn it cut ended so
        std::fs::write(&p, format!("{}{working}{cut}", channel("N-871.p8", "2026-11-19T10:09:24Z"))).unwrap();
        let mut down = Follower::new(&p.display().to_string());
        let c = down.read().cut.expect("the esc made while coordd was down is a cut");
        assert_eq!(c, (channel("N-871.p8", "2026-11-19T10:09:24Z").len() + working.len()) as u64);
        let _ = std::fs::remove_file(&p);
    }

    fn channel(id: &str, at: &str) -> String {
        format!("{{\"type\":\"user\",\"isMeta\":true,\"origin\":{{\"kind\":\"channel\"}},\"message\":{{\"role\":\"user\",\"content\":\"<channel source=\\\"coord\\\" from=\\\"owner\\\" id=\\\"{id}\\\" kind=\\\"note\\\" ticket=\\\"N-66\\\">\\nowner wrote:\\nhello\\n</channel>\"}},\"timestamp\":\"{at}\"}}\n")
    }
    fn answer(text: &str, at: &str) -> String {
        format!("{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"stop_reason\":\"end_turn\",\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}},\"timestamp\":\"{at}\"}}\n{{\"type\":\"system\",\"subtype\":\"turn_duration\",\"timestamp\":\"{at}\"}}\n")
    }

    /// w1 on docket-live: restarted with deliveries in its transcript it never turned on, then given one and turning on
    /// it. The first read (the window) ends with no output; the next turn's output is taken all the same.
    #[test]
    fn a_window_ending_without_output_does_not_cost_the_next_turns_end() {
        let path = std::env::temp_dir().join(format!("follow-primed-{}.jsonl", std::process::id()));
        let p = path.display().to_string();
        std::fs::write(&path, answer("Ten", "2026-11-11T12:40:40Z") + &channel("N-66.p1", "2026-11-11T12:46:57Z")).unwrap();
        let mut f = Follower::new(&p);
        f.read();
        assert_eq!(f.take_output(), None, "the window's output is from before");
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(channel("N-66.p2", "2026-11-11T12:59:10Z").as_bytes()).unwrap();
        f.read();
        assert_eq!(f.take_output(), None, "no turn ended yet");
        file.write_all(answer("There are 5 files.", "2026-11-11T12:59:14Z").as_bytes()).unwrap();
        let seen = f.read();
        let out = f.take_output();
        let _ = std::fs::remove_file(&path);
        assert!(out.is_some(), "the turn's end is taken ({:?})", seen.seen);
        // and a window that ends with an output: that output is old, the next one is new
        let path = std::env::temp_dir().join(format!("follow-primed2-{}.jsonl", std::process::id()));
        let p = path.display().to_string();
        std::fs::write(&path, channel("N-65.p6", "2026-11-11T12:40:38Z") + &answer("Ten", "2026-11-11T12:40:40Z")).unwrap();
        let mut f = Follower::new(&p);
        f.read();
        let old = f.take_output();
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all((channel("N-66.p2", "2026-11-11T12:59:10Z") + &answer("Five.", "2026-11-11T12:59:14Z")).as_bytes()).unwrap();
        f.read();
        let new = f.take_output();
        let _ = std::fs::remove_file(&path);
        assert_eq!((old.is_some(), new.is_some()), (false, true));
        // a new session's file (after /clear) found before its first turn: a turn that ends before the first read is
        // new all the same
        let path = std::env::temp_dir().join(format!("follow-primed3-{}.jsonl", std::process::id()));
        let p = path.display().to_string();
        std::fs::write(&path, "").unwrap();
        let mut f = Follower::new(&p);
        std::fs::write(&path, channel("N-68.p1", "2026-11-11T13:10:00Z") + &answer("Done.", "2026-11-11T13:10:01Z")).unwrap();
        f.read();
        let first = f.take_output();
        let _ = std::fs::remove_file(&path);
        assert!(first.is_some());
    }
}

/// When a Claude Code session's last turn was cut by the end of its process, as transcript PATH shows it: a turn still
/// running at the file's end (begun, and no stop, interrupt or API error ended it) was cut when its process ended,
/// which is when the file was last written (its modification time, seconds). None when no turn runs there: it ended,
/// an esc cut it (`[Request interrupted by user`), or the file cannot be read. Read from the file's last [`WINDOW`], as
/// a follower begun on it would (`Core::account_for_begin`: the start after a reboot goes on with such a turn).
pub fn cut_at(path: &str) -> Option<i64> {
    let mut f = Follower::new(path);
    f.read();
    if f.ended {
        return None;
    }
    let written = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(written.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// The text at the span OFFSET, LEN of transcript FILE (a `Loc`): the text blocks of the assistant entries in it
/// (a turn's final output), or the text of a user entry (a delivery as the session took it). None when the file no
/// longer has those bytes (the one case a referenced text must have been pinned before).
pub fn text_at(file: &str, offset: u64, len: u64) -> Option<String> {
    let mut f = std::fs::File::open(file).ok()?;
    // a place past what the file holds has no text there, and is refused before anything is made for it: a LEN a caller
    // sent wrong would otherwise be allocated whole, and a failed allocation ends coordd (consult-3 N-1396 p386)
    if offset.checked_add(len)? > f.metadata().ok()?.len() {
        return None;
    }
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0; len as usize];
    f.read_exact(&mut buf).ok()?;
    crate::counts::read("span (a text at its place)", len);
    let mut out = Vec::new();
    for line in buf.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        line_texts(line, &mut out)?;
    }
    Some(out.join("\n\n"))
}

/// How many lines past a `given` line [`fork_cut`] reads for the entry that took its message.
const FORK_SCAN: usize = 64;

/// Where a copy of transcript FILE is cut for a fork from delivery PLACE (`fork … at PLACE`, part 4, docket-core
/// N-1446 p2096): after the entry that took PLACE's message, the first from OFFSET (its `given` line's place) on: a
/// user entry, or a queued message's attachment, whose coord channel tag names PLACE ([`channel_ids`]), or, when the
/// given line's message carries no id, whose text is that message. The given line may be Claude Code's
/// `queue-operation` enqueue, which is in no chain: a copy cut after it would hold no message of PLACE's, so the cut
/// goes on to the entry a line or two down that does. The bytes up to that entry's end, its newline with them; None
/// when no such entry comes in the next [`FORK_SCAN`] lines, or a line there is not whole yet.
///
/// A message with no id: the owner's words alone, coordd's own short tag, `from` and nothing more, for his one post on
/// a thread to an agent that holds nothing else, and for one-off words (render.rs:168-173, :241-246, since 11 Nov). A
/// new agent's first message from him is always one, so a copy cut at it could not start (proto-mind's copy proof on
/// engine 81, N-1446 p3335; docket-edge7 p3338, paper-research p3339). Its entry is the first after the given line
/// whose text is the given line's message, trimmed, as the follower pairs an enqueue with the entry that took it
/// (`Queue::Took`); a given line that is the entry is its own. One limit: the same words queued before the given line
/// and not yet taken would be taken first, and the copy then cut an entry early, after the same words.
pub fn fork_cut(file: &str, offset: u64, place: &str) -> Option<u64> {
    use std::io::BufRead;
    let mut f = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    f.seek(SeekFrom::Start(offset)).ok()?;
    let (mut at, mut line) = (offset, Vec::new());
    // the given line's coord message when its tag carries no id: what the entry that took it holds
    let mut words: Option<String> = None;
    for k in 0..FORK_SCAN {
        line.clear();
        let n = f.read_until(b'\n', &mut line).ok()?;
        if line.last() != Some(&b'\n') {
            return None;
        }
        at += n as u64;
        let Ok(e) = serde_json::from_slice::<serde_json::Value>(&line) else { continue };
        let mut texts = Vec::new();
        match e["type"].as_str() {
            Some("user") => {
                line_texts(&line, &mut texts);
            }
            Some("attachment") => texts.extend(e["attachment"]["prompt"].as_str().map(str::to_string)),
            Some("queue-operation") if k == 0 && e["operation"] == "enqueue" => texts.extend(e["content"].as_str().map(str::to_string)),
            _ => continue,
        }
        if k == 0 {
            words = texts.iter().map(|t| t.trim()).find(|t| coord_message(t) && channel_ids(t).is_empty()).map(str::to_string);
        }
        if e["type"] == "queue-operation" {
            continue;
        }
        if texts.iter().any(|t| channel_ids(t).iter().any(|i| i == place) || words.as_deref() == Some(t.trim())) {
            return Some(at);
        }
    }
    None
}

/// The texts of one transcript LINE as [`text_at`] reads them, put in OUT; None when it is no JSON.
fn line_texts(line: &[u8], out: &mut Vec<String>) -> Option<()> {
    // an entry's message, or (a channel delivery as Claude Code queued it, `queue-operation`) its own content
    let e: serde_json::Value = serde_json::from_slice(line).ok()?;
    // or a Codex rollout line's message (`payload`; its blocks `input_text`, `output_text`): its tool calls and
    // reasoning say nothing here
    let codex = e.get("payload").filter(|p| p["type"] == "message");
    if e.get("payload").is_some() && codex.is_none() {
        return Some(());
    }
    let content = e.get("message").or(codex).and_then(|m| m.get("content")).or_else(|| e.get("content")).cloned().unwrap_or_default();
    match content {
        serde_json::Value::String(s) => out.push(s),
        serde_json::Value::Array(items) => out.extend(items.iter().filter(|i| matches!(i["type"].as_str(), Some("text" | "input_text" | "output_text"))).filter_map(|i| i["text"].as_str().map(String::from))),
        _ => {}
    }
    Some(())
}

/// When the entry at OFFSET of FILE was written, as its transcript says (`timestamp`), if it says.
pub fn written_at(file: &str, offset: u64) -> Option<String> {
    let mut f = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut line = String::new();
    std::io::BufRead::read_line(&mut f, &mut line).ok()?;
    crate::counts::read("written_at (one line)", line.len() as u64);
    let e: serde_json::Value = serde_json::from_str(&line).ok()?;
    e.get("timestamp")?.as_str().map(String::from)
}

/// One agent's side as a session file holds it (paper-research N-1446 p1376 point 4, p1388, p1398 5): the
/// instructions Claude Code loaded at the start and at each compaction (each file's kind, path and text as it stood
/// then), the session's first user message (the brief as the agent saw it, or a compaction's summary where the file
/// begins after one), each file its Read calls read, the requests it sent with the shapes they carried and their
/// responses' tokens (p1429 2), and where the step of each `given` began; each at the session's line, counted from 1,
/// with its time. Of a request line only its id, time, model, effort, shape and tail are read, of a blob only whether
/// it holds a given's text and its hash (no header, key or `metadata` is in either, docket-core's field counts,
/// p1397), of a response only the request it names (`requestRef`) and its usage; a subagent's entries
/// (`isSidechain`) are none of the agent's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Side {
    pub loaded: Vec<Loaded>,
    pub brief: Option<Brief>,
    pub read: Vec<Reading>,
    pub requests: Vec<Request>,
    /// Each shape by its `shapeHash`, as the first line of it holds it (the same hash comes on many lines, p1400).
    pub shapes: BTreeMap<String, Shape>,
    /// For each offset [`side`] was given, in its order, where its step began; none when no request came after it.
    pub began: Vec<Option<Began>>,
    /// The first line of each response whose `requestRef` names no request line before it: its tokens are on no
    /// request (on docket-edge5's session, 132, all before its first request line).
    pub unrequested: Vec<usize>,
    /// Each text's next version in the session (`became`, p1458 point 3): a loaded file's next text at its path, and a
    /// system prompt block's next text at its index, request by request; each at the line where the new one first
    /// stands, with the words that changed ([`difference`]).
    pub became: Vec<Became>,
}

/// A request the session sent (`api-request`): its model and effort, and its shape by `shapeHash`.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub line: usize,
    pub at: String,
    pub id: String,
    pub model: String,
    pub effort: String,
    pub shape: String,
    /// Its tokens, at the latest line of its response (an assistant entry whose `requestRef` is its id), as coordd's
    /// usage reader counts a message; None when the session holds no response to it.
    pub tokens: Option<docket_machine::graph::Tokens>,
}

/// A request's shape (`api-request-shape`): the texts of its system prompt's blocks, and its tools as [`tools_id`] of
/// their JSON, with how many and the JSON's bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub line: usize,
    /// Where its line starts in the session's file: a block's text is read from there ([`block_text`]).
    pub at: u64,
    pub system: Vec<String>,
    pub tools: String,
    pub tool_count: usize,
    pub tools_bytes: usize,
}

/// Where a `given`'s step began (p1398 5): the index in [`Side::requests`] of the first request that carried its
/// message. BY_HASH: the request's tail names an `api-request-blob` that holds the message's text. Else by order: the
/// first request after where its text went in, its own user entry, or for a message queued while the agent worked
/// (a `queue-operation` line) the meta user entry or `queued_command` attachment that holds it. A message's own entry
/// never hashes into a tail, as the request's message is no transcript entry (0 of 293 givens on docket-edge5's
/// session); the blob is looked for until that first request after its way in, as Claude Code can write the entry
/// after the request that carried it (3 of the 139 that a blob holds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Began {
    pub request: usize,
    pub by_hash: bool,
}

/// An `api-request` line's fields that [`side`] reads; `params` holds the model and effort. The one reader of a request
/// line: `transcript::Record` (a session's own record as the raw pane's, fix 5) reads it so too, with its `querySource`
/// and `keep`, whose request's messages it goes on from and how many of them.
#[derive(Deserialize)]
pub(crate) struct RequestLine {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) timestamp: String,
    #[serde(default)]
    pub(crate) params: serde_json::Value,
    #[serde(rename = "shapeHash", default)]
    pub(crate) shape: String,
    #[serde(default)]
    pub(crate) tail: Vec<String>,
    #[serde(rename = "querySource", default)]
    pub(crate) source: Option<String>,
    #[serde(default)]
    pub(crate) keep: Option<usize>,
}

/// An assistant entry's fields that [`side`] reads for its request's tokens: the request it answers (`requestRef`),
/// whether a subagent made it and its usage, its content passed over.
#[derive(Deserialize)]
struct ResponseLine {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(rename = "requestRef")]
    request: String,
    #[serde(rename = "isSidechain", default)]
    sidechain: bool,
    #[serde(default)]
    message: ResponseMessage,
}

#[derive(Default, Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    usage: Option<serde_json::Value>,
}

/// An `api-request-blob` line's hash, its message passed over (read so by [`side`] and by `transcript::Record`).
#[derive(Deserialize)]
pub(crate) struct BlobLine {
    pub(crate) hash: String,
}

/// An `api-request-shape` line's hash, its shape passed over: where `transcript::Record` finds it, which reads the
/// shape whole only for a request it lays out.
#[derive(Deserialize)]
pub(crate) struct ShapeLine {
    #[serde(rename = "shapeHash")]
    pub(crate) hash: String,
}

/// A `given` whose step has not begun: its index, what of its text a line that holds it holds (the start of its
/// trimmed text as JSON writes it inside a string, at most 400 bytes), the blobs that hold it and whether its text
/// has gone in.
#[derive(Debug)]
struct Waiting {
    given: usize,
    key: String,
    blobs: Vec<String>,
    went_in: bool,
}

/// What of TEXT a line holds when it holds TEXT: its trimmed start, escaped as inside a JSON string, at most 400 bytes,
/// cut at a character.
fn key_of(text: &str) -> String {
    let quoted = serde_json::to_string(text.trim()).unwrap_or_default();
    let inner = quoted.get(1..quoted.len().saturating_sub(1)).unwrap_or("");
    let mut end = inner.len().min(400);
    while !inner.is_char_boundary(end) {
        end -= 1;
    }
    inner[..end].to_string()
}

/// A file an `instructions` attachment loaded, with its kind as Claude Code names it (`AutoMem`, `Project`, `User`).
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub line: usize,
    pub at: String,
    pub kind: String,
    pub path: String,
    pub text: String,
}

/// The session's first user message; COMPACT when it is a compaction's summary.
#[derive(Clone, Debug, PartialEq)]
pub struct Brief {
    pub line: usize,
    pub at: String,
    pub text: String,
    pub compact: bool,
}

/// A file a Read call read.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub line: usize,
    pub at: String,
    pub path: String,
}

/// A text's next version ([`Side::became`]): the two texts' ids, the line and time where the new one first stands (its
/// load's, or the first request that carried it), and the words that changed ([`difference`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Became {
    pub line: usize,
    pub at: String,
    pub from: String,
    pub to: String,
    pub what: String,
}

/// [`Side`] of the session FILE, with where the step began of each `given` whose line starts at a byte offset in
/// GIVEN; None when the file does not open. A [`SessionReader`] read once, as coordd's kept graph reads on with one.
pub fn side(file: &str, given: &[u64]) -> Option<Side> {
    let mut r = SessionReader::default();
    r.give(file, given);
    r.read_on(file).then_some(r.side)
}

/// A session's [`Side`], read on from where it left off (paper-research N-1446 p1458), as transcript.rs's usage reader
/// reads a session (`read_byte_lines`): whole lines only, a line still being written left for the next read. It keeps
/// what one read of the whole file holds at the line it has read to: the bytes and lines read, the givens waiting for
/// their step's first request, each request by its id (for its responses) and by where its line starts, the responses
/// that named none, and the givens whose line it has not read. A `given` can be recorded after its line was read
/// (coordd records it once it has seen the delivery go in): that line is read again, up to where the reader is, for
/// that given alone, as one read would have taken it ([`SessionReader::give`]). What it read since its side was last
/// put in a graph is said apart: the requests whose tokens came, the givens whose step began, the shapes.
#[derive(Debug, Default)]
pub struct SessionReader {
    pub side: Side,
    offset: u64,
    lines: usize,
    /// The file's inode at its first read: another file at its path is not read on.
    ino: u64,
    waiting: Vec<Waiting>,
    ids: BTreeMap<String, usize>,
    unrequested: BTreeSet<String>,
    /// Where each request's line starts.
    request_at: Vec<u64>,
    /// The givens whose line is not read yet, by where it starts.
    ahead: BTreeMap<u64, Vec<usize>>,
    tokened: BTreeSet<usize>,
    begun: Vec<usize>,
    shaped: Vec<String>,
    /// Each loaded file's latest text by path, with its id: for its next version's difference.
    last_load: BTreeMap<String, (String, String)>,
    /// The system prompt's versions request by request ([`SessionReader::chain_system`]): the next request to look at,
    /// and the shape of the one before it.
    system_at: usize,
    last_shape: Option<String>,
}

impl SessionReader {
    /// Givens at the byte offsets GIVEN, after those it has; their steps are in [`Side::began`] in that order. One whose
    /// line it has read is read again from there up to where it is, for it alone; one at no line's start begins no
    /// step, as in one read.
    pub fn give(&mut self, file: &str, given: &[u64]) {
        for &at in given {
            let i = self.side.began.len();
            self.side.began.push(None);
            if at >= self.offset {
                self.ahead.entry(at).or_default().push(i);
                continue;
            }
            if !line_starts(file, at) {
                continue;
            }
            // as the reader stood at that line for this given: its line the one ahead, nothing waiting
            let (ahead, waiting) = (std::mem::replace(&mut self.ahead, BTreeMap::from([(at, vec![i])])), std::mem::take(&mut self.waiting));
            let (mut from, to) = (at, self.offset);
            crate::transcript::read_byte_lines("side (a given's line read again: the given came after it was read)", file, &mut from, |l, start| {
                let on = start < to && self.side.began[i].is_none();
                if on {
                    self.line(l, start, true);
                }
                on
            });
            // still waiting: on with the rest from here
            let left = std::mem::replace(&mut self.waiting, waiting);
            self.waiting.extend(left);
            self.ahead = ahead;
        }
    }

    /// Reads FILE on from where it left off; false when it does not open. A file at its path that is not the one first
    /// read (another inode) is read no further, nor one while it holds no more than was read of it.
    pub fn read_on(&mut self, file: &str) -> bool {
        use std::os::unix::fs::MetadataExt;
        let Ok(meta) = std::fs::metadata(file) else { return false };
        if self.offset == 0 {
            self.ino = meta.ino();
        }
        if meta.ino() != self.ino || meta.len() <= self.offset {
            return true;
        }
        let mut offset = self.offset;
        let opened = crate::transcript::read_byte_lines("side (each session a given names, on from where it left off)", file, &mut offset, |l, start| {
            self.line(l, start, false);
            true
        });
        self.offset = offset;
        self.chain_system();
        opened
    }

    /// The system prompt blocks' next versions among the requests read since ([`Side::became`]): request by request,
    /// each block whose text is not the one the request before carried at its index, at the request's line. It waits at
    /// a request whose shape has not come yet, which goes on when it comes (on research's record no shape came after
    /// its first request, on three sessions: docket-edge5's, paper-research's, docket-core's), and passes one that
    /// names none.
    fn chain_system(&mut self) {
        let mut rows = Vec::new();
        let side = &self.side;
        while let Some(q) = side.requests.get(self.system_at) {
            let Some(shape) = side.shapes.get(&q.shape) else {
                if !q.shape.is_empty() {
                    break;
                }
                self.system_at += 1;
                continue;
            };
            if let Some(last) = self.last_shape.as_ref().filter(|h| **h != q.shape).and_then(|h| side.shapes.get(h)) {
                for (old, new) in last.system.iter().zip(&shape.system).filter(|(old, new)| old != new) {
                    rows.push(Became { line: q.line, at: q.at.clone(), from: text_id(old), to: text_id(new), what: difference(old, new) });
                }
            }
            self.last_shape = Some(q.shape.clone());
            self.system_at += 1;
        }
        self.side.became.extend(rows);
    }

    /// What the whole line BUF (with its newline), starting at byte START, does to the side. CATCHING: only what it does
    /// to the one given read again from its line ([`SessionReader::give`]), a request known by where its line starts.
    fn line(&mut self, buf: &[u8], start: u64, catching: bool) {
        let line = buf.strip_suffix(b"\n").unwrap_or(buf);
        if !catching {
            self.lines += 1;
        }
        let n = self.lines;
        let has = |s: &[u8]| line.windows(s.len()).any(|w| w == s);
        let holds = |key: &str| !key.is_empty() && std::str::from_utf8(line).is_ok_and(|l| l.contains(key));
        let request = line.starts_with(b"{\"type\":\"api-request");
        // the givens whose line this is; one before it is at no line's start, and begins no step
        while self.ahead.first_key_value().is_some_and(|(at, _)| *at < start) {
            self.ahead.pop_first();
        }
        let mine = match self.ahead.first_entry() {
            Some(e) if *e.key() == start => e.remove(),
            _ => Vec::new(),
        };
        if !mine.is_empty() {
            // a given's own line: its text as text_at reads it; a queued one has yet to go in
            let mut texts = Vec::new();
            line_texts(line, &mut texts);
            let key = key_of(&texts.join("\n\n"));
            let queued = line.starts_with(b"{\"type\":\"queue-operation\"");
            self.waiting.extend(mine.into_iter().map(|i| Waiting { given: i, key: key.clone(), blobs: Vec::new(), went_in: !queued }));
        } else if !request && self.waiting.iter().any(|w| !w.went_in) && (has(b"\"queued_command\"") || has(b"\"isMeta\":true")) {
            for w in self.waiting.iter_mut().filter(|w| !w.went_in && holds(&w.key)) {
                w.went_in = true;
            }
        }
        // a response's usage, at each of its lines (each carries it; the latest counts): its request's tokens
        if !catching && !request && has(b"\"requestRef\"") {
            if let Some(a) = serde_json::from_slice::<ResponseLine>(line).ok().filter(|a| a.kind == "assistant" && !a.sidechain) {
                match self.ids.get(&a.request).copied() {
                    Some(k) => {
                        if let Some(u) = &a.message.usage {
                            self.side.requests[k].tokens = Some(crate::transcript::claude_tokens(u));
                            self.tokened.insert(k);
                        }
                    }
                    None => {
                        if self.unrequested.insert(a.request.clone()) {
                            self.side.unrequested.push(n);
                        }
                    }
                }
            }
        }
        // a request as it went: of a blob only its hash, when it holds a given's text; of a request its fields
        // (RequestLine); of a shape its system blocks' texts and its tools; any other kind never parsed
        if line.starts_with(b"{\"type\":\"api-request-blob\"") {
            if self.waiting.iter().any(|w| holds(&w.key)) {
                if let Ok(b) = serde_json::from_slice::<BlobLine>(line) {
                    for w in self.waiting.iter_mut().filter(|w| holds(&w.key)) {
                        w.blobs.push(b.hash.clone());
                    }
                }
            }
        } else if line.starts_with(b"{\"type\":\"api-request-shape\"") {
            if !catching {
                let v: serde_json::Value = serde_json::from_slice(line).unwrap_or_default();
                if let Some(h) = v["shapeHash"].as_str().filter(|h| !self.side.shapes.contains_key(*h)) {
                    let system = shape_system(&v);
                    let tools = serde_json::to_string(&v["shape"]["tools"]).unwrap_or_default();
                    let tool_count = v["shape"]["tools"].as_array().map_or(0, |t| t.len());
                    self.side.shapes.insert(h.to_string(), Shape { line: n, at: start, system, tools: tools_id(&tools), tool_count, tools_bytes: tools.len() });
                    self.shaped.push(h.to_string());
                }
            }
        } else if line.starts_with(b"{\"type\":\"api-request\"") {
            if let Ok(q) = serde_json::from_slice::<RequestLine>(line) {
                // its index: the next, or (read again) the one whose line starts here
                let k = if catching { self.request_at.binary_search(&start).ok() } else { Some(self.side.requests.len()) };
                if let Some(k) = k {
                    let (began, begun) = (&mut self.side.began, &mut self.begun);
                    self.waiting.retain(|w| {
                        let by_hash = w.blobs.iter().any(|b| q.tail.contains(b));
                        if by_hash || w.went_in {
                            began[w.given] = Some(Began { request: k, by_hash });
                            begun.push(w.given);
                        }
                        !(by_hash || w.went_in)
                    });
                }
                if !catching {
                    let word = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
                    self.ids.insert(q.id.clone(), self.side.requests.len());
                    self.request_at.push(start);
                    self.side.requests.push(Request { line: n, at: q.timestamp, id: q.id, model: word(&q.params["model"]), effort: word(&q.params["output_config"]["effort"]), shape: q.shape, tokens: None });
                }
            }
        } else if request || catching {
        } else if has(b"\"type\":\"instructions\"") {
            let v: serde_json::Value = serde_json::from_slice(line).unwrap_or_default();
            if v["type"] == "attachment" && v["attachment"]["type"] == "instructions" && v["isSidechain"] != true {
                let at = v["timestamp"].as_str().unwrap_or("").to_string();
                for x in v["attachment"]["files"].as_array().into_iter().flatten() {
                    let word = |k: &str| x[k].as_str().unwrap_or("").to_string();
                    let (path, text) = (word("path"), word("content"));
                    // the file's next version in the session: what changed since its last load (`became`)
                    let id = text_id(&text);
                    if let Some((was, old)) = self.last_load.get(&path).filter(|(was, _)| *was != id) {
                        self.side.became.push(Became { line: n, at: at.clone(), from: was.clone(), to: id.clone(), what: difference(old, &text) });
                    }
                    self.last_load.insert(path.clone(), (id, text.clone()));
                    self.side.loaded.push(Loaded { line: n, at: at.clone(), kind: word("type"), path, text });
                }
            }
        } else if (self.side.brief.is_none() && line.get(..400).unwrap_or(line).windows(13).any(|w| w == b"\"type\":\"user\"")) || (has(b"\"name\":\"Read\"") && has(b"\"type\":\"assistant\"")) {
            if let Some(e) = serde_json::from_slice::<Entry>(line).ok().filter(|e| !e.sidechain) {
                let content = e.message.map(|m| m.content).unwrap_or_default();
                match e.kind.as_str() {
                    "user" if self.side.brief.is_none() && !e.meta => {
                        let text = match &content {
                            serde_json::Value::String(s) => Some(s.clone()),
                            serde_json::Value::Array(items) => items.iter().find_map(|i| (i["type"] == "text").then(|| i["text"].as_str().map(String::from)).flatten()),
                            _ => None,
                        };
                        self.side.brief = text.map(|text| Brief { line: n, at: e.timestamp.clone(), text, compact: e.compact });
                    }
                    "assistant" => {
                        for i in content.as_array().into_iter().flatten().filter(|i| i["type"] == "tool_use" && i["name"] == "Read") {
                            if let Some(p) = i["input"]["file_path"].as_str() {
                                self.side.read.push(Reading { line: n, at: e.timestamp.clone(), path: p.to_string() });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Whether transcript FILE's first UPTO bytes end with a turn that ended (`Request::SystemPrompt`'s AFTER, accounts-lead
/// N-1446 p2447): whole lines, at least one turn in them, and the last turn-marking entry ([`classify`]) an end, as
/// [`Follower`] reads it: a copy cut there is a session between turns, which roster's `--cut` goes on from. Read from the
/// file's start, once per edit he makes.
pub fn turn_end(file: &str, upto: u64) -> bool {
    let Ok(f) = std::fs::File::open(file) else { return false };
    if upto == 0 || f.metadata().map_or(true, |m| m.len() < upto) {
        return false;
    }
    let mut buf = Vec::new();
    if f.take(upto).read_to_end(&mut buf).is_err() || buf.last() != Some(&b'\n') {
        return false;
    }
    crate::counts::read("system prompt (a turn's end)", upto);
    let last = buf.split(|b| *b == b'\n').filter(|l| !l.is_empty()).filter_map(|l| classify(l)?.mark).last();
    matches!(last, Some(Mark::Ended))
}

/// The end of transcript FILE's last turn that ended by BY (seconds): the bytes a copy cut there keeps, a session
/// between turns ([`turn_end`] holds there). Lines are read until the first written after BY. None when no turn ended
/// by then, or FILE cannot be read.
pub fn turn_end_by(file: &str, by: i64) -> Option<u64> {
    use std::io::BufRead;
    let mut f = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    let (mut at, mut ended, mut line) = (0u64, None, Vec::new());
    loop {
        line.clear();
        let n = f.read_until(b'\n', &mut line).ok()?;
        if n == 0 || line.last() != Some(&b'\n') {
            break;
        }
        at += n as u64;
        let body = &line[..n - 1];
        if written_after(body, by) {
            break;
        }
        if classify(body).and_then(|s| s.mark).is_some_and(|m| matches!(m, Mark::Ended)) {
            ended = Some(at);
        }
    }
    crate::counts::read("a rewind (a turn's end by a time)", at);
    ended
}

/// Whether entry LINE says it was written after BY (seconds): its `timestamp`, RFC 3339. A line with none is not.
fn written_after(line: &[u8], by: i64) -> bool {
    let key = b"\"timestamp\":\"";
    let Some(i) = line.windows(key.len()).position(|w| w == key) else { return false };
    let rest = &line[i + key.len()..];
    let end = rest.iter().position(|b| *b == b'"').unwrap_or(rest.len());
    std::str::from_utf8(&rest[..end]).ok().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).is_some_and(|t| t.timestamp() > by)
}

/// Whether byte AT of FILE starts a line: the file's start, or just after a newline.
fn line_starts(file: &str, at: u64) -> bool {
    if at == 0 {
        return true;
    }
    let mut b = [0u8];
    let read = std::fs::File::open(file).and_then(|mut f| {
        f.seek(SeekFrom::Start(at - 1))?;
        f.read_exact(&mut b)
    });
    read.is_ok() && b[0] == b'\n'
}

/// A shape line's system prompt blocks (`shape.system`, V its JSON): its one string, or each text block's text.
fn shape_system(v: &serde_json::Value) -> Vec<String> {
    match &v["shape"]["system"] {
        serde_json::Value::String(t) => vec![t.clone()],
        serde_json::Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

/// The system prompt block whose node is ID ([`text_id`]), from the shape line of hash SHAPE that starts at byte AT of
/// the session's FILE, that line alone read, off the lock (paper-research N-1446 p1642, 2): served only when the line
/// there is that shape's and one of its blocks hashes to ID, else refused, saying why and none of its words. Nothing of
/// it is kept. With TAIL, a text a start gave Claude Code beside its own (fix 4, docket-edge6 N-1446 p2529), which
/// Claude Code joins onto its last block after a blank line: the part of that block after two newlines in a row (one
/// of its blank lines, or a text's own first blank line, roster's `$(cat …)` keeping it) that hashes to ID, its
/// newlines at the end dropped as roster's `system_sha` drops them.
pub fn block_text(file: &str, at: u64, shape: &str, id: &str, tail: bool) -> Result<String, String> {
    use std::io::{BufRead, Seek, SeekFrom};
    let mut f = std::io::BufReader::new(std::fs::File::open(file).map_err(|e| format!("its session's file did not open: {e}"))?);
    let mut line = Vec::new();
    f.seek(SeekFrom::Start(at)).and_then(|_| f.read_until(b'\n', &mut line)).map_err(|e| format!("its session's file did not read: {e}"))?;
    if !line.starts_with(b"{\"type\":\"api-request-shape\"") {
        return Err(format!("no shape's line starts at byte {at} of its session's file"));
    }
    let v: serde_json::Value = serde_json::from_slice(&line).map_err(|e| format!("the shape's line did not read: {e}"))?;
    if v["shapeHash"].as_str() != Some(shape) {
        return Err(format!("the line at byte {at} of its session's file is another shape's"));
    }
    let blocks = shape_system(&v);
    if let Some(b) = blocks.iter().find(|b| text_id(b) == id) {
        return Ok(b.clone());
    }
    if !tail {
        return Err(format!("no block of that shape is {id}"));
    }
    // the shortest tail first: the text's own blank lines come after the one it was joined at; a tail after each
    // newline that follows another, so a text that begins with a blank line is tried too (accounts-lead p2587, 3)
    let last = blocks.last().map(String::as_str).unwrap_or_default();
    let b = last.as_bytes();
    let found = (2..=b.len()).rev().filter(|&i| b[i - 2] == b'\n' && b[i - 1] == b'\n').map(|i| last[i..].trim_end_matches('\n')).find(|t| text_id(t) == id);
    found.map(str::to_string).ok_or_else(|| format!("no block of that shape ends with {id}"))
}

/// TEXT's node in the record's graph: `text:` and its sha256's first 12 hex, so that a text that stood the same in
/// many loads and sessions is one node.
pub fn text_id(text: &str) -> String {
    format!("text:{}", sha12(text))
}

/// The node of the tools a shape carried, by their JSON TOOLS as serde_json writes it (its maps' keys sorted):
/// `tools:` and the JSON's sha256's first 12 hex.
pub fn tools_id(tools: &str) -> String {
    format!("tools:{}", sha12(tools))
}

/// TEXT's sha256's first 12 hex.
fn sha12(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes()).iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// At most this many words removed, and this many added, are written of a text's next version ([`difference`]).
const CUT: usize = 400;

/// The words that stay the same written on each side of a change ([`difference`]).
const AROUND: usize = 6;

/// The most pairs the `diff` crate is given to compare at once ([`difference`]): its table holds a u32 for each pair of
/// what lies between the two sides' common start and end, so 16 MB.
const COMPARED: usize = 4_000_000;

/// A word of [`difference`]: one that stays, one removed, one added.
#[derive(Clone, Copy, PartialEq)]
enum Word {
    Same,
    Gone,
    Came,
}

/// The words that changed from OLD to NEW, a `became` edge's `what` (paper-research N-1446 p1458): the lines that
/// differ, by the `diff` crate's difference over lines, and within each run of them the words, by its difference over
/// words, written as git's `--word-diff=plain` writes them, `[-removed-]` and `{+added+}`, with AROUND words that stay
/// on each side of a change and `…` for the rest. At most CUT words removed and CUT added are written, and the cut is
/// said; so is a run with too many words to compare (past COMPARED pairs), written as removed, then added.
pub fn difference(old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let mut long = !comparable(&a, &b);
    let lines: Vec<diff::Result<&&str>> = if long { a.iter().map(diff::Result::Left).chain(b.iter().map(diff::Result::Right)).collect() } else { diff::slice(&a, &b) };
    let (mut words, mut gone, mut came): (Vec<(Word, &str)>, Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new(), Vec::new());
    for r in lines {
        match r {
            diff::Result::Left(l) => gone.push(*l),
            diff::Result::Right(l) => came.push(*l),
            diff::Result::Both(l, _) => {
                long |= changed_words(&gone, &came, &mut words);
                gone.clear();
                came.clear();
                words.extend(l.split_whitespace().map(|w| (Word::Same, w)));
            }
        }
    }
    long |= changed_words(&gone, &came, &mut words);
    words_written(&words, long)
}

/// Whether the `diff` crate is given X and Y to compare: what lies between their common start and end is at most
/// COMPARED pairs.
fn comparable<T: PartialEq>(x: &[T], y: &[T]) -> bool {
    let start = x.iter().zip(y).take_while(|(p, q)| p == q).count();
    let end = x[start..].iter().rev().zip(y[start..].iter().rev()).take_while(|(p, q)| p == q).count();
    (x.len() - start - end).saturating_mul(y.len() - start - end) <= COMPARED
}

/// The words of a run of lines that differ, GONE's and CAME's, onto WORDS: compared word by word; or, when too many to
/// compare (true), removed, then added.
fn changed_words<'a>(gone: &[&'a str], came: &[&'a str], words: &mut Vec<(Word, &'a str)>) -> bool {
    let x: Vec<&'a str> = gone.iter().copied().flat_map(str::split_whitespace).collect();
    let y: Vec<&'a str> = came.iter().copied().flat_map(str::split_whitespace).collect();
    if !comparable(&x, &y) {
        words.extend(x.iter().map(|w| (Word::Gone, *w)).chain(y.iter().map(|w| (Word::Came, *w))));
        return true;
    }
    words.extend(diff::slice(&x, &y).into_iter().map(|r| match r {
        diff::Result::Left(w) => (Word::Gone, *w),
        diff::Result::Right(w) => (Word::Came, *w),
        diff::Result::Both(w, _) => (Word::Same, *w),
    }));
    false
}

/// WORDS as [`difference`] writes them: how many were removed and added, then each change, its words removed before
/// its words added, between the words that stay; LONG: a run had too many to compare.
fn words_written(words: &[(Word, &str)], long: bool) -> String {
    let count = |k: Word| words.iter().filter(|w| w.0 == k).count();
    let (gone, came) = (count(Word::Gone), count(Word::Came));
    let n = |n: usize| if n == 1 { "1 word".to_string() } else { format!("{n} words") };
    let mut out = match (gone, came) {
        (0, 0) => return "the same words: only their spacing or line breaks changed".to_string(),
        (g, 0) => format!("{} removed", n(g)),
        (0, c) => format!("{} added", n(c)),
        (g, c) => format!("{} removed, {c} added", n(g)),
    };
    if long {
        out.push_str(" (too many to compare word by word: written as removed, then added)");
    }
    out.push(':');
    // runs: of words that stay, or of a change (its words removed and its words added)
    enum Run<'a> {
        Same(Vec<&'a str>),
        Changed(Vec<&'a str>, Vec<&'a str>),
    }
    let mut runs: Vec<Run> = Vec::new();
    for &(k, w) in words {
        let joins = matches!((k, runs.last()), (Word::Same, Some(Run::Same(_))) | (Word::Gone | Word::Came, Some(Run::Changed(..))));
        if !joins {
            runs.push(if k == Word::Same { Run::Same(Vec::new()) } else { Run::Changed(Vec::new(), Vec::new()) });
        }
        match (k, runs.last_mut()) {
            (Word::Same, Some(Run::Same(ws))) | (Word::Gone, Some(Run::Changed(ws, _))) | (Word::Came, Some(Run::Changed(_, ws))) => ws.push(w),
            _ => {}
        }
    }
    let (mut left_gone, mut left_came, last) = (CUT, CUT, runs.len() - 1);
    for (i, run) in runs.iter().enumerate() {
        match run {
            Run::Same(ws) => {
                let start = if i > 0 { ws.len().min(AROUND) } else { 0 };
                let end = if i < last { ws.len().saturating_sub(AROUND).max(start) } else { ws.len() };
                for w in ws[..start].iter().chain(if start < end { &["…"][..] } else { &[][..] }).chain(&ws[end..]) {
                    out.push(' ');
                    out.push_str(w);
                }
            }
            Run::Changed(g, c) => {
                let mut cut = false;
                for (ws, left, open, close) in [(g, &mut left_gone, "[-", "-]"), (c, &mut left_came, "{+", "+}")] {
                    if ws.is_empty() {
                        continue;
                    }
                    let n = ws.len().min(*left);
                    *left -= n;
                    cut |= n < ws.len();
                    let more = if n == ws.len() { "" } else if n == 0 { "…" } else { " …" };
                    out.push_str(&format!(" {open}{}{more}{close}", ws[..n].join(" ")));
                }
                if cut {
                    out.push_str(" …");
                    break;
                }
            }
        }
    }
    if gone > CUT || came > CUT {
        out.push_str(&format!(" (cut there: at most {CUT} words removed and {CUT} added are written)"));
    }
    out
}

/// The agents' side in coordd's kept graph (paper-research N-1446 p1458): each session the record's `given` lines name,
/// with its agent, the record line of the first and each one's place and post, taken as the lines are recorded
/// ([`KeptSide::take`]); read on from where it left off, off the lock, in rounds ([`KeptSide::round`],
/// [`SideRound::read`]); and put in the graph under it as [`side_into`] puts in the whole of each ([`KeptSide::put`]),
/// so the graph is the same either way. A session is its file's stem ([`session_key`]), as the graph names it
/// (`session:ID`), at whatever place a `given` names it: a session the givens name at two places is one session, read
/// from the place whose given reaches furthest into it (research's cbc70c62, its file copied under an account's
/// folder while it ran on that account and named there once, was read at both and put twice: docket-core N-1446
/// p1838, p1904). Its agent is the first `given`'s that names it; another agent's givens to it are not its (no session
/// has two on research's record). A round from before a reload, of another `KeptSide` (another EPOCH), puts nothing.
#[derive(Debug)]
pub struct KeptSide {
    epoch: u64,
    sessions: Vec<KeptSession>,
    /// Each session's place in `sessions`, by [`session_key`].
    by_session: BTreeMap<String, usize>,
    /// Each agent's turn ends and cuts in record order: each one's time to the second and its line (`sent_in`).
    turns: BTreeMap<String, Vec<(String, usize)>>,
}

impl Default for KeptSide {
    fn default() -> KeptSide {
        static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        KeptSide { epoch: EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed), sessions: Vec::new(), by_session: BTreeMap::new(), turns: BTreeMap::new() }
    }
}

/// A session's key in a [`KeptSide`]: its file's stem, as the graph names a session (`session:ID`) and
/// [`KeptSide::shape_at`] finds one; the whole of FILE where it has none.
fn session_key(file: &str) -> &str {
    std::path::Path::new(file).file_stem().and_then(|x| x.to_str()).unwrap_or(file)
}

/// A session of a [`KeptSide`] as the record's lines alone make it (`take`), saved with a replay stopped at a cut
/// (ticket/replay_state.rs, I-1509): its agent, its file, the record line of the first `given` naming it, and each
/// given's place and post. What its reader has read is not the record's: a side read back reads its sessions anew, as
/// the start's side does.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SideSaved {
    agent: String,
    file: String,
    line: usize,
    given: Vec<(u64, String)>,
}

impl KeptSide {
    /// The sessions as the record's lines made them, to save: only what `take` puts in.
    pub fn saved(&self) -> Vec<SideSaved> {
        self.sessions.iter().map(|s| SideSaved { agent: s.agent.clone(), file: s.file.clone(), line: s.line, given: s.given.clone() }).collect()
    }

    /// A side of its own (a new epoch) holding SAVED, each session to be read from its start, as `take` leaves one. A
    /// state saved before p1838 holds a session once for each place a given named it at: here they are one, its
    /// givens in the order of their places, read from the place whose givens reach furthest, as `take` makes it.
    pub fn from_saved(saved: Vec<SideSaved>) -> KeptSide {
        let mut side = KeptSide::default();
        for s in saved {
            let furthest = s.given.iter().map(|(at, _)| *at).max().unwrap_or(0);
            match side.by_session.get(session_key(&s.file)) {
                Some(&i) => {
                    let k = &mut side.sessions[i];
                    if furthest > k.furthest {
                        (k.file, k.furthest) = (s.file, furthest);
                    }
                    k.line = k.line.min(s.line);
                    k.given.extend(s.given);
                    k.given.sort_by_key(|(at, _)| *at);
                }
                None => {
                    side.by_session.insert(session_key(&s.file).to_string(), side.sessions.len());
                    side.sessions.push(KeptSession { agent: s.agent, file: s.file, line: s.line, given: s.given, furthest, reader: Some(SessionReader::default()), ..Default::default() });
                }
            }
        }
        side
    }
}

/// A session of [`KeptSide`]: its agent, its file and the record line of the first `given` naming it; each given's
/// place and post, and how many its reader has had; its reader, out while a round reads it; and what of it is in the
/// graph: its node, each text's `version_of` once, its brief, its requests (whether each made its node, for its
/// tokens), the posts of each step by its first request, the requests waiting for their shape, and the ids of each
/// shape's system blocks.
#[derive(Debug, Default)]
struct KeptSession {
    agent: String,
    file: String,
    line: usize,
    given: Vec<(u64, String)>,
    /// How far into the session the furthest given's place reaches: FILE is the place one reaching that far named.
    furthest: u64,
    read: usize,
    reader: Option<SessionReader>,
    put: bool,
    stood: BTreeSet<(String, String)>,
    briefed: bool,
    made: Vec<bool>,
    steps: BTreeMap<usize, BTreeSet<String>>,
    unshaped: BTreeMap<String, Vec<usize>>,
    blocks: BTreeMap<String, Vec<String>>,
    /// The requests in no turn yet (`sent_in`): no turn of its agent's has ended at or after them yet.
    unsent: Vec<usize>,
    /// Each file its instructions loaded, by path: the line of each load and the text that stood (`ran_with`).
    loads: BTreeMap<String, Vec<(usize, String)>>,
}

/// One round of [`KeptSide`]: each session it reads, with its reader and the givens new to it.
#[derive(Debug)]
pub struct SideRound {
    epoch: u64,
    reads: Vec<SideRead>,
}

/// About the edges a hold of coordd's lock puts of a session's read (a load, a read or a request goes in whole, the
/// one that passes it last), the rest under the next hold (`KeptSide::put_part`, main.rs `put_side`). So no hold of the
/// first round after a start, which reads every session whole, is over a frame: on research's copy 68's put and the
/// index's catch-up after it took up to 2.3 µs an edge, a session's whole up to 158 ms (paper-research N-1446 p1919,
/// p1922 (a)). With 4,000 the first round's longest holds were still 21 to 26 ms, a part's catch-up at up to 5 µs an
/// edge in memory new to the process; half that is under a frame (p2072 1).
pub const SIDE_PART: usize = 2000;

#[derive(Debug)]
struct SideRead {
    session: usize,
    file: String,
    reader: SessionReader,
    given: Vec<u64>,
    opened: bool,
}

impl KeptSide {
    /// The `given` lines among LINES, the record's from its line FIRST + 1 on, that give a place: each its session's,
    /// POST giving the id of the post it handed; and each turn's end or cut, its agent's (`sent_in`). A given to another
    /// project's agent (PROJECT/NAME, `ticket::far`) makes no session: that name is a node here with a label and nothing
    /// read behind it, its sessions its own coord's (paper-research N-1446 p1828), so no round lists or opens one.
    pub fn take(&mut self, lines: &[docket_machine::Line], first: usize, post: &dyn Fn(u64, u32) -> String) {
        for (k, l) in lines.iter().enumerate() {
            if let docket_machine::Line::TurnEnded { at, agent, .. } | docket_machine::Line::TurnCut { at, agent } = l {
                self.turns.entry(agent.clone()).or_default().push((at.get(..19).unwrap_or(at.as_str()).to_string(), first + k + 1));
                continue;
            }
            let docket_machine::Line::Given { t, to, upto, place: Some(p), .. } = l else { continue };
            if crate::ticket::far(to).is_some() {
                continue;
            }
            let i = match self.by_session.get(session_key(&p.file)) {
                Some(&i) => i,
                None => {
                    self.sessions.push(KeptSession { agent: to.clone(), file: p.file.clone(), line: first + k + 1, reader: Some(SessionReader::default()), ..Default::default() });
                    self.by_session.insert(session_key(&p.file).to_string(), self.sessions.len() - 1);
                    self.sessions.len() - 1
                }
            };
            let s = &mut self.sessions[i];
            // the session named at another place (its file copied there): its reader reads on from the place whose
            // given reaches furthest into it, at the place it had got to, the copy holding the same lines before it
            if p.offset >= s.furthest {
                (s.file, s.furthest) = (p.file.clone(), p.offset);
            }
            if s.agent == *to {
                s.given.push((p.offset, post(*t, *upto)));
            }
        }
    }

    /// The agents of the sessions not read yet: which are agents is for the core to say (`Core::side_round`).
    pub fn agents(&self) -> impl Iterator<Item = &str> {
        self.sessions.iter().filter(|s| !s.put).map(|s| s.agent.as_str())
    }

    /// Where the request at line LINE of session STEM (its file's stem, as the graph names a session) found its shape:
    /// the session's file, where the shape's line starts and the shape's hash, as the session's reader holds them
    /// ([`Shape::at`]), for a block's text ([`block_text`]). Refused while a round has the reader out, and for a line
    /// that holds no request of the session's or a request whose shape is not read yet.
    pub fn shape_at(&self, stem: &str, line: usize) -> Result<(String, u64, String), String> {
        let s = self.by_session.get(stem).map(|&i| &self.sessions[i]).ok_or_else(|| format!("no session {stem} is read here"))?;
        let r = s.reader.as_ref().ok_or_else(|| format!("session {stem} is being read: ask again in a moment"))?;
        let q = r.side.requests.binary_search_by_key(&line, |q| q.line).map(|k| &r.side.requests[k]).map_err(|_| format!("no request of session {stem} is at its line {line}"))?;
        let shape = r.side.shapes.get(&q.shape).ok_or_else(|| format!("the shape of request {} is not read yet", q.id))?;
        Ok((s.file.clone(), shape.at, q.shape.clone()))
    }

    /// A round: each session read before, or of AGENTS, that no round has out, with the givens new to it.
    pub fn round(&mut self, agents: &BTreeSet<String>) -> SideRound {
        let mut reads = Vec::new();
        for (i, s) in self.sessions.iter_mut().enumerate() {
            if !(s.put || agents.contains(&s.agent)) {
                continue;
            }
            let Some(reader) = s.reader.take() else { continue };
            let given = s.given[s.read..].iter().map(|(at, _)| *at).collect();
            s.read = s.given.len();
            reads.push(SideRead { session: i, file: s.file.clone(), reader, given, opened: false });
        }
        SideRound { epoch: self.epoch, reads }
    }

    /// ROUND's reads into G, each session's readers back; nothing from another KeptSide's round (one from before a
    /// reload).
    pub fn put(&mut self, mut round: SideRound, g: &mut docket_machine::graph::Graph) {
        while !self.put_part(&mut round, g, usize::MAX) {}
    }

    /// ROUND's reads into G as [`KeptSide::put`] puts them, about MOST edges of them ([`SIDE_PART`]'s rule), each
    /// session's reader back once its read is all in: whether ROUND is. Each part takes up where the last left off,
    /// and the parts make the graph one put makes, edge for edge in its order. A round from before a reload puts
    /// nothing, and is done.
    pub fn put_part(&mut self, round: &mut SideRound, g: &mut docket_machine::graph::Graph, most: usize) -> bool {
        if round.epoch != self.epoch {
            round.reads.clear();
            return true;
        }
        let start = g.edges.len();
        while let Some(r) = round.reads.first_mut() {
            let s = &mut self.sessions[r.session];
            let turns = self.turns.get(&s.agent).map_or(&[][..], Vec::as_slice);
            let left = most.saturating_sub(g.edges.len().saturating_sub(start));
            if left == 0 || !s.put_into(g, &mut r.reader, turns, left) {
                return false;
            }
            let SideRead { session, reader, .. } = round.reads.remove(0);
            self.sessions[session].reader = Some(reader);
        }
        true
    }
}

impl SideRound {
    /// Each session read on, off the lock: the givens new to it given, then on from where it left off.
    pub fn read(mut self) -> SideRound {
        for r in &mut self.reads {
            r.reader.give(&r.file, &r.given);
            r.opened = r.reader.read_on(&r.file);
        }
        self
    }

    /// Each session's read as a round of its own, to be put under its own hold of coordd's lock (`put_side`).
    pub fn each(self) -> impl Iterator<Item = SideRound> {
        let epoch = self.epoch;
        self.reads.into_iter().map(move |r| SideRound { epoch, reads: vec![r] })
    }

    /// The session files that did not open.
    pub fn unopened(&self) -> Vec<String> {
        self.reads.iter().filter(|r| !r.opened).map(|r| r.file.clone()).collect()
    }
}

impl KeptSession {
    /// What R has read of the session that is not in G yet (p1376 point 4, p1388, p1398 5, p1429 2): the session → its
    /// agent, `session_of`, at the first `given` naming it; and each at the session's line, each file its instructions
    /// loaded, as the text that stood then (`loaded`; the text → its `file:`, `version_of`, once), its first user
    /// message (`briefed`), each file its Read calls read (`read`), and each request it sent, with its tokens (`sent`;
    /// the request → each block of its system prompt and its tools, `carried`, and → each post handed in its step,
    /// `for`). What comes to a request in already goes onto it: its tokens as its response's lines come; its `carried`
    /// when its shape's line comes after it; and when a step begins at it or before it (its `given` recorded after its
    /// line was read, [`SessionReader::give`]), the `for` of each request from there is made again. And (p1458 point 3)
    /// each request → the turn it was sent in, the first of TURNS (its agent's) that ended at or after it (`sent_in`;
    /// one in no turn yet goes in with a later round's), and → the text each loaded file stood as before it in the
    /// session (`ran_with`); each text → its next version, with the words that changed (`became`). The loads, reads and
    /// next versions put in leave the reader.
    ///
    /// About MOST edges go in ([`SIDE_PART`]'s rule: each load, read and request whole, and each request's turn and
    /// each next version so too), and whether all of R's read is in: if not, the next call takes up where this one left
    /// off, the edges in the order one call gives them (the turns and next versions after every request).
    fn put_into(&mut self, g: &mut docket_machine::graph::Graph, r: &mut SessionReader, turns: &[(String, usize)], most: usize) -> bool {
        use docket_machine::graph::{agent_id, EdgeKind, NodeKind, Source};
        let start = g.edges.len();
        let full = |g: &docket_machine::graph::Graph| g.edges.len().saturating_sub(start) >= most;
        let id = std::path::Path::new(&self.file).file_stem().and_then(|s| s.to_str()).unwrap_or(&self.file).to_string();
        let s = format!("session:{id}");
        if !std::mem::replace(&mut self.put, true) {
            g.node(s.clone(), NodeKind::Session, self.line);
            let a = g.node(agent_id(&self.agent), NodeKind::Agent, self.line);
            g.edge(&s, &a, EdgeKind::SessionOf, self.line);
        }
        let at = |n: usize| Source::Session(id.clone(), n);
        let side = &mut r.side;
        let mut loaded = std::mem::take(&mut side.loaded).into_iter();
        while !full(g) && let Some(x) = loaded.next() {
            let t = side_node(g, text_id(&x.text), NodeKind::Text, at(x.line), when_of(&x.at), || format!("{} {}, {} bytes", x.kind, x.path, x.text.len()));
            g.edge_from(&s, &t, EdgeKind::Loaded, at(x.line), when_of(&x.at));
            self.loads.entry(x.path.clone()).or_default().push((x.line, t.clone()));
            if self.stood.insert((t.clone(), x.path.clone())) {
                let f = side_node(g, format!("file:{}", x.path), NodeKind::File, at(x.line), when_of(&x.at), String::new);
                g.edge_from(&t, &f, EdgeKind::VersionOf, at(x.line), when_of(&x.at));
            }
        }
        side.loaded = loaded.collect();
        if !side.loaded.is_empty() {
            return false;
        }
        if !self.briefed && let Some(b) = &side.brief {
            self.briefed = true;
            let what = format!("{}, {} bytes", if b.compact { "a compaction's summary" } else { "the first user message" }, b.text.len());
            let t = side_node(g, text_id(&b.text), NodeKind::Text, at(b.line), when_of(&b.at), || what);
            g.edge_from(&s, &t, EdgeKind::Briefed, at(b.line), when_of(&b.at));
        }
        let mut read = std::mem::take(&mut side.read).into_iter();
        while !full(g) && let Some(x) = read.next() {
            let f = side_node(g, format!("file:{}", x.path), NodeKind::File, at(x.line), when_of(&x.at), String::new);
            g.edge_from(&s, &f, EdgeKind::Read, at(x.line), when_of(&x.at));
        }
        side.read = read.collect();
        if !side.read.is_empty() {
            return false;
        }
        // the steps begun since, each by its first request: the posts their givens handed
        let mut from = usize::MAX;
        for i in r.begun.drain(..) {
            if let Some(b) = side.began[i] {
                self.steps.entry(b.request).or_default().insert(self.given[i].1.clone());
                from = from.min(b.request);
            }
        }
        let before = self.made.len();
        // what came to requests in already: their tokens, at their responses' latest lines
        for k in std::mem::take(&mut r.tokened) {
            if k < before && self.made[k] && let Some(node) = g.get_mut(&format!("request:{}", side.requests[k].id)) {
                node.tokens = side.requests[k].tokens;
            }
        }
        // their `carried`, when their shape's line came after them
        for h in r.shaped.drain(..) {
            for k in self.unshaped.remove(&h).unwrap_or_default() {
                put_carried(g, &mut self.blocks, side, k, &id);
            }
        }
        // and when a step began at one of them, the `for` of each from there made again
        if from < before {
            let again: BTreeSet<usize> = (from..before).map(|k| side.requests[k].line).collect();
            g.retain_edges(|e| !(e.kind == EdgeKind::For && matches!(&e.source, Source::Session(x, n) if *x == id && again.contains(n))));
            for k in from..before {
                put_for(g, &self.steps, side, k, &id);
            }
        }
        // the requests not in yet, from the first (this call's own `before`, or one a call before it stopped at)
        while self.made.len() < side.requests.len() && !full(g) {
            let k = self.made.len();
            let q = &side.requests[k];
            let rq = format!("request:{}", q.id);
            let made = g.get(&rq).is_none();
            g.node_from(rq.clone(), NodeKind::Request, at(q.line), when_of(&q.at));
            if made && let Some(node) = g.get_mut(&rq) {
                node.what = format!("{} {}", q.model, q.effort);
                node.tokens = q.tokens;
            }
            self.made.push(made);
            g.edge_from(&s, &rq, EdgeKind::Sent, at(q.line), when_of(&q.at));
            if side.shapes.contains_key(&q.shape) {
                put_carried(g, &mut self.blocks, side, k, &id);
            } else {
                self.unshaped.entry(q.shape.clone()).or_default().push(k);
            }
            put_for(g, &self.steps, side, k, &id);
            // the text each loaded file stood as at it: its latest load before it (`ran_with`)
            for versions in self.loads.values() {
                if let Some((_, t)) = versions[..versions.partition_point(|(n, _)| *n < q.line)].last() {
                    g.edge_from(&rq, t, EdgeKind::RanWith, at(q.line), when_of(&q.at));
                }
            }
            self.unsent.push(k);
        }
        if self.made.len() < side.requests.len() {
            return false;
        }
        // each request in no turn yet: in the first of its agent's turns that ended at or after it, to the second, as
        // the record writes its times (`sent_in`); one with no time in none. About MOST of them a call, as the
        // requests: a long turn's requests all come at its end (17,456 of them in one hold of 32.5 ms on research's
        // copy before; paper-research N-1446 p2072 1)
        let agent = &self.agent;
        let mut unsent = std::mem::take(&mut self.unsent).into_iter();
        while !full(g) && let Some(k) = unsent.next() {
            let q = &side.requests[k];
            let Some(second) = q.at.get(..19) else { continue };
            let Some((_, line)) = turns.get(turns.partition_point(|(t, _)| t.as_str() < second)) else {
                self.unsent.push(k);
                continue;
            };
            g.edge_from(&format!("request:{}", q.id), &docket_machine::graph::turn_id(agent, *line), EdgeKind::SentIn, at(q.line), when_of(&q.at));
        }
        let rest: Vec<usize> = unsent.collect();
        let stopped = !rest.is_empty();
        self.unsent.extend(rest);
        if stopped {
            return false;
        }
        // each text's next version, with the words that changed (`became`), about MOST of them a call too
        let mut became = std::mem::take(&mut side.became).into_iter();
        while !full(g) && let Some(x) = became.next() {
            g.edge_from(&x.from, &x.to, EdgeKind::Became, at(x.line), when_of(&x.at));
            if let Some(e) = g.edges.last_mut() {
                e.what = x.what;
            }
        }
        side.became = became.collect();
        side.became.is_empty()
    }
}

/// A time as a session's line writes it; none when it writes none.
fn when_of(t: &str) -> Option<String> {
    Some(t.to_string()).filter(|t| !t.is_empty())
}

/// The side's node ID, of KIND, from SOURCE at WHEN, WHAT saying what it is: made once. One the side made takes the
/// source, time and what of its earliest occurrence (by time, then by session and line), so that the graph is the same
/// in whatever order its sessions were read, each whole or on from where it left off (p1458); one the record or git
/// made keeps its own.
fn side_node(g: &mut docket_machine::graph::Graph, id: String, kind: docket_machine::graph::NodeKind, source: docket_machine::graph::Source, when: Option<String>, what: impl FnOnce() -> String) -> String {
    use docket_machine::graph::Source;
    fn key<'a>(s: &'a Source, when: &'a Option<String>) -> Option<(bool, Option<&'a str>, &'a str, usize)> {
        match s {
            Source::Session(f, n) => Some((when.is_none(), when.as_deref(), f.as_str(), *n)),
            _ => None,
        }
    }
    let earlier = match g.get(&id) {
        None => {
            let x = g.node_from(id, kind, source, when);
            if let Some(n) = g.get_mut(&x) {
                n.what = what();
            }
            return x;
        }
        Some(n) => key(&n.source, &n.when).zip(key(&source, &when)).is_some_and(|(old, new)| new < old),
    };
    if earlier && let Some(n) = g.get_mut(&id) {
        (n.source, n.when, n.what) = (source, when, what());
    }
    id
}

/// Request K of SIDE (session ID's) → each block of its system prompt and its tools, `carried`, from its shape (the
/// blocks' ids made once a shape, in BLOCKS).
fn put_carried(g: &mut docket_machine::graph::Graph, blocks: &mut BTreeMap<String, Vec<String>>, side: &Side, k: usize, id: &str) {
    use docket_machine::graph::{EdgeKind, NodeKind, Source};
    let q = &side.requests[k];
    let Some(shape) = side.shapes.get(&q.shape) else { return };
    let (r, at, when) = (format!("request:{}", q.id), Source::Session(id.to_string(), q.line), when_of(&q.at));
    let ids = blocks.entry(q.shape.clone()).or_insert_with(|| shape.system.iter().map(|b| text_id(b)).collect());
    for (block, t) in shape.system.iter().zip(ids.iter()) {
        let t = side_node(g, t.clone(), NodeKind::Text, at.clone(), when.clone(), || format!("a system prompt block, {} bytes", block.len()));
        g.edge_from(&r, &t, EdgeKind::Carried, at.clone(), when.clone());
    }
    let tools = side_node(g, shape.tools.clone(), NodeKind::Tools, at.clone(), when.clone(), || format!("{} tools, {} bytes", shape.tool_count, shape.tools_bytes));
    g.edge_from(&r, &tools, EdgeKind::Carried, at, when);
}

/// Request K of SIDE (session ID's) → each post handed in its step, `for`: the step begun at it or the latest before
/// it, in STEPS by its first request.
fn put_for(g: &mut docket_machine::graph::Graph, steps: &BTreeMap<usize, BTreeSet<String>>, side: &Side, k: usize, id: &str) {
    use docket_machine::graph::{EdgeKind, NodeKind, Source};
    let q = &side.requests[k];
    let (r, at, when) = (format!("request:{}", q.id), Source::Session(id.to_string(), q.line), when_of(&q.at));
    for p in steps.range(..=k).next_back().map(|(_, posts)| posts).into_iter().flatten() {
        let to = side_node(g, p.clone(), NodeKind::Post, at.clone(), when.clone(), String::new);
        g.edge_from(&r, &to, EdgeKind::For, at.clone(), when.clone());
    }
}

/// The side of each agent AGENTS picks, in G (paper-research N-1446 p1376 point 4, p1388), from the sessions the
/// record's LINES name: a `given` line's `where` is the only way a session file is opened here, never a path from
/// elsewhere (p1380). It is coordd's kept side ([`KeptSide`]) in one round, each session read whole, POST giving a
/// post's id from a `given`'s ticket and post; what goes in is `KeptSession::put_into`'s. A text's `what` says what
/// it stood as at its earliest (`side_node`). Gives the session files that did not open.
pub fn side_into(g: &mut docket_machine::graph::Graph, lines: &[docket_machine::Line], agents: &dyn Fn(&str) -> bool, post: &dyn Fn(u64, u32) -> String) -> Vec<String> {
    let mut kept = KeptSide::default();
    kept.take(lines, 0, post);
    let picked: BTreeSet<String> = kept.agents().filter(|a| agents(a)).map(String::from).collect();
    let round = kept.round(&picked).read();
    let unopened = round.unopened();
    kept.put(round, g);
    unopened
}

/// What a tool call is about, for its line in the agents' list: its command, file, pattern, description or prompt.
fn what(input: &serde_json::Value) -> String {
    ["command", "file_path", "pattern", "description", "prompt"].iter().find_map(|k| input.get(*k).and_then(|v| v.as_str())).unwrap_or("").chars().take(200).collect()
}

/// The tool call I (a `tool_use` item) of an entry written AT, as the agents' list shows it: also what a background
/// shell or subagent it started is said as (`transcript::Background`, N-1334).
pub fn tool_of(i: &serde_json::Value, at: &str) -> Tool {
    Tool { id: i["id"].as_str().unwrap_or("").into(), tool: i["name"].as_str().unwrap_or("").into(), what: what(&i["input"]), about: about(&i["input"]), at: at.get(..19).map(|t| format!("{t}Z")).unwrap_or_default() }
}

/// What a tool call says it does (`Tool.about`, N-516.p3): its input's `description`, its first line, at most 200
/// characters; none when it gives none.
pub fn about(input: &serde_json::Value) -> Option<String> {
    let d = input.get("description")?.as_str()?.trim();
    let line: String = d.lines().next().unwrap_or("").trim().chars().take(200).collect();
    (!line.is_empty()).then_some(line)
}

/// A call to TOOL as insight (1) counts it: its name, with `docket_machine::insight::COORD_IN_A_SHELL` after it when its
/// COMMAND (a shell call's) runs coord alone ([`runs_coord_alone`]): `Bash(coord)`, coord's own as its MCP tools are.
fn counted(tool: &str, command: Option<&str>) -> String {
    match command {
        Some(c) if runs_coord_alone(c) => format!("{tool}{}", docket_machine::insight::COORD_IN_A_SHELL),
        _ => tool.to_string(),
    }
}

/// The command line of Codex shell call P: its `arguments`' `command` (or `cmd`), or a `local_shell_call`'s
/// `action.command`; given as words, the script of a shell's `-c`/`-lc` run of one, else the words joined.
fn codex_command(p: &serde_json::Value) -> Option<String> {
    let args = p["arguments"].as_str().and_then(|a| serde_json::from_str::<serde_json::Value>(a).ok()).unwrap_or_else(|| p["action"].clone());
    let command = if args["command"].is_null() { &args["cmd"] } else { &args["command"] };
    match command {
        serde_json::Value::String(line) => Some(line.clone()),
        serde_json::Value::Array(words) => {
            let words: Vec<&str> = words.iter().filter_map(|w| w.as_str()).collect();
            Some(match words.as_slice() {
                [_, flag, script] if flag.starts_with('-') && flag.ends_with('c') => script.to_string(),
                _ => words.join(" "),
            })
        }
        _ => None,
    }
}

/// Whether shell command CMD runs coord alone: an agent's talk (`coord post`, `coord msg`, as standing.md tells it to
/// talk), which insight (1) counts with coord's own tools (proto-reader N-1446 p511, p516; proto-lead's terms, p544, 2
/// and p546). Each of its commands, split at `&&`, `||`, `;`, a lone `&` and new lines outside quotes, is a `cd`, an
/// `export` of assignments only, or coord, by its name or a path ending in `/coord`, after any `VAR=value`, `env` and
/// `timeout N`; and one is coord. What a pipe feeds is one of [`FILTERS`], which only read coord's output (`| head`).
/// A heredoc's lines are coord's input, plain under a quoted delimiter (`<<'EOF'`, `<<"EOF"`); under an unquoted one
/// the shell runs their substitutions, so those are read as any other. A command substitution (`$(…)` or backticks)
/// that runs only `cat`, of a heredoc or a file, is coord's argument (`coord post N-1 "$(cat <<'EOF' … EOF)"`); any
/// other runs a program. `cargo test && coord post …`, `coord ls | xargs cargo test` and `coord post N x & cargo build`
/// are work, and so is `export X=$(cargo build); coord …`.
fn runs_coord_alone(cmd: &str) -> bool {
    let mut subs = Vec::new();
    let cmds = shell_commands(&mut cmd.chars().peekable(), None, &mut subs);
    run_alone(&cmds, "coord") && subs.iter().all(|sub| run_alone(sub, "cat"))
}

/// The programs a pipe may feed coord's output to with the call still coord's alone: they only read it. `sed` and `awk`
/// are not among them, as they can write (`sed -i`, `awk`'s `system`) (proto-lead N-1446 p544, 2).
const FILTERS: [&str; 8] = ["head", "tail", "grep", "wc", "cut", "sort", "uniq", "jq"];

/// A shell's commands: each one's words, and whether a pipe feeds it.
type Commands = Vec<(bool, Vec<String>)>;

/// The commands of the shell text CS holds, up to its end, or, inside a command substitution, up to the CLOSE that ends
/// it (`)` for `$(…)`, a backtick for backticks); each command substitution's commands are put in SUBS (a
/// substitution's text stands in its word as `$(…)`). Quotes are read as the shell reads them, and a heredoc's lines,
/// up to its delimiter's, are its input: skipped, but for the substitutions in them when its delimiter is unquoted.
fn shell_commands(cs: &mut std::iter::Peekable<std::str::Chars>, close: Option<char>, subs: &mut Vec<Commands>) -> Commands {
    let mut cmds: Commands = vec![(false, Vec::new())];
    let mut word: Option<String> = None;
    let mut quote: Option<char> = None;
    // the delimiters of the heredocs whose lines begin at the next new line, each with whether it was quoted
    let mut heredocs: Vec<(String, bool)> = Vec::new();
    let end = |word: &mut Option<String>, cmds: &mut Commands| {
        if let (Some(w), Some((_, words))) = (word.take(), cmds.last_mut()) {
            words.push(w);
        }
    };
    while let Some(c) = cs.next() {
        if quote.is_none() && close == Some(c) {
            break;
        }
        // a command substitution, `$(…)` or backticks, in a word or in double quotes: its own commands, read in turn
        let opens = match c {
            '$' if quote != Some('\'') && cs.next_if_eq(&'(').is_some() => Some(')'),
            '`' if quote != Some('\'') => Some('`'),
            _ => None,
        };
        if let Some(end) = opens {
            let sub = shell_commands(cs, Some(end), subs);
            subs.push(sub);
            word.get_or_insert_with(String::new).push_str("$(…)");
            continue;
        }
        if let Some(q) = quote {
            match c {
                _ if c == q => quote = None,
                '\\' if q == '"' => word.get_or_insert_with(String::new).extend(cs.next()),
                _ => word.get_or_insert_with(String::new).push(c),
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                word.get_or_insert_with(String::new);
            }
            '\\' => word.get_or_insert_with(String::new).extend(cs.next()),
            ' ' | '\t' => end(&mut word, &mut cmds),
            // `2>&1`, `>&2`, `&>`: a redirection's, not a command's end
            '&' if word.as_deref().is_some_and(|w| w.ends_with('>')) || cs.peek() == Some(&'>') => word.get_or_insert_with(String::new).push(c),
            '<' if cs.peek() == Some(&'<') => {
                end(&mut word, &mut cmds);
                cs.next();
                // `<<<`, a here-string, has no lines of its own
                if cs.next_if_eq(&'<').is_none() {
                    cs.next_if_eq(&'-');
                    while cs.next_if_eq(&' ').is_some() {}
                    let delim: String = std::iter::from_fn(|| cs.next_if(|c| !c.is_whitespace() && !matches!(c, ';' | '&' | '|' | ')'))).collect();
                    let quoted = delim.contains(['\'', '"', '\\']);
                    heredocs.push((delim.chars().filter(|c| !matches!(c, '\'' | '"' | '\\')).collect(), quoted));
                }
            }
            '\n' | ';' | '&' | '|' => {
                end(&mut word, &mut cmds);
                let piped = c == '|' && cs.next_if_eq(&'|').is_none();
                // `|&` pipes both outputs; `&&`; a lone `&` ends a command run in the background
                if piped || c == '&' {
                    cs.next_if_eq(&'&');
                }
                if c == '\n' {
                    for (delim, quoted) in std::mem::take(&mut heredocs) {
                        while cs.peek().is_some() {
                            let line: String = cs.by_ref().take_while(|c| *c != '\n').collect();
                            if line.trim() == delim {
                                break;
                            }
                            if !quoted {
                                expanded(&line, subs);
                            }
                        }
                    }
                }
                cmds.push((piped, Vec::new()));
            }
            _ => word.get_or_insert_with(String::new).push(c),
        }
    }
    end(&mut word, &mut cmds);
    cmds
}

/// The command substitutions in LINE, a heredoc's line under an unquoted delimiter, where the shell runs `$(…)` and
/// backticks: their commands put in SUBS, as a word's are. The rest of it is input.
fn expanded(line: &str, subs: &mut Vec<Commands>) {
    let mut cs = line.chars().peekable();
    while let Some(c) = cs.next() {
        let end = match c {
            '\\' => {
                cs.next();
                continue;
            }
            '$' if cs.next_if_eq(&'(').is_some() => ')',
            '`' => '`',
            _ => continue,
        };
        let sub = shell_commands(&mut cs, Some(end), subs);
        subs.push(sub);
    }
}

/// Whether CMDS run PROGRAM alone: each command a pipe feeds is one of [`FILTERS`]; each other is a `cd`, an `export`
/// whose words are all assignments (a `VAR=value` of the commands after it, consult-3 N-1446 p541, 1), or PROGRAM (by
/// its name, or a path ending in it), after any `VAR=value`, `env` and `timeout N`; and one of them is PROGRAM.
fn run_alone(cmds: &Commands, program: &str) -> bool {
    let assignment = |w: &&str| w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let mut ran = false;
    for (piped, words) in cmds.iter().filter(|(_, words)| !words.is_empty()) {
        let mut ws = words.iter().map(String::as_str).filter(|w| !assignment(w));
        let mut first = ws.next();
        while let Some(prefix @ ("env" | "timeout")) = first {
            first = ws.find(|w| prefix == "env" || !(w.starts_with('-') || w.starts_with(|c: char| c.is_ascii_digit())));
        }
        match first.map(|p| p.rsplit('/').next().unwrap_or(p)) {
            Some(p) if *piped => {
                if !FILTERS.contains(&p) {
                    return false;
                }
            }
            Some("cd") => {}
            Some("export") if ws.next().is_none() => {}
            Some(p) if p == program => ran = true,
            _ => return false,
        }
    }
    ran
}

/// What entry LINE says ([`Said`]); None for an entry that says nothing (a system record other than `turn_duration`,
/// an attachment, a meta entry, a summary, a slash command's echo).
fn classify(line: &[u8]) -> Option<Said> {
    // most of a transcript is tool output: a cheap look first
    let head = &line[..line.len().min(400)];
    let has = |s: &[u8]| line.windows(s.len()).any(|w| w == s);
    let head_has = |s: &[u8]| head.windows(s.len()).any(|w| w == s);
    // a Codex rollout line (`{"timestamp":..,"type":..,"payload":..}`): a Claude Code entry never begins so
    if line.starts_with(b"{\"timestamp\":") && head_has(b"\"payload\":") {
        return if head_has(b"\"type\":\"event_msg\"") || head_has(b"\"type\":\"response_item\"") || head_has(b"\"type\":\"turn_context\"") { codex(line) } else { None };
    }
    // a slash command's result (`/model`, `/autocompact`), as Claude Code writes it since it keeps them as system
    // entries: what he set inside the session is kept as the agent's (`local`)
    if head_has(b"\"subtype\":\"local_command\"") {
        let e: Entry = serde_json::from_slice(line).ok()?;
        return local(e.content.as_deref().unwrap_or("").trim_start(), &e.timestamp);
    }
    // a compaction is over: its boundary, written when it ends (`durationMs`)
    if head_has(b"\"subtype\":\"compact_boundary\"") && head_has(b"\"type\":\"system\"") {
        return Some(Said { compacted: true, ..Default::default() });
    }
    if !(head_has(b"\"type\":\"user\"") || head_has(b"\"type\":\"queue-operation\"") || head_has(b"\"type\":\"assistant\"") || has(b"\"subtype\":\"turn_duration\"") || has(b"\"subtype\":\"stop_hook_summary\"") || has(b"\"type\":\"user\"") || has(b"\"type\":\"assistant\"")) {
        return None;
    }
    let e: Entry = serde_json::from_slice(line).ok()?;
    if e.sidechain {
        return None;
    }
    let ended = || Some(Said { mark: Some(Mark::Ended), ..Default::default() });
    // the session stopped: the text said since the last user entry is the turn's final output
    let stopped = || Some(Said { mark: Some(Mark::Ended), closes: true, ..Default::default() });
    match e.kind.as_str() {
        "queue-operation" if e.operation == "enqueue" => {
            let t = e.content.as_deref().unwrap_or("").trim_start();
            let queue = if coord_message(t) { Queue::In(t.trim_end().to_string()) } else { Queue::None };
            Some(Said { ids: mail_ids(t), bare: bare_body(t), queue, ..Default::default() })
        }
        // a coord message queued in a turn, put into the model's input at its next tool result (its `queued_command`
        // attachment follows, stamped with the enqueue's time): delivered now. Its ids were seen at the enqueue
        "queue-operation" if e.operation == "remove" => {
            let t = e.content.as_deref().unwrap_or("").trim();
            coord_message(t).then(|| Said { queue: Queue::Took { text: t.to_string(), ids: mail_ids(t), prompt: false, when: e.timestamp.clone() }, ..Default::default() })
        }
        "system" if e.subtype == "turn_duration" || e.subtype == "stop_hook_summary" => stopped(),
        "assistant" if e.api_error => ended(),
        "assistant" => {
            let began = match e.message.as_ref().map(|m| &m.content) {
                Some(serde_json::Value::Array(items)) => items
                    .iter()
                    .filter(|i| i["type"] == "tool_use")
                    .map(|i| tool_of(i, &e.timestamp))
                    .filter(|t| !t.id.is_empty())
                    .collect(),
                _ => Vec::new(),
            };
            let text = matches!(e.message.as_ref().map(|m| &m.content), Some(serde_json::Value::Array(items)) if items.iter().any(|i| i["type"] == "text"));
            // what it calls, by name, for insight (1): a shell's call running coord alone named so (`counted`)
            let calls = match e.message.as_ref().map(|m| &m.content) {
                Some(serde_json::Value::Array(items)) => items.iter().filter(|i| i["type"] == "tool_use").filter_map(|i| i["name"].as_str().filter(|n| !n.is_empty()).map(|n| counted(n, i["input"]["command"].as_str()))).collect(),
                _ => Vec::new(),
            };
            let answered_by = e.message.and_then(|m| m.model).filter(|m| !m.starts_with('<'));
            let answer = answered_by.is_some();
            Some(Said { mark: Some(Mark::Running), began, text, calls, answered_by, answer, at: e.timestamp, ..Default::default() })
        }
        "user" if e.compact => None,
        "user" => {
            let content = e.message.map(|m| m.content).unwrap_or_default();
            let answered: Vec<String> = match &content {
                serde_json::Value::Array(items) => items.iter().filter(|i| i["type"] == "tool_result").filter_map(|i| i["tool_use_id"].as_str().map(String::from)).collect(),
                _ => Vec::new(),
            };
            let text = match &content {
                serde_json::Value::String(s) => s.as_str(),
                serde_json::Value::Array(items) => items.iter().find_map(|i| (i["type"] == "text").then(|| i["text"].as_str()).flatten()).unwrap_or(""),
                _ => "",
            };
            let t = text.trim_start();
            if t.starts_with("[Request interrupted by user") {
                return Some(Said { mark: Some(Mark::Ended), cut: true, ..Default::default() });
            }
            // a channel message is a meta entry too (`isMeta`, `origin.kind: channel`): it starts a turn and carries mail
            let channel = t.starts_with("<channel source=");
            if let Some(set) = local(t, &e.timestamp) {
                return Some(set);
            }
            // a background task's notice wakes the session: a turn begins, as a prompt does (consult-3, N-807.p63)
            let notice = t.starts_with("<task-notification>");
            if (e.meta && !channel && !notice) || t.starts_with("<command-name>") || t.starts_with("<local-command") || t.starts_with("<command-message>") {
                return None;
            }
            let (ids, bare) = (mail_ids(t), bare_body(t));
            let typed = !channel && !notice && ids.is_empty() && bare.is_none() && answered.is_empty() && is_typed(t);
            // what it carries entered the model's input here: as a prompt (a queued message dequeued, at a turn's end or
            // an esc), or with tool results into the running turn (a hook's mail in a session from before). The owner's
            // words alone carry no id (render's `words`) and leave the queue here all the same: kept there, they read as
            // still queued at the turn's end, so the turn they began was given nothing and its answer was posted nowhere
            // (docket-edge7's walk, N-1446 p3050, 1)
            let queue = if ids.is_empty() && bare.is_none() { Queue::None } else { Queue::Took { text: t.trim_end().to_string(), ids: ids.clone(), prompt: answered.is_empty(), when: e.timestamp.clone() } };
            Some(Said { mark: Some(Mark::Running), ids, bare, answered, user: true, typed, queue, at: e.timestamp, ..Default::default() })
        }
        _ => None,
    }
}

/// Whether T is a coord channel message (what the session's queue of coord messages holds).
fn coord_message(t: &str) -> bool {
    t.starts_with("<channel source=\"coord\"")
}

/// Whether a user entry's text T, carrying no coord delivery, is words typed into the terminal: not empty, not a
/// tag Claude Code or Codex writes (`<system-reminder>`, `<user_instructions>`, a command's), not the Stop hook's mail.
fn is_typed(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty() && !t.starts_with('<') && !t.contains("Mail that arrived for you (coord") && !t.starts_with("Stop hook feedback")
}

/// What Codex rollout LINE says. A turn runs from `task_started` to `task_complete` (or `turn_aborted`, an interrupt:
/// a cut). Mail and ticket deliveries come as a user message of sections headed `[ID · from NAME ...]` (main's
/// `Job::Codex`), the owner's words to an agent holding only them as `[from NAME]`; a tool's output is a user entry
/// as Claude Code's tool results are, so the final output is the assistant's messages after the last of them. The
/// running tool calls are the hooks' (`Core::hook`), as are a Codex agent's turns.
fn codex(line: &[u8]) -> Option<Said> {
    let v: serde_json::Value = serde_json::from_slice(line).ok()?;
    let p = &v["payload"];
    let kind = p["type"].as_str().unwrap_or("");
    match (v["type"].as_str().unwrap_or(""), kind) {
        ("event_msg", "task_started") => Some(Said { mark: Some(Mark::Running), ..Default::default() }),
        ("event_msg", "task_complete") => Some(Said { mark: Some(Mark::Ended), closes: true, ..Default::default() }),
        ("event_msg", "turn_aborted") => Some(Said { mark: Some(Mark::Ended), cut: true, ..Default::default() }),
        ("response_item", "function_call_output" | "custom_tool_call_output") => Some(Said { user: true, ..Default::default() }),
        // what the turn calls and the model it runs on, counted for insight (1); the running calls stay the hooks'
        ("response_item", k @ ("function_call" | "custom_tool_call" | "local_shell_call")) => Some(Said { calls: vec![counted(p["name"].as_str().unwrap_or(k), codex_command(p).as_deref())], ..Default::default() }),
        ("turn_context", _) => p["model"].as_str().map(|m| Said { answered_by: Some(m.to_string()), ..Default::default() }),
        ("response_item", "message") => {
            let text = p["content"].as_array().into_iter().flatten().filter(|i| matches!(i["type"].as_str(), Some("input_text" | "output_text" | "text"))).filter_map(|i| i["text"].as_str()).collect::<Vec<_>>().join("\n");
            match p["role"].as_str() {
                Some("assistant") => Some(Said { text: !text.is_empty(), ..Default::default() }),
                Some("user") if !text.starts_with("<environment_context>") => {
                    let (ids, bare) = codex_mail(&text);
                    let typed = ids.is_empty() && bare.is_none() && is_typed(&text);
                    // a Codex (or opencode) rollout's user message is the model's input: delivered here, at its place
                    let when = v["timestamp"].as_str().unwrap_or("").to_string();
                    let queue = if ids.is_empty() { Queue::None } else { Queue::Took { text: text.trim().to_string(), ids: ids.clone(), prompt: true, when } };
                    Some(Said { ids, bare, user: true, typed, queue, ..Default::default() })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The delivery ids a Codex user message T carries (each section's `[ID · from NAME ...]` head, at its start or after
/// a blank line), and the body of its `[from NAME]` section if it has one.
pub(crate) fn codex_mail(t: &str) -> (Vec<String>, Option<String>) {
    let starts: Vec<usize> = std::iter::once(0).chain(t.match_indices("\n\n[").map(|(i, _)| i + 2)).filter(|&i| t[i..].starts_with('[')).collect();
    let (mut ids, mut bare) = (Vec::new(), None);
    for (k, &i) in starts.iter().enumerate() {
        let Some((head, _)) = t[i + 1..].split_once(']') else { continue };
        if let Some((id, _)) = head.split_once(" · from ").filter(|(id, _)| !id.is_empty() && !id.contains(char::is_whitespace)) {
            ids.push(id.to_string());
        } else if head.starts_with("from ") && !head.contains(" · ") {
            let end = starts.get(k + 1).map_or(t.len(), |&j| j - 2);
            let section = &t[i..end];
            bare = section.split_once('\n').map(|(_, b)| b.trim_matches('\n').to_string());
        }
    }
    (ids, bare)
}

/// What the result T of a slash command typed in the session set, written AT: a `/model` pick ("Set model to
/// `Fable 5.2` …") or an `/autocompact` ("Auto-compact window for Fable 5.2 set to 600k tokens …"). The owner, 10 Nov:
/// "if i change a model with /model it should never auto overwrite that"; 19 Nov, of `/autocompact`: "all setting
/// overwrite from cc by the user hsould worl and be tracked back into ours". None for any other result ("Kept model
/// as …", "… unchanged: 350k tokens (from settings)").
fn local(t: &str, at: &str) -> Option<Said> {
    let t = t.strip_prefix("<local-command-stdout>")?;
    if let Some(m) = t.strip_prefix("Set model to `").and_then(|r| r.split('`').next()).and_then(picked) {
        return Some(Said { model: Some((m, at.to_string())), ..Default::default() });
    }
    let set = typed_point(t.strip_prefix("Auto-compact window for ")?)?;
    Some(Said { compact: Some((set, at.to_string())), ..Default::default() })
}

/// A compaction point set with `/autocompact` in a session: the point asked for (None: `auto`, Claude Code's own for
/// the model), and what the session runs at when another setting stands above the command ("…, but a
/// higher-priority override is active (350k tokens)": the command saved the point and the session kept the other).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Typed {
    pub asked: Option<u64>,
    pub held_at: Option<u64>,
}

/// Of `/autocompact`'s result after "Auto-compact window for ": "Fable 5.2 set to 600k tokens", "… set to 600k
/// tokens (capped to model limit of 500k tokens)", "… set to auto, …", "… set to 600k tokens, but a higher-priority
/// override is active (350k tokens)". None when nothing was set ("… unchanged: …").
fn typed_point(t: &str) -> Option<Typed> {
    let (_, set) = t.split_once(" set to ")?;
    let word = set.split([' ', '<']).next()?.trim_end_matches(',');
    let asked = if word == "auto" { None } else { Some(tokens(word)?) };
    let held_at = set.split_once("override is active (").and_then(|(_, r)| tokens(r.split([' ', ')']).next()?));
    Some(Typed { asked, held_at })
}

/// A token count as Claude Code writes one: `600k`, `1M`, `1.5M`, `350000`, `350,000`.
fn tokens(w: &str) -> Option<u64> {
    let w = w.replace(',', "");
    let (n, by) = match w.char_indices().last()? {
        (i, 'k' | 'K') => (&w[..i], 1_000.0),
        (i, 'm' | 'M') => (&w[..i], 1_000_000.0),
        _ => (w.as_str(), 1.0),
    };
    let n: f64 = n.parse().ok().filter(|n: &f64| n.is_finite() && *n > 0.0)?;
    Some((n * by).round() as u64)
}

/// The model id of a `/model` pick's display NAME (`Set model to `Fable 5.2``): `claude-fable-5-2`, with `[1m]` for a
/// "(1M context)" pick. A name of no Claude family (a picker entry of the settings', `Kiro Opus 5.5`) is None: the
/// session's next reply names its model.
fn picked(name: &str) -> Option<String> {
    let (name, long) = match name.split_once(" (") {
        Some((n, rest)) => (n, rest.to_ascii_lowercase().contains("1m")),
        None => (name, false),
    };
    let mut words = name.split_whitespace();
    let (family, version) = (words.next()?.to_ascii_lowercase(), words.next()?);
    let known = ["fable", "mythos", "opus", "sonnet", "haiku"].contains(&family.as_str());
    let numeric = !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    (known && numeric && words.next().is_none()).then(|| format!("claude-{family}-{}{}", version.replace('.', "-"), if long { "[1m]" } else { "" }))
}

/// The body of T when it is a coord channel entry without an `id` (`<channel source="coord" from="owner">\nWORDS\n</channel>`).
pub fn bare_body(t: &str) -> Option<String> {
    let tag_body = t.strip_prefix("<channel source=\"coord\"")?;
    let (tag, body) = tag_body.split_once('>')?;
    if tag.contains(" id=\"") {
        return None;
    }
    let body = body.trim_end();
    let body = body.strip_suffix("</channel>").unwrap_or(body);
    Some(body.trim_matches('\n').to_string())
}

/// The delivery ids of coord channel entry T (`<channel source="coord" ... id="...">`, as a user entry or a
/// `queue-operation` enqueue holds it): its tag's `id`, split on spaces, since one message joining several deliveries
/// gives all of theirs there (`render::handover`, 0.2 line 211). Empty for any other text. The one reading of the tag
/// for every reader: the transcript's follower (`mail_ids`), and what pairs an enqueue with its giving.
pub fn channel_ids(t: &str) -> Vec<String> {
    let Some(tag) = t.trim_start().strip_prefix("<channel source=\"coord\"") else { return Vec::new() };
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    // ` id="`: not the end of another attribute's name
    tag.split(" id=\"").nth(1).and_then(|r| r.split('"').next()).map(|ids| ids.split_whitespace().map(str::to_string).collect()).unwrap_or_default()
}

/// The mail ids entry text T delivered: a coord channel entry's `id` ([`channel_ids`]), or each `[mN · from X]` of a
/// hook's mail text.
fn mail_ids(t: &str) -> Vec<String> {
    if t.starts_with("<channel source=\"coord\"") {
        return channel_ids(t);
    }
    if !t.starts_with("Mail that arrived for you (coord") && !t.starts_with("Stop hook feedback") {
        return Vec::new();
    }
    t.match_indices('[')
        .filter_map(|(i, _)| {
            let rest = &t[i + 1..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let ok = id.len() > 1 && (id.starts_with('m') || id.starts_with('q')) && id[1..].chars().all(|c| c.is_ascii_digit());
            (ok && rest[id.len()..].starts_with(" · from ")).then_some(id)
        })
        .collect()
}

#[cfg(test)]
mod typed_tests {
    use super::*;

    /// One message joining several deliveries (0.2 line 211): its tag's id holds them all, space-separated. The enqueue
    /// Claude Code writes when it queues the message, and the entry when the model takes it, each give every id at that
    /// entry: one reading of the tag for every reader (consult-3, N-807.p112, 2).
    #[test]
    fn a_message_joining_several_gives_each_id_where_it_is_queued_and_taken() {
        let tag = "<channel source=\\\"coord\\\" from=\\\"coord\\\" kind=\\\"handover\\\" id=\\\"N-412.p851 N-591.p40 C-12.s3\\\">\\n2 threads, one after another.\\n</channel>";
        let ids: Vec<String> = ["N-412.p851", "N-591.p40", "C-12.s3"].map(String::from).to_vec();
        assert_eq!(channel_ids(&tag.replace("\\\"", "\"").replace("\\n", "\n")), ids);
        let enqueue = format!(r#"{{"type":"queue-operation","operation":"enqueue","timestamp":"2026-11-20T08:00:00.000Z","sessionId":"s","content":"{tag}"}}"#);
        assert_eq!(classify(enqueue.as_bytes()).map(|s| s.ids), Some(ids.clone()));
        let user = format!(r#"{{"parentUuid":"p","isSidechain":false,"type":"user","isMeta":true,"message":{{"role":"user","content":"{tag}"}},"uuid":"u","timestamp":"2026-11-20T08:00:01.000Z"}}"#);
        assert_eq!(classify(user.as_bytes()).map(|s| s.ids), Some(ids));
        // one delivery's tag, as before; the owner's words with no id, and another server's tag, give none
        assert_eq!(channel_ids("<channel source=\"coord\" id=\"N-5.p1\" ticket=\"N-5\" kind=\"thread\" from=\"w1\">\nx\n</channel>"), ["N-5.p1"]);
        assert!(channel_ids("<channel source=\"coord\" from=\"owner\">\nhis words\n</channel>").is_empty());
        assert!(channel_ids("<channel source=\"plugin\" id=\"N-5.p1\">\nx\n</channel>").is_empty());
    }

    /// What he sets inside a session is read from its record: `/autocompact`'s result as Claude Code 2.1.327 writes it
    /// (a system entry, `local_command`), and `/model`'s in the same form (the owner, N-541 post 34).
    #[test]
    fn a_point_or_a_model_set_in_the_session_is_read_from_its_record() {
        let line = |out: &str| format!(r#"{{"parentUuid":"p","isSidechain":false,"type":"system","subtype":"local_command","content":"<local-command-stdout>{out}</local-command-stdout>","level":"info","timestamp":"2026-11-19T07:06:09.098Z","uuid":"u","isMeta":false}}"#);
        let point = |out: &str| classify(line(out).as_bytes()).and_then(|s| s.compact).map(|(t, at)| (t.asked, t.held_at, at));
        let at = || "2026-11-19T07:06:09.098Z".to_string();
        assert_eq!(point("Auto-compact window for Fable 5.2 set to 600k tokens"), Some((Some(600_000), None, at())));
        assert_eq!(point("Auto-compact window for Fable 5.2 set to 600k tokens, but a higher-priority override is active (350k tokens)"), Some((Some(600_000), Some(350_000), at())));
        assert_eq!(point("Auto-compact window for Opus 5.5 set to 1M tokens (capped to model limit of 500k tokens)"), Some((Some(1_000_000), None, at())));
        assert_eq!(point("Auto-compact window for Opus 5.5 set to 350,000 tokens"), Some((Some(350_000), None, at())));
        assert_eq!(point("Auto-compact window for Fable 5.2 set to auto, but a higher-priority override is active (350k tokens)"), Some((None, Some(350_000), at())));
        assert_eq!(point("Auto-compact window for Fable 5.2 set to auto (967k tokens)"), Some((None, None, at())));
        assert_eq!(point("Auto-compact window for Fable 5.2 unchanged: 350k tokens (from settings)"), None);
        assert_eq!(point("Auto-compact is off (see /config)"), None);
        let model = |out: &str| classify(line(out).as_bytes()).and_then(|s| s.model).map(|(m, _)| m);
        assert_eq!(model("Set model to `Fable 5.2` and saved as your default for new sessions"), Some("claude-fable-5-2".to_string()));
        assert_eq!(model("Kept model as `Opus 5.5`"), None);
        // before the session's first exchange Claude Code 2.1.328 writes the answer as a user entry (docket-edge3's try,
        // N-804.p16: a session started with --autocompact 350k, then `/autocompact 600000` typed)
        let user = r#"{"parentUuid":"p","isSidechain":false,"promptId":"x","type":"user","message":{"role":"user","content":"<local-command-stdout>Auto-compact window for Opus 5.5 set to 600k tokens</local-command-stdout>"},"uuid":"u","timestamp":"2026-11-19T09:03:48.140Z","userType":"external","entrypoint":"cli","version":"2.1.328"}"#;
        assert_eq!(classify(user.as_bytes()).and_then(|s| s.compact).map(|(t, _)| (t.asked, t.held_at)), Some((Some(600_000), None)));
        // the echo of the command itself says nothing
        let echo = r#"{"type":"system","subtype":"local_command","content":"<command-name>/autocompact</command-name>\n<command-args></command-args>","timestamp":"2026-11-19T07:06:09.098Z"}"#;
        assert!(classify(echo.as_bytes()).is_none());
    }
}

/// The delivered moment (0.2 A1, N-1397 posts 5 and 6): Claude Code writes every coord message as `queue-operation
/// enqueue` first (where `given` is recorded), and the text enters the model's input later, in one of the shapes cut
/// here from research's session files of 8 to 20 Nov: idle, `dequeue` and a user entry at once; working, `remove` at the
/// next tool result, its `queued_command` stamped with the enqueue's time; compacting, `remove` after the boundary; at
/// an esc, `dequeue` and a user entry after the interrupt (51 of 52, none `remove`d).
#[cfg(test)]
mod queue_tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn msg(id: &str) -> String {
        format!("<channel source=\"coord\" from=\"owner\" id=\"{id}\" kind=\"thread\" ticket=\"N-66\">\nowner wrote:\nhello\n</channel>")
    }
    fn enqueue(id: &str, at: &str) -> serde_json::Value {
        json!({"type": "queue-operation", "operation": "enqueue", "timestamp": at, "sessionId": "s", "content": msg(id)})
    }
    fn dequeue(at: &str) -> serde_json::Value {
        json!({"type": "queue-operation", "operation": "dequeue", "timestamp": at, "sessionId": "s"})
    }
    fn remove(id: &str, at: &str) -> serde_json::Value {
        json!({"type": "queue-operation", "operation": "remove", "timestamp": at, "sessionId": "s", "content": msg(id)})
    }
    fn queued_command(id: &str, enqueued: &str) -> serde_json::Value {
        json!({"type": "attachment", "timestamp": enqueued, "attachment": {"type": "queued_command", "prompt": msg(id), "commandMode": "prompt"}})
    }
    fn prompt(id: &str, at: &str) -> serde_json::Value {
        json!({"type": "user", "isMeta": true, "origin": {"kind": "channel"}, "timestamp": at, "message": {"role": "user", "content": msg(id)}})
    }
    fn tool_use(at: &str) -> serde_json::Value {
        json!({"type": "assistant", "timestamp": at, "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "true"}}]}})
    }
    fn tool_result(at: &str) -> serde_json::Value {
        json!({"type": "user", "timestamp": at, "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}]}})
    }
    fn said(text: &str, at: &str) -> serde_json::Value {
        json!({"type": "assistant", "timestamp": at, "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]}})
    }
    fn stop(at: &str) -> serde_json::Value {
        json!({"type": "system", "subtype": "stop_hook_summary", "timestamp": at})
    }
    fn interrupt(at: &str) -> serde_json::Value {
        json!({"type": "user", "timestamp": at, "message": {"role": "user", "content": [{"type": "text", "text": "[Request interrupted by user]"}]}})
    }
    fn boundary(at: &str) -> serde_json::Value {
        json!({"type": "system", "subtype": "compact_boundary", "timestamp": at, "compactMetadata": {"trigger": "auto", "durationMs": 102610}})
    }

    /// A transcript file of this test's own (removed when dropped), and a follower begun on it while it is empty, so
    /// all it reads is new.
    struct File {
        path: std::path::PathBuf,
    }
    impl File {
        fn new(name: &str) -> (File, Follower) {
            let path = std::env::temp_dir().join(format!("follow-queue-{name}-{}.jsonl", std::process::id()));
            std::fs::write(&path, "").unwrap();
            let f = Follower::new(&path.display().to_string());
            (File { path }, f)
        }
        /// Append LINES; the offset each begins at.
        fn write(&self, lines: &[serde_json::Value]) -> Vec<u64> {
            let mut w = std::fs::OpenOptions::new().append(true).open(&self.path).unwrap();
            let mut at = std::fs::metadata(&self.path).unwrap().len();
            let mut out = Vec::new();
            for l in lines {
                let s = format!("{l}\n");
                out.push(at);
                at += s.len() as u64;
                w.write_all(s.as_bytes()).unwrap();
            }
            out
        }
        fn len(&self, line: &serde_json::Value) -> u64 {
            format!("{line}\n").len() as u64 - 1
        }
    }
    impl Drop for File {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// The turn a reboot cut (paper-research N-1104 p785): a transcript ending inside a turn, a prompt and a tool call
    /// with its result and nothing after, was cut when its file was last written; one whose turn stopped, or that an
    /// esc cut, has nothing to go on with; nor has a file that is not there.
    #[test]
    fn a_turn_left_running_at_the_files_end_was_cut_when_it_was_last_written() {
        let typed = |at: &str| json!({"type": "user", "timestamp": at, "message": {"role": "user", "content": "go on with the build"}});
        let (file, _) = File::new("cut-at");
        let path = file.path.display().to_string();
        file.write(&[typed("2026-11-22T00:30:00Z"), tool_use("2026-11-22T00:30:05Z"), tool_result("2026-11-22T00:34:59Z")]);
        let written = std::fs::metadata(&path).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        assert_eq!(cut_at(&path), Some(written), "a turn running at the end");
        file.write(&[said("Built.", "2026-11-22T00:35:10Z"), stop("2026-11-22T00:35:11Z")]);
        assert_eq!(cut_at(&path), None, "the turn stopped");
        file.write(&[typed("2026-11-22T00:36:00Z"), tool_use("2026-11-22T00:36:01Z"), interrupt("2026-11-22T00:36:02Z")]);
        assert_eq!(cut_at(&path), None, "an esc cut it");
        assert_eq!(cut_at(&format!("{path}.gone")), None, "no file");
    }

    /// The three shapes the record's `given` cannot tell apart, each delivered at the moment its text entered the
    /// model's input and paired with its own enqueue's place: idle, at its prompt; working, at its `remove` after the
    /// tool result, not at the enqueue nor at its attachment's stamp; compacting, after the boundary (a8685306, 19 Nov:
    /// enqueued 11:25:07, delivered 11:26:50, 103 s later).
    #[test]
    fn a_delivery_enters_the_input_idle_at_its_prompt_working_at_its_remove_and_compacting_after_the_boundary() {
        // idle: the turn before ended; the message is enqueued, dequeued and is the next prompt
        let (idle, mut f) = File::new("idle");
        let e = enqueue("N-66.p1", "2026-11-20T07:36:13.447Z");
        let at = idle.write(&[said("before", "2026-11-20T07:30:00.000Z"), stop("2026-11-20T07:30:00.100Z"), e.clone(), dequeue("2026-11-20T07:36:13.460Z"), prompt("N-66.p1", "2026-11-20T07:36:13.486Z")]);
        let r = f.read();
        assert_eq!(r.entered, vec![Entered { id: "N-66.p1".into(), place: (at[2], idle.len(&e)), entry: at[4], at: "2026-11-20T07:36:13Z".into(), began: true }]);
        assert!(r.at.iter().any(|(id, o, _)| id == "N-66.p1" && *o == at[2]), "seen at its enqueue, as today: {:?}", r.at);
        assert!(r.queued.is_empty());
        // working: enqueued while a tool runs, removed at its result
        let (working, mut f) = File::new("working");
        let e = enqueue("N-66.p2", "2026-11-19T11:28:18.665Z");
        let at = working.write(&[prompt("N-66.p1", "2026-11-19T11:27:00.000Z"), tool_use("2026-11-19T11:28:10.000Z"), e.clone(), tool_result("2026-11-19T11:29:29.123Z"), remove("N-66.p2", "2026-11-19T11:29:29.416Z"), queued_command("N-66.p2", "2026-11-19T11:28:18.665Z")]);
        let r = f.read();
        let p2: Vec<&Entered> = r.entered.iter().filter(|e| e.id == "N-66.p2").collect();
        assert_eq!(p2, vec![&Entered { id: "N-66.p2".into(), place: (at[2], working.len(&e)), entry: at[4], at: "2026-11-19T11:29:29Z".into(), began: false }], "at the remove, into the running turn");
        assert!(!r.at.iter().any(|(id, o, _)| id == "N-66.p2" && *o != at[2]), "the remove is no second place: {:?}", r.at);
        // compacting: enqueued as the compaction ran, removed after its boundary
        let (compacting, mut f) = File::new("compacting");
        let e = enqueue("N-947.p2", "2026-11-19T11:25:07.876Z");
        let at = compacting.write(&[prompt("N-66.p1", "2026-11-19T11:24:00.000Z"), tool_use("2026-11-19T11:25:00.000Z"), e.clone(), boundary("2026-11-19T11:26:46.420Z"), tool_result("2026-11-19T11:26:50.800Z"), remove("N-947.p2", "2026-11-19T11:26:50.859Z"), queued_command("N-947.p2", "2026-11-19T11:25:07.876Z")]);
        let r = f.read();
        let got: Vec<(&str, (u64, u64), &str)> = r.entered.iter().filter(|e| e.id == "N-947.p2").map(|e| (e.at.as_str(), e.place, e.id.as_str())).collect();
        assert_eq!(got, vec![("2026-11-19T11:26:50Z", (at[2], compacting.len(&e)), "N-947.p2")], "after the compaction, not at the enqueue");
        assert!(r.compacted, "its boundary says the compaction is over");
        // a boundary in a start's window is an old compaction's: it ends none running now
        let mut start = Follower::new(&compacting.path.display().to_string());
        assert!(!start.read().compacted);
    }

    /// The 194 shape: a message queued while the turn's last message is written, the turn ends, and the message is
    /// dequeued as the next prompt. At that turn's end it is still queued (its enqueue's place, at the output's end),
    /// and it enters as the prompt that began the next turn.
    #[test]
    fn a_message_queued_as_a_turn_ends_is_queued_at_that_end_and_begins_the_next_turn() {
        let (file, mut f) = File::new("end");
        let e = enqueue("N-412.p887", "2026-11-19T11:41:51.955Z");
        let last = said("done", "2026-11-19T11:41:53.000Z");
        let at = file.write(&[prompt("N-66.p1", "2026-11-19T11:41:00.000Z"), tool_use("2026-11-19T11:41:10.000Z"), tool_result("2026-11-19T11:41:11.000Z"), e.clone(), last.clone(), stop("2026-11-19T11:41:53.100Z"), dequeue("2026-11-19T11:41:53.200Z"), prompt("N-412.p887", "2026-11-19T11:41:53.230Z")]);
        let r = f.read();
        let end = at[4] + file.len(&last);
        assert_eq!(r.queued, vec![(end, vec![(at[3], file.len(&e))])], "queued when the turn ended");
        let p: Vec<&Entered> = r.entered.iter().filter(|e| e.id == "N-412.p887").collect();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].place, p[0].entry > end, p[0].began), ((at[3], file.len(&e)), true, true), "it began the next turn");
        assert_eq!(f.take_outputs(), vec![(at[4], file.len(&last))]);
    }

    /// The owner's words alone (render's `words`: the tag says only whom they are from, no id), as a new project's
    /// agent took "hello" (docket-edge7's walk, N-1446 p3050, 1): enqueued, dequeued, and written as the prompt that
    /// begins a turn. That prompt takes them out of the queue, so at the turn's end nothing is queued (and the turn,
    /// not the next, holds them: `Core::handed_after`), and its last message is its output.
    #[test]
    fn the_owners_words_alone_leave_the_queue_as_the_prompt_of_the_turn_they_begin() {
        let (file, mut f) = File::new("bare");
        let words = "<channel source=\"coord\" from=\"owner\">\nhello\n</channel>";
        let e = json!({"type": "queue-operation", "operation": "enqueue", "timestamp": "2026-11-24T20:21:38.937Z", "sessionId": "s", "content": words});
        let p = json!({"type": "user", "isMeta": true, "origin": {"kind": "channel"}, "timestamp": "2026-11-24T20:21:39.047Z", "message": {"role": "user", "content": words}});
        let last = said("Hello. I'm up and waiting for a task.", "2026-11-24T20:21:40.000Z");
        let at = file.write(&[e, dequeue("2026-11-24T20:21:38.991Z"), p, last.clone(), stop("2026-11-24T20:21:40.795Z")]);
        let r = f.read();
        assert!(r.queued.is_empty(), "nothing queued when the turn ended: {:?}", r.queued);
        assert!(f.queued_now().is_empty(), "nor after it");
        assert_eq!(f.take_outputs(), vec![(at[3], file.len(&last))]);
    }

    /// The run on eng-79 (docket-edge7, N-1446 p3214): try-1's session as Claude Code wrote it when his "hello" came
    /// alone (enqueue, dequeue, a meta reminder, the prompt, the answer, the Stop hook, the turn's length;
    /// testdata/bare-hello-eng79.jsonl, thinking and request bodies emptied). The words leave the queue at the prompt
    /// (H-1646), and they enter the model's input there too: one entry with no id, placed at their enqueue, as the
    /// prompt of the turn whose answer is the output. Before, no entry came for words without an id, so coordd never
    /// marked them delivered and handed them again.
    #[test]
    fn the_owners_words_alone_enter_at_their_prompt_as_an_entry_with_no_id_placed_at_their_enqueue() {
        let (file, mut f) = File::new("bare-eng79");
        let lines: Vec<serde_json::Value> = include_str!("../testdata/bare-hello-eng79.jsonl").lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let at = file.write(&lines);
        let r = f.read();
        assert_eq!(r.entered, vec![Entered { id: String::new(), place: (at[0], file.len(&lines[0])), entry: at[4], at: "2026-11-24T23:27:59Z".into(), began: true }]);
        assert!(r.queued.is_empty() && f.queued_now().is_empty(), "nothing left queued: {:?}", r.queued);
        assert_eq!(r.bare.iter().map(|(b, o, _)| (b.as_str(), *o)).collect::<Vec<_>>(), vec![("hello", at[0]), ("hello", at[4])], "the words seen at the enqueue, then at the prompt");
        assert_eq!(f.take_outputs(), vec![(at[8], file.len(&lines[8]))], "the answer is the turn's output");
    }

    /// The esc shape (consult-3, N-1397 post 6, 2): a message queued in a turn the owner interrupts is dequeued and
    /// written as a user entry after the interrupt, which goes into the next request with it. It is not removed back into
    /// his box and not dropped: it entered as the prompt of the turn after the cut, so that turn holds it.
    #[test]
    fn a_message_queued_at_an_esc_enters_after_the_interrupt_as_the_next_turns_prompt() {
        let (file, mut f) = File::new("esc");
        let e = enqueue("N-101.p287", "2026-11-16T07:53:59.253Z");
        let at = file.write(&[prompt("N-66.p1", "2026-11-16T07:53:50.000Z"), tool_use("2026-11-16T07:53:55.000Z"), tool_result("2026-11-16T07:53:56.000Z"), e.clone(), dequeue("2026-11-16T07:54:13.864Z"), interrupt("2026-11-16T07:54:13.851Z"), prompt("N-101.p287", "2026-11-16T07:54:13.881Z")]);
        let r = f.read();
        assert_eq!(r.cut, Some(at[5]));
        let p: Vec<&Entered> = r.entered.iter().filter(|e| e.id == "N-101.p287").collect();
        assert_eq!(p, vec![&Entered { id: "N-101.p287".into(), place: (at[3], file.len(&e)), entry: at[6], at: "2026-11-16T07:54:13Z".into(), began: true }]);
        assert!(r.begins.contains(&at[6]), "the turn after the cut begins with it");
    }

    /// The same id given twice, the first delivered, then a restart between the second enqueue and its remove
    /// (consult-3, N-1397 post 6, 1): the first read's window pairs the old remove with the old enqueue only, so the second
    /// giving's place is still queued; its own remove later names its own place.
    #[test]
    fn a_restarts_window_pairs_each_take_with_its_own_enqueue_never_a_later_giving_of_the_same_id() {
        let (file, _) = File::new("twice");
        let (e1, e2) = (enqueue("N-412.p9", "2026-11-20T08:00:00.000Z"), enqueue("N-412.p9", "2026-11-20T09:00:00.000Z"));
        let at = file.write(&[prompt("N-66.p1", "2026-11-20T07:59:00.000Z"), tool_use("2026-11-20T07:59:30.000Z"), e1.clone(), tool_result("2026-11-20T08:00:05.000Z"), remove("N-412.p9", "2026-11-20T08:00:05.100Z"), said("one", "2026-11-20T08:01:00.000Z"), stop("2026-11-20T08:01:00.100Z"), prompt("N-66.p2", "2026-11-20T08:59:00.000Z"), tool_use("2026-11-20T08:59:30.000Z"), e2.clone()]);
        // coordd starts at 09:01: a new follower reads the window
        let mut f = Follower::new(&file.path.display().to_string());
        let r = f.read();
        let p: Vec<(u64, u64)> = r.entered.iter().filter(|e| e.id == "N-412.p9").map(|e| e.place).collect();
        assert_eq!(p, vec![(at[2], file.len(&e1))], "the old remove is the old giving's");
        assert_eq!(f.queue.iter().map(|(o, l, _)| (*o, *l)).collect::<Vec<_>>(), vec![(at[9], file.len(&e2))], "the second is still queued");
        let more = file.write(&[tool_result("2026-11-20T09:05:00.000Z"), remove("N-412.p9", "2026-11-20T09:05:00.100Z")]);
        let r = f.read();
        let p: Vec<(u64, u64, u64)> = r.entered.iter().filter(|e| e.id == "N-412.p9").map(|e| (e.place.0, e.place.1, e.entry)).collect();
        assert_eq!(p, vec![(at[9], file.len(&e2), more[1])], "its own remove names its own place");
        assert!(f.queue.is_empty());
    }

    /// What each turn did, for insight (1) (S6): its own tool calls counted by name from its begin, a subagent's
    /// (sidechain) not among them, and the model that last answered, not Claude Code's own `<synthetic>`; kept by where
    /// the turn's final output ends, which is the output taken, and taken once. The next turn counts from its own begin,
    /// and a turn whose transcript names no model has none, not the turn before's.
    /// A turn begun before the follower did is not kept (its calls before the window would be missing). A shell call
    /// that runs coord alone is `Bash(coord)`, one that runs anything else `Bash`. A Codex rollout's function calls, its
    /// shell's `coord` likewise, and its `turn_context`'s model (written before `task_started` too) are counted alike.
    #[test]
    fn each_turns_tools_and_model_are_kept_by_where_its_output_ends() {
        use docket_machine::Worked;
        let worked = |tools: &[(&str, u32)], model: &str| Worked { tools: tools.iter().map(|(t, n)| (t.to_string(), *n)).collect(), model: Some(model.into()) };
        let call = |id: &str, name: &str, at: &str| json!({"type": "assistant", "timestamp": at, "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [{"type": "tool_use", "id": id, "name": name, "input": {}}]}});
        let side = json!({"type": "assistant", "isSidechain": true, "timestamp": "2026-11-20T09:00:04.000Z", "message": {"role": "assistant", "model": "claude-haiku-5-5", "content": [{"type": "tool_use", "id": "s1", "name": "Grep", "input": {}}]}});
        let synthetic = json!({"type": "assistant", "timestamp": "2026-11-20T09:00:07.000Z", "message": {"role": "assistant", "model": "<synthetic>", "content": []}});
        let (file, mut f) = File::new("worked");
        file.write(&[prompt("N-66.p1", "2026-11-20T09:00:00.000Z"), call("t1", "Bash", "2026-11-20T09:00:01.000Z"), tool_result("2026-11-20T09:00:01.500Z"), call("t2", "Read", "2026-11-20T09:00:02.000Z"), side, call("t3", "Bash", "2026-11-20T09:00:05.000Z"), call("t4", "mcp__coord__reply", "2026-11-20T09:00:06.000Z"), synthetic, said("Done.", "2026-11-20T09:00:08.000Z"), stop("2026-11-20T09:00:08.100Z")]);
        f.read();
        let (o, l) = f.take_output().expect("the turn's output");
        assert_eq!(f.take_worked(), vec![(o + l, worked(&[("Bash", 2), ("Read", 1), ("mcp__coord__reply", 1)], "claude-opus-5-5"))]);
        assert_eq!(f.take_worked(), vec![], "taken once");
        // the next turn: no tool, another model
        let mut answer = said("Nothing", "2026-11-20T09:10:01.000Z");
        answer["message"]["model"] = json!("claude-sonnet-5-5");
        file.write(&[prompt("N-66.p2", "2026-11-20T09:10:00.000Z"), answer, stop("2026-11-20T09:10:01.100Z")]);
        f.read();
        let (o, l) = f.take_output().expect("its output");
        assert_eq!(f.take_worked(), vec![(o + l, worked(&[], "claude-sonnet-5-5"))], "counted from its own begin");
        // a turn whose transcript names no model: not known, never the turn before's model (consult-3 N-1446 p541, 2)
        file.write(&[prompt("N-66.p9", "2026-11-20T09:15:00.000Z"), said("Nothing", "2026-11-20T09:15:01.000Z"), stop("2026-11-20T09:15:01.100Z")]);
        f.read();
        let (o, l) = f.take_output().expect("its output");
        assert_eq!(f.take_worked(), vec![(o + l, Worked::default())], "no model of the turn before");
        // a turn running when the follower began: its end is not kept; the next one is
        file.write(&[prompt("N-66.p3", "2026-11-20T09:20:00.000Z"), call("t5", "Bash", "2026-11-20T09:20:01.000Z")]);
        let mut late = Follower::new(&file.path.display().to_string());
        late.read();
        file.write(&[said("Built.", "2026-11-20T09:20:09.000Z"), stop("2026-11-20T09:20:09.100Z"), prompt("N-66.p4", "2026-11-20T09:30:00.000Z"), call("t6", "Edit", "2026-11-20T09:30:01.000Z"), said("Edited.", "2026-11-20T09:30:02.000Z"), stop("2026-11-20T09:30:02.100Z")]);
        late.read();
        // its outputs: the old turn's, then the new one's
        let ends: Vec<u64> = late.take_outputs().iter().map(|(o, l)| o + l).collect();
        assert_eq!(ends.len(), 2);
        assert_eq!(late.take_worked(), vec![(ends[1], worked(&[("Edit", 1)], "claude-opus-5-5"))], "the old turn's end not kept");
        // a turn that only talks, through coord in a shell and its reply tool (proto-reader N-1446 p511); a shell call
        // that also runs something else is the shell's own
        let bash = |id: &str, command: &str, at: &str| json!({"type": "assistant", "timestamp": at, "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": command, "description": "x"}}]}});
        file.write(&[
            prompt("N-66.p5", "2026-11-20T09:40:00.000Z"),
            bash("b1", "cd /w && ~/agent-base/bin/coord msg lead 'the build is red; looking' 2>&1 | head -5", "2026-11-20T09:40:01.000Z"),
            bash("b2", "coord ask \"the cut\" <<'EOF'\nWhich train? cargo test && rm -rf x\nEOF", "2026-11-20T09:40:02.000Z"),
            bash("b5", "cd ~/agent-coord/research && coord post N-1446 \"$(cat <<'EOF'\n**Taken** (p491): \"right\", and it's one; cargo test && make)\nEOF\n)\"", "2026-11-20T09:40:02.500Z"),
            call("b3", "mcp__coord__reply", "2026-11-20T09:40:03.000Z"),
            said("Asked.", "2026-11-20T09:40:04.000Z"),
            stop("2026-11-20T09:40:04.100Z"),
            prompt("N-66.p6", "2026-11-20T09:50:00.000Z"),
            bash("b4", "cd x && cargo test -p coordd; coord post N-66 green", "2026-11-20T09:50:01.000Z"),
            said("Green.", "2026-11-20T09:50:02.000Z"),
            stop("2026-11-20T09:50:02.100Z"),
        ]);
        late.read();
        let ends: Vec<u64> = late.take_outputs().iter().map(|(o, l)| o + l).collect();
        assert_eq!(late.take_worked(), vec![(ends[0], worked(&[("Bash(coord)", 3), ("mcp__coord__reply", 1)], "claude-opus-5-5")), (ends[1], worked(&[("Bash", 1)], "claude-opus-5-5"))]);
        // a Codex rollout, in its key order (`timestamp` first)
        let line = |kind: &str, payload: serde_json::Value| format!("{{\"timestamp\":\"2026-11-20T10:00:00.000Z\",\"type\":\"{kind}\",\"payload\":{payload}}}\n");
        let item = |payload: serde_json::Value| line("response_item", payload);
        let rollout = [
            line("turn_context", json!({"cwd": "/w", "model": "gpt-6.2-sol"})),
            line("event_msg", json!({"type": "task_started"})),
            item(json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "[N-1.p1 · from owner]\nfix it"}]})),
            item(json!({"type": "function_call", "name": "shell", "arguments": "{}", "call_id": "c1"})),
            item(json!({"type": "function_call_output", "call_id": "c1", "output": "ok"})),
            item(json!({"type": "custom_tool_call", "name": "apply_patch", "input": "x", "call_id": "c2"})),
            item(json!({"type": "custom_tool_call_output", "call_id": "c2", "output": "ok"})),
            item(json!({"type": "function_call", "name": "coord__reply", "arguments": "{}", "call_id": "c3"})),
            item(json!({"type": "function_call_output", "call_id": "c3", "output": "ok"})),
            item(json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Fixed."}]})),
            line("event_msg", json!({"type": "task_complete"})),
        ];
        let rollout = [&rollout[..], &[
            line("turn_context", json!({"cwd": "/w", "model": "gpt-6.2-sol"})),
            line("event_msg", json!({"type": "task_started"})),
            item(json!({"type": "function_call", "name": "shell", "arguments": json!({"command": ["bash", "-lc", "coord post N-1 'fixed; built'"]}).to_string(), "call_id": "c4"})),
            item(json!({"type": "function_call_output", "call_id": "c4", "output": "ok"})),
            item(json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Posted."}]})),
            line("event_msg", json!({"type": "task_complete"})),
        ]].concat();
        let (codex, mut c) = File::new("worked-codex");
        std::fs::write(&codex.path, rollout.concat()).unwrap();
        c.read();
        let ends: Vec<u64> = c.take_outputs().iter().map(|(o, l)| o + l).collect();
        assert_eq!(c.take_worked(), vec![(ends[0], worked(&[("apply_patch", 1), ("coord__reply", 1), ("shell", 1)], "gpt-6.2-sol")), (ends[1], worked(&[("shell(coord)", 1)], "gpt-6.2-sol"))]);
    }

    /// A shell command runs coord alone (insight (1), proto-reader N-1446 p511, p516) when each of its commands is a
    /// `cd`, an `export` of assignments or coord, by its name or path, after `VAR=x`, `env` and `timeout N`: quoted
    /// text, a quoted heredoc's lines, a filter a pipe feeds and a `$(cat …)` are not commands of their own. Any other
    /// program among them makes it the shell's work: after a pipe, after a lone `&`, in backticks, or in an unquoted
    /// heredoc's substitution (proto-reader p539, proto-lead p544, 2; consult-3 p541, 1).
    #[test]
    fn a_shell_call_runs_coord_alone_when_all_it_runs_is_coord() {
        for (cmd, alone) in [
            ("coord post N-1446 \"a && b; c | d\"", true),
            ("cd /w && ~/Documents/papers/soft-swarm/agent-base/bin/coord msg lead 'hi' 2>&1 | head -5", true),
            ("timeout 60 coord answer A-1 Extends --pieces '[\"x at f.rs:1\"]' 2>&1 | head -20", true),
            ("coord ask \"the cut\" <<'EOF'\nWhich train? cargo test && rm -rf x\nEOF", true),
            ("coord post N-1 \"two\nlines; cargo build\"", true),
            ("C=1 env X=2 coord show N-1 && coord post N-1 seen", true),
            ("coord post N-1 \"$(cat /tmp/m)\"", true),
            // proto-reader p516: his words passed as a heredoc through cat, quotes and a `)` in them
            ("cd ~/agent-coord/research && coord post N-1446 \"$(cat <<'EOF'\n**Taken** (p491): \"right\"; cargo test && make)\nEOF\n)\"", true),
            ("coord post N-1 \"at $(date)\"", false),
            ("coord post N-1 \"$(cat <<'EOF'\nx\nEOF\n)\" && cargo build", false),
            ("cd x && cargo test -p coordd; coord post N-1 done", false),
            ("cargo test -p coordd && coord post N-1 done", false),
            ("coord post N-1 a\ncargo build", false),
            ("coord ask s <<'EOF'\nbody\nEOF\ncargo build", false),
            ("ls | coord post N-1 -", false),
            ("cd /w", false),
            ("echo coord", false),
            ("coordx post", false),
            ("coord post N-1 <<< \"x\" ; make", false),
            // after a pipe, only a filter that reads (proto-lead p544, 2)
            ("coord show N-1.p9 2>&1 | grep -n x | sort | uniq | cut -c1-80 | tail -3 | wc -l", true),
            ("coord show N-1 | jq .text |& head", true),
            ("coord ls | xargs cargo test", false),
            ("coord show N-1 | sh", false),
            ("coord show N-1 | sed -i s/a/b/ f", false),
            ("coord show N-1 | awk '{ system(\"make\") }'", false),
            // a lone `&` ends a command; `&>`, `>&2` and `2>&1` are redirections
            ("coord post N x & cargo build", false),
            ("coord post N-1 x &", true),
            ("coord show N-1 &>/dev/null && coord post N-1 seen >&2", true),
            // backticks are a substitution, as `$(…)` is
            ("coord post N \"tree `git rev-parse HEAD`\"", false),
            ("coord post N-1 \"`cat /tmp/m`\"", true),
            // an unquoted heredoc runs its substitutions; a quoted one is plain input
            ("coord post N-1 \"$(cat <<EOF\ntree $(git rev-parse HEAD)\nEOF\n)\"", false),
            ("coord ask s <<EOF\nat `date`\nEOF", false),
            ("coord ask s <<EOF\nplain words in $HOME, \\`not run\\`\nEOF", true),
            ("coord ask s <<\"EOF\"\nat $(date) and `date`\nEOF", true),
            // an export of assignments is a prefix (consult-3 p541, 1); one that runs a program is not
            ("export COORD=/home/t/agent-coord/research PATH=$PATH:…/agent-base/bin; coord show N-568.p9 2>&1 | head", true),
            ("export X=$(cargo build); coord post N-1 x", false),
        ] {
            assert_eq!(super::runs_coord_alone(cmd), alone, "{cmd:?}");
        }
    }
}
