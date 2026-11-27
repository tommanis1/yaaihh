//! The Threads page: the list of threads and a thread as its messages, with his answers to an agent's asks in place.

use crate::{App, Pending, Proj};
use coord_types::{Request, TicketView};
use docket_machine::asks::Mark;
use docket_machine::line::AsksClosed;
use docket_machine::{Post, ViewStatus};
use rune::{El, GrowthSpec, Link, LinkPaint, PillStyle, ScrollSpec, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::Value;
use std::collections::HashSet;
use std::collections::HashMap;
use rune::Icon;
use crate::ticket::{detail, side_panel};
use crate::kinds::{KindForm, open_form};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_post_is_cut_into_its_asks_the_lines_with_options_and_the_rest() {
        let t = "Intro line\n1. Genre? arcade or puzzle\n2. Size?\n\n- Orbit dodge: a game\nBye";
        let a1 = t.find("1. Genre").unwrap();
        let a2 = t.find("2. Size?").unwrap();
        let ar = t.find("arcade").unwrap();
        let orbit = t.find("Orbit dodge").unwrap();
        let marks = vec![
            Mark { at: [a1, a1 + "1. Genre? arcade or puzzle".len()], options: vec![[ar, ar + 6]] },
            Mark { at: [a2, a2 + 8], options: vec![[orbit, orbit + 11]] },
        ];
        let bs = blocks(t, &marks);
        let kinds: Vec<String> = bs.iter().map(|b| format!("{:?}:{}", b.ask, &t[b.from..b.to].replace('\n', "/"))).collect();
        assert_eq!(kinds, vec!["None:Intro line", "Some(0):1. Genre? arcade or puzzle", "Some(1):2. Size?", "None:", "None:- Orbit dodge: a game", "None:Bye"]);
        assert!(bs[4].pills, "a line with an option outside its ask is drawn with its chips");
        // two asks on one line: a box each, the words before the first in the first's
        let t2 = "Found it. Which one first? And the stills?\nBye";
        let (q1, q2) = (t2.find("Which").unwrap(), t2.find("And").unwrap());
        let two = vec![Mark { at: [q1, q1 + 16], options: vec![] }, Mark { at: [q2, q2 + 15], options: vec![] }];
        let kinds2: Vec<String> = blocks(t2, &two).iter().map(|b| format!("{:?}:{}", b.ask, &t2[b.from..b.to])).collect();
        assert_eq!(kinds2, vec!["Some(0):Found it. Which one first? ", "Some(1):And the stills?", "None:Bye"]);
        assert!(!bs[5].pills);
        // the option outside its ask carries the ask's number; the text is otherwise unchanged
        let (shown, chips) = chipped(t, &marks, bs[4].from, bs[4].to, None);
        assert_eq!(shown, "- Orbit dodge²: a game");
        assert_eq!(&shown[chips[0].0.clone()], "Orbit dodge²");
        assert_eq!((chips[0].1, chips[0].2.as_str()), (1, "Orbit dodge"));
        let (shown, chips) = chipped(t, &marks, bs[1].from, bs[1].to, Some(0));
        assert_eq!(shown, "1. Genre? arcade or puzzle");
        assert_eq!(&shown[chips[0].0.clone()], "arcade");
    }

    #[test]
    fn a_line_with_chips_draws_its_bold_code_and_bullet() {
        let t = "- **Restore it:** I put `241df30` back.";
        let s = t.find("Restore it").unwrap();
        let marks = vec![Mark { at: [0, 0], options: vec![[s, s + 10]] }];
        let (shown, chips) = chipped(t, &marks, 0, t.len(), None);
        assert_eq!(shown, "- **Restore it¹:** I put `241df30` back.");
        let (out, runs, ranges) = inline_md(&shown, &chips);
        assert_eq!(out, "• Restore it¹: I put 241df30 back.");
        assert_eq!(&out[ranges[0].clone()], "Restore it¹");
        assert_eq!(runs.iter().map(|(a, b, bold)| (&out[*a..*b], *bold)).collect::<Vec<_>>(), vec![("Restore it¹:", true), ("241df30", false)]);
    }

    #[test]
    fn the_answers_post_quotes_each_answered_ask_and_reads_back() {
        let asks = vec!["Does it suit you (puzzle, clicker)?".to_string(), "Mobile\ntoo?".to_string(), "Where?".to_string()];
        let typed = vec!["puzzle".to_string(), "mobile too, keyboard first".to_string(), "  ".to_string()];
        let text = answers_text(&asks, &typed);
        assert_eq!(text, "> Does it suit you (puzzle, clicker)?\n\npuzzle\n\n> Mobile\n> too?\n\nmobile too, keyboard first");
        let back = sections(&text).1;
        assert_eq!(back, vec![("Does it suit you (puzzle, clicker)?".to_string(), "puzzle".to_string()), ("Mobile\ntoo?".to_string(), "mobile too, keyboard first".to_string())]);
    }

    /// An answer stands apart from its quote after a blank line, and both shapes read back the same.
    #[test]
    fn an_answer_stands_apart_from_its_quoted_question() {
        let asks = vec!["Right?".to_string()];
        let his = "so yeah make sure eveyrting is moving / staffed okay for the part we dont need t otalk about";
        let (text, n) = answers_from_box(&asks, his);
        assert_eq!((text.as_str(), n), (format!("> Right?\n\n{his}").as_str(), 1));
        let lines: Vec<&str> = text.lines().collect();
        let last_quote = lines.iter().rposition(|l| l.starts_with('>')).unwrap();
        assert_eq!(lines.get(last_quote + 1), Some(&""), "a blank line ends the quote before his words: {text:?}");
        let old = format!("> Right?\n{his}");
        assert_eq!(sections(&old), sections(&text), "p969 as it was sent reads back the same");
        assert_eq!(typed(&asks, &old), vec![his.to_string()]);
    }

    #[test]
    fn the_box_holds_each_answer_under_its_asks_quote() {
        let asks = vec!["Which one first?".to_string(), "And the\nstills?".to_string()];
        let t = put_answer(&asks, 1, "", "both");
        assert_eq!(t, "> And the\n> stills?\n\nboth");
        // his own words before the quotes are kept; a second pick of an ask replaces its answer; quotes in the asks' order
        let t = put_answer(&asks, 0, &format!("thanks.\n\n{t}"), "orbit");
        let t = put_answer(&asks, 0, &t, "puzzle");
        assert_eq!(t, "thanks.\n\n> Which one first?\n\npuzzle\n\n> And the\n> stills?\n\nboth");
        assert_eq!(typed(&asks, &t), vec!["puzzle".to_string(), "both".to_string()]);
        assert_eq!(answers_from_box(&asks, &t), (t.clone(), 2));
        // words under no quote: with one ask they answer it, quoted; with several they go as they are
        assert_eq!(answers_from_box(&asks[..1], "puzzle"), ("> Which one first?\n\npuzzle".to_string(), 1));
        assert_eq!(answers_from_box(&asks, "puzzle, and both"), ("puzzle, and both".to_string(), 0));
        assert_eq!(answers_from_box(&asks, "  "), (String::new(), 0));
    }

    #[test]
    fn the_minders_step_is_read_from_its_program() {
        assert_eq!(step_words("r <- step lead \"Go ahead: \\\"fix it\\\", the owner said.\" -> Done;\n...").as_deref(), Some("Go ahead: \"fix it\", the owner said."));
        assert_eq!(step_words("..."), None);
    }

    #[test]
    fn a_thread_is_listed_where_it_stands() {
        let v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-12T10:00:00Z", "given": [],
            "posts": [{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "owner", "text": "hi", "names": ["lead"]},
                      {"n": 2, "at": "2026-11-12T10:01:00Z", "by": "lead", "text": "A? B?", "told": ["owner"], "asks": [{"at": [0, 2]}, {"at": [3, 5]}], "asks_open": true}],
            "waits_on": [{"ticket": "N-1", "agent": "owner", "call": "asks", "post": 2}]}))
        .unwrap();
        assert_eq!(group(&v), Group::Waiting);
        assert_eq!(asks_line(&v).as_deref(), Some("lead asks you 2 things"));
        // before the minder rested there is no waits_on, and its open asks still put it under Waiting on you
        let looking = TicketView { waits_on: vec![], ..v.clone() };
        assert_eq!(group(&looking), Group::Waiting);
        assert_eq!(foot(&looking, &looking.posts[1]), Foot::Looking);
        assert_eq!(foot(&v, &v.posts[1]), Foot::Left { undone: false });
        // an agents' thread to the owner: the same open asks, no minder's words
        let between_ = TicketView { status: serde_json::from_value(json!({"is": "done", "result": {"head": "Between"}})).unwrap(), ..v.clone() };
        assert_eq!(foot(&between_, &between_.posts[1]), Foot::Nothing);
        assert_eq!(group(&between_), Group::Waiting);
        // answered: starred, else the rest
        let mut done = looking.clone();
        done.posts[1].asks_open = false;
        assert_eq!(group(&done), Group::Else);
        let starred = TicketView { starred: true, ..done };
        assert_eq!(group(&starred), Group::Starred);
        // an ask typed to him, marking no passage: one thing, under Waiting, and no minder looks at it
        let mut typed = TicketView { waits_on: vec![], ..v.clone() };
        typed.posts[1].asks.clear();
        typed.posts[1].told.clear();
        typed.posts[1].names = vec!["owner".into()];
        typed.posts[1].kind = Some(docket_machine::PostKind::Ask { to: vec!["owner".into()] });
        assert_eq!(asks_line(&typed).as_deref(), Some("lead asks you 1 thing"));
        assert_eq!(group(&typed), Group::Waiting);
        assert_eq!(foot(&typed, &typed.posts[1]), Foot::Nothing);
    }

    #[test]
    fn a_chip_line_finds_its_paths_as_linkify_does() {
        // "See docs/design.md and calc.py, or ~/x/y.txt." with `calc.py` code (ticks gone)
        let t = "See docs/design.md and calc.py, or ~/x/y.txt.";
        let c = t.find("calc.py").unwrap();
        let got = crate::paths::ranges(t, &[c..c + 7]);
        let words: Vec<&str> = got.iter().map(|(r, _)| &t[r.clone()]).collect();
        assert_eq!(words, vec!["docs/design.md", "calc.py", "~/x/y.txt"]);
        // a bare calc.py (not code, no folder) is no link, as in linkify
        assert!(crate::paths::ranges("edit calc.py now", &[]).is_empty());
    }

    #[test]
    fn a_click_on_a_path_in_a_post_opens_it_from_the_posting_agents_folder() {
        // the agent's folder, with docs/design.md in it; its session runs in a subfolder, as an agent's may
        let dir = std::env::temp_dir().join(format!("yaaihh-links-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("docs/design.md"), "# d\n").unwrap();
        let cwd = dir.join("sub").display().to_string();
        // what each kind of line hands its link: the Markdown one (linkify) and the chip one (ranges, "file:" + path)
        let md = crate::paths::linkify("I wrote the design in `docs/design.md:12`.");
        assert!(md.contains("](file:docs/design.md:12)"), "{md}");
        let line = "the design is in docs/design.md, see it";
        let (r, path) = crate::paths::ranges(line, &[]).remove(0);
        assert_eq!(&line[r], "docs/design.md");
        // the file found from the agent's folder up, its line apart
        assert_eq!(crate::paths::without_line("file:docs/design.md:12"), "docs/design.md");
        assert_eq!(crate::paths::resolve(&path, Some(&cwd)), Some(dir.join("docs/design.md")));
        // no file there and no search folders: nothing is found
        assert_eq!(crate::paths::resolve("docs/missing.md", Some(&cwd)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // a choice under a post whose asks closed is still a click, putting its words in the box to answer again
    #[test]
    fn a_choice_under_a_closed_ask_still_answers_that_post() {
        use crate::message_box::{Target, Way};
        let (_held, mut s) = crate::view_cost::test_app();
        let cfg = crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false };
        s.projects = vec![crate::Proj::new(&cfg, Vec::new())];
        s.cur = 0;
        let words = "Genre? arcade or puzzle";
        let v: TicketView = serde_json::from_value(json!({"id": "N-5", "n": 5, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 3, "opened_at": "2026-11-12T10:00:00Z", "given": [], "waits_on": [],
            "posts": [{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "owner", "text": "hi", "names": ["lead"]},
                      {"n": 2, "at": "2026-11-12T10:01:00Z", "by": "lead", "text": words, "told": ["owner"], "asks": [{"at": [0, words.len()], "options": [[7, 13], [17, 23]]}],
                       "asks_closed": {"how": "NothingToAnswer"}},
                      {"n": 3, "at": "2026-11-12T10:02:00Z", "by": "lead", "text": "Later, do you want more?", "told": ["owner"], "asks": [{"at": [0, 24]}], "asks_open": true}]}))
        .unwrap();
        assert!(!v.posts[1].asks_open, "the choices' asks are closed");
        s.projects[0].live.tickets.insert(5, v);
        pick(&mut s, 5, 2, 0, "arcade");
        let place = crate::message_box::thread_place("N-5");
        let typed = s.composers.get(&(0, place.clone())).map(|e| e.text.clone()).unwrap_or_default();
        assert!(typed.contains("> Genre? arcade or puzzle") && typed.trim_end().ends_with("arcade"), "{typed:?}");
        // the box answers post 2, not the newer asking post 3 it would answer by default
        assert!(matches!(s.box_targets.get(&(0, place)), Some(Target::Thread { post: Some(2), way: Way::Here, .. })));
    }

    #[test]
    fn the_card_foot_follows_how_the_asks_closed() {
        let program = json!({"head": "Program", "text": "r <- step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n..."});
        let mut v: TicketView = serde_json::from_value(json!({"id": "N-5", "n": 5, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 3, "opened_at": "2026-11-12T10:00:00Z", "given": [], "waits_on": [],
            "posts": [{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "owner", "text": "hi", "names": ["lead"]},
                      {"n": 2, "at": "2026-11-12T10:01:00Z", "by": "lead", "text": "Go?", "told": ["owner"], "asks": [{"at": [0, 3]}], "asks_closed": {"how": "Told", "step": 3, "minder": 1}}],
            "shaped": [{"n": 1, "at": "2026-11-12T10:01:10Z", "answer": program, "undone": false}]}))
        .unwrap();
        // the minder's answer stands: its words, and Undo since it is the last
        assert_eq!(foot(&v, &v.posts[1]), Foot::Answered("Go ahead.".into(), true));
        // undone: the asks are open again and the thread waits on the owner
        v.shaped[0].undone = true;
        v.posts[1].asks_closed = None;
        v.posts[1].asks_open = true;
        v.waits_on = serde_json::from_value(json!([{"ticket": "N-5", "agent": "owner", "call": "asks", "post": 2}])).unwrap();
        assert_eq!(foot(&v, &v.posts[1]), Foot::Left { undone: true });
        // nothing to answer: no foot words (the box says it)
        v.waits_on.clear();
        v.posts[1].asks_open = false;
        v.posts[1].asks_closed = Some(AsksClosed::NothingToAnswer);
        assert_eq!(foot(&v, &v.posts[1]), Foot::Nothing);
    }

    // threads piece 1: an ask withdrawn, or answered in a subthread, says so under it, faint; read from a coordd's view
    #[test]
    fn an_ask_withdrawn_or_answered_in_a_subthread_says_how() {
        let view = |closed: serde_json::Value| -> TicketView {
            serde_json::from_value(json!({"id": "N-5", "n": 5, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
                "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 3, "opened_at": "2026-11-12T10:00:00Z", "given": [], "waits_on": [],
                "posts": [{"n": 1, "at": "2026-11-12T10:01:00Z", "by": "lead", "text": "Go?", "told": ["owner"], "asks": [{"at": [0, 3]}], "asks_closed": closed}]}))
            .unwrap()
        };
        let words = |v: &TicketView| v.posts[0].asks_closed.as_ref().and_then(closed_words);
        let v = view(json!({"how": "Withdrawn", "by": "lead", "why": " overtaken "}));
        assert_eq!(words(&v).as_deref(), Some("withdrawn by lead: overtaken"));
        assert_eq!(foot(&v, &v.posts[0]), Foot::Nothing);
        assert_eq!(words(&view(json!({"how": "Withdrawn", "by": "orch", "why": ""}))).as_deref(), Some("withdrawn by orch"));
        assert_eq!(words(&view(json!({"how": "Subthread", "ticket": "N-9"}))).as_deref(), Some("answered in the subthread N-9"));
        // the ways the box answers itself, or a newer one, say nothing here
        assert_eq!(words(&view(json!({"how": "Answered", "post": 3, "by": "owner"}))), None);
        assert_eq!(words(&view(json!({"how": "SomethingNewer"}))), None);
    }
}


#[cfg(test)]
mod quiet_tests {
    use super::*;
    use serde_json::json;

    // an agent that read a post and ended with Nothing is shown under that post
    #[test]
    fn read_no_reply_names_each_agent_under_the_post_it_read() {
        let v: TicketView = serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "orch", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 1, "opened_at": "2026-11-12T10:00:00Z", "given": [],
            "posts": [{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "orch", "text": "the build is green", "names": ["pipe", "cone"]},
                      {"n": 2, "at": "2026-11-12T10:01:00Z", "by": "orch", "text": "and merged", "names": ["pipe"]}],
            "quiet": [{"agent": "pipe", "upto": 1}, {"agent": "cone", "upto": 1}, {"agent": "pipe", "upto": 2}]}))
        .unwrap();
        assert_eq!(quiet_on(&v, &v.posts[0]), ["pipe", "cone"]);
        assert_eq!(quiet_on(&v, &v.posts[1]), ["pipe"]);
        let none = TicketView { quiet: vec![], ..v.clone() };
        assert!(quiet_on(&none, &none.posts[0]).is_empty());
    }
}

#[cfg(test)]
mod between_tests {
    use super::*;
    use serde_json::json;

    // design part 9b: agents' threads and old notes the owner takes no part in go under "Between agents"
    #[test]
    fn between_agents_holds_what_the_owner_takes_no_part_in_and_says_whom_it_waits_on() {
        let t = |kind: &str, status: serde_json::Value, posts: serde_json::Value, waits: serde_json::Value| -> TicketView {
            serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": kind, "opener": "pipe", "ctx": {}, "form": {}, "title": "t", "status": status,
                "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-12T10:00:00Z", "given": [], "posts": posts, "waits_on": waits})).unwrap()
        };
        let between_ = json!({"is": "done", "result": {"head": "Between"}});
        let p1 = json!([{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "pipe", "text": "x", "names": ["cone"]}]);
        let w = json!([{"ticket": "N-1", "agent": "cone", "call": "reply"}]);
        let agents = t("thread", between_.clone(), p1.clone(), w.clone());
        assert!(between(&agents));
        assert_eq!(waits_for(&agents), ["cone"]);
        assert!(between(&t("note", json!({"is": "done", "result": []}), p1.clone(), json!([]))), "an old note");
        // told him by the norms, naming only cone: between agents still; whom the norms told decides nothing
        let told = json!([{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "pipe", "text": "x", "names": ["cone"], "told": ["owner"]}]);
        assert!(between(&t("thread", between_.clone(), told, w.clone())));
        // the owner takes part, a post names him: his thread, not between agents
        let named = json!([{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "pipe", "text": "x", "names": ["cone", "owner"]}]);
        assert!(!between(&t("thread", between_.clone(), named, w.clone())));
        // the owner's own thread, closed as Kept: his
        assert!(!between(&t("thread", json!({"is": "done", "result": {"head": "Kept"}}), p1, json!([]))));
        assert_eq!(sender_line(&agents, &["pipe".into(), "cone".into()]).as_deref(), Some("pipe with cone"));
        let to_owner = t("thread", between_.clone(), json!([{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "dial", "text": "x", "names": ["owner"]}]), json!([]));
        assert_eq!(sender_line(&TicketView { opener: Some("dial".into()), ..to_owner }, &["dial".into()]).as_deref(), Some("dial with you"));
    }
}

// ---- the list ------------------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Group {
    Waiting,
    Working,
    Starred,
    Else,
}

/// "halted: 3 cards wait on you": the halts down V's tree (slow mode).
fn halted_line(p: &Proj, v: &TicketView) -> String {
    let n = crate::slow::shown(p, v).len().max(1);
    format!("halted: {n} card{} wait{} on you", if n == 1 { "" } else { "s" }, if n == 1 { "s" } else { "" })
}


/// coordd's answer OUT to his step's answer, as said under the box: "Answered step 1 of S-1099511627777." names a
/// step and a ticket he never sees, so it says only that it is answered, and what else it says.
pub(crate) fn answered_words(out: &str) -> String {
    let t = out.trim();
    let Some(rest) = t.strip_prefix("Answered step ") else { return t.to_string() };
    rest.find(['.', ';']).map_or_else(|| "Answered.".to_string(), |i| format!("Answered{}", &rest[i..]))
}

/// The owner takes part in V: he opened it, wrote on it, or a post names him; whom the norms told decides nothing.
pub(crate) fn owners(v: &TicketView) -> bool {
    v.opener.as_deref() == Some("owner") || v.posts.iter().any(|x| x.by == "owner" || x.names.iter().any(|n| n == "owner"))
}

/// Between agents (design part 9b): an old note, or an agents' thread (result Between), that the owner takes no part in.
fn between(v: &TicketView) -> bool {
    let agents = v.kind == "note" || (v.kind == "thread" && matches!(&v.status, ViewStatus::Done { result } if result["head"] == "Between"));
    agents && !owners(v)
}

/// The agents V waits on, the owner aside (a step owed, or a post told and no turn ended on it).
pub(crate) fn waits_for(v: &TicketView) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in v.waits_on.iter().filter(|w| w.agent != "owner") {
        if !out.contains(&w.agent) {
            out.push(w.agent.clone());
        }
    }
    out
}

/// Who a thread is between, for its header: `with lead` on his threads, else the sender first, `pipe with cone`.
fn sender_line(v: &TicketView, with: &[String]) -> Option<String> {
    let agents_only = v.kind == "note" || matches!(&v.status, ViewStatus::Done { result } if result["head"] == "Between");
    match v.opener.as_deref().filter(|o| agents_only && *o != "owner") {
        Some(o) => {
            let mut rest: Vec<&str> = with.iter().map(String::as_str).filter(|a| *a != o && *a != "owner").collect();
            // an agent's message to the owner: "dial with you"
            if owners(v) {
                rest.push("you");
            }
            Some(if rest.is_empty() { o.to_string() } else { format!("{o} with {}", rest.join(", ")) })
        }
        None => (!with.is_empty()).then(|| format!("with {}", with.join(", "))),
    }
}

/// The last post's time, else its opening: the order "Between agents" lists by.
fn last_at(v: &TicketView) -> String {
    v.posts.last().map(|x| x.at.to_string()).unwrap_or_else(|| v.opened_at.to_string())
}

fn group(v: &TicketView) -> Group {
    if crate::waits::owes(v) {
        Group::Waiting
    } else if !matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. } | ViewStatus::Resting) {
        Group::Working
    } else if v.starred {
        Group::Starred
    } else {
        Group::Else
    }
}

/// "lead asks you 3 things": the open asks, per asker (a typed ask is one, `asks_count`).
fn asks_line(v: &TicketView) -> Option<String> {
    let mut by: Vec<(String, usize)> = Vec::new();
    for p in v.posts.iter().filter(|p| p.asks_open) {
        match by.iter_mut().find(|(b, _)| *b == p.by) {
            Some((_, n)) => *n += asks_count(p),
            None => by.push((p.by.clone(), asks_count(p))),
        }
    }
    let said: Vec<String> = by.into_iter().map(|(b, n)| format!("{b} asks you {n} thing{}", if n == 1 { "" } else { "s" })).collect();
    (!said.is_empty()).then(|| said.join("; "))
}

/// A row's second line: what it asks the owner, the owner's step, who works on it, else its last post.
fn second_line(pg: &Page, p: &Proj, v: &TicketView) -> (String, bool) {
    // halted (slow mode): it waits on the owner's cards, not on whoever would be given what is halted
    if crate::waits::halted(v) {
        return (halted_line(p, v), true);
    }
    // an agent's question: its first words under `designer15 asks`
    if let Some((_, q)) = crate::ask::question_headline(pg, v).filter(|h| !h.1.is_empty()) {
        let owed = asks_line(v).is_some() || v.waits_on.iter().any(|w| w.agent == "owner");
        return (q, owed);
    }
    if let Some(a) = asks_line(v) {
        return (a, true);
    }
    if let Some(w) = v.waits_on.iter().find(|w| w.agent == "owner") {
        return (format!("yours: {} ({})", w.call, w.ticket), true);
    }
    let waits = waits_for(v);
    if !waits.is_empty() {
        return (format!("waits on {}", waits.join(", ")), false);
    }
    match v.posts.iter().rev().find(|p| p.visible() && p.moved.is_none()) {
        Some(p) => (format!("{}: {}", crate::refs::person(&p.by), crate::sidebar::first_line(&words(pg, p))), false),
        None => (String::new(), false),
    }
}

