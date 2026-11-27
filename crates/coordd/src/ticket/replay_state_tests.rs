//! A start's replay saved at a cut (I-1509; paper-research N-1446 p1526, p1538): the state saved at a cut, read back
//! and gone on with the lines after it, is the whole record's replay, part by part (the machine and each of its
//! tickets, the graph and each of its nodes and edges, the agents' side, the text places, the quiet, git's side, what
//! the replay found, its lines); a start goes on from the state saved for it, says why when it does not, and the
//! saver (`coordd save-replay`) goes on from the state before it. Then, ignored, the same on a copy of research's
//! record, and what a start and the saver cost on it.

use super::*;
use crate::core::{Core, Who};
use crate::ticket::Said;
use crate::ticket::edit_tests::core;
use docket_machine::{Input, Loc, Text};
use serde_json::json;
use std::path::Path;

/// A sha256 written into as rmp writes ([`digest`]).
struct Hashing(Sha256);

impl Write for Hashing {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// V as a state writes it (rmp, named), its sha256's first 16 hex digits.
fn digest<T: Serialize + ?Sized>(v: &T) -> String {
    let mut h = Hashing(Sha256::new());
    rmp_serde::encode::write_named(&mut h, v).unwrap();
    hex(&h.0.finalize())[..16].to_string()
}

/// What a replay made, part by part, each part's digest: the machine whole and each of its tickets, the graph's builder
/// whole and each node and edge of its graph, the agents' side, the places, the quiet, git's side, what it found, and
/// how many lines it took. Two replays that differ say where.
#[allow(clippy::too_many_arguments)]
fn parts(m: &Machine, graph: &Builder, side: &KeptSide, places: &TextPlaces, quiet: &Quiet, git: &GraphGit, errors: &[String], lines: usize) -> BTreeMap<String, String> {
    let mut p = BTreeMap::new();
    p.insert("machine".to_string(), digest(m));
    for (t, x) in m.tickets.iter() {
        p.insert(format!("ticket {}", m.id(*t)), digest(x));
    }
    p.insert("graph".to_string(), digest(graph));
    // what the graph's serde leaves out and decisions read (docket-core N-1446 p1702)
    for (id, j) in &graph.graph.jobs {
        p.insert(format!("graph job {id}"), digest(j));
    }
    p.insert("graph releases".to_string(), digest(&graph.graph.releases));
    for n in &graph.graph.nodes {
        p.insert(format!("node {}", n.id), digest(n));
    }
    for (i, e) in graph.graph.edges.iter().enumerate() {
        p.insert(format!("edge {i}"), digest(e));
    }
    p.insert("side".to_string(), digest(&side.saved()));
    p.insert("places".to_string(), digest(&places.saved()));
    p.insert("quiet".to_string(), digest(quiet));
    p.insert("git".to_string(), digest(&git.saved()));
    p.insert("errors".to_string(), digest(errors));
    p.insert("lines".to_string(), lines.to_string());
    p
}

fn ended_parts(e: &Ended) -> BTreeMap<String, String> {
    parts(&e.m, &e.graph, &e.side, &e.places, &e.quiet, &e.git, &e.errors, e.lines)
}

fn tickets_parts(t: &super::super::Tickets) -> BTreeMap<String, String> {
    parts(&t.m, &t.graph, &t.side, &t.places.read().unwrap(), &t.quiet, &t.graph_git, &t.errors, t.graph_lines)
}

/// The parts A and B do not hold alike (the first 12), A's and B's digest.
fn differs(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter().filter(|k| a.get(*k) != b.get(*k)).take(12).map(|k| format!("{k}: {:?} against {:?}", a.get(k), b.get(k))).collect()
}

/// Lines that read, each one's place in the record, and the words for those that did not.
type Split = (Vec<Line>, Vec<(usize, usize)>, Vec<String>);

/// TEXT, the record LOG, cut before its file line K (from 0; its number of lines: its end): the lines before the cut
/// that read with their places, those that did not, and the cut.
fn cut_at(log: &str, text: &str, k: usize) -> (Split, Cut) {
    let bytes = text.split_inclusive('\n').take(k).map(str::len).sum();
    let read = split(log, read_lines(&text[..bytes], 0, 0));
    let cut = Cut::of(text, bytes, read.0.len(), k);
    (read, cut)
}

/// TEXT's lines after CUT: those that read with their places, those that did not, and the cut at its end (as `start`
/// makes it).
fn tail_of(log: &str, text: &str, cut: &Cut) -> (Split, Cut) {
    let read = split(log, read_lines(&text[cut.bytes..], cut.bytes, cut.file_lines));
    let end = Cut { lines: cut.lines + read.0.len(), bytes: text.len(), file_lines: cut.file_lines + text[cut.bytes..].lines().count(), prefix: String::new() };
    (read, end)
}

/// TEXT, the record LOG, replayed to its file line K and the state saved in COORD for KEY.
fn saved_at(defs: &Defs, log: &str, text: &str, k: usize, coord: &str, key: &Key) -> AtCut {
    let ((lines, places, unread), cut) = cut_at(log, text, k);
    let at = AtCut::replayed(defs.clone(), text, &lines, &places, unread, cut);
    at.save(coord, key).unwrap();
    at
}

/// The state saved in COORD for KEY read back and gone on with TEXT's lines after it: ended, or why it was refused.
fn gone_on(defs: &Defs, log: &str, text: &str, coord: &str, key: &Key) -> Result<Ended, String> {
    let back = AtCut::read(coord, key, defs.clone()).unwrap();
    let ((tail, places, unread), end) = tail_of(log, text, &back.cut);
    back.on(text, &tail, &places, unread, end).map(|a| a.end(log))
}

/// TEXT's first file line from WANT on (from 0) that is an input; its number of lines when none is.
fn input_from(text: &str, want: usize) -> usize {
    let read = read_lines(text, 0, 0);
    (want..read.len()).find(|k| read[*k].line.as_ref().is_ok_and(Line::is_input)).unwrap_or(read.len())
}

/// The whole of TEXT, the record LOG, replayed with DEFS and ended.
fn whole(defs: &Defs, log: &str, text: &str) -> Ended {
    let ((lines, places, unread), cut) = cut_at(log, text, text.split_inclusive('\n').count());
    AtCut::replayed(defs.clone(), text, &lines, &places, unread, cut).end(log)
}

fn base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

/// The defs a start of the coord COORD reads (the tickets numbered from 1), and its record's name.
fn defs_of(core: &Core) -> (Defs, String) {
    let places = crate::policy::Places { base: base(), project: core.project.as_ref().map(|p| p.dir.clone()), coord: core.coord.clone(), ..Default::default() };
    (super::super::Tickets::read_defs(&places, 1).0, format!("{}/tickets/log.jsonl", core.coord))
}

/// A round of the coord's life in small, ROUND naming it: two `e` tickets opened, their steps given to w2 in a session
/// and answered, one step's template edited by the owner; an owner's message makes a thread, he posts on it, w2's turn
/// ends, the thread is ended; with REKIND, the second ticket, still open, is rekinded.
fn drive(core: &mut Core, coord: &Path, round: &str, rekind: bool) {
    let open = |core: &mut Core, q: String| {
        core.ticket_input(Input::Open { kind: "e".into(), under: None, opener: "owner".into(), form: json!({"q": q}), text: None, names: vec![], told: vec![] }).unwrap();
        core.tickets.m.tickets.values().filter(|x| x.kind == "e").map(|x| x.t).max().unwrap()
    };
    let session = coord.join("w2.jsonl").display().to_string();
    let give = |core: &mut Core| {
        for d in core.ticket_due("w2") {
            core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: session.clone(), offset: 0, len: 1 }) }).unwrap();
        }
    };
    let answer = |core: &mut Core, t: u64, text: &str| {
        core.ticket_input(Input::Answer { t, by: "w2".into(), head: json!({"head": "Said", "text": text}), text: Text::New(text.into()) }).unwrap();
    };
    // a runner job's start and end, and a release act, as the runner and release.sh write them: the graph keeps them
    // for decisions (`Graph::jobs`, `Graph::releases`)
    let act = |core: &mut Core, is: &str, more: serde_json::Value| {
        core.ticket_input(Input::Act { agent: "w2".into(), is: is.into(), text: format!("{is} {round}"), more: more.as_object().unwrap().clone(), t: None }).unwrap();
    };
    let job = json!({"id": format!("7001-{round}"), "branch": "main", "commit": "c0ffee12", "changed": "0", "verb": "test", "packages": ["coordd"], "since": "2026-11-22T09:50:00+0100", "items": ["I-7"], "tree": "c210cae5"});
    let with = |v: &serde_json::Value, more: serde_json::Value| {
        let mut v = v.clone();
        v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        v
    };
    act(core, "job", with(&job, json!({"phase": "start"})));
    let a = open(core, format!("{round} one"));
    let b = open(core, format!("{round} two"));
    act(core, "job", with(&job, json!({"phase": "end", "rc": "0", "passed": "12", "failed": "0", "secs": "40"})));
    act(core, "release", json!({"phase": "switch", "checkout": "/home/t/Documents/papers/soft-swarm", "coord": "research", "coordd": "20261122-1500", "coordd_commit": format!("{round}0000")}));
    give(core);
    let id_b = core.tickets.m.id(b);
    let r = core.ticket_template(&Who::Owner, &id_b, "ask".into(), "step".into(), "Edited: {{ q }}!".into(), "clearer".into());
    assert!(r.ok, "{}", r.out);
    answer(core, a, "a1");
    give(core);
    answer(core, a, "a2");
    answer(core, b, "b1");
    give(core);
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": format!("{round}: the parser"), "addressed": {"head": "Named"}}) }).unwrap();
    let th = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    core.ticket_input(Input::Post { t: th, by: "owner".into(), text: Text::New(format!("{round}: and the lexer")), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    give(core);
    let transcript = coord.join(format!("w2-{round}.jsonl"));
    let said = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "done"}], "stop_reason": "end_turn"}}));
    std::fs::write(&transcript, &said).unwrap();
    core.ticket_turn_ended("w2", Loc { file: transcript.display().to_string(), offset: 0, len: said.len() as u64 });
    let id_th = core.tickets.m.id(th);
    let r = core.ticket_end(&Who::Owner, &id_th, "done", "over", Said::default());
    assert!(r.ok, "{}", r.out);
    if rekind {
        // to another kind, of another letter: its posts' ids on the agents' side change with it
        let r = core.ticket_rekind(&Who::Owner, &id_b, "question".into(), json!({"ask": format!("{round} again")}), "asked again".into(), Said::default());
        assert!(r.ok, "{}", r.out);
        give(core);
    }
}

