//! Today's items moved onto tickets once (plan/edge.md S4, E1; paper-research m790): each record of
//! `$COORD/items/log.jsonl` becomes the machine's `Import` (an item ticket, its number kept) or `Started`, at the time it
//! was recorded. An item closed already is imported closed: its program never runs and nobody is told. An open one is
//! held (its `hold role lead close(...)`), with nothing delivered. When every item went in, the log is renamed
//! `items/log.jsonl.imported`, so the next start does not import it again; what happened is written to
//! `items/import.txt` either way.
//!
//! A close record says only why. It reads as Dropped when the why says the item was given up (withdrawn, subsumed,
//! dropped), else Done. A `started` on an item that was closed later is left out: the machine takes `started` only on
//! an open item (docket-core m809).

use crate::core::{Core, Who, err, ok};
use crate::project::Notes;
use coord_types::{ItemRecord, Reply, Request};
use docket_machine::{Closing, Effect, Input, parse_ticket};
use serde_json::json;
use std::collections::BTreeMap;

/// One item's history: how it was opened, how closed (the last close, unless reopened after it), who started it.
#[derive(Default)]
struct Item<'a> {
    opened: Option<&'a ItemRecord>,
    closed: Option<&'a ItemRecord>,
    started: Vec<&'a ItemRecord>,
}

/// The inputs RECORDS become, each with its time, item by item in number order; what could not be read; and what went
/// in other than as recorded, said in the report.
fn plan(records: &[ItemRecord]) -> (Vec<(Input, String)>, Vec<String>, Vec<String>) {
    let mut items: BTreeMap<u64, Item> = BTreeMap::new();
    let (mut bad, mut said) = (Vec::new(), Vec::new());
    for r in records {
        let Some(n) = docket_machine::parse_ticket(&r.id) else {
            bad.push(format!("{}: not an item number", r.id));
            continue;
        };
        let it = items.entry(n).or_default();
        match r.ev.as_str() {
            "opened" => {
                // a reopening undoes the close before it
                if it.opened.is_none() {
                    it.opened = Some(r);
                }
                it.closed = None;
            }
            "closed" => it.closed = Some(r),
            "started" => {
                if !it.started.iter().any(|s| s.by == r.by) {
                    it.started.push(r);
                }
            }
            ev => bad.push(format!("{}: no event {ev}", r.id)),
        }
    }
    let mut out = Vec::new();
    for (n, it) in items {
        // today's `coord item close` took any id (cut-over dry run, I-162): an id closed and never opened is
        // imported opened and closed by its closer, so that its number still names it and is not given again
        let (o, title) = match (it.opened, it.closed) {
            (Some(o), _) => (o, o.title.clone().unwrap_or_default()),
            (None, Some(c)) => {
                said.push(format!("I-{n:03}: closed and never opened; imported as opened and closed by {}", c.by));
                (c, "(closed in the item log without being opened)".to_string())
            }
            (None, None) => {
                bad.push(format!("I-{n:03}: never opened"));
                continue;
            }
        };
        let closed = it.closed.map(|c| {
            let why = c.why.clone().unwrap_or_default();
            Closing { by: c.by.clone(), at: c.at.clone(), head: json!({"head": head_of(&why), "why": why}), text: None }
        });
        let open = closed.is_none();
        let form = json!({"title": title, "note": null});
        out.push((Input::Import { t: n, kind: "item".into(), opener: o.by.clone(), form, text: None, closed }, o.at.clone()));
        if open {
            for s in it.started {
                out.push((Input::Started { t: n, by: s.by.clone() }, s.at.clone()));
            }
        }
    }
    (out, bad, said)
}

/// A close's result from its WHY: Dropped when the item was given up, else Done.
fn head_of(why: &str) -> &'static str {
    let w = why.to_lowercase();
    if ["withdrawn", "subsumed", "dropped", "abandoned", "given up"].iter().any(|x| w.contains(x)) { "Dropped" } else { "Done" }
}