/// `Take back` on his post until every agent it went to answered it, from a coord that takes it; not on a first post.
pub(crate) fn take_back(s: &App, p: &Proj, v: &TicketView, post: &Post, k: &Kit) -> Option<El> {
    let pi = pi_of(s, p);
    if post.by != "owner" || post.n == 1 || post.dropped || crate::message_box::threads_taken(s, pi, "take back").is_err() || crate::whereabouts::of(p, v, post).is_some_and(|w| w.settled()) {
        return None;
    }
    let (t, n) = (v.id.clone(), post.n);
    // a small word, as Reply beside it (subthread::actions), not a button standing over each of his messages
    Some(k.interactive(row().key(format!("act-Take back-{}-{n}", v.id)).child(text("Take back").font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| {
        let place = crate::message_box::thread_place(&t);
        s.box_said.remove(&(pi, place.clone()));
        if !s.request_in(pi, Request::TakeBack { t: t.clone(), n }, Pending::TicketDone { clear: Cleared::TakeBack(pi, place.clone()) }) {
            let why = s.notice.take().unwrap_or_else(|| "not taken back".into());
            s.box_said.insert((pi, place), Err(why));
        }
    }))
}


/// A post's words, once read.
/// Words that are no words: a span read empty, or not read at all (crate::texts::words' "(not read: …)").
pub(crate) fn unread(body: &str) -> bool {
    body.trim().is_empty() || body.starts_with("(not read: ")
}

pub(crate) fn words(pg: &Page, p: &Post) -> String {
    state(pg, p).words()
}

/// A post's words as [`crate::texts::TextState`]: loading while its place is read.
pub(crate) fn state(pg: &Page, p: &Post) -> crate::texts::TextState {
    crate::texts::state(pg, &serde_json::to_value(&p.text).unwrap_or(Value::Null))
}

/// Its words read (not "…" while asked): what the marks' offsets are into.
pub(crate) fn read(pg: &Page, p: &Post) -> Option<String> {
    match serde_json::to_value(&p.text).unwrap_or(Value::Null) {
        Value::String(s) => Some(s),
        v => {
            let l: docket_machine::Loc = serde_json::from_value(v.get("loc")?.clone()).ok()?;
            match pg.texts.get(&crate::texts::key(&l)) {
                Some(Some(Ok(t))) => Some(t.clone()),
                _ => None,
            }
        }
    }
}

/// The list of threads, one column: the Threads page's (WHO None), or agent WHO's threads in With you.
pub fn list(s: &App, p: &Proj, who: Option<&str>, picked: Option<u64>, focused: bool, k: &Kit) -> El {
    // the narrow pane of two: about a quarter of what the sidebar leaves
    let mut c = col().key("list-column").w(0.0).grow(1.0).min_w(250.0).max_w(340.0).h_full().min_h(0.0).pt(12.0).gap(6.0).border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_weak);
    if focused && !crate::find::in_sidebar(s) {
        c = c.child(row().w_full().px(12.0).child(crate::find::search(s, who, k)));
    }
    c.child(listing(s, p, who, picked, focused, k))
}

/// The threads, or a search's hits in their place, each in its own viewport, under Outline | By time and + New thread.
pub(crate) fn listing(s: &App, p: &Proj, who: Option<&str>, picked: Option<u64>, focused: bool, k: &Kit) -> El {
    let pi = pi_of(s, p);
    let scrolled = |key: String| col().w_full().grow(1.0).min_h(0.0).px(12.0).pb(12.0).gap(6.0).key(key).scroll(rune::ScrollSpec { stick_bottom: false });
    let hits = match who {
        None => crate::find::results(s, p, k).map(|found| scrolled(format!("ticket-hits-{}", s.tickets.find.hits_n)).child(found)),
        // each search's hits in a viewport of their own, opened at their top (find.rs `hits_n`)
        Some(a) => focused.then(|| crate::find::agent_results(s, pi, a, k)).flatten().map(|found| {
            col().key(format!("with-you-hits-{pi}-{a}-{}", s.tickets.find.hits_n)).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().pb(8.0).child(found))
        }),
    };
    if let Some(hits) = hits {
        return hits;
    }
    let pg = &s.tickets;
    // where he was in the list, given back when the hits that take its place are cleared (find.rs)
    let key = who.map_or_else(|| "ticket-list".to_string(), |a| format!("with-you-list-{pi}-{a}"));
    let mut c = scrolled(key.clone()).on_scroll_offset(crate::find::watch(s, key));
    // his new chat's first message, on its way from the Archive's card until its thread is in (archive.rs)
    if who.is_none() {
        c = c.children(crate::archive::on_its_way(s, pi, k));
    }
    // the Archive press, on Threads only
    let archive_all = || if who.is_none() { crate::archive::foot(s, k) } else { None };
    // the outline of the open work first, the list by time behind the second tab, + New thread beside them
    let outline = crate::outline::outline(s, pi, who, picked, k);
    let tabs = outline.is_some().then(|| rune_kit::SegmentedControl::new(&["Outline", "By time"], Some(usize::from(pg.by_time))).label("Threads").fit().on_pick(|s: &mut App, i, _| s.tickets.by_time = i == 1).build(k));
    c = c.child(row().w_full().pt(6.0).gap(8.0).center_y().flex_wrap().children(tabs).child(new_thread(pi, who, k)));
    if let Some(o) = outline.filter(|_| !pg.by_time) {
        let c = c.child(col().w_full().pt(6.0).child(o)).children(archived_foot(s, p, who, picked, k)).children(archive_all());
        return if who.is_none() { threads_foot(c, false, pg.foot_menu, crate::archive::in_menu(s), k) } else { c };
    }
    // a ticket opened under a thread is reached through it, not listed again; a subthread stays in the list
    let under_thread = |v: &TicketView| v.kind != "thread" && v.under.as_deref().or(v.parent.as_deref()).is_some_and(|u| p.live.tickets.values().any(|t| t.id == u && t.kind == "thread"));
    // an agent's question is drawn on the ticket it is about, and is a row only while it waits on the owner
    let quiet_ask = |v: &TicketView| v.kind == "ask" && v.under.is_some() && !crate::waits::owes(v);
    // with WHO: its threads only, as its outline holds them (outline.rs `is_with`)
    let with = |v: &TicketView| who.is_none_or(|a| crate::outline::is_with(v, a));
    // what his Archive holds out (archive.rs): now the threads opened before its moment, there those after
    let top: Vec<&TicketView> = p.live.tickets.values().rev().filter(|v| v.parent.is_none() && !under_thread(v) && !quiet_ask(v) && listed(v) && !crate::archive::hides(s, p, v) && with(v)).collect();
    // design part 9b: Waiting on you; yours (at work, starred, the rest); Between agents, folded; Before tickets
    let header = |name: String| row().pt(10.0).pb(2.0).child(text(name).font(k.ty.small_medium).color(k.c.text_muted));
    // a stop passed to him anywhere in its tree waits on him too, as coordd's `owed` says (its row says so, its answers
    // are under it)
    let waiting: Vec<usize> = (0..top.len()).filter(|i| group(top[*i]) == Group::Waiting).collect();
    if !waiting.is_empty() {
        c = c.child(header("WAITING ON YOU".into()));
        for i in &waiting {
            c = c.child(item(s, p, top[*i], who, picked, k));
            // a stop passed to him, answered in its thread's one box aimed at the stop
            for (_, h) in owners_stops(p, top[*i]) {
                c = c.child(col().w_full().pl(10.0).child(aimed_card(s, p, top[*i], h, Some(who), k)));
            }
        }
    }
    // the tickets the program opened itself (builds), in a folded section of their own under his threads, by kind
    let made: Vec<usize> = (0..top.len()).filter(|i| !waiting.contains(i) && made(top[*i])).collect();
    // threads he takes no part in go under Between agents, as in the outline; a subthread goes with its topic
    let aside = |v: &TicketView| between(v) || crate::outline::apart(crate::outline::goal(p, v), &[v]);
    let mut yours: Vec<usize> = (0..top.len()).filter(|i| !waiting.contains(i) && !made.contains(i) && !aside(top[*i])).collect();
    yours.sort_by_key(|i| match group(top[*i]) {
        Group::Working => 0,
        Group::Starred => 1,
        _ => 2,
    });
    if !yours.is_empty() {
        c = c.child(header("YOURS".into()));
        c = c.children(yours.iter().map(|i| item(s, p, top[*i], who, picked, k)));
    }
    let mut kinds: Vec<&str> = Vec::new();
    for i in &made {
        if !kinds.contains(&top[*i].kind.as_str()) {
            kinds.push(top[*i].kind.as_str());
        }
    }
    let labels = crate::picture::labels(s, p);
    for kind in kinds {
        let these: Vec<&TicketView> = made.iter().map(|i| top[*i]).filter(|v| v.kind == kind).collect();
        let open_n = these.iter().filter(|v| v.closed_at.is_none()).count();
        let red = these.iter().filter(|v| is_red(p, v)).count();
        let open = pg.made_open.contains(kind);
        let line = format!("{} {} · {open_n} open, {red} red", if open { "▾" } else { "▸" }, made_name(labels.kinds.get(kind).map(String::as_str), kind));
        let key = kind.to_string();
        c = c.child(k.interactive(row().key(format!("made-{kind}")).pt(10.0).pb(2.0).child(text(line).font(k.ty.small_medium).color(k.c.text_muted))).on_press(move |s: &mut App| {
            if !s.tickets.made_open.remove(&key) {
                s.tickets.made_open.insert(key.clone());
            }
        }));
        if open {
            c = c.children(these.into_iter().map(|v| item(s, p, v, who, picked, k)));
        }
    }
    let mut agents: Vec<&TicketView> = (0..top.len()).filter(|i| !waiting.contains(i) && aside(top[*i])).map(|i| top[i]).collect();
    if !agents.is_empty() {
        // the waiting ones first, then by last post
        agents.sort_by_key(|v| (waits_for(v).is_empty(), std::cmp::Reverse(last_at(v))));
        let w = agents.iter().filter(|v| !waits_for(v).is_empty()).count();
        let open = pg.between || picked.is_some_and(|n| agents.iter().any(|v| v.n == n));
        let line = format!("{} BETWEEN AGENTS · {} thread{}, {w} wait{} on someone", if open { "▾" } else { "▸" }, agents.len(), if agents.len() == 1 { "" } else { "s" }, if w == 1 { "s" } else { "" });
        c = c.child(k.interactive(row().key("between-agents").pt(10.0).pb(2.0).child(text(line).font(k.ty.small_medium).color(k.c.text_muted))).on_press(|s: &mut App| s.tickets.between = !s.tickets.between));
        if open {
            c = c.children(agents.into_iter().map(|v| item(s, p, v, who, picked, k)));
        }
    }
    if who.is_none() {
        c = c.children(crate::views::before_tickets(s, p, k));
    }
    c = c.children(archived_foot(s, p, who, picked, k)).children(archive_all());
    let Some(a) = who else { return threads_foot(c, top.is_empty(), pg.foot_menu, crate::archive::in_menu(s), k) };
    if top.is_empty() {
        c = c.child(text(crate::look::say(s, "nothing_with", &[("agent", a)])).font(k.ty.small).color(k.c.text_muted));
    }
    c
}

/// The list's last lines after C: EMPTY's words, ⋯ (OPEN) to open a thread of a kind, and Archive (ARCHIVE).
fn threads_foot(mut c: El, empty: bool, open: bool, archive: bool, k: &Kit) -> El {
    if empty {
        c = c.child(text("Nothing yet: write to the project above.").font(k.ty.body).color(k.c.text_faint));
    }
    // the trigger here; its items in the window's layer (`foot_layer`), anchored to it
    c.child(row().pt(16.0).child(foot_menu(open, archive, k).build(k, &mut Vec::new())))
}

/// The Threads list's "⋯" (OPEN): "Open a thread of a kind…", and "Archive everything up to now…" when ARCHIVE.
fn foot_menu(open: bool, archive: bool, k: &Kit) -> rune_kit::Menu {
    let trigger = Button::new("⋯").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.tickets.foot_menu = !s.tickets.foot_menu).build(k).key("threads-foot-more");
    let menu = rune_kit::Menu::new("threads-foot-menu").trigger(trigger).open(open).placement(rune_kit::Placement::BottomStart).on_close(|s: &mut App| s.tickets.foot_menu = false);
    let menu = if archive {
        menu.item(rune_kit::MenuItem::new("Archive everything up to now…").on_select(|s: &mut App| {
            s.tickets.foot_menu = false;
            crate::archive::open_card(s);
        }))
    } else {
        menu
    };
    menu.item(rune_kit::MenuItem::new("Open a thread of a kind…").on_select(|s: &mut App| {
        s.tickets.foot_menu = false;
        // a change if the project has one, else the first kind a person opens (crate::kinds::owners_kinds)
        let first = s.tickets.kinds.as_ref().and_then(|k| k.as_ref().ok()).and_then(|ks| {
            let mine = crate::kinds::owners_kinds(ks);
            mine.iter().find(|f| f.kind == "change").or(mine.first()).map(|f| f.kind.clone())
        });
        s.tickets.opening = Some(first.unwrap_or_else(|| "change".into()));
        s.tickets.open_folds.clear();
        s.tickets.said = None;
    }))
}

/// The Threads list's "⋯" open: its items in LAYER, over the page (the trigger is drawn in the list).
pub(crate) fn foot_layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    if s.tickets.foot_menu && matches!(s.sel, crate::Sel::Tickets) {
        foot_menu(true, crate::archive::in_menu(s), k).build(k, layer);
    }
}

/// The threads he archived, folded under one entry at the list's foot, each with Unarchive; with WHO, those with it.
fn archived_foot(s: &App, p: &Proj, who: Option<&str>, picked: Option<u64>, k: &Kit) -> Option<El> {
    let mut them: Vec<&TicketView> = p.live.tickets.values().rev().filter(|v| v.archived && v.parent.is_none() && who.is_none_or(|a| crate::outline::is_with(v, a))).collect();
    if them.is_empty() {
        return None;
    }
    them.sort_by_key(|v| std::cmp::Reverse(last_at(v)));
    let open = s.tickets.archived_open;
    let line = format!("{} ARCHIVED · {}", if open { "▾" } else { "▸" }, them.len());
    let head = k.interactive(row().key("archived").pt(10.0).pb(2.0).child(text(line).font(k.ty.small_medium).color(k.c.text_muted))).on_press(|s: &mut App| s.tickets.archived_open = !s.tickets.archived_open);
    let rows = open.then(|| them.into_iter().map(|v| col().w_full().gap(2.0).child(item(s, p, v, who, picked, k)).child(archive_button(p, v, k).map(|b| row().pl(10.0).child(b)).unwrap_or_else(row))));
    Some(col().w_full().child(head).children(rows.into_iter().flatten()))
}

/// "Archive" or, on an archived thread, "Unarchive": the owner's request, answered quietly. Nothing is ended. Archive
/// only where P's coordd takes it live (`ARCHIVE_PROTOCOL`, Archive B); an older one reads an archive only in a record
/// it replays, and takes his Unarchive of such a thread (Archive A).
pub(crate) fn archive_button(p: &Proj, v: &TicketView, k: &Kit) -> Option<El> {
    let (id, on) = (v.id.clone(), !v.archived);
    if on && p.protocol < coord_types::ARCHIVE_PROTOCOL {
        return None;
    }
    Some(Button::new(if on { "Archive" } else { "Unarchive" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        s.request_in(s.cur, Request::Archive { t: id.clone(), on }, Pending::TicketDone { clear: Cleared::Quiet });
    }).build(k))
}

/// A section's name for the tickets of a kind the program opens: its doc's first line made plural ("a build of the
/// cases": "BUILDS OF THE CASES"), else its name.
fn made_name(doc: Option<&str>, kind: &str) -> String {
    let d = doc.unwrap_or(kind).trim();
    let d = ["a ", "an ", "the "].iter().find_map(|a| d.strip_prefix(a)).unwrap_or(d);
    let (first, rest) = d.split_once(' ').unwrap_or((d, ""));
    let first = if first.ends_with('s') { first.to_string() } else { format!("{first}s") };
    format!("{first} {rest}").trim().to_uppercase()
}

/// A build red: its newest answer says red, or a ticket down it stopped or closed failed.
fn is_red(p: &Proj, v: &TicketView) -> bool {
    let last = v.posts.iter().rev().find_map(|q| q.head.as_ref().and_then(|h| h.get("head")).and_then(Value::as_str));
    last.is_some_and(|h| h.eq_ignore_ascii_case("red")) || (v.closed_at.is_none() && trouble(p, v).is_some())
}

/// How ticket T ended, under its stages on the thread: on the line, with each commit by its subject (coordd's label
/// of `REPO:SHA`) and its repository and hash, and who made it; or that it failed and with what.
fn ended_line(p: &Proj, t: &TicketView, k: &Kit) -> Option<El> {
    match &t.status {
        ViewStatus::Done { result } => {
            let commits = result.get("commit").and_then(Value::as_object)?;
            let on_line = result.get("top").is_some();
            let mut r = row().w_full().gap(6.0).flex_wrap().center_y().child(text(if on_line { "On the line:" } else { "Made:" }.to_string()).font(k.ty.small_medium).color(k.c.success.fg));
            for (repo, sha) in commits.iter().filter_map(|(r, x)| Some((r, x.as_str()?))) {
                let label = p.labels.get(&format!("{repo}:{sha}")).unwrap_or("a commit").to_string();
                r = r.child(crate::refs::el(&crate::refs::Ref { label, id: format!("{repo} {sha}") }, k.ty.small, k.c.text_base, k));
            }
            if let Some(by) = result.get("by").and_then(Value::as_str) {
                r = r.child(text(format!("by {by}")).font(k.ty.small).color(k.c.text_muted));
            }
            Some(r)
        }
        ViewStatus::Failed { raised } => Some(text(format!("Failed: {}", raised.exc)).font(k.ty.small_medium).color(k.c.danger.fg)),
        _ => None,
    }
}

fn star(k: &Kit, t: &TicketView) -> El {
    let (id, on) = (t.id.clone(), t.starred);
    k.interactive(row().px(4.0).child(text(if on { "★" } else { "☆" }).font(k.ty.body).color(if on { k.c.warning.fg } else { k.c.text_faint })))
        .key(format!("star-{}", t.id))
        .on_press(move |s: &mut App| {
            s.request_in(s.cur, Request::Star { t: id.clone(), on: !on }, Pending::TicketDone { clear: Cleared::Quiet });
        })
}

/// A row's title: its title's first line, else its form's words' first line, else coordd's label, else id and kind.
fn row_title(pg: &Page, p: &Proj, v: &TicketView) -> String {
    // an agent's question: who asks, and the question's first line
    if let Some(h) = crate::ask::question_headline(pg, v) {
        return h.0;
    }
    if !v.title.is_empty() {
        return crate::sidebar::first_line(&v.title);
    }
    let labelled = p.labels.get(&v.id).map(str::to_string);
    let asked = crate::ask::asked_words(v).map(|t| crate::texts::words(pg, t));
    match asked {
        Some(w) if !w.trim().is_empty() => crate::sidebar::first_line(w.trim()),
        _ => labelled.unwrap_or_else(|| crate::refs::unlabelled(v)),
    }
}

/// The notices coordd raised on thread V (a model call on it failed), its own and its handlers'.
fn notices<'a>(p: &'a Proj, v: &'a TicketView) -> Vec<&'a coord_types::Notice> {
    p.live.notices.iter().filter(|x| x.ticket.as_deref().is_some_and(|t| t == v.id || v.handlers.iter().any(|h| h.id == t))).collect()
}

/// Thread V's notices as one muted red line each, at its foot above the composer.
fn notice_lines(p: &Proj, v: &TicketView, k: &Kit) -> Vec<El> {
    notices(p, v).into_iter().map(|x| text(format!("{} · {}", x.text, crate::when::at(&x.at))).font(k.ty.small).color(k.c.danger.fg.alpha(0.75)).wrap()).collect()
}

/// Thread V's row in the list by time, in the list of the Threads page (WHO None) or of agent WHO's With you: PICKED
/// drawn as picked, a press picks it there ([`set_picked`]).
fn item(s: &App, p: &Proj, v: &TicketView, who: Option<&str>, picked: Option<u64>, k: &Kit) -> El {
    let pg = &s.tickets;
    let (n, pi, by) = (v.n, pi_of(s, p), who.map(str::to_string));
    let selected = picked == Some(n) && (who.is_some() || pg.opening.is_none());
    let (second, owed) = second_line(pg, p, v);
    let title = row_title(pg, p, v);
    let body = col()
        .grow(1.0)
        .w(0.0)
        .gap(2.0)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base).truncate())
        .children((!second.is_empty()).then(|| text(second).font(k.ty.small).color(if owed { k.c.warning.fg } else { k.c.text_muted }).truncate()));
    let r = row()
        .key(format!("ticket-{}", v.id))
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(8.0)
        .radius(k.radius.sm)
        .fill(k.c.layer_01)
        .border(1.0, if selected { k.c.border_focus } else { k.c.layer_01 })
        .child(k.interactive(body).on_press(move |s: &mut App| set_picked(s, pi, by.as_deref(), Some(n))))
        .child(star(k, v));
    // a small mark while coordd has a notice on it
    if notices(p, v).is_empty() { r } else { r.child(rune::el().key(format!("notice-mark-{}", v.id)).w(6.0).h(6.0).radius(3.0).fill(k.c.danger.fg)) }
}

// ---- a thread ------------------------------------------------------------------------------------------------------

/// Thread V: its head, its messages, the owner's steps, the composer at its foot.
/// Thread V's header: its title, who is on it and its state, what is behind "⋯", what it continues or is continued in.
fn head(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    let pg = &s.tickets;
    let n = v.n;
    let mut c = col().w_full().gap(10.0);
    // its heading is its label: coordd's, else its words' first line, cut
    let title = head_title(p, v);
    // the header shows the title, who is on it and what it asks; the rest is behind ⋯
    let more = pg.more.contains(&n);
    let how = pg.how.contains(&n);
    let dots = Button::new(if more { "×" } else { "⋯" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        if !s.tickets.more.remove(&n) {
            s.tickets.more.insert(n);
        }
    });
    // a subthread says what it is part of, one dim line above its title
    c = c.children(crate::outline::part_of(p, v, k));
    // its program on the Docket at the Thread scale: one press there, one back
    let docket = (!v.program.trim().is_empty()).then(|| Button::new("Docket").variant(ButtonVariant::Ghost).help("Its program drawn, lit where it stands").on_press(move |s: &mut App| crate::picture::docket::open_thread(s, n)).build(k).key("thread-docket"));
    // what ran and the record as written, one press each, from a coordd that gives its record
    let depths = (p.protocol >= coord_types::RECORD_PROTOCOL).then(|| {
        [("Events", crate::lens::Depth::Events, "What ran on it, from the post found"), ("Record", crate::lens::Depth::Record, "The record's lines as written, from the post found")].map(|(name, depth, help)| {
            let id = v.id.clone();
            Button::new(name).variant(ButtonVariant::Ghost).help(help).on_press(move |s: &mut App| {
                let node = crate::lens::node_of(s, n, &id);
                crate::lens::zoom(s, depth, &node);
            }).build(k).key(format!("thread-{}", name.to_lowercase()))
        })
    });
    // its id small right after the title
    let named = row().grow(1.0).shrink().min_w(0.0).gap(8.0).center_y().child(retitle(s, p, v, title, k)).child(text(v.id.clone()).font(k.ty.small).color(k.c.text_faint).key(format!("title-id-{n}")));
    // Docket, Events and Record are engine detail behind ⋯; the title, its id and ⋯ stay on the header
    c = c.child(row().w_full().gap(10.0).center_y().child(named).child(dots.build(k).key("thread-more")));
    // where other threads mention it, one dim line each (coordd's `mentioned`, docket-core5)
    c = c.children(crate::mentions::back(&v.mentioned, k));
    let waits = waits_for(v);
    let agents_only = v.kind == "note" || matches!(&v.status, ViewStatus::Done { result } if result["head"] == "Between");
    // its state, and whether it asks something of someone (then it stays on the header)
    let stops = owners_stops(p, v).len();
    let (state, asks) = match group(v) {
        _ if crate::waits::halted(v) => (halted_line(p, v), true),
        _ if stops > 0 => (if stops == 1 { "stopped: waiting on you".to_string() } else { format!("{stops} stops wait on you") }, true),
        Group::Waiting => ("waiting on you".to_string(), true),
        _ if !waits.is_empty() => (format!("waits on {}", waits.join(", ")), true),
        Group::Working => ("agents at work".into(), true),
        // an agents' thread is never closed by anyone: with nobody waited on it is quiet (part 9b)
        _ if agents_only => ("quiet".into(), false),
        _ if matches!(v.status, ViewStatus::Resting) => ("resting".into(), false),
        _ => ("closed".into(), false),
    };
    let with = crate::outline::agents_on(v);
    let mut sub: Vec<String> = sender_line(v, &with).into_iter().collect();
    if asks {
        sub.push(state.clone());
    }
    // under the title only what needs him, in the accent
    let owes = asks && state.contains("you");
    let quiet_words = sub.join(" · ");
    let sub: Vec<String> = if owes { vec![state.clone()] } else { Vec::new() };
    // slow mode is said by the Step | Run in the dock over its box, not here
    let mut line = row().w_full().gap(10.0).center_y();
    if !sub.is_empty() {
        // the line wraps rather than run under the switch when the thread's column is narrow
        line = line.child(text(sub.join(" · ")).font(k.ty.small_medium).color(k.c.text_accent).wrap().grow(1.0).shrink());
    } else {
        line = line.child(rune::el().grow(1.0));
    }
    if how && !more {
        line = line.child(Button::new("‹ the conversation").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            s.tickets.how.remove(&n);
        }).build(k));
    }
    if !sub.is_empty() || (how && !more) {
        c = c.child(line);
    }
    if more {
        let mut m = row().w_full().gap(10.0).center_y().flex_wrap().px(10.0).py(6.0).radius(k.radius.sm).fill(k.c.layer_01);
        let quiet = if asks { format!("{} · {quiet_words}", v.id) } else if quiet_words.is_empty() { format!("{} · {state}", v.id) } else { format!("{} · {quiet_words} · {state}", v.id) };
        m = m.child(text(quiet).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink());
        m = m.child(crate::find::star(v, None, k));
        if let Some(pin) = s.thread_pin(v) {
            let on = s.pins.has(&pin);
            m = m.child(Button::new(if on { "Unpin" } else { "Pin" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| s.pins.toggle(pin.clone())).build(k));
        }
        m = m.children(archive_button(p, v, k)).children(crate::ending::button(s, v, k)).children(crate::ending::reopen(v, k));
        m = m.child(Button::new("details").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            s.tickets.details.insert(n);
        }).build(k));
        // how it runs takes the conversation's place: its rows and program text, ‹ the conversation back; the composer stays
        m = m.child(Button::new(if how { "‹ the conversation" } else { "how it runs" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            if !s.tickets.how.remove(&n) {
                s.tickets.how.insert(n);
            }
        }).build(k));
        c = c.child(m);
        if docket.is_some() || depths.is_some() {
            c = c.child(row().w_full().gap(10.0).center_y().key("thread-depths").children(docket).children(depths.into_iter().flatten()));
        }
    }
    // how it ended, and the end box while it is open (ending.rs)
    c = c.children(crate::ending::line(v, k)).children(crate::ending::panel(s, v, k));
    // a closed thread's follower, and the one it follows
    if let Some(f) = v.form.get("follows").and_then(Value::as_str) {
        c = c.child(goto(s, p, f, &format!("continues {f} ›"), k));
    }
    if let Some(next) = p.live.tickets.values().find(|t| t.form.get("follows").and_then(Value::as_str) == Some(v.id.as_str())) {
        c = c.child(goto(s, p, &next.id, &format!("continued in {} ›", next.id), k));
    }
    c = c.children(crate::ticket::broken_line(v, k));
    c
}