fn fresh(coord: &Path) {
    let _ = std::fs::remove_dir_all(coord.join(FOLDER));
}

/// The plan's test in small (p1526): at every input of a record, a replay stopped there, saved, read back and gone on
/// with the lines after it is the whole record's replay, part by part; or it is refused, saying why, where the lines
/// after it would answer differently what the lines before looked up ahead or rekind a ticket. Most cuts go on.
#[test]
fn a_replay_saved_at_any_input_and_gone_on_with_the_lines_after_it_is_the_whole_records() {
    let (mut core, coord) = core("replay-cuts");
    drive(&mut core, &coord, "first", false);
    drive(&mut core, &coord, "second", true);
    drive(&mut core, &coord, "third", false);
    let (defs, log) = defs_of(&core);
    let c = core.coord.clone();
    let text = std::fs::read_to_string(&log).unwrap();
    let key = Key::of(&defs, &[]).unwrap();
    let all = whole(&defs, &log, &text);
    assert!(all.errors.is_empty(), "{:?}", all.errors);
    // each round's job and release act, which decisions read: compared at every cut with the rest
    assert_eq!((all.graph.graph.jobs.len(), all.graph.graph.releases.len()), (3, 3));
    assert!(all.graph.graph.jobs.values().all(|j| j.rc == Some(0)), "each job's end kept");
    let want = ended_parts(&all);
    let read = read_lines(&text, 0, 0);
    let rekinded = read.iter().position(|r| matches!(&r.line, Ok(Line::Edited { what, .. }) if what == "rekind")).expect("a rekind in the record");
    let inputs: Vec<usize> = (0..=read.len()).filter(|k| *k == read.len() || read[*k].line.as_ref().is_ok_and(Line::is_input)).collect();
    let (mut went_on, mut refused) = (0, Vec::new());
    for k in inputs {
        fresh(&coord);
        saved_at(&defs, &log, &text, k, &c, &key);
        assert_eq!(AtCut::read(&c, &key, defs.clone()).unwrap().cut.file_lines, k);
        match gone_on(&defs, &log, &text, &c, &key) {
            Ok(ended) => {
                let d = differs(&ended_parts(&ended), &want);
                assert!(d.is_empty(), "cut before line {}: {d:#?}", k + 1);
                went_on += 1;
            }
            Err(why) => refused.push((k, why)),
        }
    }
    // every cut before the rekind is refused for it (research's record has none: 0 in 162,901 lines), every one after
    // goes on; what reaches back is refused in the machine's tests (docket-machine's live_edits)
    for (k, why) in &refused {
        assert!(*k <= rekinded && why.contains("rekinds"), "cut before line {}: {why}", k + 1);
    }
    assert!(!refused.is_empty(), "a cut before the rekind is refused for it");
    assert!(went_on > 0 && refused.iter().all(|(k, _)| *k <= rekinded), "{went_on} cuts went on, {} were refused: {refused:?}", refused.len());
    let _ = std::fs::remove_dir_all(&coord);
}