/// Today's item verbs as ticket requests (E2): `open TITLE` opens an item; `close ID WHY` answers its close Done and
/// `drop ID WHY` Dropped (whoever policy's `may edit` lets; docket-core m807); `start ID` stays an item request,
/// answered by `Core::item_start`.
pub fn item_request(action: &str, arg: String, why: Option<String>) -> Result<Request, String> {
    let why = why.filter(|w| !w.trim().is_empty());
    match (action, why) {
        ("open", _) if !arg.trim().is_empty() => Ok(Request::Open { kind: "item".into(), under: None, form: json!({"title": arg.trim()}), text: None }),
        ("close", Some(why)) => Ok(Request::Answer { t: arg, head: json!({"head": "Done"}), text: Some(why) }),
        ("drop", Some(why)) => Ok(Request::Answer { t: arg, head: json!({"head": "Dropped"}), text: Some(why) }),
        ("start", _) => Ok(Request::Item { action: action.into(), arg, why: None }),
        _ => Err("usage: coord item start ID | open TITLE | close ID WHY | drop ID WHY".into()),
    }
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("git").arg("-C").arg(repo).args(args).output().map_err(|e| format!("git: {e}"))?;
    if !o.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim_end().to_string())
}

impl Notes {
    /// The names of the files in the notes folder: the folder's, or the line's head's (none when it has no such folder).
    fn names(&self) -> Vec<String> {
        match self {
            Notes::Dir(dir) => std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()).unwrap_or_default(),
            Notes::Line { repo, root, rel, .. } => git(repo, &["ls-tree", "--name-only", &format!("{root}:{rel}")]).map(|o| o.lines().map(str::to_string).collect()).unwrap_or_default(),
        }
    }

    /// The text of note file NAME, as the folder or the line's head has it.
    fn read(&self, name: &str) -> Option<String> {
        match self {
            Notes::Dir(dir) => std::fs::read_to_string(format!("{dir}/{name}")).ok(),
            Notes::Line { repo, root, rel, .. } => git(repo, &["show", &format!("{root}:{rel}/{name}")]).ok(),
        }
    }
}

/// The numbers of the item notes in NOTES: each `I-NNN-SLUG.md` or `I-NNN.md`.
fn note_numbers(notes: &Notes) -> Vec<u64> {
    notes.names().iter().filter_map(|f| note_number(f)).collect()
}

/// The note of item N in NOTES (`I-NNN-*.md` or `I-NNN.md`, any width): its title and status from the front matter.
fn read_note(notes: &Notes, n: u64) -> Option<(String, String)> {
    let name = notes.names().into_iter().find(|f| note_number(f) == Some(n))?;
    let text = notes.read(&name)?;
    let field = |k: &str| text.lines().skip(1).take_while(|l| *l != "---").find_map(|l| l.strip_prefix(&format!("{k}:")).map(|v| v.trim().to_string()));
    Some((field("title").unwrap_or_else(|| name.trim_end_matches(".md").to_string()), field("status").unwrap_or_default()))
}

/// The item number a note's file name gives: `I-NNN-SLUG.md` or `I-NNN.md`.
fn note_number(file: &str) -> Option<u64> {
    let rest = file.strip_suffix(".md")?.strip_prefix("I-")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty() && (rest.len() == digits.len() || rest[digits.len()..].starts_with('-'))).then(|| digits.parse().ok()).flatten()
}