/// A ticket thread V opened, by its reference (refs.rs): its label, its id small; a subthread one line where it split
/// off (outline.rs); a question the step asked, with the box he answers in; what a step took the owner's words as.
fn opened_line(s: &App, p: &Proj, v: &TicketView, t: &TicketView, k: &Kit) -> El {
    let pg = &s.tickets;
    let (pi, to, at) = (pi_of(s, p), t.n, t.id.clone());
    let r = crate::refs::ticket(Some(p), t);
    // under his words: what a step opened from them, a question it asked, a subthread, or a mark's one line
    if t.drawn.is_some_and(docket_machine::syntax::Drawn::is_mark) {
        return crate::outline::mark_line(s, p, v, t, k);
    }
    if t.kind == "thread" {
        return crate::outline::subthread_line(p, t, k);
    }
    if v.kind == "thread" && t.kind == docket_machine::machine::ASK {
        return crate::ask::ask_block_in(pg, p, t, Some(&|h: &TicketView| aim(s, p, v, h, false, k)), k);
    }
    if v.kind == "thread" {
        let labels = crate::picture::labels(s, p);
        let as_what = labels.kinds.get(&t.kind).map_or_else(|| "Taken as".to_string(), |d| format!("Taken as {d}:"));
        // the words are the message above: the line names it by its id alone
        let as_what = as_what.trim_end_matches(':').to_string();
        let head = k.interactive(row().w_full().gap(6.0).flex_wrap().center_y().child(text(as_what).font(k.ty.small).color(k.c.text_muted)).child(text("·".to_string()).font(k.ty.small).color(k.c.text_faint)).child(text(format!("{} ›", t.id)).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| crate::outline::open_from_thread(s, pi, to, &at));
        // a build of its stopped or failed, and a stop passed to him, as on its page
        let mut b = col().w_full().gap(6.0).pl(14.0).border_sides([0.0, 0.0, 0.0, 2.0], k.c.border_weak).child(head).child(crate::picture::stages_of(s, p, t, k)).children(blocked_line(p, t, k)).children(ended_line(p, t, k));
        for (_, h) in owners_stops(p, t) {
            b = b.child(aimed_card(s, p, v, h, None, k));
        }
        return b;
    }
    k.interactive(row().gap(6.0).center_y().child(text("opened".to_string()).font(k.ty.small).color(k.c.text_muted)).child(crate::refs::el(&r, k.ty.small, k.c.text_accent, k)).child(text("›".to_string()).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| crate::outline::open_from_thread(s, pi, to, &at))
}

/// Step H that V owes him, drawn where it stands: its card with no text box, and [`aim`] under it.
fn aimed_card(s: &App, p: &Proj, v: &TicketView, h: &TicketView, from_list: Option<Option<&str>>, k: &Kit) -> El {
    let open = from_list.is_some();
    use crate::message_box::{Target, Way};
    let card = crate::ask::answer_card(&s.tickets, p, h, true, k);
    let aimed = !open && matches!(&foot_target(s, p, v).0, Target::Thread { way: Way::Step { s: st }, .. } if *st == h.id);
    let card = if aimed {
        card
    } else {
        // a press on the step answers it in the one box; its own fields and buttons keep theirs
        let to = Target::Thread { project: pi_of(s, p), t: v.id.clone(), post: None, way: Way::Step { s: h.id.clone() } };
        let (n, pi, by) = (v.n, pi_of(s, p), from_list.map(|w| w.map(str::to_string)));
        k.interactive(card).help("Answer it in the box").on_press(move |s: &mut App| {
            // from a list: the thread picked there first (`set_picked`)
            if let Some(who) = &by {
                set_picked(s, pi, who.as_deref(), Some(n));
            }
            answer_step(s, to.clone());
        })
    };
    col().w_full().gap(6.0).child(card).child(aim(s, p, v, h, open, k))
}

/// The thread's one box aimed at step TO and given the keyboard, a question's box closed: he answers the step there.
fn answer_step(s: &mut App, to: crate::message_box::Target) {
    crate::message_box::choose(s, to.clone());
    give_keys(s, &to);
}

/// The thread's one box aimed at post N of thread T, with the keyboard, as for a step: he answers a typed ask there.
fn answer_post(s: &mut App, pi: usize, t: &str, n: u32) {
    let to = aim_at(s, pi, t, Some(n));
    give_keys(s, &to);
}

/// The keyboard to the box TO is aimed at, a question's box closed.
fn give_keys(s: &mut App, to: &crate::message_box::Target) {
    s.answering = None;
    s.composers.entry((to.project(), to.place())).or_insert_with(|| TextEdit::new("")).focused = true;
}

/// Under step H of thread V: "Answer it in the box", which aims V's box at H (OPEN: and opens V, from the list), or,
/// while the box is aimed at it, a line that says so.
fn aim(s: &App, p: &Proj, v: &TicketView, h: &TicketView, open: bool, k: &Kit) -> El {
    use crate::message_box::{Target, Way};
    let target = foot_target(s, p, v).0;
    if !open && matches!(&target, Target::Thread { way: Way::Step { s: st }, .. } if *st == h.id) {
        return row().w_full().justify_end().child(text("answered in the box below").font(k.ty.small).color(k.c.text_muted));
    }
    let to = Target::Thread { project: pi_of(s, p), t: v.id.clone(), post: None, way: Way::Step { s: h.id.clone() } };
    let n = v.n;
    row().w_full().justify_end().child(Button::new("Answer it in the box").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        if open {
            s.tickets.shown = Some(n);
        }
        answer_step(s, to.clone());
    }).build(k))
}

/// The steps of thread V its box can be aimed at: those it owes him at its foot ([`owed_steps`]), a stop passed to him
/// in its tree, and the step a question a step asked waits on (where they stand, [`aim`]).
fn aimable_steps<'a>(p: &'a Proj, v: &'a TicketView) -> Vec<&'a TicketView> {
    let mut out = owed_steps(p, v);
    let owner = |t: &TicketView| matches!(&t.status, ViewStatus::Awaits { agent, .. } if agent == "owner");
    let stops = owners_stops(p, v).into_iter().map(|(_, h)| h);
    let asked = v.children.iter().filter_map(|id| p.live.tickets.values().find(|t| &t.id == id && t.kind == docket_machine::machine::ASK)).flat_map(|a| {
        a.children.iter().filter_map(|id| p.live.tickets.values().find(|t| &t.id == id && t.kind == "question")).chain(std::iter::once(a)).filter(|t| owner(t)).take(1)
    });
    for h in stops.chain(asked) {
        if !out.iter().any(|o| o.id == h.id) {
            out.push(h);
        }
    }
    out
}

/// The foot of thread V: the steps the owner owes down its tree (pick, clarify, a decision), as the Tickets page
/// answers them, its notices, and the box he writes on it in.
fn thread_foot(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    use crate::message_box::{Target, Way};
    let pg = &s.tickets;
    let pi = pi_of(s, p);
    let target = foot_target(s, p, v).0;
    let mut c = col().w_full().gap(10.0);
    for t in owed_steps(p, v) {
        // its reference, label then id (refs.rs); the thread's own step needs none
        let head = if t.id == v.id { row() } else { crate::refs::el(&crate::refs::ticket(Some(p), t), k.ty.small_medium, k.c.warning.fg, k) };
        // the box below answers the step it is aimed at; another is aimed at with one press
        let aimed = matches!(&target, Target::Thread { way: Way::Step { s: st }, .. } if *st == t.id);
        let mut card = col().key(format!("step-{}", t.id)).w_full().gap(6.0).child(head).child(crate::ask::answer_card(pg, p, t, true, k));
        if !aimed {
            // a press on the step answers it in the one box; its own fields and buttons keep theirs
            let to = Target::Thread { project: pi, t: v.id.clone(), post: None, way: Way::Step { s: t.id.clone() } };
            let at = to.clone();
            card = card.child(row().w_full().justify_end().child(Button::new("Answer it in the box").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| answer_step(s, to.clone())).build(k)));
            card = k.interactive(card).help("Answer it in the box").on_press(move |s: &mut App| answer_step(s, at.clone()));
        }
        c = c.child(card);
    }
    if v.kind == "thread" && v.posts.is_empty() {
        if let Some(goes) = crate::whereabouts::gathers(v) {
            let body = v.form["text"].as_str().unwrap_or("").trim();
            c = c.child(
                col()
                    .key(format!("held-{}", v.id))
                    .w_full()
                    .gap(6.0)
                    .padding(10.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_weak)
                    .fill(k.c.layer_01)
                    .child(text(format!("you · {}", crate::when::at(&v.opened_at.to_string()))).font(k.ty.small).color(k.c.text_muted).wrap())
                    .child(text(body.to_string()).font(k.ty.body).color(k.c.text_base).wrap())
                    .child(row().gap(8.0).items_center().child(text(crate::whereabouts::held_goes(goes.as_deref())).font(k.ty.small).color(k.c.text_faint)).child(crate::chat::send_now(pi, &v.id, k))),
            );
        }
    }
    // what the thread's agent is doing now, in a turn or on background work
    let working = crate::outline::holder(v).and_then(|h| p.live.agents.iter().find(|a| a.name == h)).and_then(|a| crate::streams::working_now(a, k));
    c.children(working.map(|w| row().key(format!("working-{}", v.id)).w_full().child(w))).children(crate::seen::above(s, s.cur, v, k)).children(notice_lines(p, v, k)).children(crate::split::foot(s, p, v, k)).child(foot_composer(s, p, v, k))
}

/// The steps thread V owes him down its tree (pick, clarify, a decision, a stop), oldest first; a thread's question
/// under his words is answered there (opened_line), not here.
fn owed_steps<'a>(p: &'a Proj, v: &'a TicketView) -> Vec<&'a TicketView> {
    let mut out: Vec<&TicketView> = Vec::new();
    for w in v.waits_on.iter().filter(|w| w.agent == "owner" && w.call != "asks" && w.call != "halt") {
        let owed = p.live.tickets.values().chain(v.handlers.iter()).find(|t| t.id == w.ticket && matches!(&t.status, ViewStatus::Awaits { agent, .. } if agent == "owner"));
        if let Some(t) = owed.filter(|t| !(v.kind == "thread" && t.kind == docket_machine::machine::ASK && v.children.contains(&t.id))) {
            if !out.iter().any(|o| o.id == t.id) {
                out.push(t);
            }
        }
    }
    out
}

/// The box at V's foot: its target and default (the oldest step he owes, else the oldest post asking him, else the foot).
fn foot_target(s: &App, p: &Proj, v: &TicketView) -> (crate::message_box::Target, crate::message_box::Target) {
    use crate::message_box::{Target, Way};
    let pi = pi_of(s, p);
    let steps = owed_steps(p, v);
    let default = match steps.first() {
        Some(h) => Target::Thread { project: pi, t: v.id.clone(), post: None, way: Way::Step { s: h.id.clone() } },
        None => Target::Thread { project: pi, t: v.id.clone(), post: asking(v).map(|q| q.n), way: Way::Here },
    };
    let target = crate::message_box::chosen(s, default.clone());
    let stale = matches!(&target, Target::Thread { way: Way::Step { s: st }, .. } if !aimable_steps(p, v).iter().any(|h| h.id == *st));
    (if stale { default.clone() } else { target }, default)
}

/// One row of a thread's page, in its order (`seq`): the header, a halt card (the ticket it is on, its place in that
/// ticket's halts), a ticket it opened, a post, the foot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TRow {
    Head,
    Halt(String, usize),
    Opened(u64),
    Post(u32),
    /// His post taken back after an agent had it (`TicketView.struck`).
    Struck(u32),
    Foot,
}

/// The thread's rows with what they were built from, so a frame doesn't rebuild them (61 ms on 1,880 posts).
#[derive(Default)]
pub struct Rows(std::cell::RefCell<Option<(RowsFrom, std::rc::Rc<Vec<TRow>>)>>);

type RowsFrom = (String, u64, u64, usize, usize, chrono::NaiveDate);

fn kept(rows: &Rows, p: &Proj, v: &TicketView) -> std::rc::Rc<Vec<TRow>> {
    let from: RowsFrom = (p.id.clone(), v.n, p.live.tickets_rev, p.live.tickets.len(), v.posts.len(), chrono::Local::now().date_naive());
    if let Some((at, r)) = rows.0.borrow().as_ref()
        && *at == from
    {
        return r.clone();
    }
    let r = std::rc::Rc::new(seq(p, v));
    *rows.0.borrow_mut() = Some((from, r.clone()));
    r
}

/// Thread V's rows in order: its posts, where they happen in time slow mode's halts (a post about to be delivered in
/// its own place) and the tickets it opened, under its header and above its foot.
pub(crate) fn seq(p: &Proj, v: &TicketView) -> Vec<TRow> {
    let halts = crate::slow::shown(p, v);
    let row_of = |t: &TicketView, h: &docket_machine::ViewHalt| TRow::Halt(t.id.clone(), t.halts.iter().position(|x| std::ptr::eq(x, h)).unwrap_or(0));
    let mut timed: Vec<(&TicketView, &docket_machine::ViewHalt)> = halts.iter().copied().filter(|(t, h)| !(t.id == v.id && matches!(h, docket_machine::ViewHalt::Deliver { .. }))).collect();
    timed.sort_by_key(|(_, h)| crate::slow::at_of(h));
    let mut timed = timed.into_iter().peekable();
    let mut opened: Vec<&TicketView> = v.children.iter().filter_map(|id| p.live.tickets.values().find(|t| &t.id == id)).collect();
    for t in crate::outline::subthreads(p, v).into_iter().chain(crate::outline::marks(p, v)) {
        if !opened.iter().any(|o| o.id == t.id) {
            opened.push(t);
        }
    }
    opened.sort_by_key(|t| crate::outline::place(v, t));
    let mut opened = opened.into_iter().peekable();
    let mut out = vec![TRow::Head];
    let mut struck: Vec<u32> = v.struck.iter().map(|x| x.post.n).collect();
    struck.sort();
    let mut struck = struck.into_iter().peekable();
    for post in &v.posts {
        while let Some(n) = struck.next_if(|n| *n < post.n) {
            out.push(TRow::Struck(n));
        }
        while let Some((t, h)) = timed.next_if(|(_, h)| crate::slow::at_of(h) < post.at.to_string()) {
            out.push(row_of(t, h));
        }
        while let Some(t) = opened.next_if(|t| crate::outline::place(v, t) < post.at.to_string()) {
            out.push(TRow::Opened(t.n));
        }
        // the step's answer that opened the ticket drawn just after it says nothing its line does not (`take`'s
        // "Direct agent: writer" above "Taken as a change made directly · D-2")
        if v.kind == "thread" && post.step.is_some() && post.by != "owner" && opened.peek().is_some_and(|t| t.kind != "thread" && v.posts.iter().find(|q| q.n > post.n).is_none_or(|q| t.opened_at.to_string() < q.at.to_string())) {
            continue;
        }
        // his answers stand in the post they answer and where he sent them; a card of several stands at the first
        if let Some((t, h, first)) = halts.iter().filter(|(t, _)| t.id == v.id).find_map(|(t, h)| crate::slow::on_card(h, post.n).map(|f| (t, h, f))) {
            if first {
                out.push(row_of(t, h));
            }
            continue;
        }
        out.push(TRow::Post(post.n));
    }
    out.extend(struck.map(TRow::Struck));
    out.extend(timed.map(|(t, h)| row_of(t, h)));
    out.extend(opened.map(|t| TRow::Opened(t.n)));
    out.push(TRow::Foot);
    out
}

/// Row R's key on thread V's page: a post's is the one a search brings into view (find.rs `seek`).
pub(crate) fn row_key(id: &str, r: &TRow) -> String {
    match r {
        TRow::Head => format!("thread-head-{id}"),
        TRow::Halt(t, i) => format!("halt-{t}-{i}"),
        TRow::Opened(n) => format!("opened-{n}"),
        TRow::Post(n) => format!("post-{id}-{n}"),
        TRow::Struck(n) => format!("struck-{id}-{n}"),
        TRow::Foot => format!("thread-foot-{id}"),
    }
}

/// Row R of thread V, drawn.
fn row_el(s: &App, p: &Proj, v: &TicketView, r: &TRow, k: &Kit) -> El {
    let pg = &s.tickets;
    match r {
        TRow::Head => head(s, p, v, k),
        TRow::Halt(id, i) => {
            let t = if *id == v.id { Some(v) } else { p.live.tickets.values().chain(v.handlers.iter()).chain(p.live.tickets.values().flat_map(|t| t.handlers.iter())).find(|t| t.id == *id) };
            let Some((t, h)) = t.and_then(|t| Some((t, t.halts.get(*i)?))) else { return col() };
            let first = crate::slow::shown(p, v).first().map(|(t, h)| (t.id.clone(), crate::slow::at_of(h)));
            crate::slow::card(pg, p, v, t, h, first == Some((t.id.clone(), crate::slow::at_of(h))), k)
        }
        TRow::Opened(n) => p.live.tickets.get(n).map_or_else(col, |t| opened_line(s, p, v, t, k)),
        TRow::Post(n) => {
            let Some(post) = v.posts.iter().find(|x| x.n == *n) else { return col() };
            let m = col().w_full().child(crate::post::post(s, p, v, post, &crate::post::Form::Thread, k)).children(crate::whereabouts::line_by_dock(s, p, v, post, k)).children(not_reached_line(s, p, v, post, k));
            // a reply to a post (a quote reply, `answers`): under it, a rule down its left
            match post.answers {
                // the rule down its left says it is a reply
                Some(_) => col().w_full().gap(4.0).pl(28.0).border_sides([0.0, 0.0, 0.0, 2.0], k.c.border_weak).child(m),
                None => m,
            }
        }
        TRow::Struck(n) => struck_row(s, v, *n, k),
        TRow::Foot => thread_foot(s, p, v, k),
    }
}

/// His post N of V taken back after an agent had it: struck through and faint (rune's `strike`), with when.
pub(crate) fn struck_row(s: &App, v: &TicketView, n: u32, k: &Kit) -> El {
    let Some(x) = v.struck.iter().find(|x| x.post.n == n) else { return col() };
    let words = words(&s.tickets, &x.post);
    let end = words.len();
    let head = crate::subthread::head("you", &format!("{} · taken back {}", crate::when::at(&x.post.at.to_string()), crate::when::at(&x.at.to_string())), k);
    col().w_full().gap(4.0).items_end().child(row().gap(10.0).center_y().child(head)).child(crate::post::bubble(text(words).font(k.ty.body).color(k.c.text_faint).strike(0, end, Some(k.c.text_faint)).wrap(), false, k))
}

/// Thread V's page as it is read, in the reading column: its title, the "how it runs" view or a ticket that ran a
/// program (ran.rs), all of it drawn.
pub fn thread(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    let pg = &s.tickets;
    let c = col().w_full().px(28.0).py(18.0).gap(10.0).child(head(s, p, v, k));
    if pg.how.contains(&v.n) {
        let all: Vec<&TicketView> = p.live.tickets.values().collect();
        return commentable(s, p, v, c.child(crate::runs::picture(pg, v, &all, k)).children(notice_lines(p, v, k)).children(crate::split::foot(s, p, v, k)).child(foot_composer(s, p, v, k)));
    }
    // a ticket with no program, or a thread, is its posts in order, with a line where it opened each ticket
    let c = if !v.run.is_empty() && v.kind != "thread" {
        c.child(col().w_full().gap(10.0).children(crate::ran::thread(s, p, v, k)))
    } else {
        c.children(seq(p, v).iter().filter(|r| !matches!(r, TRow::Head | TRow::Foot)).map(|r| row_el(s, p, v, r, k)))
    };
    commentable(s, p, v, c.child(thread_foot(s, p, v, k)))
}

/// Thread V's rows in a scroll keyed KEY, drawn only in view, opening where he last read; None where drawn whole.
pub fn thread_list(s: &App, p: &Proj, v: &TicketView, key: String, k: &Kit) -> Option<El> {
    if !in_rows(s, v) {
        return None;
    }
    let rows_keys: Vec<String> = kept(&s.tickets.thread_rows, p, v).iter().map(|r| row_key(&v.id, r)).collect();
    let list = rows_in_view(s, p, v, k);
    // where he reads, kept across a swap and a visit elsewhere (read_place.rs)
    let (view, scope, keys) = (crate::read_place::thread_view(&p.id, &v.id), format!("thread-rows-{}", v.id), rows_keys);
    let (wheel, last) = (view.clone(), keys.len());
    let list = list
        .on_visible_rows(move |s: &mut App, r| {
            let at_end = r.end >= last;
            s.read_places.report(&view, &scope, &keys, r, at_end, true)
        })
        // his own scroll wins over a scroll back at once
        .on_wheel(move |s: &mut App, _, _| {
            s.read_places.his_scroll(&wheel);
            false
        });
    let watch = s.read_places.offset_watch(crate::read_place::thread_view(&p.id, &v.id));
    let scroll = col().key(key).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: true }).on_scroll_offset(watch).child(list);
    Some(commentable(s, p, v, col().key(format!("reading-{}", v.id)).w(0.0).grow(3.0).min_w(0.0).h_full().min_h(0.0).child(scroll)))
}

/// Thread V's reading pane ([`thread_list`]), or, where it is drawn whole ("how it runs", a ticket that ran a
/// program), the whole of it in the reading column ([`column`]): the Threads page's and With you's alike, KEY its
/// scroll's key.
pub fn reading(s: &App, p: &Proj, v: &TicketView, key: String, k: &Kit) -> El {
    thread_list(s, p, v, key.clone(), k).unwrap_or_else(|| column(s, thread(s, p, v, k), key, k))
}

/// READING in the reading column, scrolled (KEY): at most [`READING_W`] wide, centred in what it is given, the said
/// line above it; at its top when it opens.
fn column(s: &App, reading: El, key: String, k: &Kit) -> El {
    let column = col().w_full().max_w(READING_W).children(said_line(&s.tickets, k)).child(reading);
    col().w(0.0).grow(3.0).min_w(0.0).h_full().min_h(0.0).key(key).scroll(ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(column))
}

/// Thread V is drawn as a list of its rows: not "how it runs", nor a ticket that ran a program.
fn in_rows(s: &App, v: &TicketView) -> bool {
    !(s.tickets.how.contains(&v.n) || !v.run.is_empty() && v.kind != "thread")
}

/// Thread V's rows as a column of the rows in view only (rune's `virtual_rows`), each as `thread` draws it whole, at
/// most [`READING_W`] wide, the said line above its header. With marks standing at its posts, each row's left padding
/// is its margin (margin.rs), where they are drawn; with none, the rows are as before.
fn rows_in_view(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    let rows = kept(&s.tickets.thread_rows, p, v);
    let (pi, n, id) = (pi_of(s, p), v.n, v.id.clone());
    let (keys, kit) = (rows.clone(), k.clone());
    let signs = crate::margin::of(p, v);
    let places = (!signs.is_empty()).then(|| crate::margin::places(&rows));
    // its rows, built as they come into view, drawn in the split's pane it is in (the comment box's passage lit in its own:
    // ask.rs `ours`)
    let pane = crate::ask::pane_drawn(s);
    col().key(format!("thread-rows-{id}")).w_full().growth(GrowthSpec::default()).virtual_rows(
        rows.len(),
        120.0,
        move |_: &App, i| std::borrow::Cow::Owned(row_key(&id, &keys[i])),
        move |s: &App, i| crate::ask::in_pane(s, pane, || {
            let Some(p) = s.projects.get(pi) else { return col() };
            let Some(v) = crate::find::ticket(s, p, n) else { return col() };
            let r = &rows[i];
            let el = match r {
                TRow::Head => col().w_full().gap(10.0).children(said_line(&s.tickets, &kit)).child(row_el(s, p, v, r, &kit)),
                _ => row_el(s, p, v, r, &kit),
            };
            let (top, bottom) = match r {
                TRow::Head => (18.0, 10.0),
                TRow::Foot => (0.0, 18.0),
                _ => (0.0, 10.0),
            };
            let column = col().w_full().max_w(READING_W);
            let column = match &places {
                None => column.px(28.0).pt(top).pb(bottom).child(el),
                Some(at) => {
                    let margin = crate::margin::cell(&signs, at[i], &kit);
                    column.pr(28.0).child(row().w_full().child(margin).child(col().w(0.0).grow(1.0).min_w(0.0).pt(top).pb(bottom).child(el)))
                }
            };
            row().w_full().justify_center().child(column)
        }),
    )
}