/// coordd's start (`load_tickets`, states on): a whole replay saves its state at the record's end; the next start goes
/// on from it with the lines written since, and holds what a whole replay holds (`Tickets::load`, which never reads a
/// state), the agents' side, the places, the quiet and git's side with it.
#[test]
fn a_start_goes_on_from_the_state_the_last_whole_replay_saved() {
    let (mut core, coord) = core("replay-start");
    drive(&mut core, &coord, "first", false);
    let c = core.coord.clone();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    fresh(&coord);
    let first = super::super::Tickets::load_joint(&c, &base(), project.as_deref(), 1, false, true);
    let n = first.record_lines();
    assert_eq!(first.saved_at, Some(n), "the whole replay saved its state at its end");
    assert_eq!(states(&format!("{c}/{FOLDER}")).len(), 1);
    drive(&mut core, &coord, "second", false);
    let again = super::super::Tickets::load_joint(&c, &base(), project.as_deref(), 1, false, true);
    assert_eq!(again.saved_at, Some(n), "gone on from the state at line {n}, nothing saved");
    assert!(again.record_lines() > n);
    let whole = super::super::Tickets::load(&c, &base(), project.as_deref(), 1);
    assert!(whole.errors.is_empty(), "{:?}", whole.errors);
    let d = differs(&tickets_parts(&again), &tickets_parts(&whole));
    assert!(d.is_empty(), "{d:#?}");
    // the core's own start, as coordd's, goes on from it too (what its load writes after the replay is its own)
    let mut restarted = Core::load(&c, &base());
    restarted.project = crate::project::Project::load(&c).unwrap();
    restarted.load_tickets();
    assert_eq!(restarted.tickets.saved_at, Some(n));
    let _ = std::fs::remove_dir_all(&coord);
}

/// A state not for this start is not read, and the journal says why: none saved; one another program made, or made
/// with other defs; a record whose bytes before the cut are not those it was cut from, or shorter than it; one that
/// does not read. The whole record is replayed then, as before, and its state saved.
#[test]
fn a_start_says_why_it_replays_the_whole_record() {
    let (mut core, coord) = core("replay-why");
    drive(&mut core, &coord, "first", false);
    drive(&mut core, &coord, "second", false);
    let (defs, log) = defs_of(&core);
    let c = core.coord.clone();
    let text = std::fs::read_to_string(&log).unwrap();
    let key = Key::of(&defs, &[]).unwrap();
    let file_lines = text.lines().count();
    let half = input_from(&text, file_lines / 2);
    let save = |k: usize, key: &Key| {
        saved_at(&defs, &log, &text, k, &c, key);
    };
    let said = |text: &str| start(&c, &log, text, defs.clone(), &[], true).said;
    fresh(&coord);
    let s = said(&text);
    assert!(s.contains("whole (no state is saved)") && s.contains("its state saved at line"), "{s}");
    assert!(said(&text).contains("from the state saved at line"), "the state that start saved is read by the next");
    fresh(&coord);
    save(half, &Key { exe: "0".repeat(64), defs: key.defs.clone() });
    let s = said(&text);
    assert!(s.contains("was made by another program)"), "{s}");
    fresh(&coord);
    save(half, &Key { exe: key.exe.clone(), defs: "0".repeat(64) });
    let s = said(&text);
    assert!(s.contains("was made with other defs)"), "{s}");
    fresh(&coord);
    save(half, &key);
    let s = said(&text);
    assert!(s.contains("from the state saved at line"), "{s}");
    let edited = text.replacen("\"t\":1,", "\"t\":1 ,", 1);
    assert_ne!(edited, text);
    let s = said(&edited);
    assert!(s.contains("are not those the state was cut from"), "{s}");
    fresh(&coord);
    save(file_lines, &key);
    let shorter: String = text.split_inclusive('\n').take(file_lines / 2).collect();
    let s = said(&shorter);
    assert!(s.contains("the record is shorter"), "{s}");
    fresh(&coord);
    save(half, &key);
    let file = format!("{c}/{FOLDER}/{}", key.name());
    let bytes = std::fs::read(&file).unwrap();
    std::fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
    let s = said(&text);
    assert!(s.contains(&format!("whole ({file}:")), "{s}");
    let _ = std::fs::remove_dir_all(&coord);
}

/// `coordd save-replay` (`save_on`): it saves at the record's last input but one, from the record's first line when no
/// state is saved, then from the state before it; a start then goes on from what it saved and holds what a whole
/// replay holds. Only the two newest keys' states are kept.
#[test]
fn the_saver_goes_on_from_the_state_before_it_and_a_start_from_what_it_saved() {
    let (mut core, coord) = core("replay-saver");
    drive(&mut core, &coord, "first", false);
    let (defs, log) = defs_of(&core);
    let c = core.coord.clone();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    let last_input = |log: &str| {
        let text = std::fs::read_to_string(log).unwrap();
        let lines: Vec<Line> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        lines.iter().rposition(Line::is_input).unwrap()
    };
    fresh(&coord);
    let n1 = last_input(&log);
    let said = save_on(&c, &base(), project.as_deref(), 1, &[]).unwrap();
    assert!(said.starts_with(&format!("saved at line {n1} from the record's first line (no state is saved)")), "{said}");
    drive(&mut core, &coord, "second", false);
    let n2 = last_input(&log);
    let said = save_on(&c, &base(), project.as_deref(), 1, &[]).unwrap();
    assert!(said.starts_with(&format!("saved at line {n2} from the state at line {n1} and the {} lines after it", n2 - n1)), "{said}");
    let text = std::fs::read_to_string(&log).unwrap();
    let s = start(&c, &log, &text, defs.clone(), &super::super::folders(&c, &base(), project.as_deref()), true);
    assert_eq!(s.saved_at, Some(n2), "{}", s.said);
    let d = differs(&ended_parts(&s.ended), &ended_parts(&whole(&defs, &log, &text)));
    assert!(d.is_empty(), "{d:#?}");
    // other keys' states: the two newest kept
    let ((lines, places, unread), cut) = cut_at(&log, &text, 3);
    let at = AtCut::replayed(defs.clone(), &text, &lines, &places, unread, cut);
    for i in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        at.save(&c, &Key { exe: format!("{i}"), defs: String::new() }).unwrap();
    }
    let kept = states(&format!("{c}/{FOLDER}"));
    assert_eq!(kept.len(), KEPT, "{kept:?}");
    let _ = std::fs::remove_dir_all(&coord);
}