/// TEXT as file PATH on top of branch ROOT's head in REPO, without a checkout (INDEX a scratch index file), on a new
/// branch `note/ID` (MESSAGE's first word): its commit. The branch keeps the commit until it is on the line; it is
/// never the line itself, which only the runner moves.
fn commit_note(repo: &str, root: &str, path: &str, text: &str, index: &str, message: &str) -> Result<String, String> {
    use std::io::Write;
    let head = git(repo, &["rev-parse", "--verify", "--quiet", &format!("{root}^{{commit}}")]).map_err(|_| format!("{repo} has no branch {root}"))?;
    let mut c = std::process::Command::new("git").arg("-C").arg(repo).args(["hash-object", "-w", "--stdin"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| format!("git: {e}"))?;
    c.stdin.take().map(|mut i| i.write_all(text.as_bytes())).transpose().map_err(|e| format!("git hash-object: {e}"))?;
    let o = c.wait_with_output().map_err(|e| format!("git hash-object: {e}"))?;
    if !o.status.success() {
        return Err(format!("git hash-object: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    let blob = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let with_index = |args: &[&str]| -> Result<String, String> {
        let o = std::process::Command::new("git").arg("-C").arg(repo).env("GIT_INDEX_FILE", index).args(args).output().map_err(|e| format!("git: {e}"))?;
        if !o.status.success() {
            return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&o.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    with_index(&["read-tree", &head])?;
    with_index(&["update-index", "--add", "--cacheinfo", &format!("100644,{blob},{path}")])?;
    let tree = with_index(&["write-tree"])?;
    let commit = git(repo, &["commit-tree", &tree, "-p", &head, "-m", message])?;
    let id = message.split(':').next().unwrap_or_default();
    git(repo, &["update-ref", &format!("refs/heads/note/{id}"), &commit, ""])?;
    Ok(commit)
}

/// Whether a note's status says the item is over (as orch's notes write it).
fn closed_status(status: &str) -> bool {
    matches!(status, "closed" | "withdrawn" | "landed")
}

impl Core {
    /// The number after the highest item note in the project's notes folder, if it has one with notes.
    pub fn item_number(&self) -> Option<u64> {
        let notes = self.project.as_ref().and_then(|p| p.notes())?;
        note_numbers(&notes).into_iter().max().map(|n| n + 1)
    }

    /// `coord item start ID`: the agent works on the open item ID (its ticket's started line); its view says so. The
    /// vault's notes count (orch m14266, m14290): an id with only a note is taken in as an item ticket at its number
    /// (closed too when its note says so); a closed item whose note still says open is reopened.
    pub fn item_start(&mut self, who: &Who, id: &str) -> Reply {
        let Who::Agent(me) = who else { return err("coord item start: an agent says what it works on".into()) };
        let Some(t) = parse_ticket(id) else { return err(format!("coord item start: no item {id}")) };
        let note = || self.project.as_ref().and_then(|p| p.notes()).and_then(|notes| read_note(&notes, t));
        match self.tickets.m.tickets.get(&t).map(|x| (x.kind == "item", x.is_open())) {
            Some((false, _)) => return err(format!("coord item start: no item {id}")),
            Some((true, true)) => {}
            Some((true, false)) => match note() {
                // the vault's note wins over the closed ticket: still open, the item is open again
                Some((_, status)) if !closed_status(&status) => {
                    if let Err(e) = self.ticket_input(Input::Reopen { t, by: me.clone() }) {
                        return err(format!("coord item start: {e}"));
                    }
                }
                _ => return err(format!("coord item start: {} is closed", self.tickets.m.id(t))),
            },
            None => {
                // an item written straight into the notes folder: its note makes it known
                let Some((title, status)) = note() else { return err(format!("coord item start: no item {id}")) };
                let closed = closed_status(&status).then(|| {
                    let why = format!("its note says status: {status}");
                    Closing { by: "notes".into(), at: crate::core::now(), head: json!({"head": head_of(&why), "why": why}), text: None }
                });
                let shut = closed.is_some();
                let input = Input::Import { t, kind: "item".into(), opener: "notes".into(), form: json!({"title": title, "note": null}), text: None, closed };
                if let Err(e) = self.ticket_input(input) {
                    return err(format!("coord item start: {e}"));
                }
                if shut {
                    return err(format!("coord item start: {} is closed", self.tickets.m.id(t)));
                }
            }
        }
        let me = me.clone();
        match self.ticket_input(Input::Started { t, by: me.clone() }) {
            Ok(_) => {
                self.changed(&me);
                ok(format!("{me} works on {}", self.tickets.m.id(t)))
            }
            Err(e) => err(format!("coord item start: {e}")),
        }
    }

    /// After `coord item open TITLE` opened item ID (REPLY, `Opened I-n.`): its note written in the project's notes
    /// folder, if it has one, and named on the ticket by a post; the reply is today's `ID` or `ID<TAB>PATH`. Where the
    /// notes are on the line (`Notes::Line`), the note is a commit on the line's head, on branch `note/ID`, handed in
    /// through the runner as a ticket of kind `note` under the item: the reply is `ID<TAB>note/ID`. While the runner is
    /// down that hand-in waits for it as any runner step does; the item is open and numbered meanwhile.
    pub fn item_opened(&mut self, who: &Who, title: &str, reply: Reply) -> Reply {
        let Some(id) = reply.out.strip_prefix("Opened ").map(|r| r.trim_end_matches('.').to_string()).filter(|_| reply.ok) else { return reply };
        let Some(notes) = self.project.as_ref().and_then(|p| p.notes()) else { return ok(id) };
        let dir = match notes {
            Notes::Dir(dir) => dir,
            Notes::Line { repo, root, rel, .. } => {
                let (name, text) = crate::core::note_text(&id, title, &who.name());
                let index = format!("{}/.note-index-{id}", self.places().coord);
                let made = commit_note(&repo, &root, &format!("{rel}/{name}"), &text, &index, &format!("{id}: its note ({title}), opened by {}", who.name()));
                let _ = std::fs::remove_file(&index);
                let sha = match made {
                    Ok(sha) => sha,
                    Err(e) => return ok(format!("{id} (no note: {e})")),
                };
                let branch = format!("note/{id}");
                let r = self.ticket_open(who, "note".into(), Some(id.clone()), json!({"commit": sha}), None, Default::default());
                if !r.ok {
                    return ok(format!("{id} (its note is on {branch}, not handed in: {})", r.out));
                }
                let n = r.out.strip_prefix("Opened ").map(|r| r.trim_end_matches('.').to_string()).unwrap_or(r.out);
                let _ = self.ticket_post(who, &id, format!("The note: {rel}/{name}, committed on {branch}; {n} hands it in, and it is on the line when the runner has taken it."), None, None, Default::default(), None, false);
                return ok(format!("{id}\t{branch}"));
            }
        };
        match crate::core::write_note(&dir, &id, title, &who.name()) {
            Ok(path) => {
                let _ = self.ticket_post(who, &id, format!("The note: {path}"), None, None, Default::default(), None, false);
                ok(format!("{id}\t{path}"))
            }
            Err(e) => ok(format!("{id} (no note: {e})")),
        }
    }

    /// Import `items/log.jsonl` onto tickets, once (the module's doc). Called at start, after the tickets are loaded
    /// and before `ticket::start`.
    pub fn import_items(&mut self) {
        let path = self.path("items/log.jsonl");
        if !std::path::Path::new(&path).exists() {
            return;
        }
        let (inputs, mut refused, said) = plan(&self.items);
        let (mut imported, mut closed, mut started, mut woke) = (0, 0, 0, Vec::new());
        let log = self.path("tickets/log.jsonl");
        for (input, at) in inputs {
            let what = match &input {
                Input::Import { t, closed: c, .. } => format!("I-{t:03} import{}", if c.is_some() { " (closed)" } else { "" }),
                Input::Started { t, by } => format!("I-{t:03} started by {by}"),
                _ => String::new(),
            };
            let was_closed = matches!(&input, Input::Import { closed: Some(_), .. });
            let is_import = matches!(&input, Input::Import { .. });
            let out = self.tickets.m.apply(input, at);
            for l in &out.lines {
                crate::jsonl::later(&log, serde_json::to_string(l).expect("a line is JSON"));
            }
            // nothing imported may wake anyone: a delivery or a notice would be a fault, and is reported
            for e in out.effects {
                match e {
                    Effect::Deliver { .. } => woke.push(format!("{what}: a delivery")),
                    Effect::Notify { .. } => woke.push(format!("{what}: a notice")),
                    _ => {}
                }
            }
            // the effects are not carried out here: this runs at start before the task runner is (ticket::start),
            // whose resume_effects asks each open item's fills then; nobody watches yet
            match out.reply {
                Ok(_) if is_import => {
                    imported += 1;
                    closed += was_closed as usize;
                }
                Ok(_) => started += 1,
                Err(e) => refused.push(format!("{what}: {e}")),
            }
        }
        let mut report = format!(
            "{}: {imported} items imported from {path} ({closed} closed, {} open), {started} started; {} refused; {} woke anyone\n",
            crate::core::now(),
            imported - closed,
            refused.len(),
            woke.len()
        );
        for r in refused.iter().chain(&woke).chain(&said) {
            report.push_str(&format!("  {r}\n"));
        }
        let done = refused.is_empty() && woke.is_empty();
        if done {
            match std::fs::rename(&path, format!("{path}.imported")) {
                Ok(()) => report.push_str("  items/log.jsonl renamed items/log.jsonl.imported\n"),
                Err(e) => report.push_str(&format!("  items/log.jsonl not renamed: {e}\n")),
            }
        } else {
            report.push_str("  items/log.jsonl left as it is: an import that did not all go in is looked at first\n");
        }
        eprint!("coordd: {report}");
        let _ = std::fs::write(self.path("items/import.txt"), report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ev: &str, id: &str, by: &str, extra: serde_json::Value) -> ItemRecord {
        let mut v = json!({"ev": ev, "id": id, "at": "2026-11-03T00:00:00Z", "by": by});
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn items_become_imports_closed_ones_closed_and_started_only_on_open_ones() {
        let rs = vec![
            rec("opened", "I-001", "owner", json!({"title": "one"})),
            rec("closed", "I-001", "owner", json!({"why": "carried over from the note's status: withdrawn"})),
            rec("opened", "I-002", "orch", json!({"title": "two"})),
            rec("started", "I-002", "TIDY", json!({})),
            rec("started", "I-002", "TIDY", json!({})),
            rec("started", "I-002", "CONE", json!({})),
            rec("opened", "I-003", "owner", json!({"title": "three"})),
            rec("started", "I-003", "LAZY", json!({})),
            rec("closed", "I-003", "orch", json!({"why": "landed in batch 42"})),
            rec("closed", "I-004", "orch", json!({"why": "landed in batch 53"})),
        ];
        let (inputs, bad, said) = plan(&rs);
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(said, ["I-004: closed and never opened; imported as opened and closed by orch"]);
        let shown: Vec<String> = inputs
            .iter()
            .map(|(i, _)| match i {
                Input::Import { t, opener, closed, form, .. } => format!("{t} {opener} {} {}", form["title"].as_str().unwrap(), closed.as_ref().map_or("open".into(), |c| c.head["head"].to_string())),
                Input::Started { t, by } => format!("{t} started {by}"),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(shown, ["1 owner one \"Dropped\"", "2 orch two open", "2 started TIDY", "2 started CONE", "3 owner three \"Done\"", "4 orch (closed in the item log without being opened) \"Done\""]);
    }

    /// The real log in ITEMS_LOG (a copy), imported into a fresh machine with agent-base's kinds: every item goes in,
    /// and nothing is delivered or noticed. `ITEMS_LOG=... cargo test --release -p coordd -- --ignored real_items`
    #[test]
    #[ignore]
    fn real_items_import_and_wake_nobody() {
        let path = std::env::var("ITEMS_LOG").expect("ITEMS_LOG");
        let rs: Vec<ItemRecord> = std::fs::read_to_string(&path).unwrap().lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
        let coord = std::env::temp_dir().join(format!("coordd-items-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut t = crate::ticket::Tickets::load(&coord.display().to_string(), &base, None, 1);
        assert!(t.errors.is_empty(), "{:?}", t.errors);
        let (inputs, bad, _) = plan(&rs);
        assert!(bad.is_empty(), "{bad:?}");
        let (mut ok, mut refused, mut woke, mut asks) = (0, Vec::new(), 0, 0);
        for (i, at) in inputs {
            let out = t.m.apply(i, at);
            for e in &out.effects {
                match e {
                    Effect::Deliver { .. } | Effect::Notify { .. } => woke += 1,
                    Effect::Ask { .. } => asks += 1,
                    _ => {}
                }
            }
            match out.reply {
                Ok(_) => ok += 1,
                Err(e) => refused.push(e),
            }
        }
        let open = t.m.tickets.values().filter(|x| matches!(x.status, docket_machine::Status::Asking { .. } | docket_machine::Status::Awaits { .. })).count();
        eprintln!("{} records: {ok} inputs went in, {} refused, {woke} woke anyone, {asks} asks; {} tickets, {open} open", rs.len(), refused.len(), t.m.tickets.len());
        std::fs::remove_dir_all(&coord).ok();
        assert!(refused.is_empty() && woke == 0, "{refused:?}");
    }
}

/// `coord ready --item` (agent-base/bin/coord): the project file's `ready_check = false` keeps the check ticket from
/// opening; absent or true, it opens as before.
#[cfg(test)]
mod ready_check_tests {
    use std::path::Path;
    use std::process::Command;

    fn sh(dir: &Path, cmd: &str) {
        let out = Command::new("bash").arg("-c").arg(cmd).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "{cmd}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// `coord ready ARGS` in a fresh repo whose branch is ahead of dev, with the project file holding PROJECT: its
    /// exit status, its stdout, and the arguments coordc was called with (none: it was not).
    fn ready(name: &str, project: Option<&str>, args: &[&str]) -> (i32, String, Option<String>) {
        let t = std::env::temp_dir().join(format!("coord-ready-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "repo", "coord", "proj"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let coord_sh = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agent-base/bin/coord").canonicalize().unwrap();
        std::os::unix::fs::symlink(&coord_sh, t.join("bin/coord")).unwrap();
        let fake = format!("#!/bin/sh\necho \"$*\" >{}/coordc.args\necho 'check t12 opened'\n", t.display());
        std::fs::write(t.join("bin/coordc"), fake).unwrap();
        sh(&t, "chmod +x bin/coordc");
        let repo = t.join("repo");
        // fixed dates: each call's repo then has the same commits, so its sha is the same (two calls a second apart
        // gave two shas, and the last lines differed)
        sh(&repo, "export GIT_AUTHOR_DATE=2026-01-01T00:00:00Z GIT_COMMITTER_DATE=2026-01-01T00:00:00Z && git init -q -b dev && git -c user.name=t -c user.email=t@t commit -q --allow-empty -m one \
                   && git checkout -q -b feature && git -c user.name=t -c user.email=t@t commit -q --allow-empty -m two");
        if let Some(text) = project {
            std::fs::write(t.join("proj/project.toml"), text).unwrap();
            std::fs::write(t.join("coord/project"), format!("{}\n", t.join("proj/project.toml").display())).unwrap();
        }
        let out = Command::new("bash")
            .arg(t.join("bin/coord"))
            .arg("ready")
            .args(args)
            .current_dir(&repo)
            .env("AGENT_NAME", "a1")
            .env("COORD", t.join("coord"))
            .env("COORDD_SOCK", t.join("none.sock"))
            .output()
            .unwrap();
        let called = std::fs::read_to_string(t.join("coordc.args")).ok();
        let queued = std::fs::read_to_string(t.join("coord/queue/a1.json")).unwrap_or_default();
        assert!(queued.contains("\"branch\":\"feature\""), "{name}: queue {queued:?}; {}", String::from_utf8_lossy(&out.stderr));
        let _ = std::fs::remove_dir_all(&t);
        (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), called)
    }

    fn last(s: &str) -> &str {
        s.lines().last().unwrap_or("")
    }

    #[test]
    fn ready_check_false_writes_the_queue_and_opens_no_check() {
        let off = "[project]\nnotes = \"items\"\nready_check = false  # no runner answers a check here\n\n[[view]]\nid = \"v\"\n";
        let (code, out, called) = ready("off", Some(off), &["--item", "7"]);
        assert_eq!(code, 0, "{out}");
        assert_eq!(called, None, "no check ticket");
        let (_, plain, _) = ready("plain", Some(off), &[]);
        assert_eq!(last(&out), last(&plain), "the same last line as without --item");
        assert!(last(&out).starts_with("queued feature at "), "{out}");
    }

    #[test]
    fn without_the_key_or_with_it_true_the_check_opens_as_before() {
        let cases = [
            ("absent", None),
            ("nokey", Some("[project]\nnotes = \"items\"\n")),
            ("true", Some("[project]\nready_check = true\n")),
            // the key in another table is not the project's
            ("elsewhere", Some("[project]\nnotes = \"items\"\n\n[other]\nready_check = false\n")),
        ];
        for (name, project) in cases {
            let (code, out, called) = ready(name, project, &["--item", "7"]);
            assert_eq!(code, 0, "{name}: {out}");
            let called = called.unwrap_or_else(|| panic!("{name}: no check ticket"));
            assert!(called.starts_with("open check --under 7 "), "{name}: {called}");
            assert_eq!(last(&out), "check t12 opened", "{name}");
        }
    }
}

/// Item notes on the line (N-101 post 406; the joint proposal's J): a project whose notes folder lies in one of its
/// repositories and that has a line reads its notes from the line's head and hands a new one in through the runner.
#[cfg(test)]
mod notes_on_the_line_tests {
    use super::*;
    use crate::project::Notes;

    fn sh(dir: &std::path::Path, cmd: &str) -> String {
        let o = std::process::Command::new("bash").arg("-c").arg(cmd).current_dir(dir).output().unwrap();
        assert!(o.status.success(), "{cmd}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    #[test]
    fn a_new_note_is_a_commit_on_the_lines_head_handed_to_the_runner_and_notes_are_read_from_the_line() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let t = std::env::temp_dir().join(format!("docket-notes-line-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let vault = t.join("vault");
        std::fs::create_dir_all(vault.join("items")).unwrap();
        std::fs::write(vault.join("items/I-005-old.md"), "---\ntitle: old\nstatus: open\n---\n").unwrap();
        std::fs::write(vault.join("items/I-007-shut.md"), "---\ntitle: shut\nstatus: closed\n---\n").unwrap();
        sh(&vault, "git init -q -b landed && git -c user.name=t -c user.email=t@t add . && git -c user.name=t -c user.email=t@t commit -qm one");
        // a note only in the checkout, not on the line: not seen
        std::fs::write(vault.join("items/I-040-loose.md"), "---\ntitle: loose\nstatus: open\n---\n").unwrap();
        let coord = t.join("coord");
        let project = coord.join("files");
        std::fs::create_dir_all(&project).unwrap();
        let v = std::fs::canonicalize(&vault).unwrap().display().to_string();
        std::fs::write(project.join("project.toml"), format!("[project]\nname = \"n\"\nrepos = {{ vault = \"{v}\" }}\nrepo = \"vault\"\nroot = \"landed\"\nnotes = \"{v}/items\"\nrunner = \"sh runner.sh\"\n")).unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert_eq!(core.project.as_ref().unwrap().notes(), Some(Notes::Line { name: "vault".into(), repo: v.clone(), root: "landed".into(), rel: "items".into() }));
        assert_eq!(core.item_number(), Some(8), "the line's notes count, the checkout's loose one does not");
        let head = sh(&vault, "git rev-parse landed");

        // open: the item, its note a commit on the line's head on note/ID, and a `note` ticket under it for the runner
        assert!(core.add(&Who::Owner, "dev", "claude", "claude-haiku-5-5").ok);
        let who = Who::Agent("dev".into());
        let r = core.ticket_open(&who, "item".into(), None, json!({"title": "A new one"}), None, Default::default());
        assert!(r.ok, "{}", r.out);
        let r = core.item_opened(&who, "A new one", r);
        assert_eq!(r.out, "I-008\tnote/I-008");
        assert_eq!(sh(&vault, "git rev-parse landed"), head, "the line is the runner's to move");
        assert_eq!(sh(&vault, "git rev-parse note/I-008^"), head, "on the line's head");
        assert_eq!(sh(&vault, "git diff --name-only landed note/I-008"), "items/I-008-a-new-one.md");
        assert!(sh(&vault, "git show note/I-008:items/I-008-a-new-one.md").contains("title: A new one"));
        assert!(!vault.join("items/I-008-a-new-one.md").exists(), "nothing written in the checkout");
        assert!(!coord.join(".note-index-I-008").exists(), "the scratch index removed");
        let sha = sh(&vault, "git rev-parse note/I-008");
        let note = core.tickets.m.tickets.values().find(|x| x.kind == "note").map(|x| x.t).expect("a note ticket");
        let d = core.due_now("runner");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].t, note);
        let docket_machine::What::Step { args, call, .. } = &d[0].what else { panic!("{:?}", d[0].what) };
        assert_eq!(call, "hand_in");
        let out = crate::runner::step_json(call, &crate::render::resolved(args), &core.runner_place(note));
        assert_eq!(out, json!({"commits": {"vault": sha}}), "as any hand-in, by repository");

        // the line's notes decide: I-007's note says closed, so it is taken in closed
        let r = core.item_start(&who, "I-007");
        assert!(!r.ok && r.out.contains("closed"), "{}", r.out);
        let r = core.item_start(&who, "I-005");
        assert!(r.ok, "{}", r.out);
        let r = core.item_start(&who, "I-040");
        assert!(!r.ok, "a note not on the line makes no item: {}", r.out);
        let _ = std::fs::remove_dir_all(&t);
    }
}