/// The comment target of thread V: its composer's (`thread:N-221`), apart from any agent's chat.
pub fn target(v: &TicketView) -> String {
    format!("thread:{}", v.id)
}

/// P's place among the open projects (a pane's project), as a comment keeps it.
fn pi_of(s: &App, p: &Proj) -> usize {
    s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur)
}

/// The thread picked in the pane of project PI for WHO (item 15: one pick for one pane): on Threads (None) the page's
/// `tickets.shown`, beside agent WHO's list its `withyou.inside`. They stay two fields, as ui.json keeps them and as
/// the Threads page's pick is read in many places; the pane reads and writes them here.
pub(crate) fn picked(s: &App, pi: usize, who: Option<&str>) -> Option<u64> {
    match who {
        None => s.tickets.shown,
        Some(a) => s.withyou.inside.get(&(pi, a.to_string())).copied(),
    }
}

/// Thread N picked in the pane of project PI for WHO, as [`picked`] reads it; None: none. On Threads its form is put
/// away and the said line with it, as a row's press always did; beside WHO's list it is seen (outline.rs
/// `open_beside`).
pub(crate) fn set_picked(s: &mut App, pi: usize, who: Option<&str>, n: Option<u64>) {
    match (who, n) {
        (None, n) => {
            s.tickets.shown = n;
            s.tickets.opening = None;
            s.tickets.said = None;
            s.tickets.new_box &= n.is_none();
        }
        (Some(a), Some(n)) => crate::outline::open_beside(s, pi, a, n),
        (Some(a), None) => {
            s.withyou.inside.remove(&(pi, a.to_string()));
        }
    }
}

/// + New thread: the box alone; on Threads to the project, beside agent WHO's list to start a thread with it.
fn new_thread(pi: usize, who: Option<&str>, k: &Kit) -> El {
    let by = who.map(str::to_string);
    Button::new("+ New thread").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        set_picked(s, pi, by.as_deref(), None);
        match by.as_deref() {
            None => s.tickets.new_box = true,
            Some(a) => crate::withyou::unbind(s, pi, a),
        }
    }).build(k).key("new-thread")
}

/// "‹ Threads" or, beside agent WHO's list, "‹ With you", over what a narrow window shows in the list's place: back to
/// the list, one way back for both (item 15). Nothing stays picked, nor "+ New thread" chosen.
pub(crate) fn back(pi: usize, who: Option<&str>, k: &Kit) -> El {
    let by = who.map(str::to_string);
    let label = if who.is_some() { "‹ With you" } else { "‹ Threads" };
    row().w_full().px(12.0).pt(8.0).child(Button::new(label).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        set_picked(s, pi, by.as_deref(), None);
        match by.as_deref() {
            None => s.tickets.new_box = false,
            Some(a) => {
                let key = (pi, a.to_string());
                if s.withyou.on.get(&key) == Some(&None) {
                    s.withyou.on.remove(&key);
                }
            }
        }
    }).build(k))
}

/// Thread V's page C, its text commented on as an agent's chat is (chat.rs): a passage selected inside one post offers
/// Comment (one across posts copies alone, `ask::one_source`), its box opens at it, and the comments wait under their
/// posts until the reply is sent (ask.rs).
fn commentable(s: &App, p: &Proj, v: &TicketView, c: El) -> El {
    let (pi, target) = (pi_of(s, p), target(v));
    c.selection_group(true)
        .selection_menu_item_when("comment", "Comment", crate::ask::one_source, move |s: &mut App, sel, tx| crate::ask::comment_on(s, pi, &target, sel, tx))
        .on_highlight_press(|s: &mut App, key, _| crate::ask::highlight_pressed(s, key))
}

/// A link to thread ID in a thread's head, opened where it was pressed (`outline::open_from_thread`).
fn goto(s: &App, p: &Proj, id: &str, label: &str, k: &Kit) -> El {
    let (pi, to, at) = (pi_of(s, p), p.live.tickets.values().find(|t| t.id == id).map(|t| t.n), id.to_string());
    k.interactive(row().child(text(label.to_string()).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
        if let Some(n) = to {
            crate::outline::open_from_thread(s, pi, n, &at);
        }
    })
}

/// The oldest post of V whose asks wait on him with no answer of his on the way: the box's default.
pub(crate) fn asking(v: &TicketView) -> Option<&Post> {
    v.posts.iter().find(|p| open_to_owner(v, p))
}

/// Post P of V asks him things still open (marked passages or a typed ask), no answer of his on the way or held.
pub(crate) fn open_to_owner(v: &TicketView, p: &Post) -> bool {
    let to_him = if p.asks.is_empty() { docket_machine::machine::asked_him(p) } else { p.told.iter().chain(&p.names).any(|n| n == "owner") };
    p.by != "owner" && p.asks_open && to_him && !v.posts.iter().any(|q| q.answers == Some(p.n))
}

/// Post P is an agent's ask typed to him (no marked passage): one question, its words whole, drawn as one box.
pub(crate) fn typed_ask(p: &Post) -> bool {
    p.asks.is_empty() && docket_machine::machine::asked_him(p)
}

/// How many questions post P asks him: its marked passages, one for a typed ask.
pub(crate) fn asks_count(p: &Post) -> usize {
    if typed_ask(p) { 1 } else { p.asks.len() }
}

fn foot_composer(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    use crate::message_box::{Target, Way};
    let pi = pi_of(s, p);
    let (target, default) = foot_target(s, p, v);
    let place = target.place();
    let mut actions = crate::message_box::thread_actions();
    // aimed at a step of his: its results are the box's actions, no Send
    if let Target::Thread { way: Way::Step { s: st }, .. } = &target
        && let Some(h) = aimable_steps(p, v).into_iter().find(|h| h.id == *st)
    {
        let (right, placeholder) = crate::ask::step_actions(s, h, &target, k);
        actions.right = right;
        actions.send = None;
        actions.placeholder = placeholder;
    }
    // aimed at a post that asks him: the words answer it, and "nothing to answer" closes its asks without words
    if let Target::Thread { post: Some(n), way: Way::Here, .. } = &target
        && let Some(q) = v.posts.iter().find(|q| q.n == *n).filter(|q| open_to_owner(v, q))
    {
        let typed = crate::message_box::written(s, &target).map(|e| e.text.clone()).unwrap_or_default();
        let asks = ask_texts(&s.tickets, q);
        // with his answers written in place under its questions: one Send sends them all, these words first
        let (text, answered) = answers_from_box(&asks, &with_answers(s, pi, &v.id, q.n, &asks, &typed));
        // a typed ask is one question with no options: its answer is whatever he writes
        actions.send = Some(match answered {
            _ if text.is_empty() && !asks.is_empty() => format!("Send answers to {}", q.by),
            n => send_label(n, &q.by),
        });
        actions.placeholder = match asks.len() {
            0 => "Your answer…".into(),
            1 => "Your answer…  (or click an option above)".into(),
            _ => "Your answers…  (an option clicked above goes under its question)".into(),
        };
        let (tn, pn, at) = (v.id.clone(), q.n, place.clone());
        actions.right.push(Button::new("nothing to answer").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            crate::log::line(&format!("nothing to answer pressed: {tn} post {pn}"));
            let key = (pi, at.clone());
            s.box_said.remove(&key);
            if !s.request_in(pi, Request::NothingToAnswer { t: tn.clone(), n: pn }, Pending::TicketDone { clear: Cleared::Nothing(pi, at.clone()) }) {
                // said once, under the box where he pressed
                let why = s.notice.take().unwrap_or_else(|| "not sent".into());
                crate::log::line(&format!("nothing to answer on {tn} post {pn}: {why}"));
                s.box_said.insert(key, Err(why));
            }
        }).build(k));
    }
    let boxed = col()
        .key(format!("post-box-{}", v.id))
        .w_full()
        .gap(6.0)
        .padding(10.0)
        .radius(k.radius.md)
        .border(1.0, k.c.border_weak)
        .children(answers_left(s, pi, v, k))
        .child(crate::message_box::build(s, &target, &default, actions, k))
        .children(crate::message_box::said(s, pi, &target.place(), k));
    // his messages on their way from this box, above its frame from the press on (outgoing.rs)
    col().w_full().gap(8.0).mt(8.0).children(crate::outgoing::dock(s, pi, &place, k)).child(boxed)
}

/// The reply on thread ID that COMMENTS and the TYPED words make: under the post the first comment is on (else on the
/// thread), to each agent tagged @NAME.
pub(crate) fn reply_with(s: &App, pi: usize, id: &str, comments: &[crate::ask::Comment], typed: &str) -> Result<Request, String> {
    let p = s.projects.get(pi).ok_or("no project")?;
    let follows = comments.first().and_then(|c| crate::ask::follows_of(p, &c.source)).unwrap_or_else(|| id.to_string());
    let notes: Vec<&str> = std::iter::once(typed).chain(comments.iter().map(|c| c.note.as_str())).collect();
    let names = crate::ask::tagged(p, &notes)?;
    // it goes as a post on the thread quoted (coordd's `follows`), whose `@NAME`s coordd reads itself: a group only from
    // GROUPS_PROTOCOL
    if crate::ask::group_refused(p, &notes) {
        return Err(crate::ask::NO_GROUP_ON_POST.into());
    }
    let (text, _) = crate::ask::compose(comments, typed);
    Ok(Request::Say { to: None, names, text, follows: Some(follows), wait: false, under: None, label: None, thread: false, typed: None, split_from: None, comments: crate::ask::data(p, comments) })
}

/// The scene's comment on post POST of thread V: QUOTE found in its words as drawn, with NOTE.
pub(crate) fn scene_comment(s: &App, v: &TicketView, post: u32, quote: &str, note: &str) -> Option<crate::ask::Comment> {
    let x = v.posts.iter().find(|x| x.n == post)?;
    let src = crate::paths::linkify(&words(&s.tickets, x));
    let at = rune_kit::markdown::find(&src, quote, rune_kit::markdown::Case::Sensitive).into_iter().next()?;
    let label = s.projects.get(s.cur).map(|_| format!("{}'s message #{post}", x.by)).unwrap_or_default();
    Some(crate::ask::Comment { source: format!("{}.p{post}", v.id), label, quote: quote.to_string(), note: note.to_string(), at })
}

/// Under his post, the tagged group's agents it never reached, read from the post itself so it survives a restart.
pub(crate) fn not_reached_line(s: &App, p: &Proj, v: &TicketView, post: &Post, k: &Kit) -> Option<El> {
    if post.by != "owner" || post.dropped || post.taken_back {
        return None;
    }
    let said = words(&s.tickets, post);
    if !said.contains('@') {
        return None;
    }
    let mut reached: Vec<&str> = post.names.iter().chain(&post.told).chain(&post.inform).map(String::as_str).collect();
    if post.n == 1 {
        reached.extend(v.form.get("to").and_then(|t| t.as_str()));
    }
    let who = crate::ask::not_reached(p, &said, &reached, &post.at);
    let t = match who.as_slice() {
        [] => return None,
        [one] => format!("not sent to {one}: it was stopped when you sent this"),
        [rest @ .., last] => format!("not sent to {} and {last}: they were stopped when you sent this", rest.join(", ")),
    };
    Some(row().key(format!("not-reached-{}.p{}", v.id, post.n)).w_full().justify_end().child(text(t).font(k.ty.small).color(k.c.text_faint)))
}

/// The agents handed the posts up to POST whose turn ended with Nothing, so silence isn't read as still at work.
pub(crate) fn quiet_on(v: &TicketView, post: &Post) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for q in v.quiet.iter().filter(|q| q.upto == post.n) {
        if !out.contains(&q.agent) {
            out.push(q.agent.clone());
        }
    }
    out
}

/// "read, no reply: PIPE, CONE", muted, under the post they read.
pub(crate) fn quiet_line(v: &TicketView, post: &Post, k: &Kit) -> Option<El> {
    let who = quiet_on(v, post);
    (!who.is_empty()).then(|| text(format!("read, no reply: {}", who.join(", "))).key(format!("quiet-{}-{}", v.id, post.n)).font(k.ty.small).color(k.c.text_faint))
}



// ---- the asks, drawn in place --------------------------------------------------------------------------------------

/// A run of a post's lines drawn one way: inside ask ASK's box, with chips (an option outside its ask), or as Markdown.
#[derive(Debug)]
struct Block {
    from: usize,
    to: usize,
    ask: Option<usize>,
    pills: bool,
}

/// TEXT cut at its lines into blocks: each ask's lines, an option line outside its ask, and the runs between; bytes kept.
fn blocks(text: &str, marks: &[Mark]) -> Vec<Block> {
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    for l in text.split('\n') {
        let (s, e) = (at, at + l.len());
        let mut starts: Vec<usize> = marks.iter().filter(|m| m.at[0] < m.at[1] && m.at[0] < e.max(s + 1) && s < m.at[1]).map(|m| m.at[0]).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut from = s;
        for c in starts.into_iter().skip(1).filter(|c| *c > s && *c < e && text.is_char_boundary(*c)) {
            lines.push((from, c));
            from = c;
        }
        lines.push((from, e));
        at += l.len() + 1;
    }
    let mut out: Vec<Block> = Vec::new();
    for (s, e) in lines {
        let ask = marks.iter().position(|m| m.at[0] < e.max(s + 1) && s < m.at[1] && m.at[0] < m.at[1]);
        let pills = ask.is_none() && marks.iter().any(|m| m.options.iter().any(|o| o[0] >= s && o[1] <= e && o[0] < o[1]));
        match out.last_mut() {
            Some(b) if b.ask.is_some() && b.ask == ask => b.to = e,
            Some(b) if ask.is_none() && b.ask.is_none() && !pills && !b.pills => b.to = e,
            _ => out.push(Block { from: s, to: e, ask, pills }),
        }
    }
    out
}

const SUP: [&str; 10] = ["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"];

fn sup(n: usize) -> String {
    n.to_string().chars().map(|c| SUP[c.to_digit(10).unwrap_or(0) as usize]).collect()
}

/// Bytes FROM..TO of TEXT as shown, with a chip per option in them: the text unchanged but for the number an option
/// outside its ask carries (ASK is the box drawn, if any); each chip's range in what is shown, its ask, its words.
fn chipped(text: &str, marks: &[Mark], from: usize, to: usize, ask: Option<usize>) -> (String, Vec<(std::ops::Range<usize>, usize, String)>) {
    let mut opts: Vec<(usize, usize, usize)> = marks.iter().enumerate().flat_map(|(i, m)| m.options.iter().map(move |o| (o[0], o[1], i))).filter(|(s, e, _)| *s >= from && *e <= to && s < e).collect();
    opts.sort();
    let (mut shown, mut chips, mut cur) = (String::new(), Vec::new(), from);
    for (s, e, i) in opts {
        if s < cur || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
            continue;
        }
        shown.push_str(&text[cur..s]);
        let start = shown.len();
        shown.push_str(&text[s..e]);
        if ask != Some(i) {
            shown.push_str(&sup(i + 1));
        }
        chips.push((start..shown.len(), i, text[s..e].to_string()));
        cur = e;
    }
    shown.push_str(&text[cur..to]);
    (shown, chips)
}

/// SHOWN's inline Markdown drawn rather than shown (a line with chips is not given to the Markdown view, which cannot
/// hold them): `**` and `` ` `` markers outside the chips dropped, their runs bold or mono, a line's leading `- ` or
/// `* ` a bullet. The words are unchanged; the chips' ranges move with them.
fn inline_md(shown: &str, chips: &[(std::ops::Range<usize>, usize, String)]) -> (String, Vec<(usize, usize, bool)>, Vec<std::ops::Range<usize>>) {
    let inside = |i: usize| chips.iter().any(|(r, _, _)| r.start <= i && i < r.end);
    let mut out = String::new();
    let mut map = vec![0usize; shown.len() + 1];
    let mut runs: Vec<(usize, usize, bool)> = Vec::new();
    let (mut bold, mut code): (Option<usize>, Option<usize>) = (None, None);
    let mut line_start = true;
    let mut i = 0;
    while i < shown.len() {
        map[i] = out.len();
        let rest = &shown[i..];
        if line_start && (rest.starts_with("- ") || rest.starts_with("* ")) && !inside(i) {
            out.push_str("• ");
            map[i + 1] = out.len();
            i += 2;
            line_start = false;
            continue;
        }
        if code.is_none() && rest.starts_with("**") && !inside(i) && !inside(i + 1) {
            match bold.take() {
                Some(b) => runs.push((b, out.len(), true)),
                None => bold = Some(out.len()),
            }
            map[i + 1] = out.len();
            i += 2;
            continue;
        }
        if rest.starts_with('`') && !inside(i) {
            match code.take() {
                Some(c) => runs.push((c, out.len(), false)),
                None => code = Some(out.len()),
            }
            i += 1;
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        out.push(c);
        for k in 1..c.len_utf8() {
            map[i + k] = out.len();
        }
        line_start = c == '\n';
        i += c.len_utf8();
    }
    map[shown.len()] = out.len();
    runs.retain(|(a, b, _)| a < b);
    (out, runs, chips.iter().map(|(r, _, _)| map[r.start]..map[r.end]).collect())
}

/// The asks of post P as its words have them, by ask (empty while its words are not read).
pub(crate) fn ask_texts(pg: &Page, p: &Post) -> Vec<String> {
    let t = read(pg, p).unwrap_or_default();
    p.asks.iter().map(|m| t.get(m.at[0]..m.at[1]).unwrap_or("").to_string()).collect()
}

/// What the box's words TYPED answer of ASKS, by ask: the answer under each ask's quote.
fn typed(asks: &[String], typed: &str) -> Vec<String> {
    let (_, got) = sections(typed);
    asks.iter().map(|a| got.iter().find(|(q, _)| q.trim() == a.trim()).map(|(_, x)| x.clone()).unwrap_or_default()).collect()
}

// The sources of the message bodies built (`post::message_body`, a body with a source), for the tests that count
// them (view_cost.rs).
#[cfg(test)]
thread_local! {
    pub(crate) static BODIES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// TEXT's ids (`N-412`, `N-412.p30`, `Q-445`, `@NAME`) as links with their labels; one without a label stays as written.
pub(crate) fn mentioned(p: &Proj, text: &str) -> String {
    crate::mentions::link(text, |id| {
        if let Some(name) = id.strip_prefix('@') {
            return p.live.agents.iter().any(|a| a.name == name).then(|| name.to_string());
        }
        let l = match p.live.tickets.values().find(|t| t.id == id) {
            Some(t) => crate::outline::label(p, t),
            None => p.labels.get(id)?.to_string(),
        };
        Some(crate::refs::cut(&l, 40))
    })
}

/// Post P's words T with its asks, open or answered, the rest as message words; SHOW: a chip click opens the thread too.
pub(crate) fn drawn(s: &App, pj: &Proj, v: &TicketView, p: &Post, t: &str, cwd: Option<String>, show: bool, k: &Kit) -> Vec<El> {
    if p.asks.is_empty() {
        return vec![typed_box(s, pj, v, p, t, cwd, show, k)];
    }
    let pg = &s.tickets;
    let answered: Option<(&Post, (String, Vec<(String, String)>))> = v.posts.iter().rev().find(|q| q.answers == Some(p.n)).map(|q| (q, sections(&words(pg, q))));
    let asks: Vec<String> = p.asks.iter().map(|m| t.get(m.at[0]..m.at[1]).unwrap_or("").to_string()).collect();
    let in_box = s.composers.get(&(s.cur, crate::message_box::thread_place(&v.id))).map(|e| e.text.clone()).unwrap_or_default();
    let lines = typed(&asks, &in_box);
    // sent but held (the asker not told yet): drawn as answers, sending…, not as an open card with its lines emptied
    let open = p.asks_open && answered.is_none();
    let (vn, pn) = (v.n, p.n);
    let pi = pi_of(s, pj);
    // his answer to it on its way (outgoing.rs) and the one it got: a choice he clicked stays lit from the click on
    let on_way: Vec<Vec<String>> = going(s, pi, &v.id, p.n).map(|o| typed(&asks, &o.text)).collect();
    let got: Vec<String> = answered.as_ref().map(|(q, _)| typed(&asks, &words(pg, q))).unwrap_or_default();
    // while his answer is on its way the post asks nothing more: each question shows it, with no box for a second
    let away: Option<(String, Vec<String>)> = going(s, pi, &v.id, p.n).last().filter(|_| open).map(|o| (crate::outgoing::mark(o), typed(&asks, &o.text)));
    let asks_him = open && away.is_none();
    // his answers written in place under each question, and why one can no longer go once the post stops asking
    let drafts: Vec<String> = (0..asks.len()).map(|i| answer_draft(s, pi, &v.id, p.n, i)).collect();
    let why = match (p.asks_closed.as_ref().and_then(closed_words), &answered) {
        (Some(w), _) => Some(w),
        (None, Some((q, _))) => Some(format!("you answered it in #{}", q.n)),
        (None, None) => None,
    };
    let source = format!("{}.p{}", v.id, p.n);
    let mut out = Vec::new();
    // the post's lines drawn so far: a passage is lit once, where he selected it or where it first stands
    let mut earlier = String::new();
    for b in blocks(t, &p.asks) {
        let (shown, chips) = chipped(t, &p.asks, b.from, b.to, b.ask);
        if b.ask.is_none() && !b.pills {
            if !shown.trim().is_empty() {
                // the comments' marks in the block holding their words: a passage he selects is lit, its box opening after it
                out.push(crate::post::message_part(Some(pj), &shown, Some((s, source.as_str())), Some(&earlier), cwd.clone(), k));
                earlier.push_str(&shown);
                earlier.push_str("\n\n");
            }
            continue;
        }
        let (shown, runs, ranges) = inline_md(&shown, &chips);
        let mut spans: Vec<rune::Span> = runs.iter().map(|(a, b, bold)| if *bold { rune::Span::new(*a, *b).bold() } else { rune::Span::new(*a, *b).mono() }).collect();
        // the paths it names, as links (the same rules as `paths::linkify`), none over a chip
        let code: Vec<std::ops::Range<usize>> = runs.iter().filter(|r| !r.2).map(|r| r.0..r.1).collect();
        // only the files coordd found are links; one not found is drawn as written (the owner's walk, item 9)
        let files: Vec<(std::ops::Range<usize>, String)> = crate::paths::ranges(&shown, &code)
            .into_iter()
            .filter(|(r, _)| !ranges.iter().any(|c| c.start < r.end && r.start < c.end))
            .filter(|(_, path)| crate::paths::found(&pj.id, path, cwd.as_deref()).is_some())
            .collect();
        spans.extend(files.iter().map(|(r, _)| rune::Span::new(r.start, r.end).color(k.c.text_accent)));
        let mut el = rune::rich(shown, spans).font(k.ty.body).color(k.c.text_base);
        for (r, path) in &files {
            el = el.link(Link::new(r.clone(), format!("file:{path}")).hover(LinkPaint::new().underline(k.c.text_accent)));
        }
        let mut pills = Vec::new();
        // every choice stays a click once its asks closed, putting its words in the box to answer again
        for ((_, i, words), r) in chips.iter().zip(&ranges) {
            let is = |d: Option<&String>| d.is_some_and(|d| d.trim() == words.trim());
            let on = is(drafts.get(*i)) || is(lines.get(*i)) || is(got.get(*i)) || on_way.iter().any(|g| is(g.get(*i)));
            let st = if on { PillStyle::new().bg(k.c.bg_accent).fg(k.c.text_contrast) } else { PillStyle::new().bg(k.c.layer_03).fg(k.c.text_base) };
            pills.push((r.clone(), st));
            let mut l = Link::new(r.clone(), format!("{vn}|{pn}|{i}|{words}")).hover(LinkPaint::new().ring(k.c.border_focus, 1.0));
            if on {
                // a tooltip of its own, so the question's ("Answer this question here") never shows over his answer
                l = l.label(format!("{words}, your answer")).help("Your answer");
            } else if asks_him {
                l = l.help(if goes_at_once(s, pi, &v.id, pn, *i, asks.len()) { "Sends this as your answer" } else { "Picks this as your answer: Send sends it" });
            }
            el = el.link(l);
        }
        el = el.pills(pills);
        if !chips.is_empty() || !files.is_empty() {
            let (cwd, pid) = (cwd.clone(), pj.id.clone());
            el = el.on_link(move |s: &mut App, id: &str, _tx: &mut rune::Tx| {
                if id.starts_with("file:") {
                    if !crate::paths::open_found(s, &pid, id, cwd.as_deref()) {
                        crate::paths::open(id, cwd.clone());
                    }
                    return;
                }
                let mut it = id.splitn(4, '|');
                let (Some(t), Some(p), Some(i), Some(w)) = (it.next(), it.next(), it.next(), it.next()) else { return };
                let (Ok(t), Ok(p), Ok(i)) = (t.parse::<u64>(), p.parse::<u32>(), i.parse::<usize>()) else { return };
                pick(s, t, p, i, w);
                if show {
                    crate::threads::show(s, t);
                }
            });
        }
        let Some(i) = b.ask else {
            out.push(el);
            continue;
        };
        // its words take the pointer only on chips and links, so a click on a choice opens no box
        let mut bx = ask_box(format!("ask-{}-{}-{i}", v.id, p.n), asks_him, el, k);
        if let Some((mark, said)) = &away {
            match said.get(i).filter(|a| !a.trim().is_empty()) {
                Some(a) => bx = bx.child(answer_row(mark.clone(), a, k)),
                None => bx = bx.child(text("not answered").font(k.ty.small).color(k.c.text_faint)),
            }
        } else if open {
            // a press opens the one box under it; Send sends the post's answers as one message
            let t = v.id.clone();
            bx = k.interactive(bx).help("Answer this question here").on_press(move |s: &mut App| open_answer(s, pi, &t, pn, i));
        } else if let Some(w) = p.asks_closed.as_ref().and_then(closed_words) {
            // closed without an answer in this box: one faint line saying how, in the style of "not answered"
            bx = bx.child(text(w).font(k.ty.small).color(k.c.text_faint));
        } else if let Some((q, (_, got))) = &answered {
            let ask = t.get(p.asks[i].at[0]..p.asks[i].at[1]).unwrap_or("");
            match got.iter().find(|(a, _)| a.trim() == ask.trim()) {
                Some((_, a)) => {
                    let when = if p.asks_open || q.held.is_some() { "sending…".to_string() } else { format!("you {}", crate::when::at(&q.at.to_string())) };
                    bx = bx.child(answer_row(when, a, k));
                }
                None => bx = bx.child(text("not answered").font(k.ty.small).color(k.c.text_faint)),
            }
        }
        // his answer under the question, outside its pressable block, so a press or a word in the box stays the box's
        let under = if open {
            answer_under(s, pi, v, p, i, true, None, k)
        } else if !drafts.get(i).is_none_or(|d| d.is_empty()) {
            answer_under(s, pi, v, p, i, false, why.clone(), k)
        } else {
            Vec::new()
        };
        out.push(if under.is_empty() { bx } else { col().key(format!("asked-{}-{}-{i}", v.id, p.n)).w_full().gap(6.0).child(bx).children(under) });
    }
    // words of his answer under no ask's quote: once, under the asks
    if let Some((q, (pre, _))) = answered.as_ref().filter(|(_, (pre, _))| !pre.trim().is_empty()) {
        let when = if p.asks_open || q.held.is_some() { "sending…".to_string() } else { format!("you {}", crate::when::at(&q.at.to_string())) };
        out.push(answer_row(when, pre.trim(), k));
    }
    out
}

/// An ask's box KEY over its words EL: open (ASKS_HIM) with the accent's rail, else the success border.
fn ask_box(key: String, asks_him: bool, el: El, k: &Kit) -> El {
    col().key(key).w_full().gap(8.0).px(14.0).py(10.0).radius(k.radius.sm).fill(k.c.layer_02).border_sides([0.0, 0.0, 0.0, 3.0], if asks_him { k.c.danger.fg } else { k.c.success.border }).child(el)
}

/// A typed ask (post P of V, no marked passage): one ask box over its words T and its answer state; SHOW opens the thread.
#[allow(clippy::too_many_arguments)]
fn typed_box(s: &App, pj: &Proj, v: &TicketView, p: &Post, t: &str, cwd: Option<String>, show: bool, k: &Kit) -> El {
    let pi = pi_of(s, pj);
    let source = format!("{}.p{}", v.id, p.n);
    let answered = v.posts.iter().rev().find(|q| q.answers == Some(p.n));
    let open = p.asks_open && answered.is_none();
    let away = going(s, pi, &v.id, p.n).last().filter(|_| open);
    let mut bx = ask_box(format!("ask-{}-{}-0", v.id, p.n), open && away.is_none(), crate::post::message_body(Some(pj), t, Some((s, source.as_str())), cwd, k), k);
    if let Some(o) = away {
        bx = bx.child(answer_row(crate::outgoing::mark(o), o.text.trim(), k));
    } else if open {
        let (id, vn, pn) = (v.id.clone(), v.n, p.n);
        let to = Button::new("Answer it in the box").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            if show {
                crate::threads::show(s, vn);
            }
            answer_post(s, pi, &id, pn);
        });
        bx = bx.child(row().w_full().justify_end().child(to.build(k)));
    } else if let Some(q) = answered.filter(|q| p.asks_open || q.held.is_some()) {
        bx = bx.child(answer_row("sending…".into(), words(&s.tickets, q).trim(), k));
    } else {
        // only his post closes a typed ask as answered (the machine's `closers_in`); with no how in the view, his
        // answer recorded says it
        let how = match p.asks_closed.as_ref() {
            Some(AsksClosed::Answered { post, .. }) => Some(format!("you answered it in #{post}")),
            Some(c) => closed_words(c),
            None => answered.map(|q| format!("you answered it in #{}", q.n)),
        };
        if let Some(w) = how {
            bx = bx.child(text(w).font(k.ty.small).color(k.c.text_faint));
        }
    }
    bx
}