/// The gate's state as the install's seed (paper-research N-1446 p2658): the saver run on a copy of the coord (its record
/// copied, its project's files in another folder, agent-base under another path) saves the state of the key the coord's
/// own defs make, though the defs name other files; the coord's saver, once its record has grown past the copy, goes on
/// from that state (`coordd save-replay COORD SEED`) and a start from what it saved holds what a whole replay holds. A
/// seed of other defs (a kind changed in the copy) is passed by, said, and the record replayed from its first line.
#[test]
fn the_saver_goes_on_from_the_state_saved_on_a_copy_of_its_record_read_from_other_folders() {
    let (mut core, coord) = core("replay-seed");
    drive(&mut core, &coord, "first", false);
    let (defs, log) = defs_of(&core);
    let c = core.coord.clone();
    let project = core.project.as_ref().map(|p| p.dir.clone()).unwrap();
    fresh(&coord);
    // the gate's copy: the record, the project's files, agent-base by another path
    let copy = std::env::temp_dir().join(format!("docket-seed-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    std::fs::create_dir_all(copy.join("tickets")).unwrap();
    std::fs::copy(&log, copy.join("tickets/log.jsonl")).unwrap();
    let files = copy.join("project-copy");
    for sub in ["kinds", "calls"] {
        std::fs::create_dir_all(files.join(sub)).unwrap();
        for e in std::fs::read_dir(format!("{project}/{sub}")).unwrap().flatten() {
            std::fs::copy(e.path(), files.join(sub).join(e.file_name())).unwrap();
        }
    }
    std::fs::copy(format!("{project}/project.toml"), files.join("project.toml")).unwrap();
    std::fs::write(copy.join("project"), files.join("project.toml").display().to_string()).unwrap();
    let base_copy = copy.join("agent-base");
    std::os::unix::fs::symlink(base(), &base_copy).unwrap();
    let (cc, bc, pc) = (copy.display().to_string(), base_copy.display().to_string(), files.display().to_string());
    let copy_defs = super::super::Tickets::read_defs(&crate::policy::Places { base: bc.clone(), project: Some(pc.clone()), coord: cc.clone(), ..Default::default() }, 1).0;
    assert_ne!(format!("{defs:?}"), format!("{copy_defs:?}"), "the copy's defs name its own files");
    let here = super::super::folders(&c, &base(), Some(&project));
    assert_eq!(Key::of(&copy_defs, &super::super::folders(&cc, &bc, Some(&pc))).unwrap(), Key::of(&defs, &here).unwrap(), "one key for the same files");
    let said = save_on(&cc, &bc, Some(&pc), 1, &[]).unwrap();
    assert!(said.contains("from the record's first line (no state is saved)"), "{said}");
    let seed = states(&format!("{cc}/{FOLDER}")).pop().expect("the copy's state");
    let n1 = said.strip_prefix("saved at line ").and_then(|s| s.split(' ').next()).unwrap().to_string();
    // the coord goes on, then its saver takes the seed
    drive(&mut core, &coord, "second", false);
    let said = save_on(&c, &base(), Some(&project), 1, std::slice::from_ref(&seed)).unwrap();
    assert!(said.contains(&format!("from the state at line {n1} of the seed {seed} and the ")), "{said}");
    let text = std::fs::read_to_string(&log).unwrap();
    let s = start(&c, &log, &text, defs.clone(), &here, true);
    assert!(s.said.contains("from the state saved at line"), "{}", s.said);
    let d = differs(&ended_parts(&s.ended), &ended_parts(&whole(&defs, &log, &text)));
    assert!(d.is_empty(), "{d:#?}");
    // a seed of other defs: passed by, and said
    fresh(&coord);
    std::fs::write(files.join("kinds/e.kind"), format!("/// changed in the copy\n{}", super::super::edit_tests::KIND)).unwrap();
    std::fs::copy(&log, copy.join("tickets/log.jsonl")).unwrap();
    let other = save_on(&cc, &bc, Some(&pc), 1, &[]).unwrap();
    assert!(other.contains("from the record's first line"), "{other}");
    let seed2 = states(&format!("{cc}/{FOLDER}")).into_iter().next().unwrap();
    assert_ne!(seed2, seed, "another key's state");
    let said = save_on(&c, &base(), Some(&project), 1, std::slice::from_ref(&seed2)).unwrap();
    assert!(said.contains(&format!("from the record's first line (the seed {seed2}: {seed2} was made by another program or with other defs; no state is saved)")), "{said}");
    let _ = std::fs::remove_dir_all(&copy);
    let _ = std::fs::remove_dir_all(&coord);
}

/// The cut before the last input (`last_input_at`; a2h-speed H-1590 p1, paper-research H-1590 p2):
/// - At the end of every line of a record, and with a half-written line after it, the cut stands where the saver's rule
///   put it before (its whole lines read, back to the last input among them).
/// - A replay of a live record reads up to that cut, or up to a byte given, and nothing past it (`read_to`).
/// - These are refused in words: a byte that is no line's end, a byte past the end, a cut that is neither a byte nor
///   `input`, and a record with no input.
/// - A load with no cut given reads the whole record (`read_record`).
#[test]
fn a_replay_of_a_live_record_reads_up_to_the_savers_cut_or_a_line_given_and_nothing_past() {
    let (mut core, coord) = core("replay-read-to");
    drive(&mut core, &coord, "first", false);
    let (_, log) = defs_of(&core);
    let text = std::fs::read_to_string(&log).unwrap();
    // the saver's rule as it stood before: the whole lines read, the byte of the last input among them
    let before = |t: &str| {
        let whole = t.rfind('\n').map_or(0, |i| i + 1);
        let read = read_lines(&t[..whole], 0, 0);
        read.iter().rposition(|r| r.line.as_ref().is_ok_and(Line::is_input)).map(|k| read[k].byte)
    };
    let ends: Vec<usize> = text.match_indices('\n').map(|(i, _)| i + 1).collect();
    let half = "{\"ev\":\"post\",\"t\":";
    for &e in &ends {
        assert_eq!(last_input_at(&text[..e]), before(&text[..e]), "at byte {e}");
        assert_eq!(last_input_at(&format!("{}{half}", &text[..e])), before(&text[..e]), "at byte {e}, a half line after it");
    }
    let cut = last_input_at(&text).expect("the record has an input");
    assert!(cut < text.len());
    // the live file grows: a half line written after the record
    std::fs::write(&log, format!("{text}{half}")).unwrap();
    let (read, said) = read_to(&log, "input").unwrap();
    assert_eq!(read, &text[..cut], "up to the last input's start");
    assert!(said.ends_with(&format!("read up to byte {cut}, its first {} lines (REPLAY_BYTES=input)", text[..cut].lines().count())), "{said}");
    // a line given: its byte, and nothing past it however the file has grown
    let e = ends[ends.len() / 2];
    let (read, said) = read_to(&log, &e.to_string()).unwrap();
    assert_eq!(read, &text[..e]);
    assert!(said.ends_with(&format!("read up to byte {e}, its first {} lines (REPLAY_BYTES={e})", text[..e].lines().count())), "{said}");
    // refused in words
    for (to, words) in [((e - 1).to_string(), "not the end of a line"), ((text.len() + half.len() + 1).to_string(), "past the record's end"), ("line 3".to_string(), "neither a byte nor `input`")] {
        let r = read_to(&log, &to).map(|(t, s)| (t.len(), s));
        assert!(r.as_ref().is_err_and(|w| w.contains(words)), "{to}: {r:?}");
    }
    let outputs_only = coord.join("outputs.jsonl");
    let output = text.lines().find(|l| serde_json::from_str::<Line>(l).is_ok_and(|l| !l.is_input())).expect("an output line");
    std::fs::write(&outputs_only, format!("{output}\n{half}")).unwrap();
    let r = read_to(&outputs_only.display().to_string(), "input").map(|(t, s)| (t.len(), s));
    assert!(r.as_ref().is_err_and(|w| w.contains("REPLAY_BYTES=input: no input among its 1 whole lines")), "{r:?}");
    // with no cut given, a load reads the record whole, the half line too
    assert_eq!(read_record(&log).unwrap(), (format!("{text}{half}"), None));
    let _ = std::fs::remove_dir_all(&coord);
}

/// The saver behind the replay lock (paper-research N-1446 p1638): while another replay holds the lock it waits, as long
/// as that takes, and saves nothing; once the lock is let go it takes it, saves, and says how long it waited.
#[test]
fn the_saver_waits_while_the_replay_lock_is_held_then_saves() {
    let (mut core, coord) = core("replay-lock");
    drive(&mut core, &coord, "first", false);
    let c = core.coord.clone();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    fresh(&coord);
    let lock = coord.join("locks/replay.lock").display().to_string();
    std::fs::create_dir_all(coord.join("locks")).unwrap();
    // another replay holds it, as `flock LOCK` would
    let other = std::fs::OpenOptions::new().append(true).create(true).open(&lock).unwrap();
    assert_eq!(unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&other), libc::LOCK_EX) }, 0);
    let (tx, rx) = std::sync::mpsc::channel();
    let (l, cc, base) = (lock.clone(), c.clone(), base());
    std::thread::spawn(move || tx.send(save_locked(&l, &cc, &base, project.as_deref(), 1, &[])).unwrap());
    assert!(rx.recv_timeout(std::time::Duration::from_millis(1500)).is_err(), "the saver ran while the lock was held");
    assert!(states(&format!("{c}/{FOLDER}")).is_empty(), "nothing saved while it waits");
    drop(other);
    let said = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("the saver runs once the lock is let go").unwrap();
    assert!(said.starts_with("saved at line ") && said.contains("waiting for the replay lock"), "{said}");
    assert_eq!(states(&format!("{c}/{FOLDER}")).len(), 1);
    let _ = std::fs::remove_dir_all(&coord);
}