/// His answer A under a question, after its mark WHEN (`sending…`, `you 10:47`): the engine's copy of his answer, or
/// his message on its way.
fn answer_row(when: String, a: &str, k: &Kit) -> El {
    row().w_full().gap(10.0).center_y().child(row().px(6.0).py(1.0).radius(k.radius.sm).fill(k.c.info.bg).child(text(when).font(k.ty.small).color(k.c.text_accent))).child(text(a.to_string()).font(k.ty.body).color(k.c.text_base).wrap().grow(1.0).shrink())
}

/// The words under an ask closed some way other than an answer in its box, faint (None: the box shows the answer).
fn closed_words(c: &AsksClosed) -> Option<String> {
    Some(match c {
        // the owner said there was nothing to answer
        AsksClosed::NothingToAnswer => "you: nothing to answer".into(),
        // answered on a closed ticket: his words opened a thread that follows it (3h), the answer is there
        AsksClosed::Followed { ticket } => format!("you answered in {ticket}"),
        // he cancelled the ticket, which closes what it asked him
        AsksClosed::Cancelled => "closed: you cancelled this thread".into(),
        // the asker or an agent above it took the asks back, with its reason
        AsksClosed::Withdrawn { by, why } if why.trim().is_empty() => format!("withdrawn by {by}"),
        AsksClosed::Withdrawn { by, why } => format!("withdrawn by {by}: {}", why.trim()),
        // answered in a thread tied at this post
        AsksClosed::Subthread { ticket } => format!("answered in the subthread {ticket}"),
        _ => return None,
    })
}

/// A chip of ask I of post PN on ticket VN clicked: its words W are that question's answer, written under it while the
/// post asks him, and sent at once when that is all he says to the post (`goes_at_once`), else with his next Send; once
/// its asks closed, in the thread's box under the ask's quote; the box answers that post.
fn pick(s: &mut App, vn: u64, pn: u32, i: usize, w: &str) {
    let pi = s.cur;
    let Some(v) = s.projects.get(pi).and_then(|p| p.live.tickets.get(&vn)) else { return };
    let Some(p) = v.posts.iter().find(|p| p.n == pn) else { return };
    let (t, open, asks) = (v.id.clone(), open_to_owner(v, p), ask_texts(&s.tickets, p));
    if open {
        // one click is his answer: written under the question, chip lit, no box opened; a second click sends nothing more
        if going(s, pi, &t, pn).any(|o| typed(&asks, &o.text).get(i).is_some_and(|x| x.trim() == w.trim())) {
            return;
        }
        let now = goes_at_once(s, pi, &t, pn, i, asks.len());
        s.composers.insert((pi, crate::message_box::answer_place(&t, pn, i)), TextEdit::new(w));
        if s.answering.as_ref().is_some_and(|(p, x, m, j)| *p == pi && *x == t && *m == pn && *j == i) {
            s.answering = None;
        }
        if now {
            // as his answer, a post of its own (`send_answers`)
            send_answers(s, pi, &t, pn);
            return;
        }
    } else {
        // its asks closed: the words go in the thread's box under the ask's quote, to answer again
        let place = crate::message_box::thread_place(&t);
        let had = s.composers.get(&(pi, place.clone())).map(|e| e.text.clone()).unwrap_or_default();
        let now = put_answer(&asks, i, &had, w);
        let mut e = TextEdit::new(&now);
        e.set_caret(now.len());
        s.composers.insert((pi, place), e);
    }
    let _ = aim_at(s, pi, &t, Some(pn));
}

/// Whether a click on a choice of question I of post N of thread T (ASKS questions) is all he says to that post, so it
/// goes at once: the post asks him one question, nothing of his is written for it (in the thread's box, under the
/// question) and no answer of his to it is on its way. Else the click picks it and his next Send sends it with the
/// rest: one message answers all a post asks (the engine closes its asks at the first), so a click on the first of two
/// questions must not close the second, nor send words he is still writing.
fn goes_at_once(s: &App, pi: usize, t: &str, n: u32, i: usize, asks: usize) -> bool {
    let typed = s.composers.get(&(pi, crate::message_box::thread_place(t))).is_some_and(|e| !e.text.trim().is_empty());
    asks == 1 && !typed && answer_draft(s, pi, t, n, i).is_empty() && going(s, pi, t, n).next().is_none()
}

/// His messages on their way answering post N of thread T; a refused one is not, one waiting for its pipe is.
fn going<'a>(s: &'a App, pi: usize, t: &'a str, n: u32) -> impl Iterator<Item = &'a crate::outgoing::Out> + 'a {
    s.outgoing.list.iter().filter(move |o| {
        o.key.0 == pi && !(matches!(o.is, crate::outgoing::Is::NotSent(_)) && !o.unsent) && matches!(&o.req, Request::Post { t: x, answers: Some(m), .. } if x == t && *m == n)
    })
}

/// The box's words read as answers: the words before any quoted ask, and each quoted ask (its `> ` lines) with the
/// words under it.
pub(crate) fn sections(text: &str) -> (String, Vec<(String, String)>) {
    let mut pre: Vec<&str> = Vec::new();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut q: Vec<&str> = Vec::new();
    let mut a: Vec<&str> = Vec::new();
    for l in text.lines() {
        if let Some(x) = l.strip_prefix("> ").or_else(|| (l == ">").then_some("")) {
            if !a.is_empty() || (q.is_empty() && !out.is_empty()) {
                out.push((q.join("\n"), a.join("\n").trim().to_string()));
                q.clear();
                a.clear();
            }
            q.push(x);
        } else if q.is_empty() && out.is_empty() {
            pre.push(l);
        } else {
            a.push(l);
        }
    }
    if !q.is_empty() {
        out.push((q.join("\n"), a.join("\n").trim().to_string()));
    }
    (pre.join("\n").trim().to_string(), out)
}

/// TEXT with WORDS as the answer to ask I of ASKS: under its quote, the quotes in the asks' order, the words before
/// them kept.
fn put_answer(asks: &[String], i: usize, text: &str, words: &str) -> String {
    let (pre, mut got) = sections(text);
    let Some(ask) = asks.get(i) else { return text.to_string() };
    match got.iter_mut().find(|(q, _)| q.trim() == ask.trim()) {
        Some((_, a)) => *a = words.to_string(),
        None => got.push((ask.clone(), words.to_string())),
    }
    got.sort_by_key(|(q, _)| asks.iter().position(|a| a.trim() == q.trim()).unwrap_or(usize::MAX));
    let mut parts: Vec<String> = Vec::new();
    if !pre.is_empty() {
        parts.push(pre);
    }
    for (q, a) in got {
        let quoted: Vec<String> = q.lines().map(|l| format!("> {l}")).collect();
        parts.push(format!("{}\n\n{}", quoted.join("\n"), a));
    }
    parts.join("\n\n")
}

// ---- his answers written in place under a post's questions ------------------------------------------------

/// His answer to question I of post N of thread T, written in place under it (empty: none).
pub(crate) fn answer_draft(s: &App, pi: usize, t: &str, n: u32, i: usize) -> String {
    s.composers.get(&(pi, crate::message_box::answer_place(t, n, i))).map(|e| e.text.trim().to_string()).unwrap_or_default()
}

/// The posts of thread T and their questions' indexes with an answer of his written in place, in order.
fn drafts_of(s: &App, pi: usize, t: &str) -> Vec<(u32, usize)> {
    let pre = format!("answer:{t}.p");
    let mut out: Vec<(u32, usize)> = s
        .composers
        .iter()
        .filter(|((i, _), e)| *i == pi && !e.text.trim().is_empty())
        .filter_map(|((_, place), _)| {
            let (n, i) = place.strip_prefix(&pre)?.split_once('.')?;
            Some((n.parse().ok()?, i.parse().ok()?))
        })
        .collect();
    out.sort();
    out
}

/// Whether post N of thread T still asks him what his answers written in place under it answer: then the thread's
/// box sends them with its words (message_box.rs `send`).
pub(crate) fn has_answers(s: &App, pi: usize, t: &str, n: u32) -> bool {
    let open = s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| v.id == t)).and_then(|v| v.posts.iter().find(|q| q.n == n).map(|q| open_to_owner(v, q)));
    open == Some(true) && drafts_of(s, pi, t).iter().any(|(m, _)| *m == n)
}

/// TYPED (the thread's box's words) with his answers written in place under the questions ASKS of post N put in, each
/// under its question's quote, in the questions' order, the words before them kept (as a chip puts one).
pub(crate) fn with_answers(s: &App, pi: usize, t: &str, n: u32, asks: &[String], typed: &str) -> String {
    let mut text = typed.to_string();
    for i in 0..asks.len() {
        let a = answer_draft(s, pi, t, n, i);
        if !a.is_empty() {
            text = put_answer(asks, i, &text, &a);
        }
    }
    text
}

/// The thread's box aimed at post N of thread T (its default when that is the post the box answers).
fn aim_at(s: &mut App, pi: usize, t: &str, n: Option<u32>) -> crate::message_box::Target {
    use crate::message_box::{Target, Way};
    let target = Target::Thread { project: pi, t: t.to_string(), post: n, way: Way::Here };
    let default = s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| v.id == t)).map(|v| asking(v).map(|q| q.n));
    if default == Some(n) {
        s.box_targets.remove(&(pi, target.place()));
    } else {
        crate::message_box::choose(s, target.clone());
    }
    target
}

/// His answers written in place under post N of thread T sent as one message, the thread's box's words first: put in
/// that box under their questions' quotes and sent from it as the post's answers, so the cleaning, the line under
/// the box and his words back on a failure are the box's, as for every message. A post that no longer asks him (its
/// questions answered from elsewhere, or closed) sends nothing: each answer stays under its question, `not sent`,
/// with why and `Send as a message` (`drawn`).
pub(crate) fn send_answers(s: &mut App, pi: usize, t: &str, n: u32) {
    s.answering = None;
    let Some((open, asks)) = s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| v.id == t)).and_then(|v| v.posts.iter().find(|q| q.n == n).map(|q| (open_to_owner(v, q), ask_texts(&s.tickets, q)))) else { return };
    if !open {
        return;
    }
    let place = crate::message_box::thread_place(t);
    let typed = s.composers.get(&(pi, place.clone())).map(|e| e.text.clone()).unwrap_or_default();
    let text = with_answers(s, pi, t, n, &asks, &typed);
    let mut e = TextEdit::new(&text);
    e.set_caret(text.len());
    s.composers.insert((pi, place), e);
    for i in 0..asks.len() {
        s.composers.remove(&(pi, crate::message_box::answer_place(t, n, i)));
    }
    let target = aim_at(s, pi, t, Some(n));
    crate::message_box::send(s, &target);
}

/// His answer to question I of post N of thread T, which can no longer go as that question's answer, sent as a
/// message on the thread: under the question's quote, at that post, after the box's words; with the post gone (taken
/// back), as words of the thread's.
pub(crate) fn send_as_message(s: &mut App, pi: usize, t: &str, n: u32, i: usize) {
    let draft = answer_draft(s, pi, t, n, i);
    if draft.is_empty() {
        return;
    }
    let asks = s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| v.id == t)).and_then(|v| v.posts.iter().find(|q| q.n == n)).map(|q| ask_texts(&s.tickets, q));
    let place = crate::message_box::thread_place(t);
    let typed = s.composers.get(&(pi, place.clone())).map(|e| e.text.clone()).unwrap_or_default();
    let (text, post) = match asks {
        Some(asks) if i < asks.len() => (put_answer(&asks, i, &typed, &draft), Some(n)),
        _ if typed.trim().is_empty() => (draft, None),
        _ => (format!("{}\n\n{draft}", typed.trim_end()), None),
    };
    let mut e = TextEdit::new(&text);
    e.set_caret(text.len());
    s.composers.insert((pi, place), e);
    remove_answer(s, pi, t, n, i);
    let target = aim_at(s, pi, t, post);
    crate::message_box::send(s, &target);
}

/// His answer to question I of post N of thread T removed, its box closed.
pub(crate) fn remove_answer(s: &mut App, pi: usize, t: &str, n: u32, i: usize) {
    let place = crate::message_box::answer_place(t, n, i);
    s.composers.remove(&(pi, place.clone()));
    s.writing_changed(pi, &place);
    if s.answering.as_ref().is_some_and(|(p, x, m, j)| *p == pi && x == t && *m == n && *j == i) {
        s.answering = None;
    }
}

/// The box of question I of post N of thread T opened under it, with the keyboard; another question's closes, its
/// words kept.
pub(crate) fn open_answer(s: &mut App, pi: usize, t: &str, n: u32, i: usize) {
    // the keyboard leaves this thread's other boxes (its own, another question's); a box elsewhere is not touched
    let (foot, pre) = (crate::message_box::thread_place(t), format!("answer:{t}.p"));
    for (_, e) in s.composers.iter_mut().filter(|((q, place), _)| *q == pi && (*place == foot || place.starts_with(&pre))) {
        e.focused = false;
    }
    let e = s.composers.entry((pi, crate::message_box::answer_place(t, n, i))).or_insert_with(|| TextEdit::new(""));
    e.focused = true;
    let end = e.text.len();
    e.set_caret(end);
    s.answering = Some((pi, t.to_string(), n, i));
}

/// Esc in a question's box: it closes, his words kept.
pub(crate) fn answer_key(s: &mut App, target: &crate::message_box::Target, ev: &rune::KeyEvent) -> bool {
    if !matches!(target, crate::message_box::Target::Answer { .. }) || !matches!(ev.key, rune::Key::Escape) {
        return false;
    }
    s.answering = None;
    true
}

/// How many of post N's questions ASKS have an answer of his, written in place or in the thread's box TYPED.
fn answered_count(s: &App, pi: usize, t: &str, n: u32, asks: &[String], typed: &str) -> usize {
    answers_from_box(asks, &with_answers(s, pi, t, n, asks, typed)).1
}

/// The label of Send for post Q's answers, naming whom they go to.
fn send_label(answered: usize, by: &str) -> String {
    // the words of the thread's box's Send (`foot_composer`), so the two never say it two ways
    match answered {
        0 => format!("Send answer to {by}"),
        1 => format!("Send 1 answer to {by}"),
        k => format!("Send {k} answers to {by}"),
    }
}

/// Under question I of post Q of V (its words ASK): his answer written in place. Open: the one message box, writing
/// to it; else, when he wrote one, `your answer · not sent: …`, a press opening its box again, and Remove. A post
/// that no longer asks him: the answer with why it can no longer go as that answer, and `Send as a message`.
#[allow(clippy::too_many_arguments)]
fn answer_under(s: &App, pi: usize, v: &TicketView, q: &Post, i: usize, open: bool, why: Option<String>, k: &Kit) -> Vec<El> {
    use crate::message_box::{Actions, Target};
    let (t, n) = (v.id.clone(), q.n);
    let draft = answer_draft(s, pi, &t, n, i);
    let here = s.answering.as_ref().is_some_and(|(p, x, m, j)| *p == pi && *x == t && *m == n && *j == i);
    let mut out = Vec::new();
    if open && here {
        let target = Target::Answer { project: pi, t: t.clone(), post: n, ask: i };
        let asks = ask_texts(&s.tickets, q);
        let typed = s.composers.get(&(pi, crate::message_box::thread_place(&t))).map(|e| e.text.clone()).unwrap_or_default();
        let actions = Actions { left: None, right: Vec::new(), send: Some(send_label(answered_count(s, pi, &t, n, &asks, &typed), &q.by)), placeholder: "Your answer to this question…".into() };
        out.push(col().key(format!("answer-box-{t}-{n}-{i}")).w_full().child(crate::message_box::build(s, &target, &target, actions, k)));
        out.extend(crate::message_box::said(s, pi, &target.place(), k));
        return out;
    }
    if draft.is_empty() {
        return out;
    }
    let shown = draft.split_whitespace().collect::<Vec<_>>().join(" ");
    let shown = if shown.chars().count() > 140 { format!("{}…", shown.chars().take(140).collect::<String>()) } else { shown };
    let (t1, t2, t3) = (t.clone(), t.clone(), t.clone());
    let mut line = row().key(format!("answer-draft-{t}-{n}-{i}")).w_full().gap(8.0).center_y();
    line = line.child(
        k.interactive(text(format!("your answer · not sent: {shown}")).font(k.ty.small).color(k.c.text_muted).wrap())
            .grow(1.0)
            .shrink()
            .on_press(move |s: &mut App| open_answer(s, pi, &t1, n, i)),
    );
    if !open {
        line = line.child(Button::new("Send as a message").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| send_as_message(s, pi, &t2, n, i)).build(k));
    }
    line = line.child(Button::new("Remove").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| remove_answer(s, pi, &t3, n, i)).build(k));
    out.push(line);
    if !open {
        let why = why.unwrap_or_else(|| "the question is closed".into());
        out.push(text(format!("it can no longer go as this question's answer: {why}")).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    out
}

/// His answers written in place under questions of thread V whose posts it no longer shows (taken back), above its
/// box: each `not sent`, with `Send as a message` and Remove, so nothing he wrote goes unseen.
fn answers_left(s: &App, pi: usize, v: &TicketView, k: &Kit) -> Vec<El> {
    let mut out = Vec::new();
    for (n, i) in drafts_of(s, pi, &v.id).into_iter().filter(|(n, _)| !v.posts.iter().any(|q| q.n == *n)) {
        let draft = answer_draft(s, pi, &v.id, n, i);
        let shown = draft.split_whitespace().collect::<Vec<_>>().join(" ");
        let shown = if shown.chars().count() > 140 { format!("{}…", shown.chars().take(140).collect::<String>()) } else { shown };
        let (t1, t2) = (v.id.clone(), v.id.clone());
        out.push(
            col().key(format!("answer-left-{}-{n}-{i}", v.id)).w_full().gap(4.0).child(
                row().w_full().gap(8.0).center_y()
                    .child(text(format!("your answer · not sent: {shown}")).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink())
                    .child(Button::new("Send as a message").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| send_as_message(s, pi, &t1, n, i)).build(k))
                    .child(Button::new("Remove").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| remove_answer(s, pi, &t2, n, i)).build(k)),
            ).child(text(format!("it can no longer go as an answer: post {n} it answered was taken back")).font(k.ty.small).color(k.c.text_muted).wrap()),
        );
    }
    out
}

/// What the box's words TYPED send as the answers to ASKS, and how many asks they answer: each quoted ask with words
/// under it; words under no quote go first, and with one ask they are its answer.
pub(crate) fn answers_from_box(asks: &[String], typed: &str) -> (String, usize) {
    let (pre, got) = sections(typed);
    let got: Vec<(String, String)> = got.into_iter().filter(|(_, a)| !a.trim().is_empty()).collect();
    if got.is_empty() {
        if pre.is_empty() {
            return (String::new(), 0);
        }
        if asks.len() == 1 {
            return (answers_text(asks, &[pre]), 1);
        }
        return (pre, 0);
    }
    let qs: Vec<String> = got.iter().map(|(q, _)| q.clone()).collect();
    let ans: Vec<String> = got.iter().map(|(_, a)| a.clone()).collect();
    let quoted = answers_text(&qs, &ans);
    let n = got.iter().filter(|(q, _)| asks.iter().any(|a| a.trim() == q.trim())).count();
    (if pre.is_empty() { quoted } else { format!("{pre}\n\n{quoted}") }, n)
}

/// The answers post: each answered ask quoted with `> `, a blank line, then its answer (else Markdown joins them).
fn answers_text(asks: &[String], typed: &[String]) -> String {
    let mut parts = Vec::new();
    for (a, t) in asks.iter().zip(typed) {
        if t.trim().is_empty() {
            continue;
        }
        let q: Vec<String> = a.lines().map(|l| format!("> {l}")).collect();
        parts.push(format!("{}\n\n{}", q.join("\n"), t.trim()));
    }
    parts.join("\n\n")
}

/// What the card's foot says of the minder.
#[derive(Debug, PartialEq)]
enum Foot {
    /// It has not rested on this post yet: the card is drawn and usable.
    Looking,
    /// It looked and left the asks with the owner; UNDONE when that was his Undo of the minder's answer.
    Left { undone: bool },
    /// The minder's answer K told the asker (asks_closed Told): its step's words, and whether Undo (the ticket's last
    /// answer not undone) is this one.
    Answered(String, bool),
    Nothing,
}