/// A replay lock that does not open (a box whose box.sh could not link the host's into its $COORD): the saver saves
/// without it and says so, with why (paper-research N-1446 p1644).
#[test]
fn a_replay_lock_that_does_not_open_is_said_and_the_saver_saves_without_it() {
    let (mut core, coord) = core("replay-nolock");
    drive(&mut core, &coord, "first", false);
    let c = core.coord.clone();
    let project = core.project.as_ref().map(|p| p.dir.clone());
    fresh(&coord);
    // under a file, not a folder: neither made nor opened
    std::fs::write(coord.join("not-a-folder"), "").unwrap();
    let lock = coord.join("not-a-folder/replay.lock").display().to_string();
    let said = save_locked(&lock, &c, &base(), project.as_deref(), 1, &[]).unwrap();
    assert!(said.starts_with("saved at line ") && said.contains(&format!("without the replay lock, which does not open ({lock}:")) && said.contains("right only while"), "{said}");
    assert_eq!(states(&format!("{c}/{FOLDER}")).len(), 1);
    let _ = std::fs::remove_dir_all(&coord);
}

// ---- research's copy (ignored): REPLAY_COORD (a copy of the coord), REPLAY_BASE, REPLAY_PROJECT, as
// `a_coords_record_replays`; REPLAY_STATES, a folder of the states made. Each in a process of its own for its memory:
//   build-runner cargo --folder engine test --no-run --profile replay -p coordd, then the binary with `--ignored --exact`

/// This process's memory peaks (VmHWM, VmPeak), in MB.
fn peaks() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let mb = |k: &str| s.lines().find_map(|l| l.strip_prefix(k).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())).map_or(0, |kb| kb >> 10);
    (mb("VmHWM:"), mb("VmPeak:"))
}

struct Copy {
    coord: String,
    base: String,
    project: Option<String>,
    states: String,
    defs: Defs,
    log: String,
    text: String,
}

fn copy() -> Option<Copy> {
    let (Ok(coord), Ok(base), Ok(states)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE"), std::env::var("REPLAY_STATES")) else { return None };
    let project = std::env::var("REPLAY_PROJECT").ok();
    let places = crate::policy::Places { base: base.clone(), project: project.clone(), coord: coord.clone(), ..Default::default() };
    let defs = super::super::Tickets::read_defs(&places, 1).0;
    let log = format!("{coord}/tickets/log.jsonl");
    let text = std::fs::read_to_string(&log).unwrap();
    Some(Copy { coord, base, project, states, defs, log, text })
}

/// The folders the copy's defs are read from (`ticket::folders`), as its start's key takes them.
fn folders_of(cp: &Copy) -> Vec<String> {
    super::super::folders(&cp.coord, &cp.base, cp.project.as_deref())
}

/// The cuts of the plan (p1526): the record's half, 40,000 and 5,000 lines before its end, and its end; each at the
/// first input at or after that file line. Name and file line.
fn cuts(text: &str) -> Vec<(&'static str, usize)> {
    let n = text.split_inclusive('\n').count();
    let at = |want: usize| input_from(text, want);
    vec![("half", at(n / 2)), ("n-40000", at(n.saturating_sub(40_000))), ("n-5000", at(n.saturating_sub(5_000))), ("n", n)]
}

/// The joint's history as its own whole replay gave it before I-1509 (`joint::history`, with BETWEEN the agents'
/// threads held): each input fed, with its lines and what the machine showed; the tickets held.
type Fed = (Vec<(usize, Vec<Line>, crate::joint::Seen)>, BTreeSet<u64>);

fn history_fed(defs: &Defs, text: &str, between: bool) -> Fed {
    let (lines, seen, only, _) = crate::joint::history(defs.clone(), text, usize::MAX, between);
    (seen.into_iter().map(|(i, j, s)| (i, lines[i..j].to_vec(), s)).collect(), only)
}

/// The same from the one replay's (`Ended::joint`).
fn ended_fed(e: &Ended, text: &str, between: bool) -> Fed {
    let only = crate::joint::only(&e.m, between);
    (e.joint.fed(text, &only).map(|(i, l, s)| (i, l, s.clone())).collect(), only)
}

/// FED through a joint on the library LIBRARY: every difference, and what it holds then.
fn through_joint(library: &str, fed: &Fed) -> (Vec<crate::joint::Difference>, crate::joint::Held) {
    let mut j = crate::joint::Joint::holding(library, fed.1.clone()).unwrap();
    let mut diffs = Vec::new();
    for (i, lines, seen) in &fed.0 {
        diffs.extend(j.take(i + 1, lines, seen));
    }
    (diffs, j.held())
}

/// Where A, the one replay's, is not B, the joint's own replay's (the first 5).
fn fed_differs(a: &Fed, b: &Fed) -> Vec<String> {
    let mut out = Vec::new();
    if a.1 != b.1 {
        out.push(format!("tickets held: {:?} against {:?}", a.1.symmetric_difference(&b.1).take(10).collect::<Vec<_>>(), b.1.len()));
    }
    if a.0.len() != b.0.len() {
        out.push(format!("{} inputs fed against {}", a.0.len(), b.0.len()));
    }
    out.extend(a.0.iter().zip(&b.0).filter(|(x, y)| x != y).take(5).map(|(x, y)| format!("input {} against input {}: {}", x.0, y.0, if x.1 != y.1 { "its lines" } else { "what the machine showed" })));
    out
}

/// The joint's history taken along the one replay (paper-research N-1446 p1644): at every input of a record, a replay
/// stopped there, saved, read back and gone on to the end feeds the joint exactly what its own whole replay fed it
/// before (`joint::history`): the same inputs, each with the same lines, read back from the record at their place, and
/// what the machine showed after it; the same tickets held; and the joint makes the same of them: every difference,
/// the tickets compared and differing, the table's counts, each ticket's duties. With the agents' threads held and
/// without. A cut between an item's close and its reopening is refused for it, and says so; every other cut goes on.
#[test]
fn the_joints_history_from_the_one_replay_is_what_its_own_replay_gave() {
    let (core, coord) = core("replay-joint");
    // a second agent, for an agents' thread (`Between`)
    let c = core.coord.clone();
    drop(core);
    let mut registry = std::fs::read_to_string(coord.join("registry.jsonl")).unwrap();
    registry.push_str("{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"w3\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n");
    std::fs::write(coord.join("registry.jsonl"), registry).unwrap();
    let mut core = Core::load(&c, &base());
    core.project = crate::project::Project::load(&c).unwrap();
    core.load_tickets();
    drive(&mut core, &coord, "first", false);
    let r = core.ticket_msg(&Who::Agent("w2".into()), None, vec!["w3".into()], "the parser drops comments", false, None, Vec::new(), Said::default());
    assert!(r.ok, "{}", r.out);
    let th = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    assert!(core.tickets.m.tickets[&th].between());
    // an item closed before (imported closed, as the notes import it), reopened after (`item start`)
    let closing = docket_machine::Closing { by: "owner".into(), at: "2026-11-22T10:00:00Z".into(), head: json!({"head": "Done", "why": "landed"}), text: None };
    core.ticket_input(Input::Import { t: 900, kind: "item".into(), opener: "owner".into(), form: json!({"title": "faster replay", "note": null}), text: None, closed: Some(closing) }).unwrap();
    drive(&mut core, &coord, "second", false);
    core.ticket_input(Input::Reopen { t: 900, by: "w2".into() }).unwrap();
    let id_th = core.tickets.m.id(th);
    let r = core.ticket_post(&Who::Agent("w3".into()), &id_th, "on it".into(), None, None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
    drive(&mut core, &coord, "third", false);
    let (defs, log) = defs_of(&core);
    let text = std::fs::read_to_string(&log).unwrap();
    let key = Key::of(&defs, &[]).unwrap();
    let library = crate::joint::library_text(&base(), core.project.as_ref().map(|p| p.dir.as_str())).unwrap();
    let read = read_lines(&text, 0, 0);
    let at = |ev: &str| read.iter().position(|r| r.line.as_ref().is_ok_and(|l| serde_json::to_value(l).unwrap()["ev"] == ev && serde_json::to_value(l).unwrap()["t"] == 900)).unwrap_or_else(|| panic!("no {ev} of 900"));
    // the import is an open line with its close in it
    let (closed, reopened) = (at("open"), at("reopen"));
    let reference = joint_reference(&defs, &text, &library);
    assert!(reference.iter().all(|(f, _)| f.1.contains(&900) && f.0.iter().any(|(i, ..)| *i < closed)), "the item is held at the end, with its import fed");
    assert!(reference[1].0.0.len() > reference[0].0.0.len(), "the agents' threads feed more");
    assert!(reference.iter().all(|(_, (_, held))| held.compared > 0), "the joint compares tickets");
    let (went_on, refused) = joint_at_every_cut(&defs, &log, &text, &coord, &key, &library, &reference);
    for (k, why) in &refused {
        assert!(closed < *k && *k <= reopened && why.contains(&format!("{} ended at the cut and is held again after it (named again at line {})", core.tickets.m.id(900), reopened + 1)), "cut before line {}: {why}", k + 1);
    }
    let between = (closed + 1..=reopened).filter(|k| read[*k].line.as_ref().is_ok_and(Line::is_input)).count();
    assert!(between > 0 && refused.len() == between, "every cut after the close, up to the reopening, is refused: {} of {between} ({went_on} went on)", refused.len());
    let _ = std::fs::remove_dir_all(&coord);
}

/// What the joint's own whole replay of TEXT fed it (`joint::history`), and what it made of it, without the agents'
/// threads and with them.
fn joint_reference(defs: &Defs, text: &str, library: &str) -> Vec<(Fed, (Vec<crate::joint::Difference>, crate::joint::Held))> {
    [false, true]
        .into_iter()
        .map(|between| {
            let fed = history_fed(defs, text, between);
            let joint = through_joint(library, &fed);
            (fed, joint)
        })
        .collect()
}

/// At every input of TEXT (the record LOG, replayed with DEFS), a replay stopped there, saved in COORD for KEY, read back
/// and gone on to the end feeds the joint what REFERENCE says, and the joint makes the same of it, with the agents'
/// threads and without: else it fails, saying where. How many cuts went on, and those refused with why.
fn joint_at_every_cut(defs: &Defs, log: &str, text: &str, coord: &Path, key: &Key, library: &str, reference: &[(Fed, (Vec<crate::joint::Difference>, crate::joint::Held))]) -> (usize, Vec<(usize, String)>) {
    let c = coord.display().to_string();
    let read = read_lines(text, 0, 0);
    let inputs: Vec<usize> = (0..=read.len()).filter(|k| *k == read.len() || read[*k].line.as_ref().is_ok_and(Line::is_input)).collect();
    let (mut went_on, mut refused) = (0, Vec::new());
    for k in inputs {
        fresh(coord);
        saved_at(defs, log, text, k, &c, key);
        match gone_on(defs, log, text, &c, key) {
            Ok(ended) => {
                for (between, (fed, joint)) in [false, true].into_iter().zip(reference) {
                    let ours = ended_fed(&ended, text, between);
                    let d = fed_differs(&ours, fed);
                    assert!(d.is_empty(), "cut before line {}, threads {between}: {d:#?}", k + 1);
                    let j = through_joint(library, &ours);
                    assert!(j == *joint, "cut before line {}, threads {between}: the joint differs: {:#?}\nagainst {:#?}", k + 1, j.1, joint.1);
                }
                went_on += 1;
            }
            Err(why) => refused.push((k, why)),
        }
    }
    assert!(went_on > 0, "no cut went on: {refused:?}");
    (went_on, refused)
}

/// A ticket the machine ends, whose close the log records as an agents' thread's (`Between`): the replay's end closes
/// it as the log says (`Machine::replay_end`), so it is held at the end with the agents' threads, though the machine
/// had it ended at every cut after its close. The joint's history keeps its inputs there all the same
/// (`Machine::log_leaves_between`; paper-research N-1446 p1687): at every cut it is what the joint's own replay gave.
#[test]
fn a_ticket_the_logs_end_leaves_a_thread_keeps_its_history_though_the_machine_ended_it() {
    let (mut core, coord) = core("replay-between");
    drive(&mut core, &coord, "first", false);
    let th = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let x = &core.tickets.m.tickets[&th];
    assert!(x.kind == "thread" && !x.is_open() && !x.between(), "his thread, ended as done");
    drive(&mut core, &coord, "second", false);
    let (defs, log) = defs_of(&core);
    // its close, as the record has it, made an agents' thread's
    let recorded = std::fs::read_to_string(&log).unwrap();
    let mut edited = 0;
    let text: String = recorded
        .split_inclusive('\n')
        .map(|l| {
            let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
            if v["ev"] == "closed" && v["t"] == th {
                v["result"] = json!({"head": "Between"});
                edited += 1;
                format!("{v}\n")
            } else {
                l.to_string()
            }
        })
        .collect();
    assert_eq!(edited, 1, "one close of {th}");
    let key = Key::of(&defs, &[]).unwrap();
    let library = crate::joint::library_text(&base(), core.project.as_ref().map(|p| p.dir.as_str())).unwrap();
    // the machine ends it otherwise, before the replay's end; the end leaves it a thread
    let ((lines, places, unread), cut) = cut_at(&log, &text, text.split_inclusive('\n').count());
    let at = AtCut::replayed(defs.clone(), &text, &lines, &places, unread, cut);
    assert!(!at.m.tickets[&th].is_open() && !at.m.tickets[&th].between() && at.m.log_leaves_between(th));
    assert!(at.end(&log).m.tickets[&th].between());
    let reference = joint_reference(&defs, &text, &library);
    assert!(reference[1].0.1.contains(&th) && !reference[0].0.1.contains(&th), "held with the agents' threads only");
    assert!(reference[1].0.0.iter().any(|(_, _, s)| s.ids.contains_key(&th)), "its inputs fed");
    let (_, refused) = joint_at_every_cut(&defs, &log, &text, &coord, &key, &library, &reference);
    assert!(refused.is_empty(), "{refused:?}");
    let _ = std::fs::remove_dir_all(&coord);
}

/// The plan's test on research's copy (p1526): the whole record replayed as the reference; one replay stopped and saved
/// at each cut in turn (gone on from the cut before, as the saver goes on); then each state read back and gone on with
/// the record's lines after it to the end, compared part by part with the whole. And the joint's history (p1644): the
/// joint's own whole replay first (`joint::history`, the agents' threads held as the copy's `run-acts` says), then at
/// the whole and at each cut what the one replay feeds it is the same; with REPLAY_JOINT_FEED, through the joint too.
#[test]
#[ignore]
fn research_saved_states_go_on_as_the_whole_record() {
    let Some(cp) = copy() else { return };
    let key = Key::of(&cp.defs, &folders_of(&cp)).unwrap();
    let between = crate::joint::threads_on(&cp.coord);
    let through = std::env::var_os("REPLAY_JOINT_FEED").is_some();
    let library = crate::joint::library_text(&cp.base, cp.project.as_deref()).unwrap();
    let t = std::time::Instant::now();
    let reference = history_fed(&cp.defs, &cp.text, between);
    println!("the joint's own replay (threads {between}): {} inputs fed, {} tickets held, in {:.1} s; peak {} MB", reference.0.len(), reference.1.len(), t.elapsed().as_secs_f64(), peaks().0);
    let t = std::time::Instant::now();
    let joint_ref = through.then(|| through_joint(&library, &reference));
    if let Some((diffs, held)) = &joint_ref {
        println!("  through the joint in {:.1} s: {} differences, {} compared, {} differing", t.elapsed().as_secs_f64(), diffs.len(), held.compared, held.differed);
    }
    let mut failed: Vec<String> = Vec::new();
    // whether the one replay's history at NAME feeds the joint what its own replay did, said
    let joint_alike = |name: &str, e: &Ended| -> bool {
        let ours = ended_fed(e, &cp.text, between);
        let d = fed_differs(&ours, &reference);
        let joint = joint_ref.as_ref().map(|j| through_joint(&library, &ours) == *j);
        println!("  {name}: the joint's history kept {} inputs, fed {}: {}{}", e.joint.kept.len(), ours.0.len(), if d.is_empty() { "its own replay's".to_string() } else { format!("{d:#?}") }, match joint { Some(true) => "; through the joint, the same", Some(false) => "; through the joint, NOT the same", None => "" });
        d.is_empty() && joint != Some(false)
    };
    let t = std::time::Instant::now();
    let all = whole(&cp.defs, &cp.log, &cp.text);
    let want = ended_parts(&all);
    println!("whole: {} lines in {:.1} s, {} parts, {} errors; peak {} MB", all.lines, t.elapsed().as_secs_f64(), want.len(), all.errors.len(), peaks().0);
    if !joint_alike("whole", &all) {
        failed.push("joint at whole".into());
    }
    drop(all);
    let cuts = cuts(&cp.text);
    let mut at: Option<AtCut> = None;
    for (name, k) in &cuts {
        let t = std::time::Instant::now();
        let ((lines, places, unread), cut) = cut_at(&cp.log, &cp.text, *k);
        let how = match at.take().map(|a| {
            let (from, upto) = (a.cut.lines, cut.lines);
            let before = a.unread.len();
            a.on(&cp.text, &lines[from..upto], &places[from..upto], unread[before..].to_vec(), cut.clone())
        }) {
            Some(Ok(a)) => {
                at = Some(a);
                "gone on from the cut before".to_string()
            }
            Some(Err(why)) => format!("whole ({why})"),
            None => "whole".to_string(),
        };
        let a = at.take().unwrap_or_else(|| AtCut::replayed(cp.defs.clone(), &cp.text, &lines, &places, unread, cut));
        let replay_s = t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        let dir = format!("{}/{name}", cp.states);
        fresh(Path::new(&dir));
        let size = a.save(&dir, &key).unwrap();
        println!("cut {name} at file line {k} (line {}): replayed {how} in {replay_s:.1} s; saved in {:.1} s, {} MB", a.cut.lines, t.elapsed().as_secs_f64(), size >> 20);
        at = Some(a);
    }
    drop(at);
    for (name, k) in &cuts {
        let dir = format!("{}/{name}", cp.states);
        let t = std::time::Instant::now();
        let back = AtCut::read(&dir, &key, cp.defs.clone()).unwrap();
        let read_s = t.elapsed().as_secs_f64();
        let ((tail, places, unread), end) = tail_of(&cp.log, &cp.text, &back.cut);
        let n = tail.len();
        match back.on(&cp.text, &tail, &places, unread, end) {
            Ok(a) => {
                let ended = a.end(&cp.log);
                let d = differs(&ended_parts(&ended), &want);
                println!("cut {name} (file line {k}): read in {read_s:.1} s, {n} lines after it in {:.1} s in all: {}", t.elapsed().as_secs_f64(), if d.is_empty() { "the whole record's, every part".to_string() } else { format!("{d:#?}") });
                if !d.is_empty() {
                    failed.push(name.to_string());
                }
                if !joint_alike(name, &ended) {
                    failed.push(format!("joint at {name}"));
                }
            }
            Err(why) => println!("cut {name} (file line {k}): refused: {why}"),
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}

/// A start's cost on research's copy: REPLAY_FROM names a state's folder (REPLAY_STATES/NAME, made by the test above),
/// or none for the whole record as today; with REPLAY_JOINT set, the joint started after it on the history the start
/// took along (`joint::feed`, as its thread is fed), in the same process. REPLAY_JOINT_BEFORE: the joint's own replay
/// alone, as its thread ran it before I-1509 (`joint::history`), and no start. Times and peaks, one line each.
#[test]
#[ignore]
fn research_start_cost() {
    let Some(cp) = copy() else { return };
    let threads = crate::joint::threads_on(&cp.coord);
    if std::env::var_os("REPLAY_JOINT_BEFORE").is_some() {
        let t = std::time::Instant::now();
        let (lines, seen, only, held) = crate::joint::history(cp.defs.clone(), &cp.text, usize::MAX, threads);
        let (hwm, peak) = peaks();
        println!("the joint's own replay, before I-1509 (threads {threads}): {} lines in {:.1} s ({} inputs fed, {} tickets, {} MB held); VmHWM {hwm} MB, VmPeak {peak} MB", lines.len(), t.elapsed().as_secs_f64(), seen.len(), only.len(), held >> 20);
        return;
    }
    let from = std::env::var("REPLAY_FROM").ok();
    let t = std::time::Instant::now();
    let s = match &from {
        Some(name) => start(&format!("{}/{name}", cp.states), &cp.log, &cp.text, cp.defs.clone(), &folders_of(&cp), true),
        None => start(&cp.coord, &cp.log, &cp.text, cp.defs.clone(), &folders_of(&cp), false),
    };
    let (hwm, peak) = peaks();
    println!("start from {}: {:.1} s, VmHWM {hwm} MB, VmPeak {peak} MB; {}; the joint's history: {} inputs kept", from.as_deref().unwrap_or("nothing"), t.elapsed().as_secs_f64(), s.said, s.ended.joint.kept.len());
    if std::env::var_os("REPLAY_JOINT").is_some() {
        let t = std::time::Instant::now();
        let only = crate::joint::only(&s.ended.m, threads);
        let library = crate::joint::library_text(&cp.base, cp.project.as_deref()).unwrap();
        let mut j = crate::joint::Joint::holding(&library, only).unwrap();
        let mut n = 0usize;
        let fed = crate::joint::feed(&mut j, &cp.text, &s.ended.joint, &mut |_| n += 1);
        let (hwm, peak) = peaks();
        println!("the joint fed after it (threads {threads}): {fed} inputs in {:.1} s, {n} differences, {} compared, {} differing; VmHWM {hwm} MB, VmPeak {peak} MB", t.elapsed().as_secs_f64(), j.compared, j.differed);
    }
}

/// The saver's cost on research's copy (`coordd save-replay`): REPLAY_SAVE_FROM names a state's folder (REPLAY_STATES/
/// NAME), copied into a folder of its own beside the copy's record, or none for the saver with no state before it.
#[test]
#[ignore]
fn research_saver_cost() {
    let Some(cp) = copy() else { return };
    let dir = format!("{}/saver", cp.states);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(format!("{dir}/tickets/replay")).unwrap();
    std::os::unix::fs::symlink(&cp.log, format!("{dir}/tickets/log.jsonl")).unwrap();
    let from = std::env::var("REPLAY_SAVE_FROM").ok();
    if let Some(name) = &from {
        for e in std::fs::read_dir(format!("{}/{name}/{FOLDER}", cp.states)).unwrap().flatten() {
            std::fs::copy(e.path(), format!("{dir}/{FOLDER}/{}", e.file_name().to_string_lossy())).unwrap();
        }
    }
    drop(cp.text);
    let t = std::time::Instant::now();
    let said = save_on(&dir, &cp.base, cp.project.as_deref(), 1, &[]);
    let (hwm, peak) = peaks();
    println!("saver from {}: {:.1} s, VmHWM {hwm} MB, VmPeak {peak} MB; {said:?}", from.as_deref().unwrap_or("nothing"), t.elapsed().as_secs_f64());
    let _ = std::fs::remove_dir_all(&dir);
}