fn foot(v: &TicketView, p: &Post) -> Foot {
    // no minder looks at an agents' thread, nor at an ask typed to him: nothing to say
    if matches!(&v.status, ViewStatus::Done { result } if result["head"] == "Between") || typed_ask(p) {
        return Foot::Nothing;
    }
    let answer = |s: &docket_machine::view::ViewShaped| s.refused.is_none() && s.answer.get("head").and_then(Value::as_str) != Some("Rest");
    if v.waits_on.iter().any(|w| w.call == "asks" && w.post == Some(p.n)) {
        // the latest answer after the post: an undone one gave the asks back
        let undone = v.shaped.iter().rev().find(|s| s.at.to_string() > p.at.to_string()).is_some_and(|s| s.undone && answer(s));
        return Foot::Left { undone };
    }
    if let Some(AsksClosed::Told { minder: Some(m), .. }) = &p.asks_closed {
        if let Some(s) = v.shaped.iter().find(|s| s.n == *m) {
            let last = v.shaped.iter().rev().find(|x| !x.undone && answer(x)).is_some_and(|x| x.n == s.n);
            let prog = s.answer.get("text").and_then(Value::as_str).unwrap_or("");
            return Foot::Answered(step_words(prog).unwrap_or_else(|| prog.to_string()), last);
        }
    }
    if p.asks_open { Foot::Looking } else { Foot::Nothing }
}

/// The words of a program's first inline step: the string after `step WORKER`.
fn step_words(program: &str) -> Option<String> {
    let at = program.find("step ")?;
    let rest = &program[at..];
    let q = rest.find('"')?;
    let mut out = String::new();
    let mut esc = false;
    for c in rest[q + 1..].chars() {
        match (esc, c) {
            (true, c) => {
                out.push(match c {
                    'n' => '\n',
                    c => c,
                });
                esc = false;
            }
            (false, '\\') => esc = true,
            (false, '"') => return Some(out),
            (false, c) => out.push(c),
        }
    }
    None
}

pub(crate) fn card_foot(v: &TicketView, p: &Post, k: &Kit) -> El {
    let mut r = row().w_full().gap(8.0).center_y().pt(4.0);
    let t = v.id.clone();
    let mut under = None;
    let said = match foot(v, p) {
        Foot::Looking => "the minder is looking".to_string(),
        Foot::Left { undone: false } => "the minder looked · left with you".to_string(),
        Foot::Left { undone: true } => "you undid the minder's answer · left with you".to_string(),
        Foot::Answered(words, undo) => {
            let tu = t.clone();
            under = Some(
                col()
                    .w_full()
                    .gap(6.0)
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.sm)
                    .fill(k.c.layer_02)
                    .child(text(format!("the minder answered for you, to {}:", p.by)).font(k.ty.small).color(k.c.text_muted))
                    .child(text(words).font(k.ty.body).color(k.c.text_base).wrap())
                    .children(undo.then(|| {
                        row().w_full().justify_end().child(Button::new("Undo").variant(ButtonVariant::Neutral).on_press(move |s: &mut App| {
                            s.request_in(s.cur, Request::Undo { t: tu.clone() }, Pending::TicketDone { clear: Cleared::Quiet });
                        }).build(k))
                    })),
            );
            String::new()
        }
        Foot::Nothing => String::new(),
    };
    let sent = v.posts.iter().any(|q| q.answers == Some(p.n));
    let said = match (sent && p.asks_open, typed_ask(p)) {
        (true, true) => format!("your answer is being sent to {}", p.by),
        (true, false) => format!("your answers are being sent to {}", p.by),
        _ => said,
    };
    // Send N answers and nothing to answer are the box's actions at the foot
    r = r.child(text(said).font(k.ty.small).color(k.c.text_muted));
    // one shape whatever it holds, each part keyed, so what comes appears in place
    let (vn, pn) = (v.n, p.n);
    col()
        .key(format!("card-foot-{vn}-{pn}"))
        .instant_changes()
        .w_full()
        .gap(8.0)
        .children(under.map(|u| u.key(format!("card-minder-{vn}-{pn}"))))
        .child(r.key(format!("card-said-{vn}-{pn}")))
}

/// The scenes `thread-card` and `thread-card-send`: the card of post N on the shown thread filled from CARD
/// (`N: 1=puzzle; 2=mobile too`, asks counted from 1), as clicks on its options and typing in its lines leave it; with
/// SEND, what its Send button posts is sent, for real (the scratch copy only), and the reply returned.
pub fn scene_card(s: &mut App, card: &str, send: bool) -> Result<String, String> {
    let vn = s.tickets.shown.ok_or("no thread shown")?;
    let (pn, lines) = card.split_once(':').ok_or("CARD is `N: 1=text; 2=text`")?;
    let pn: u32 = pn.trim().parse().map_err(|_| "CARD's post number")?;
    for l in lines.split(';').filter(|l| !l.trim().is_empty()) {
        let (i, t) = l.split_once('=').ok_or("CARD is `N: 1=text; 2=text`")?;
        let i: usize = i.trim().parse().map_err(|_| "CARD's ask number")?;
        pick(s, vn, pn, i.saturating_sub(1), t.trim());
    }
    if !send {
        return Ok(String::new());
    }
    let v = s.proj().and_then(|p| p.live.tickets.get(&vn)).ok_or("no thread shown")?;
    let target = crate::message_box::Target::Thread { project: s.cur, t: v.id.clone(), post: Some(pn), way: crate::message_box::Way::Here };
    let req = crate::message_box::request(s, &target)?;
    eprintln!("sending: {}", serde_json::to_string(&req).unwrap_or_default());
    s.projects.get(s.cur).ok_or("no project")?.ask_once(&req)
}

/// The page's state.
#[derive(Default)]
pub struct Page {
    /// The open thread's rows, kept until it or the tickets move (threads.rs `thread_list`).
    pub thread_rows: Rows,
    /// The ticket shown, by number.
    pub shown: Option<u64>,
    /// Texts read by their place (`key`): None while asked.
    pub texts: HashMap<String, Option<Result<String, String>>>,
    /// Texts drawn as … not yet asked, each with the project of the pane that drew it (None: `cur`), asked at the next poll.
    pub unread: std::cell::RefCell<Vec<(Option<usize>, docket_machine::Loc)>>,
    pub kinds: Option<Result<Vec<KindForm>, String>>,
    pub kinds_asked: bool,
    /// The project KINDS was last asked of, by id (texts.rs `poll`): whose kinds a cloud's card names (cloud.rs `whose`).
    pub kinds_of: Option<String>,
    /// The open form shows, for this kind.
    pub opening: Option<String>,
    /// The open form's fields by name ("" the first post).
    pub fields: HashMap<String, TextEdit>,
    /// The owner's answer text, and its other fields, per ticket.
    pub answer: HashMap<u64, TextEdit>,
    pub answer_fields: HashMap<(u64, String), TextEdit>,
    /// The deliveries whose text is unfolded, by delivery id.
    pub unfolded: HashSet<String>,
    /// How a ticket's answers were read (`Request::Forming`), by ticket number, once asked for from its page.
    pub forming: HashMap<u64, Result<Vec<coord_types::FormingRead>, String>>,
    /// The program picture's docs (docket-ls): a name hovered shows its declaration and doc.
    pub docs: crate::docket_ls::Docs,
    /// The stages' words (the calls' and kinds' docs), by project id: read at the first picture, dropped at a reload.
    pub labels: std::sync::Mutex<HashMap<String, std::sync::Arc<crate::picture::Labels>>>,
    /// What the last open, answer or post said (Err: refused).
    pub said: Option<Result<String, String>>,
    /// The change box, per ticket (A4): the why, the program for Edit (from the printed continuation), the program
    /// that goes on in place of a withdrawn step.
    pub why: HashMap<u64, TextEdit>,
    pub edit: HashMap<u64, TextEdit>,
    pub withdraw: HashMap<u64, TextEdit>,
    /// The post being moved (ticket n, post n) and where to (a ticket id; empty: a thread of its own), the owner's Move.
    pub moving: Option<(u64, u32)>,
    /// The end boxes open on threads, by number (ending.rs).
    pub ending: HashMap<u64, crate::ending::Ending>,
    /// The post under the pointer (ticket n, post n), whose actions show at its right edge; the post whose "⋯" is open
    /// (subthread.rs `actions`).
    pub hover_post: Option<(u64, u32)>,
    /// What he has seen of each thread (seen.rs).
    pub seen: crate::seen::Seen,
    pub post_more: Option<(u64, u32)>,
    pub move_to: TextEdit,
    /// The Threads page (threads.rs): the messages folded to one line (kept while the window lives); the threads whose
    /// "how it runs" is open; the threads shown as the Tickets detail.
    pub folded: HashSet<(u64, u32)>,
    /// A thread's label being renamed in its head, by thread number (`retitle`).
    pub retitle: HashMap<u64, TextEdit>,
    pub how: HashSet<u64>,
    pub details: HashSet<u64>,
    /// The threads and tickets whose less-used header controls, or whose further details, are shown ("⋯", "more").
    pub more: HashSet<u64>,
    /// The open form's folds unfolded: "kinds-all" (the kinds the program opens itself), "first-post".
    pub open_folds: HashSet<String>,
    /// "Between agents" on the Threads list unfolded (design part 9b: folded at first).
    pub between: bool,
    /// "Archived" at the foot of the Threads list unfolded (folded at first).
    pub archived_open: bool,
    /// The ⋯ at the foot of the Threads list is open (Open a thread of a kind…).
    pub foot_menu: bool,
    /// The Threads list by time instead of the outline of open work (outline.rs), which comes first; With you's too.
    pub by_time: bool,
    /// "+ New thread" pressed on Threads: a narrow window shows the box to the project in the list's place, until
    /// "‹ Threads" or a thread picked (`set_picked`).
    pub new_box: bool,
    /// The Threads list's sections of tickets the program opened itself (builds), by kind, unfolded.
    pub made_open: HashSet<String>,
    /// A kind or a call's template edited, a ticket's step edited once (edit.rs).
    pub edits: crate::edit::State,
    /// Slow mode's edits being written (slow.rs).
    pub slow: crate::slow::State,
    /// The thread's lines that stand open, showing what hangs under them (ran.rs), by line key.
    pub ran: HashSet<String>,
    /// The opens of a program's picture shown open (picture.rs), by key.
    pub pic_open: HashSet<String>,
    /// The tickets whose program's picture is hidden beside their thread.
    pub pic_hidden: HashSet<u64>,
    /// A kinds file drawn on the Kinds page (a draft not installed; the scene's YAAIHH_PICTURE_FILE).
    pub picture_file: Option<String>,
    /// The find box, the hits and the starred list, the old tickets fetched for them (find.rs).
    pub find: crate::find::State,
}

/// The handler ticket ID running under V (design §13; `TicketView.handlers`, kept out of every list): for the clarify
/// chip, which answers its step.
pub fn handler<'a>(_p: &'a Proj, v: &'a TicketView, id: &str) -> Option<&'a TicketView> {
    v.handlers.iter().find(|h| h.id == id)
}

pub(crate) fn open(v: &TicketView) -> bool {
    !matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. } | ViewStatus::Resting)
}

/// A thread at its hole: open in the machine (posts land on it) but owing nothing, so not listed or counted as open.
pub(crate) fn resting(v: &TicketView) -> bool {
    matches!(v.status, ViewStatus::Resting)
}

/// Whether V is listed: not a one-off with no ticket (unless asks to him stay open), nor words, nor the owner model.
pub(crate) fn listed(v: &TicketView) -> bool {
    v.kind != "words" && v.kind != coord_types::OWNER_MODEL && v.kind != coord_types::OWNER_UPDATE && !v.archived && v.archived_under.is_none() && (!left_no_ticket(v) || crate::waits::asks_owner(v))
}

fn left_no_ticket(v: &TicketView) -> bool {
    v.kind == "thread" && matches!(&v.status, ViewStatus::Done { result } if result["head"] == "OneOff" || result["head"] == "Moved")
}

/// Ticket N opened on the Tickets page, from another view.
pub(crate) fn show(s: &mut App, n: u64) {
    s.sel = crate::Sel::Tickets;
    s.tickets.shown = Some(n);
    s.tickets.opening = None;
    s.tickets.said = None;
}

/// Thread V's heading: its label, coordd's, else the first line of its words, cut, else its id.
fn head_title(p: &Proj, v: &TicketView) -> String {
    let title = p.labels.get(&v.id).map(str::to_string).filter(|l| !l.is_empty()).unwrap_or_else(|| crate::refs::cut(&crate::sidebar::first_line(&v.title), 90));
    if title.is_empty() { v.id.clone() } else { title }
}

/// For a still: YAAIHH_RETITLE=ID opens that thread's label as a field in its head, as a click on it does.
pub fn retitle_scene(s: &mut App, pi: usize) {
    let Some((n, now)) = std::env::var("YAAIHH_RETITLE").ok().and_then(|id| {
        let p = s.projects.get(pi)?;
        let v = p.live.tickets.values().find(|v| v.id == id)?;
        Some((v.n, head_title(p, v)))
    }) else {
        return;
    };
    let mut e = TextEdit::new(&now);
    (e.anchor, e.focus, e.focused) = (0, now.len(), true);
    s.tickets.retitle.insert(n, e);
}

/// Thread V's label in its head: a click makes it a field, Enter sends `Request::Title`, Esc leaves it.
fn retitle(s: &App, p: &Proj, v: &TicketView, title: String, k: &Kit) -> El {
    let (n, pi) = (v.n, pi_of(s, p));
    let Some(e) = s.tickets.retitle.get(&n) else {
        return text(title.clone()).key(format!("title-{}", v.id)).font(k.ty.body_medium).color(k.c.text_base).wrap().shrink().min_w(0.0).cursor(rune::Cursor::Text).on_press(move |s: &mut App| {
            let mut e = TextEdit::new(&title);
            (e.anchor, e.focus, e.focused) = (0, title.len(), true);
            s.tickets.retitle.insert(n, e);
        });
    };
    let id = v.id.clone();
    let field = rune_kit::TextInput::new(e)
        .width(None)
        .on_edit(move |s: &mut App| s.tickets.retitle.entry(n).or_insert_with(|| TextEdit::new("")))
        .on_submit(move |s: &mut App, _| {
            let label = s.tickets.retitle.get(&n).map(|e| e.text.trim().to_string()).unwrap_or_default();
            if label.is_empty() {
                return;
            }
            s.request_in(pi, Request::Title { t: id.clone(), label: label.clone() }, Pending::TicketDone { clear: Cleared::Title(pi, id.clone(), label, n) });
        })
        .on_key(move |s: &mut App, key, _| {
            if key.key != rune::Key::Escape {
                return false;
            }
            s.tickets.retitle.remove(&n);
            true
        })
        .build(k);
    col().key(format!("retitle-{}", v.id)).grow(1.0).shrink().child(field)
}

/// A reply to one of the page's requests.
pub fn replied(s: &mut App, what: Pending, ok: bool, out: String) {
    match what {
        Pending::TicketText { key } => {
            s.tickets.texts.insert(key, Some(if ok { Ok(out) } else { Err(out) }));
        }
        Pending::TicketKinds => {
            s.tickets.kinds = Some(if ok { serde_json::from_str(&out).map_err(|e| format!("the kinds: {e}")) } else { Err(out) });
        }
        Pending::TicketDone { clear } => {
            if ok {
                match clear {
                    Cleared::Open => {
                        s.tickets.opening = None;
                        s.tickets.fields.clear();
                        if let Some(n) = out.trim().trim_start_matches("Opened ").trim_end_matches('.').rsplit('-').next().and_then(|n| n.parse().ok()) {
                            s.tickets.shown = Some(n);
                        }
                    }
                    Cleared::Answer(t) => {
                        s.tickets.answer.remove(&t);
                        s.tickets.answer_fields.retain(|(n, _), _| *n != t);
                    }
                    Cleared::Change(t) => {
                        s.tickets.why.remove(&t);
                        s.tickets.edit.remove(&t);
                        s.tickets.withdraw.remove(&t);
                    }
                    Cleared::Box(ref pi, ref place) => {
                        s.composers.remove(&(*pi, place.clone()));
                        crate::changed::sent(s, &(*pi, place.clone()));
                    }
                    // the box emptied at the press, its words in his bubble (outgoing.rs): what he typed since stays
                    Cleared::Out(ref pi, ref place) => {
                        crate::changed::sent(s, &(*pi, place.clone()));
                    }
                    Cleared::Ended(t) => crate::ending::close(s, t),
                    Cleared::Step(ref pi, ref place, t) => {
                        s.composers.remove(&(*pi, place.clone()));
                        crate::changed::sent(s, &(*pi, place.clone()));
                        s.tickets.answer_fields.retain(|(n, _), _| *n != t);
                    }
                    Cleared::Title(pi, ref t, ref label, n) => {
                        s.tickets.retitle.remove(&n);
                        if let Some(p) = s.projects.get_mut(pi) {
                            p.labels.set(t, label);
                        }
                    }
                    Cleared::Halt { ref id, tn, hn, step } => {
                        s.tickets.slow.editing.remove(&(tn, hn));
                        s.tickets.slow.rejecting.remove(&(tn, hn));
                        if step {
                            crate::slow::approve(s, &id.clone(), hn);
                        }
                    }
                    Cleared::Act | Cleared::Quiet | Cleared::Reload | Cleared::Once | Cleared::Nothing(..) | Cleared::TakeBack(..) | Cleared::Comment(..) => {}
                }
            }
            // his writing in that box, or in the comment that went there, which waited for this answer, said now (lib.rs
            // `writing_changed`)
            if let Cleared::Out(pi, place) | Cleared::Box(pi, place) | Cleared::Step(pi, place, _) | Cleared::Comment(pi, place) = &clear {
                s.writing_changed(*pi, place);
            }
            // his message's answer: on its bubble (sent, or not sent and why), said there once (outgoing.rs); when its
            // copy came first, the post says it, and only a refusal is said, under the box. A comment on a passage is
            // one of his messages from its press (ask.rs `send`), its answer on its bubble the same way
            if let Cleared::Out(pi, place) | Cleared::Comment(pi, place) = &clear {
                let what = if matches!(clear, Cleared::Comment(..)) { "a comment sent at its press to" } else { "send from the box of" };
                crate::log::line(&format!("{what} {place}: {} {}", if ok { "answered" } else { "refused" }, out.trim()));
                if !crate::outgoing::answered(s, &(*pi, place.clone()), ok, &out) && !ok {
                    s.box_said.insert((*pi, place.clone()), Err(out.clone()));
                }
                return;
            }
            // a box's answer under the box where he pressed, and in the log, so no press ends in silence
            if let Cleared::Box(pi, place) | Cleared::Nothing(pi, place) | Cleared::TakeBack(pi, place) | Cleared::Step(pi, place, _) = &clear {
                let what = match clear {
                    Cleared::Nothing(..) => "nothing to answer",
                    Cleared::TakeBack(..) => "take back",
                    Cleared::Step(..) => "a step's answer",
                    _ => "send",
                };
                crate::log::line(&format!("{what} from the box of {place}: {} {}", if ok { "answered" } else { "refused" }, out.trim()));
                // a take back pressed on his bubble (outgoing.rs): the bubble goes, or stays as the engine has its post
                if let Cleared::TakeBack(..) = clear {
                    crate::outgoing::took_back(s, &(*pi, place.clone()), ok, &out);
                }
                let said = if matches!(clear, Cleared::Step(..)) { answered_words(&out) } else { out.clone() };
                s.box_said.insert((*pi, place.clone()), if ok { Ok(said) } else { Err(said) });
                // said there once, not also in the page's line at the top
                return;
            }
            // a Save's answers go to the edit's own line
            if matches!(clear, Cleared::Reload | Cleared::Once) {
                crate::edit::replied(s, matches!(clear, Cleared::Reload), ok, &out);
                return;
            }
            // an override from the chat: its answer where the owner is, not only on the Tickets page
            if matches!(clear, Cleared::Act) {
                s.notice = Some(out.clone());
            }
            s.tickets.said = Some(if ok { Ok(out) } else { Err(out) });
        }
        _ => {}
    }
}

/// What to clear when a request is taken.
pub enum Cleared {
    Open,
    Answer(u64),
    /// A cancel, withdraw or edit of ticket N taken: its change box empties.
    Change(u64),
    /// One of the owner's overrides from the chat (Keep, DropThread, Move): nothing to empty; its answer is the notice.
    Act,
    /// "nothing to answer" on the box of a place (project, place key): nothing to empty; its answer is shown under it.
    Nothing(usize, String),
    /// The one message box of a place (project, place key) sent: it empties when taken; its answer is shown under it.
    Box(usize, String),
    /// His message from the box of a place (project, place key): the box emptied at the press and the message stands
    /// above it (outgoing.rs); its answer goes to that message.
    Out(usize, String),
    /// His post taken back, from its card (project, the thread's place key): nothing to empty; its answer is shown
    /// under the thread's box.
    TakeBack(usize, String),
    /// His step N answered from the box of a place (project, place key): the box and the step's fields empty when it
    /// is taken; its answer is shown under the box.
    Step(usize, String, u64),
    /// His comment on a passage, sent at its press to a place (project, place key; ask.rs `send`): nothing to empty, the
    /// place's box holding what he has begun there; its answer goes to the comment, his message on its way
    /// (outgoing.rs), as `Out`'s does.
    Comment(usize, String),
    /// coordd asked to read the kinds and calls again after a Save (edit.rs): its answer is added to the Save's line.
    Reload,
    /// A step's prompt edited once (edit.rs's once-panel): its box empties when it is taken.
    Once,
    /// Thread N ended (ending.rs): its end box closes.
    Ended(u64),
    /// A star, Undo: nothing to empty, the page shows the answer.
    Quiet,
    /// An edit or reject of halt HN of ticket ID (number TN): taken, its box closes (with STEP the halt goes on).
    Halt { id: String, tn: u64, hn: u32, step: bool },
    /// Thread T (number N) given LABEL in project P: its field closes and the label shows at once.
    Title(usize, String, String, u64),
}

/// The widest the ticket's reading column grows: about seventy characters of its body type, with its padding.
pub(crate) const READING_W: f32 = 700.0;

pub(crate) static CLOSE: Icon = Icon { name: "yaaihh-close", view: 16.0, body: r#"<path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"# };

/// What the composer's last say came to, above the reading column.
fn said_line(pg: &Page, k: &Kit) -> Option<El> {
    Some(row().px(24.0).pt(12.0).child(crate::message_box::answer_line(pg.said.as_ref()?, k)))
}

pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let pg = &s.tickets;
    // two panes: the threads list narrow, the open thread or ticket the rest
    let list = list(s, p, None, pg.shown, true, k);
    let shown = pg.shown.and_then(|n| crate::find::ticket(s, p, n));
    // a thread in its reading pane, as beside an agent's list (`reading`); else the form, the ticket or the box in the
    // reading column, centred in what it is given, the said line above it
    let key = || "ticket-detail".to_string();
    let reading = match (&pg.opening, shown) {
        (Some(kind), _) => column(s, open_form(pg, p.down.is_some(), kind, k), key(), k),
        // an old note is drawn as a thread too (design part 9b: one thread)
        (None, Some(v)) if (v.kind == "thread" || v.kind == "note") && !pg.details.contains(&v.n) => reading(s, p, v, key(), k),
        (None, Some(v)) => {
            let n = v.n;
            let back = (v.kind == "thread" || v.kind == "note").then(|| row().px(24.0).pt(12.0).child(Button::new("‹ the thread").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
                s.tickets.details.remove(&n);
            }).build(k)));
            column(s, col().w_full().children(back).child(detail(s, p, v, k)), key(), k)
        }
        // no thread open: the one box, to the project (the clerk files it), unless he chose otherwise
        (None, None) => {
            let default = crate::message_box::Target::Intake { project: s.cur };
            let target = crate::message_box::chosen(s, default.clone());
            let actions = crate::message_box::Actions { left: None, right: Vec::new(), send: Some("Send".into()), placeholder: "Write to the project…".into() };
            column(s, col().padding(24.0).gap(8.0).children(crate::outgoing::dock(s, s.cur, &target.place(), k)).child(crate::message_box::build(s, &target, &default, actions, k)).children(crate::message_box::said(s, s.cur, &target.place(), k)), key(), k)
        }
    };
    // a stage clicked, or a step picked in the whole program, opens a panel on the right inside the ticket, about
    // two fifths of it; closed, the reading column has the width. The whole program open apart has it beside it, there
    // only (popup.rs).
    let panel = match (&pg.opening, shown) {
        (None, Some(v)) if v.kind != "thread" && !crate::popup::shows(s, &crate::popup::What::Program { n: v.n }) => side_panel(s, p, v, k),
        _ => None,
    };
    let panel_col = |panel: El| col().w(0.0).grow(2.0).min_w(0.0).h_full().min_h(0.0).key("stage-panel").scroll(ScrollSpec { stick_bottom: false }).px(20.0).py(18.0).border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_weak).child(panel);
    // a narrow window shows one pane at a time: the list, else the ticket with a way back, else its stage
    if s.narrow.get() {
        if shown.is_none() && pg.opening.is_none() && !pg.new_box {
            return row().w_full().h_full().child(list.max_w(f32::INFINITY));
        }
        return col().w_full().h_full().min_h(0.0).child(back(pi_of(s, p), None, k)).child(row().w_full().grow(1.0).min_h(0.0).child(match panel {
            Some(panel) => panel_col(panel),
            None => reading,
        }));
    }
    let ticket = match panel {
        Some(panel) => row().w(0.0).grow(3.0).min_w(0.0).h_full().child(reading).child(panel_col(panel)),
        None => row().w(0.0).grow(3.0).min_w(0.0).h_full().child(reading),
    };
    row().w_full().h_full().child(list).child(ticket)
}

/// The tickets down V's tree, V first (its children by id, then theirs).
fn tree_of<'a>(p: &'a Proj, v: &'a TicketView) -> Vec<&'a TicketView> {
    let mut out = vec![v];
    let mut i = 0;
    while i < out.len() {
        let kids: Vec<&TicketView> = out[i].children.iter().filter_map(|id| p.live.tickets.values().find(|t| &t.id == id)).collect();
        out.extend(kids);
        i += 1;
    }
    out
}

/// A ticket an event's binding opened: no parent and no opener (a build); the program's, not the owner's.
pub(crate) fn made(v: &TicketView) -> bool {
    v.parent.is_none() && v.opener.is_none() && !matches!(v.kind.as_str(), "thread" | "note")
}

/// The ticket ids an answer HEAD names: its fields whose value is an id or a list of them (`carries`).
fn head_ids(h: &Value) -> Vec<String> {
    let id = |x: &Value| x.as_str().filter(|s| s.split_once('-').is_some_and(|(a, n)| !a.is_empty() && a.chars().all(|c| c.is_ascii_uppercase()) && n.parse::<u64>().is_ok())).map(str::to_string);
    let Some(o) = h.as_object() else { return Vec::new() };
    o.iter().filter(|(k, _)| *k != "head").flat_map(|(_, x)| match x {
        Value::Array(xs) => xs.iter().filter_map(id).collect::<Vec<_>>(),
        x => id(x).into_iter().collect(),
    }).collect()
}

/// What stands in the way down B's tree: a ticket stopped, else one closed failed; with it, in words.
fn trouble<'a>(p: &'a Proj, b: &'a TicketView) -> Option<(&'a TicketView, String)> {
    let tree = tree_of(p, b);
    if let Some((x, raised, to)) = tree.iter().find_map(|x| match &x.status {
        ViewStatus::Stopped { raised, to, .. } => Some((*x, raised, to)),
        _ => None,
    }) {
        let who = if to == "owner" { crate::refs::person(to) } else { Some(crate::refs::agent_label(p, to)).filter(|l| !l.is_empty()).map_or_else(|| to.clone(), |l| format!("{to} ({l})")) };
        return Some((x, format!("stopped ({}); {who} to say whether it runs again", raised.exc)));
    }
    tree.iter().find_map(|x| match &x.status {
        ViewStatus::Failed { raised } => Some((*x, format!("closed failed ({})", raised.exc))),
        _ => None,
    })
}

/// Where change V waits on a build that is stopped or failed: the build, the ticket, and what happened and who has it.
pub(crate) fn blocked<'a>(p: &'a Proj, v: &'a TicketView) -> Option<(&'a TicketView, &'a TicketView, String)> {
    let mine: Vec<&str> = tree_of(p, v).iter().map(|t| t.id.as_str()).collect();
    let builds = p.live.tickets.values().filter(|b| made(b) && b.id != v.id && b.posts.iter().any(|q| q.head.as_ref().is_some_and(|h| head_ids(h).iter().any(|i| mine.contains(&i.as_str())))));
    for b in builds {
        if let Some((x, words)) = trouble(p, b) {
            return Some((b, x, words));
        }
    }
    None
}

/// Change V's line for a build of its in trouble (`blocked`): "waits on" the build, the ticket that stopped or
/// failed, and what happened, each ticket by its label and id.
pub(crate) fn blocked_line(p: &Proj, v: &TicketView, k: &Kit) -> Option<El> {
    if v.closed_at.is_some() {
        return None;
    }
    let (b, x, words) = blocked(p, v)?;
    let link = |t: &TicketView| {
        let to = t.n;
        k.interactive(crate::refs::el(&crate::refs::ticket(Some(p), t), k.ty.small_medium, k.c.warning.fg, k)).on_press(move |s: &mut App| s.tickets.shown = Some(to))
    };
    let mut r = row().w_full().gap(6.0).flex_wrap().center_y().child(text("Waits on".to_string()).font(k.ty.small_medium).color(k.c.warning.fg)).child(link(b));
    if x.id != b.id {
        r = r.child(text("·".to_string()).font(k.ty.small).color(k.c.text_faint)).child(link(x));
    }
    Some(r.child(text(words).font(k.ty.small_medium).color(k.c.warning.fg).wrap()))
}

/// The stops down V's tree passed to the owner: each stopped ticket with the hidden ticket that asks him.
pub(crate) fn owners_stops<'a>(p: &'a Proj, v: &'a TicketView) -> Vec<(&'a TicketView, &'a TicketView)> {
    tree_of(p, v).into_iter().filter(|x| matches!(&x.status, ViewStatus::Stopped { to, .. } if to == "owner")).filter_map(|x| {
        let ViewStatus::Stopped { by, .. } = &x.status else { return None };
        x.handlers.iter().find(|h| &h.id == by).map(|h| (x, h))
    }).collect()
}

/// The agents' questions about V (`ask` tickets opened under it), oldest first.
pub(crate) fn asks_on<'a>(p: &'a Proj, v: &TicketView) -> Vec<&'a TicketView> {
    let mut out: Vec<&TicketView> = p.live.tickets.values().filter(|a| a.kind == "ask" && a.under.as_deref() == Some(v.id.as_str())).collect();
    out.sort_by_key(|a| a.opened_at.to_string());
    out
}

/// The one message box answers every step a thread owes him; no step card has a text box of its own.
#[cfg(test)]
mod aim_tests {
    use crate::App;
    use crate::message_box::{Target, Way};
    use serde_json::json;

    fn at(m: u32) -> String {
        format!("2026-11-19T09:{m:02}:00Z")
    }

    /// Thread N-450 owing him two steps (its own and N-451's pick), and question N-453 asked by ask N-452.
    pub(super) fn two_steps() -> App {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let awaits = |call: &str, head: serde_json::Value| json!({"is": "awaits", "agent": "owner", "call": call, "step": 1, "since": at(3), "taken": false, "refusals": 0, "forming": false, "head": head, "args": {}});
        let t = |v: serde_json::Value| -> docket_machine::TicketView {
            let mut base = json!({"ctx": {}, "form": {"to": "paper-research"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [], "posts": []});
            base.as_object_mut().unwrap().extend(v.as_object().unwrap().clone());
            serde_json::from_value(base).unwrap()
        };
        let thread = t(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "title": "Windows window",
            "status": awaits("decide", json!({"Variant": [["Go", []], ["Hold", [["why", "Text"]]]]})),
            "waits_on": [{"ticket": "N-450", "agent": "owner", "call": "decide"}, {"ticket": "N-451", "agent": "owner", "call": "pick"}],
            "children": ["N-451", "N-452"],
            "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box, and which stills", "names": ["paper-research"], "told": ["paper-research"]}]}));
        let pick = t(json!({"id": "N-451", "n": 451, "kind": "pick", "opener": "paper-research", "title": "Which stills", "parent": "N-450", "under": "N-450",
            "status": awaits("pick", json!({"Variant": [["Wide", []], ["Narrow", []]]}))}));
        let ask = t(json!({"id": "N-452", "n": 452, "kind": "ask", "opener": "paper-research", "title": "The mic", "under": "N-450", "children": ["N-453"],
            "form": {"text": "Should the comment box show the mic?"}, "status": {"is": "resting"}}));
        let question = t(json!({"id": "N-453", "n": 453, "kind": "question", "opener": "paper-research", "title": "The mic", "parent": "N-452",
            "form": {"ask": "Should the comment box show the mic?"},
            "status": awaits("answer", json!({"Variant": [["Answered", [["text", "Text"]]], ["Dismissed", [["why", "Text"]]]]}))}));
        for (seq, v) in [thread, pick, ask, question].into_iter().enumerate() {
            p.live.apply(coord_types::Event { seq: seq as u64 + 1, body: coord_types::Body::Ticket(v) });
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(450);
        s
    }

    fn texts(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect()
    }

    fn settle(rt: &mut rune::runtime::Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    /// A press on a step's card answers it in the one box: the box aimed at it, with the keyboard.
    #[test]
    fn a_press_on_a_step_aims_the_box_at_it() {
        let mut rt = crate::view_cost::window(two_steps());
        settle(&mut rt);
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("step-N-451")).expect("the pick's card").rect;
        // its foot's empty left end: its reference and its own button keep their presses
        let (x, y) = (r[0] + 6.0, r[1] + r[3] - 6.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        let place = crate::message_box::thread_place("N-450");
        assert!(matches!(rt.state.box_targets.get(&(0, place.clone())), Some(Target::Thread { way: Way::Step { s }, .. }) if s == "N-451"), "{:?}", rt.state.box_targets);
        assert!(rt.state.composers.get(&(0, place)).is_some_and(|e| e.focused), "the box has the keyboard");
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Wide") && t.iter().any(|x| x == "Narrow"), "its answers beside the box: {t:?}");
    }

    #[test]
    fn a_second_step_and_a_question_a_step_asked_are_answered_in_the_one_box() {
        let mut rt = crate::view_cost::window(two_steps());
        settle(&mut rt);
        let t = texts(&rt);
        // aimed at his first step by default: its answers beside the box; no step card has a text box of its own
        assert!(t.iter().any(|x| x == "Go") && t.iter().any(|x| x == "Hold"), "{t:?}");
        assert!(!t.iter().any(|x| x.starts_with("Your answer: why") || x == "Why, if you end it: one sentence the record keeps"), "a card's own box: {t:?}");
        let presses = rt.elements().into_iter().filter(|e| e.role == "button" && (e.text.as_deref() == Some("Answer it in the box") || e.label.as_deref() == Some("Answer it in the box"))).count();
        assert_eq!(presses, 2, "the pick and the question, each with its press: {t:?}");
        // the second step aimed at: its answers come beside the box, the first step's go
        rt.update(|s, _| crate::message_box::choose(s, Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Step { s: "N-451".into() } }));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Wide") && t.iter().any(|x| x == "Narrow") && !t.iter().any(|x| x == "Go"), "{t:?}");
        // the question a step asked, where it stands in the thread: aimed at too, not taken for a stale choice
        rt.update(|s, _| crate::message_box::choose(s, Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Step { s: "N-453".into() } }));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Answered") && t.iter().any(|x| x == "Dismissed"), "{t:?}");
        assert!(t.iter().any(|x| x == "answered in the box below"), "{t:?}");
    }

    /// The comment box is the one box: where speech is built (the train's exe), it has the mic as every box does. A
    /// build without speech cannot show it; there the test is ignored.
    #[test]
    #[cfg_attr(not(feature = "speech-engine"), ignore = "speech is not built in")]
    fn the_comment_box_has_the_mic_where_speech_is_built() {
        let mut s = two_steps();
        let mut rt = crate::view_cost::window({
            let v = s.projects[0].live.tickets.get(&450).cloned().unwrap();
            let c = super::scene_comment(&s, &v, 1, "which stills", "").expect("the passage");
            crate::ask::write(&mut s, crate::ask::Picked { target: "thread:N-450".into(), source: c.source, label: c.label, quote: c.quote, at: c.at, pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() }, "");
            s
        });
        settle(&mut rt);
        assert!(crate::speech::shown(), "speech is built in");
        let mics = rt.elements().into_iter().filter(|e| e.role == "button" && e.label.as_deref() == Some("Speak")).count();
        assert_eq!(mics, 2, "the thread's box and the comment box each have the mic");
    }

    /// A still (STILL_OUT): the thread with two steps, the second aimed at the box.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn two_steps_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        std::fs::create_dir_all(&out).unwrap();
        fn second() -> App {
            let mut s = two_steps();
            crate::message_box::choose(&mut s, Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Step { s: "N-451".into() } });
            s
        }
        let path = std::path::Path::new(&out).join("two-steps.png").display().to_string();
        let args = vec!["--snap".to_string(), path, "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), second, crate::view) }).args(args).try_run().unwrap();
    }
}

#[cfg(test)]
mod answer_tests {
    //! His questions answered in place under each question and sent as one message.
    use super::*;
    use crate::message_box::{answer_place, Target};
    use serde_json::{Value, json};

    const WORDS: &str = "Which genre first? Orbit or puzzle? And the stills, now or later?";
    const ASKS: [&str; 3] = ["Which genre first?", "Orbit or puzzle?", "And the stills, now or later?"];

    fn at(m: u32) -> String {
        format!("2026-11-20T09:{m:02}:00Z")
    }

    /// Thread N-7: his question, then paper-research's post 2 asking him three things.
    fn thread() -> TicketView {
        let ask = |q: &str| {
            let i = WORDS.find(q).unwrap();
            json!({"at": [i, i + q.len()]})
        };
        thread_of(WORDS, ASKS.iter().map(|q| ask(q)).collect())
    }

    /// One made-up question with three choices.
    const CHOICE: &str = "Which mark for a message that waits? sending, not sent, or no bubble at all";
    /// Two, each with two choices.
    const CHOICES: &str = "Which mark for a message that waits? sending or no bubble at all\nAnd the stills: today or tomorrow?";

    /// Thread N-7 with post 2's words W asking him each question of QS (its first line of the asked words, and the
    /// choices in it).
    fn choosing(w: &str, qs: &[(&str, Vec<&str>)]) -> TicketView {
        let at = |x: &str| {
            let i = w.find(x).unwrap();
            [i, i + x.len()]
        };
        thread_of(w, qs.iter().map(|(q, os)| json!({"at": at(*q), "options": os.iter().map(|o| at(*o)).collect::<Vec<_>>()})).collect())
    }

    /// Thread N-7: his question, then paper-research's post 2, words W, asking him ASKS.
    fn thread_of(w: &str, asks: Vec<Value>) -> TicketView {
        serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"}, "title": "genres",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 2, "opened_at": at(0), "given": [],
            "posts": [{"n": 1, "at": at(0), "by": "owner", "text": "which next?", "names": ["paper-research"], "told": ["paper-research"]},
                      {"n": 2, "at": at(1), "by": "paper-research", "text": w, "told": ["owner"], "asks": asks, "asks_open": true}],
            "waits_on": [{"ticket": "N-7", "agent": "owner", "call": "asks", "post": 2}]}))
        .unwrap()
    }

    /// The window on thread N-7, its requests written to OUT.
    fn app(out: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        app_on(out, thread())
    }

    /// The window on thread V (N-7), its requests written to OUT.
    fn app_on(out: &std::path::Path, v: TicketView) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.tickets.insert(7, v);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        // the coord's cleaning of his words is not what these read: a Send posts at once (words.rs `wanted`)
        s.spell.on = false;
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        (held, s, child)
    }

    /// The posts the window sent, in order.
    fn posts(s: &mut App, child: std::process::Child, out: &std::path::Path) -> Vec<Value> {
        asked(s, child, out).into_iter().filter(|r| r["op"] == "post").collect()
    }

    /// The requests the window sent, in order.
    fn asked(s: &mut App, child: std::process::Child, out: &std::path::Path) -> Vec<Value> {
        s.projects[0].link = None;
        let mut child = child;
        let _ = child.wait();
        std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|v| v["req"].clone()).collect()
    }

    fn out(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yaaihh-answers-{name}-{}.jsonl", std::process::id()))
    }

    /// His words W typed in question I's box.
    fn write_in(s: &mut App, i: usize, w: &str) {
        let mut e = TextEdit::new(w);
        e.focused = true;
        e.set_caret(w.len());
        s.composers.insert((0, answer_place("N-7", 2, i)), e);
    }

    /// The post that answers post 2, as coordd records it once the message is taken (asks closed).
    fn answered(s: &mut App, text: &str) {
        let v = s.projects[0].live.tickets.get_mut(&7).unwrap();
        v.posts[1].asks_open = false;
        v.posts.push(serde_json::from_value(json!({"n": 3, "at": at(5), "by": "owner", "text": text, "answers": 2})).unwrap());
        v.waits_on.clear();
    }

    /// The words drawn, an element playing its exit left out (rune lists it until the exit ends).
    fn texts(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
    }

    fn settle(rt: &mut rune::runtime::Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    fn click(rt: &mut rune::runtime::Runtime<App>, r: [f32; 4]) {
        press(rt, r[0] + 12.0, r[1] + 10.0);
    }

    fn press(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
    }

    /// The tooltip rune shows with the pointer come from away and resting at (X, Y).
    fn tip_at(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) -> Option<String> {
        rt.pointer_move(1.0, 1.0);
        rt.frame(0.016);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        let wait = rt.help_delay + 0.1;
        rt.frame(wait);
        rt.frame(0.3);
        rt.help_tooltip().map(|(t, _)| t.to_owned())
    }

    fn rect(rt: &rune::runtime::Runtime<App>, key: &str) -> [f32; 4] {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} drawn")).rect
    }

    /// Three questions: he presses the first and writes, presses the third and writes, and one Send sends one post
    /// answering post 2 to its asker, each answer under its question's quote, the second left out; drawn after, the
    /// first and third show his answers and the second `not answered`.
    #[test]
    fn two_of_three_questions_answered_in_place_go_as_one_message() {
        let file = out("one");
        let (_held, s, child) = app(&file);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        // a press on the first question opens the one box under it
        let r = rect(&rt, "ask-N-7-2-0");
        click(&mut rt, r);
        assert_eq!(rt.state.answering, Some((0, "N-7".to_string(), 2, 0)), "the first question's box is open");
        settle(&mut rt);
        rect(&rt, "answer-box-N-7-2-0");
        rt.update(|s, _| write_in(s, 0, "orbit"));
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|t| t == "Send 1 answer to paper-research"), "the button names whom it goes to: {:?}", texts(&rt));
        // the third: its box opens, the first keeps his words under its question
        let r = rect(&rt, "ask-N-7-2-2");
        click(&mut rt, r);
        assert_eq!(rt.state.answering, Some((0, "N-7".to_string(), 2, 2)));
        rt.update(|s, _| write_in(s, 2, "later, after the build"));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "your answer · not sent: orbit"), "{t:?}");
        assert!(t.iter().any(|x| x == "Send 2 answers to paper-research"), "{t:?}");
        assert!(!rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some("answer-box-N-7-2-0")), "one box open at a time");
        // Esc closes the box, his words stay
        rt.update(|s, _| s.answering = None);
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "your answer · not sent: later, after the build"));
        // one Send
        rt.update(|s, _| crate::message_box::send(s, &Target::Answer { project: 0, t: "N-7".into(), post: 2, ask: 2 }));
        assert!(!rt.state.composers.keys().any(|(_, p)| p.starts_with("answer:")), "the answers went into the one message");
        let sent = posts(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "one message: {sent:?}");
        let want = format!("> {}\n\norbit\n\n> {}\n\nlater, after the build", ASKS[0], ASKS[2]);
        assert_eq!(sent[0]["text"], want.as_str());
        assert_eq!(sent[0]["answers"], 2, "it answers post 2");
        assert_eq!(sent[0]["to"], "paper-research");
        assert!(!sent[0]["text"].as_str().unwrap().contains(ASKS[1]), "the question he left is not quoted");
        // drawn once coordd has it: the first and third answered, the second not
        rt.update(|s, _| answered(s, &want));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "orbit") && t.iter().any(|x| x == "later, after the build"), "{t:?}");
        assert_eq!(t.iter().filter(|x| *x == "not answered").count(), 1, "{t:?}");
        let _ = std::fs::remove_file(&file);
    }

    /// Words in the thread's own box go first, then the answers; a chip goes into its question's box.
    #[test]
    fn the_threads_words_go_first_and_a_chip_answers_its_question() {
        let file = out("words");
        let (_held, mut s, child) = app(&file);
        s.composers.insert((0, crate::message_box::thread_place("N-7")), TextEdit::new("Thanks, both are good."));
        let vn = 7;
        pick(&mut s, vn, 2, 1, "puzzle");
        assert_eq!(answer_draft(&s, 0, "N-7", 2, 1), "puzzle", "the chip's words are that question's answer");
        assert_eq!(s.answering, None, "no box opened: the click is the answer (N-1104 p501)");
        assert!(!s.composers.values().any(|e| e.focused), "the keyboard left where it was");
        crate::message_box::send(&mut s, &Target::Thread { project: 0, t: "N-7".into(), post: Some(2), way: crate::message_box::Way::Here });
        let sent = posts(&mut s, child, &file);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0]["text"], format!("Thanks, both are good.\n\n> {}\n\npuzzle", ASKS[1]).as_str());
        assert_eq!(sent[0]["answers"], 2);
        let _ = std::fs::remove_file(&file);
    }

    /// Writing is said from the box: on at his first word, off once emptied and the on (or his post) is answered.
    #[test]
    fn his_writing_on_a_thread_is_said_from_its_box_and_its_off_goes_after_his_post() {
        let file = out("writing");
        let (_held, s, child) = app(&file);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let at = (0usize, crate::message_box::thread_place("N-7"));
        // coordd's answer to the window's request on its way that WHAT picks out
        let answer = |rt: &mut rune::runtime::Runtime<App>, what: &dyn Fn(&crate::Pending) -> bool| {
            let id = rt.state.projects[0].pending.iter().find(|(_, w)| what(w)).map(|(id, _)| *id).expect("a request on its way");
            rt.state.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out: "Posted on N-7 as post 3.".into(), post: None }));
        };
        let writing = |w: &crate::Pending| matches!(w, crate::Pending::Writing { .. });
        let post = |w: &crate::Pending| matches!(w, crate::Pending::TicketDone { clear: Cleared::Out(..) });
        let field = rt.elements().into_iter().find(|e| e.role == "textbox" && e.path.contains("composer-0-thread:N-7")).map(|e| e.rect).expect("the thread's box");
        click(&mut rt, field);
        crate::view_cost::type_in(&mut rt, "ok");
        assert_eq!(rt.state.composers.get(&at).map(|e| e.text.as_str()), Some("ok"), "typed in the thread's box");
        assert!(rt.state.writing.contains(&at), "said on at his first word");
        // emptied before coordd answered the on: the off waits for that answer
        crate::view_cost::press(&mut rt, rune::Key::Backspace);
        crate::view_cost::press(&mut rt, rune::Key::Backspace);
        assert!(rt.state.writing.contains(&at), "the off waits for the on's answer");
        answer(&mut rt, &writing);
        assert!(!rt.state.writing.contains(&at), "then it is said");
        answer(&mut rt, &writing);
        // written again and sent: no off while his post is on its way
        crate::view_cost::type_in(&mut rt, "ship it");
        answer(&mut rt, &writing);
        rt.update(|s, _| crate::message_box::send(s, &Target::Thread { project: 0, t: "N-7".into(), post: None, way: crate::message_box::Way::Here }));
        assert!(rt.state.composers.get(&at).is_none_or(|e| e.text.is_empty()), "the box emptied at Send");
        assert!(rt.state.writing.contains(&at) && !rt.state.projects[0].pending.values().any(writing), "no off before coordd has his post");
        answer(&mut rt, &post);
        assert!(!rt.state.writing.contains(&at), "said off once coordd has answered his post");
        answer(&mut rt, &writing);
        // an answer typed under a question: writing on the thread
        let r = rect(&rt, "ask-N-7-2-1");
        click(&mut rt, r);
        settle(&mut rt);
        crate::view_cost::type_in(&mut rt, "puzzle");
        assert_eq!(rt.state.composers.get(&(0, answer_place("N-7", 2, 1))).map(|e| e.text.as_str()), Some("puzzle"), "typed under the second question");
        assert!(rt.state.writing.contains(&at), "said on for the thread");
        let asked = asked(&mut rt.state, child, &file);
        let said: Vec<String> = asked
            .iter()
            .filter(|r| r["op"] == "writing" || r["op"] == "post")
            .map(|r| if r["op"] == "post" { "post".to_string() } else { format!("{} {}", r["thread"].as_str().unwrap_or("no thread"), r["on"]) })
            .collect();
        assert_eq!(said, ["N-7 true", "N-7 false", "N-7 true", "post", "N-7 false", "N-7 true"], "{asked:?}");
        assert!(asked.iter().filter(|r| r["op"] == "writing").all(|r| r["agent"].as_str().unwrap_or("") == ""), "to no agent: {asked:?}");
        let _ = std::fs::remove_file(&file);
    }

    /// The chip of choice W of question I as drawn: its rect, its accessible name (`W, your answer` once picked) and
    /// its tooltip (what a click does, until it is his answer).
    fn chip(rt: &rune::runtime::Runtime<App>, i: usize, w: &str) -> ([f32; 4], String, Option<String>) {
        let key = format!("7|2|{i}|{w}");
        let e = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key.as_str())).unwrap_or_else(|| panic!("{key} drawn"));
        (e.rect, e.label.unwrap_or_default(), e.help)
    }

    /// One click on a choice is his answer at once: one post answering post 2, no box opened, no text selected.
    #[test]
    fn one_click_on_a_choice_is_his_answer_at_once() {
        let file = out("click");
        let (_held, s, child) = app_on(&file, choosing(CHOICE, &[(CHOICE, vec!["sending", "not sent", "no bubble at all"])]));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let (r, name, help) = chip(&rt, 0, "no bubble at all");
        assert_eq!(name, "no bubble at all", "not picked yet");
        assert_eq!(help.as_deref(), Some("Sends this as your answer"), "the chip says a click sends it");
        // inside the chip, before its label's first glyph: rune's link takes its whole chip
        press(&mut rt, r[0] + 3.0, r[1] + r[3] * 0.5);
        assert_eq!(rt.state.answering, None, "no box opened");
        assert!(!rt.state.composers.values().any(|e| e.focused), "no box has the keyboard");
        assert_eq!(rt.selected_text(), None, "no text selected");
        settle(&mut rt);
        assert!(!rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some("answer-box-N-7-2-0")), "the question's box stays shut");
        assert_eq!(chip(&rt, 0, "no bubble at all").1, "no bubble at all, your answer", "picked while on its way");
        let (r, _, help) = chip(&rt, 0, "no bubble at all");
        assert_eq!(help.as_deref(), Some("Your answer"), "his answer: a tooltip of its own, not what a click does");
        assert_eq!(tip_at(&mut rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5).as_deref(), Some("Your answer"), "the question's tooltip never shows over his answer");
        // on its way, the question offers no box for a second answer and shows his with its message's mark
        let b = rect(&rt, "ask-N-7-2-0");
        let inside = |r: [f32; 4]| r[1] >= b[1] && r[1] + r[3] <= b[1] + b[3];
        let in_box: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && inside(e.rect)).filter_map(|e| e.text).collect();
        assert!(in_box.iter().any(|x| x == "sending…") && in_box.iter().any(|x| x == "no bubble at all"), "{in_box:?}");
        assert_eq!(tip_at(&mut rt, b[0] + b[2] - 6.0, b[1] + b[3] - 4.0), None, "no `Answer this question here`");
        press(&mut rt, b[0] + b[2] - 6.0, b[1] + b[3] - 4.0);
        settle(&mut rt);
        assert_eq!(rt.state.answering, None, "a press on the question opens no box");
        assert_eq!(chip(&rt, 0, "sending").1, "sending", "the others not");
        // a second click on it sends nothing more, and writes nothing under the question
        let (r, _, _) = chip(&rt, 0, "no bubble at all");
        press(&mut rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        assert_eq!(answer_draft(&rt.state, 0, "N-7", 2, 0), "", "nothing written under it");
        let sent = posts(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "one message, at once: {sent:?}");
        let want = format!("> {CHOICE}\n\nno bubble at all");
        assert_eq!(sent[0]["text"], want.as_str());
        assert_eq!(sent[0]["answers"], 2, "it answers post 2");
        assert_eq!(sent[0]["to"], "paper-research");
        // the engine has it: his answer is the post's, and the choice still shows as his
        rt.update(|s, _| {
            answered(s, &want);
            s.outgoing.list.clear();
        });
        settle(&mut rt);
        assert_eq!(chip(&rt, 0, "no bubble at all").1, "no bubble at all, your answer", "picked once answered");
        let _ = std::fs::remove_file(&file);
    }

    /// An answer the engine refuses after his click is no longer his: the chip plain again, the message `not sent`.
    #[test]
    fn an_answer_refused_after_the_click_is_his_no_more_and_its_choice_clicks_again() {
        let file = out("refused");
        let (_held, s, child) = app_on(&file, choosing(CHOICE, &[(CHOICE, vec!["sending", "not sent", "no bubble at all"])]));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let (r, _, _) = chip(&rt, 0, "no bubble at all");
        press(&mut rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        settle(&mut rt);
        assert_eq!(chip(&rt, 0, "no bubble at all").1, "no bubble at all, your answer", "on its way");
        // his other window answered first, and the engine refuses this one
        let why = "post 2 no longer asks you: you answered it in #3";
        rt.update(|s, _| {
            answered(s, &format!("> {CHOICE}\nsending"));
            let key = s.outgoing.list[0].key.clone();
            assert!(crate::outgoing::answered(s, &key, false, &format!("not sent: {why}")), "the engine's answer is this message's");
        });
        settle(&mut rt);
        assert_eq!(chip(&rt, 0, "no bubble at all").1, "no bubble at all", "refused: not his answer");
        assert_eq!(chip(&rt, 0, "no bubble at all").2, None, "and no tooltip of a lit chip");
        assert_eq!(chip(&rt, 0, "sending").1, "sending, your answer", "the answer the post got");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x.ends_with(&format!("not sent · {why}"))), "his message, refused, with why: {t:?}");
        assert!(t.iter().any(|x| x == "Send again"), "{t:?}");
        assert!(!t.iter().any(|x| x == "sending…"), "nothing of his on its way: {t:?}");
        // a click on his choice: in the thread's box under the question's quote; nothing goes by itself
        let (r, _, _) = chip(&rt, 0, "no bubble at all");
        press(&mut rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        let boxed = rt.state.composers.get(&(0, crate::message_box::thread_place("N-7"))).map(|e| e.text.clone()).unwrap_or_default();
        assert_eq!(boxed, format!("> {CHOICE}\n\nno bubble at all"), "answering post 2 again, from the thread's box");
        let sent = posts(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "only the click's first message: {sent:?}");
        let _ = std::fs::remove_file(&file);
    }

    /// Two questions with choices: a click picks, and nothing goes until his Send (the engine closes a post's asks at
    /// its first answer); the pick stands under its question, the box's Send counts it, and one Send sends both.
    #[test]
    fn with_two_questions_a_click_picks_and_send_sends_them() {
        let file = out("picks");
        let lines: Vec<&str> = CHOICES.lines().collect();
        let (_held, s, child) = app_on(&file, choosing(CHOICES, &[(lines[0], vec!["sending", "no bubble at all"]), (lines[1], vec!["today", "tomorrow"])]));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let (r, _, help) = chip(&rt, 0, "no bubble at all");
        assert_eq!(help.as_deref(), Some("Picks this as your answer: Send sends it"), "the chip says a click waits for Send");
        press(&mut rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        settle(&mut rt);
        assert_eq!(answer_draft(&rt.state, 0, "N-7", 2, 0), "no bubble at all", "picked as the first question's answer");
        assert_eq!(rt.state.answering, None, "no box opened");
        assert!(!rt.state.composers.values().any(|e| e.focused), "no box has the keyboard");
        assert_eq!(chip(&rt, 0, "no bubble at all").1, "no bubble at all, your answer");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "your answer · not sent: no bubble at all"), "{t:?}");
        assert!(t.iter().any(|x| x == "Send 1 answer to paper-research"), "{t:?}");
        let (r, _, _) = chip(&rt, 1, "tomorrow");
        press(&mut rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Send 2 answers to paper-research"), "{:?}", texts(&rt));
        rt.update(|s, _| crate::message_box::send(s, &Target::Thread { project: 0, t: "N-7".into(), post: Some(2), way: crate::message_box::Way::Here }));
        let sent = posts(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "one message, at his Send: {sent:?}");
        assert_eq!(sent[0]["text"], format!("> {}\n\nno bubble at all\n\n> {}\n\ntomorrow", lines[0], lines[1]).as_str());
        assert_eq!(sent[0]["answers"], 2);
        let _ = std::fs::remove_file(&file);
    }

    /// The state changes between his writing and his Send: the question is answered from elsewhere (his other window),
    /// or the post is taken back. His words are not dropped: `not sent`, one muted line saying why, and `Send as a
    /// message`, which sends them as words on the thread.
    #[test]
    fn an_answer_whose_question_closed_before_send_stays_with_why() {
        let file = out("closed");
        let (_held, mut s, child) = app(&file);
        write_in(&mut s, 1, "puzzle, then orbit");
        s.answering = None;
        // answered from his other window, before he pressed Send here
        answered(&mut s, &format!("> {}\narcade", ASKS[0]));
        assert!(!has_answers(&s, 0, "N-7", 2), "nothing goes as post 2's answers now");
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "your answer · not sent: puzzle, then orbit"), "{t:?}");
        assert!(t.iter().any(|x| x == "it can no longer go as this question's answer: you answered it in #3"), "{t:?}");
        assert!(t.iter().any(|x| x == "Send as a message"), "{t:?}");
        // Send from the thread's box sends nothing of it as an answer
        rt.update(|s, _| send_answers(s, 0, "N-7", 2));
        assert_eq!(answer_draft(&rt.state, 0, "N-7", 2, 1), "puzzle, then orbit", "kept");
        // taken back: the post leaves the view; the answer stands above the thread's box, saying why
        rt.update(|s, _| {
            let v = s.projects[0].live.tickets.get_mut(&7).unwrap();
            v.posts.retain(|q| q.n != 2 && q.n != 3);
        });
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "it can no longer go as an answer: post 2 it answered was taken back"), "{t:?}");
        assert!(t.iter().any(|x| x == "your answer · not sent: puzzle, then orbit"), "{t:?}");
        // Send as a message: his words go as words on the thread, at no post
        rt.update(|s, _| send_as_message(s, 0, "N-7", 2, 1));
        assert_eq!(answer_draft(&rt.state, 0, "N-7", 2, 1), "", "sent, gone from under the question");
        let sent = posts(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0]["text"], "puzzle, then orbit");
        assert!(sent[0]["answers"].is_null(), "no post to answer");
        let _ = std::fs::remove_file(&file);
    }

    /// Post 2's words, typed an ask to him.
    const TYPED: &str = "Ship B tonight, or with the rest on Monday? Its line is green and its stills are in.";

    /// A typed ask on post 2: one box with Answer it in the box; his answer shown while held or sent; closed says how.
    #[test]
    fn an_ask_typed_to_him_is_one_box_answered_in_the_threads_box_and_says_how_it_closed() {
        let file = out("typed");
        let mut v = thread_of(TYPED, Vec::new());
        v.posts[1].told.clear();
        v.posts[1].names = vec!["owner".into()];
        v.posts[1].kind = Some(docket_machine::PostKind::Ask { to: vec!["owner".into()] });
        v.waits_on.clear();
        let (_held, mut s, child) = app_on(&file, v);
        let place = crate::message_box::thread_place("N-7");
        // the box aimed at the thread, not at post 2
        crate::message_box::choose(&mut s, Target::Thread { project: 0, t: "N-7".into(), post: None, way: crate::message_box::Way::Here });
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        rect(&rt, "ask-N-7-2-0");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x.contains("Ship B tonight, or with the rest on Monday?")), "its words, in the box: {t:?}");
        // the button: the thread's box aimed at post 2 (its default, so no choice kept) and given the keys
        let r = super::archive_tests::button(&rt, "Answer it in the box").unwrap_or_else(|| panic!("Answer it in the box: {t:?}"));
        click(&mut rt, r);
        assert!(rt.state.box_targets.get(&(0, place.clone())).is_none(), "{:?}", rt.state.box_targets);
        assert!(rt.state.composers.get(&(0, place.clone())).is_some_and(|e| e.focused), "the box has the keyboard");
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x.ends_with("› answers paper-research's question in post 2")), "one question: {t:?}");
        assert!(t.iter().any(|x| x == "Send answer to paper-research") && t.iter().any(|x| x == "nothing to answer"), "{t:?}");
        // his words, sent: one post to the asker, answering post 2, as he wrote them
        let said = "With the rest: Monday.";
        rt.update(|s, _| {
            let mut e = TextEdit::new(said);
            e.focused = true;
            e.set_caret(said.len());
            s.composers.insert((0, place.clone()), e);
        });
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Send answer to paper-research"));
        rt.update(|s, _| crate::message_box::send(s, &Target::Thread { project: 0, t: "N-7".into(), post: Some(2), way: crate::message_box::Way::Here }));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(!t.iter().any(|x| x == "Answer it in the box"), "on its way, it asks him nothing more: {t:?}");
        assert!(t.iter().any(|x| x == said), "{t:?}");
        let sent = posts(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!((sent[0]["text"].as_str(), sent[0]["answers"].as_u64(), sent[0]["to"].as_str()), (Some(said), Some(2), Some("paper-research")));
        // coordd has it, held: not open, "sending…", and the foot says it is being sent
        let set = |rt: &mut rune::runtime::Runtime<App>, f: &dyn Fn(&mut TicketView)| {
            rt.update(|s, _| f(s.projects[0].live.tickets.get_mut(&7).unwrap()));
            settle(rt);
            texts(rt)
        };
        rt.update(|s, _| s.outgoing.list.clear());
        let t = set(&mut rt, &|v| v.posts.push(serde_json::from_value(json!({"n": 3, "at": at(5), "by": "owner", "text": said, "answers": 2, "held": "N-8"})).unwrap()));
        assert!(!open_to_owner(&rt.state.projects[0].live.tickets[&7], &rt.state.projects[0].live.tickets[&7].posts[1]), "his answer held is on its way");
        assert!(t.iter().any(|x| x == "sending…") && t.iter().any(|x| x == "your answer is being sent to paper-research"), "{t:?}");
        assert!(!t.iter().any(|x| x == "Answer it in the box" || x == "nothing to answer"), "{t:?}");
        // told, closed by his post: "you answered it in #3", from the view's how, his post a plain one
        let t = set(&mut rt, &|v| {
            v.posts[2].held = None;
            v.posts[2].answers = None;
            v.posts[1].asks_open = false;
            v.posts[1].asks_closed = Some(AsksClosed::Answered { post: 3, by: "owner".into() });
        });
        assert!(t.iter().any(|x| x == "you answered it in #3"), "{t:?}");
        assert!(!t.iter().any(|x| x == "your answer is being sent to paper-research"), "{t:?}");
        // closed some other way: its words, faint
        let t = set(&mut rt, &|v| {
            v.posts.truncate(2);
            v.posts[1].asks_closed = Some(AsksClosed::NothingToAnswer);
        });
        assert!(t.iter().any(|x| x == "you: nothing to answer"), "{t:?}");
        let t = set(&mut rt, &|v| v.posts[1].asks_closed = Some(AsksClosed::Withdrawn { by: "paper-research".into(), why: "B ships with the rest".into() }));
        assert!(t.iter().any(|x| x == "withdrawn by paper-research: B ships with the rest"), "{t:?}");
        rect(&rt, "ask-N-7-2-0");
        let _ = std::fs::remove_file(&file);
    }
}

#[cfg(test)]
mod archive_tests {
    //! Archive behind a thread's ⋯.
    use super::*;
    use serde_json::{Value, json};

    /// The window on thread N-122 with a coordd at PROTOCOL, ARCHIVED or not, its requests written to OUT.
    pub(super) fn app(protocol: u32, archived: bool, out: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = protocol;
        let v: TicketView = serde_json::from_value(json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "Hacker News scan", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-20T07:00:00Z",
            "given": [], "archived": archived,
            "posts": [{"n": 1, "at": "2026-11-20T07:01:00Z", "by": "owner", "text": "scan Hacker News for P4", "names": ["paper-research"]},
                      {"n": 2, "at": "2026-11-20T07:05:00Z", "by": "paper-research", "text": "Nothing new on P4 today.", "told": ["owner"]}]}))
        .unwrap();
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(122);
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        (held, s, child)
    }

    fn requests(s: &mut App, child: std::process::Child, out: &std::path::Path) -> Vec<Value> {
        s.projects[0].link = None;
        let mut child = child;
        let _ = child.wait();
        std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["req"]["op"] == "archive").map(|v| v["req"].clone()).collect()
    }

    pub(super) fn out(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yaaihh-archive-{name}-{}.jsonl", std::process::id()))
    }

    fn settle(rt: &mut rune::runtime::Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    pub(super) fn button(rt: &rune::runtime::Runtime<App>, label: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.role == "button" && e.label.as_deref() == Some(label)).map(|e| e.rect)
    }

    fn texts(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
    }

    fn press(rt: &mut rune::runtime::Runtime<App>, r: [f32; 4]) {
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
    }

    /// Archive B's coordd: Archive is behind the thread's `⋯`, not on its page, and its press sends his archive. An
    /// older coordd (Archive A, or before): no Archive behind `⋯`, as it would be refused.
    #[test]
    fn archive_is_behind_the_threads_dots_and_sends_his_archive() {
        let file = out("dots");
        {
            let (_held, s, child) = app(coord_types::ARCHIVE_PROTOCOL, false, &file);
            let mut rt = crate::view_cost::window(s);
            settle(&mut rt);
            assert!(button(&rt, "Archive").is_none(), "not on the page: behind ⋯");
            rt.update(|s, _| {
                s.tickets.more.insert(122);
            });
            settle(&mut rt);
            let at = button(&rt, "Archive").expect("Archive behind ⋯");
            press(&mut rt, at);
            let sent = requests(&mut rt.state, child, &file);
            assert_eq!(sent, vec![json!({"op": "archive", "t": "N-122", "on": true})]);
        }
        let (_held, mut s, child) = app(coord_types::ARCHIVE_PROTOCOL - 1, false, &file);
        s.tickets.more.insert(122);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(button(&rt, "details").is_some(), "⋯ open");
        assert!(button(&rt, "Archive").is_none(), "a coordd that refuses it live: no Archive");
        assert!(requests(&mut rt.state, child, &file).is_empty());
        let _ = std::fs::remove_file(&file);
    }

    /// An archived thread is out of the Threads list and under one folded `ARCHIVED` entry at its foot; unfolded, its
    /// row with Unarchive, which an Archive A coordd takes too (a thread a record archived), and which sends it.
    #[test]
    fn an_archived_thread_sits_under_archived_with_its_unarchive() {
        let file = out("foot");
        let (_held, mut s, child) = app(coord_types::ARCHIVE_PROTOCOL - 1, true, &file);
        s.tickets.shown = None;
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "▸ ARCHIVED · 1"), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("Hacker News scan")), "out of the list while folded: {t:?}");
        let head = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("archived")).expect("the entry").rect;
        press(&mut rt, head);
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x.contains("Hacker News scan")), "unfolded: its row");
        let r = button(&rt, "Unarchive").expect("its Unarchive");
        press(&mut rt, r);
        let sent = requests(&mut rt.state, child, &file);
        assert_eq!(sent, vec![json!({"op": "archive", "t": "N-122", "on": false})]);
        let _ = std::fs::remove_file(&file);
    }
}

#[cfg(test)]
mod page_tests {
    //! A drag from a thread's title into its second post selects the words between, and Ctrl+C copies them.
    use crate::App;
    use rune::runtime::Runtime;
    use rune::{Key, KeyEvent, Mods};

    /// The element that says TEXT.
    fn says(rt: &Runtime<App>, text: &str) -> [f32; 4] {
        let all = || rt.elements().into_iter().filter(|e| !e.exiting);
        all().find(|e| e.text.as_deref() == Some(text)).map(|e| e.rect).unwrap_or_else(|| panic!("{text:?} in {:?}", all().filter_map(|e| e.text).collect::<Vec<_>>()))
    }

    fn drag(rt: &mut Runtime<App>, from: [f32; 2], to: [f32; 2]) {
        rt.pointer_move(from[0], from[1]);
        rt.frame(0.016);
        rt.pointer_down(from[0], from[1]);
        rt.frame(0.016);
        for k in 1..=8 {
            let f = k as f32 / 8.0;
            rt.pointer_move(from[0] + (to[0] - from[0]) * f, from[1] + (to[1] - from[1]) * f);
            rt.frame(0.016);
        }
        rt.pointer_up(to[0], to[1]);
        rt.frame(0.016);
    }

    #[test]
    fn a_drag_from_a_threads_title_into_its_second_post_selects_all_between() {
        let file = super::archive_tests::out("page");
        let (_held, s, mut child) = super::archive_tests::app(coord_types::ARCHIVE_PROTOCOL, false, &file);
        let mut rt = crate::view_cost::window(s);
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        // the title over the posts, not the list's row that says it too
        let last = says(&rt, "Nothing new on P4 today.");
        let title = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting && e.text.as_deref() == Some("Hacker News scan") && e.rect[1] < last[1] && e.rect[0] + e.rect[2] > last[0])
            .map(|e| e.rect)
            .next_back()
            .unwrap_or_else(|| panic!("the thread's title over its posts at {last:?}"));
        drag(&mut rt, [title[0] + 1.0, title[1] + title[3] / 2.0], [last[0] + last[2] - 1.0, last[1] + last[3] / 2.0]);
        let got = rt.selected_text().unwrap_or_default();
        println!("selected: {got:?}");
        assert!(got.starts_with("Hacker News scan") && got.contains("scan Hacker News for P4") && got.ends_with("Nothing new on P4 today."), "{got:?}");
        // no button's words: the page's controls are not its text (a label no text of the page says too)
        let all = rt.elements();
        let held: Vec<[f32; 4]> = all.iter().filter(|e| !e.exiting && e.role == "button").map(|e| e.rect).collect();
        let inside = |r: [f32; 4], b: &[f32; 4]| {
            let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3]
        };
        // the page's texts, a button's own label not among them
        let texts: Vec<&str> = all.iter().filter(|e| !e.exiting && e.role != "button" && !held.iter().any(|b| inside(e.rect, b))).filter_map(|e| e.text.as_deref()).collect();
        let buttons: Vec<&str> = all.iter().filter(|e| !e.exiting && e.role == "button").filter_map(|e| e.label.as_deref()).filter(|l| l.len() > 3 && !texts.contains(l)).collect();
        assert!(!buttons.is_empty(), "the page draws buttons");
        let pieces: Vec<&str> = got.split(['\n', '\t']).map(str::trim).collect();
        assert!(buttons.iter().all(|b| !pieces.contains(b)), "a button's label selected: {buttons:?} in {got:?}");
        // nor the post's actions, shown under the hand at its end (subthread.rs `actions`)
        assert!(rt.elements().iter().any(|e| !e.exiting && e.text.as_deref() == Some("Reply")), "the post's actions shown");
        assert!(!pieces.contains(&"Reply"), "the post's Reply selected: {got:?}");
        // the title's highlight in the kit's colour, as the posts' is (lib.rs `view`), not rune's own grey
        let kit = [rune_kit::Kit::neutral(false).selection().premul(1.0), rune_kit::Kit::neutral(true).selection().premul(1.0)];
        let (x, y) = (title[0] + 4.0, title[1] + title[3] / 2.0);
        let under: Vec<[f32; 4]> = rt.instances.iter().filter(|i| i.rect[0] <= x && x <= i.rect[0] + i.rect[2] && i.rect[1] <= y && y <= i.rect[1] + i.rect[3]).map(|i| i.fill).collect();
        assert!(under.iter().any(|f| kit.iter().any(|c| f.iter().zip(c).all(|(a, b)| (a - b).abs() < 1e-3))), "the kit's highlight under the title: {under:?}");
        // Ctrl+C (rune says cmd with it off macOS)
        rt.key(KeyEvent { key: Key::Command('c'), mods: Mods { cmd: true, ctrl: true, ..Mods::default() } });
        rt.frame(0.016);
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some(got.as_str()), "copied as selected");
        rt.state.projects[0].link = None;
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&file);
    }
}

#[cfg(test)]
mod question_link_tests {
    //! A `Q-…` in a post opens that question.
    use super::*;
    use serde_json::json;

    const ASK: &str = "yaaihh without the gateway: where coordd's own calls go (a, b or c)";

    /// Thread N-871 with accounts-lead's post naming its question Q-928, and Q-928 waiting on him.
    fn app() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let at = |m: u32| format!("2026-11-19T11:{m:02}:00Z");
        let thread: TicketView = serde_json::from_value(json!({"id": "N-871", "n": 871, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "accounts-lead"},
            "title": "accounts", "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 2, "opened_at": at(0), "given": [],
            "posts": [{"n": 1, "at": at(0), "by": "owner", "text": "the accounts page", "names": ["accounts-lead"]},
                      {"n": 2, "at": at(14), "by": "accounts-lead", "text": "- One question of mine waits on you: Q-928.", "told": ["owner"]}]}))
        .unwrap();
        let question: TicketView = serde_json::from_value(json!({"id": "Q-928", "n": 928, "kind": "question", "opener": "accounts-lead", "ctx": {},
            "form": {"ask": ASK, "context": "I would take (a), straight to Codex."}, "title": "",
            // as the record has it: step 1 ask_owner, its args the ask and its context (research's log, Q-928)
            "status": {"is": "awaits", "agent": "owner", "call": "ask_owner", "step": 1, "since": at(13), "taken": false, "refusals": 0, "forming": false,
                       "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {"ask": ASK, "context": "I would take (a), straight to Codex."}},
            "children": [], "program": "", "bound": 1, "spent": 0,
            "opened_at": at(13), "given": [], "posts": []}))
        .unwrap();
        for (seq, v) in [(1, thread), (2, question)] {
            p.live.apply(coord_types::Event { seq, body: coord_types::Body::Ticket(v) });
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(871);
        (held, s)
    }

    fn settle(rt: &mut rune::runtime::Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    /// The `Q-928` in the post is a link; pressed, the window shows that question: its ask.
    #[test]
    fn a_question_id_in_a_post_opens_that_question() {
        let (_held, s) = app();
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let link = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some("Q-928") && e.path.contains("post-N-871-2")).unwrap_or_else(|| {
            let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
            panic!("Q-928 drawn as a link of its own: {t:?}")
        });
        let (x, y) = (link.rect[0] + link.rect[2] / 2.0, link.rect[1] + link.rect[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        settle(&mut rt);
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(928)), "the question is open");
        // its page is drawn: who asks (its context is folded behind `› more about it`, as on every question's page)
        let t: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
        assert!(t.iter().any(|x| x == "accounts-lead asks"), "the question's page: {t:?}");
        // and what it asks, under who asks: not only behind `› more about it`
        assert!(t.iter().any(|x| x.contains("own calls go (a, b or c)")), "the question's words on its page: {t:?}");
    }
}
