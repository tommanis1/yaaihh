//! coordc: coordd's client (design §5, §7). `coord` runs it for the mail and agent commands; the hooks and roster run
//! it directly.
//!
//!   coordc msg NAME[,NAME...] TEXT...        a thread with NAME (S5: a `thread` ticket opened by me, naming them); it
//!                                              reaches them when their turn ends, or at once out of a turn
//!   coordc msg --re ID NAME[,...] TEXT         a post on ticket ID (or on the ticket of post ID) naming NAME and
//!                                              the author replied to; an old mail id (m12) opens a thread `re m12`;
//!                                              with --re or --thread, KIND as for post says what the post is
//!   coordc msg --thread ID TEXT...             a post on ticket ID naming everyone in it but me
//!                                              NAME may be another project's agent, PROJECT/NAME (where the project
//!                                              file says host_relay = true); its answer comes on the same ticket.
//!                                              TEXT - : the text from stdin
//!   coordc esc NAME[,NAME...]                  stop NAME's running turn, nothing else (agents below me; the norms'
//!                                              `esc`); to say something too, coord msg NAME TEXT after it
//!   coordc esc --all                           stop the running turn of every agent of the project in one, nothing
//!                                              else: they are not kept stopped (the owner's, or as esc for each)
//!   coordc hold-mail on|off                    (the owner) hold agents' mail: only the owner's posts pass, until
//!                                              off; one record line each way
//!   coordc effort NAME LEVEL                   NAME runs at effort LEVEL from now on (agents below me): its record
//!                                              at once; a Claude Code agent is restarted on its own session once its
//!                                              turn has ended, a Codex agent's next turn takes it
//!   coordc model NAME MODEL                    a Claude Code agent's model, as effort: recorded with its effort, and
//!                                              its session restarted on it once its turn has ended (no gpt-*)
//!   coordc can [--json] [--as NAME] [THING...] the acts I may do now at THING (a ticket; names: the acts naming any
//!                                              of them; none: all), as the norms stand; greyed ones with the
//!                                              clause that fails (the owner: --as)
//!   coordc duties [--json] [--as NAME]        the duties I hold now (the owner: NAME's, or all), violated ones
//!                                              said, with the acts that end each
//!   coordc compact NAME TOKENS                 where a Claude Code or Codex agent compacts its context from its next start
//!                                              (100000 to 1000000, or 0: the coord's own); not restarted for it
//!   coordc native NAME on|off                  (the owner) a Claude Code agent runs on one of the owner's own accounts,
//!                                              chosen by tier and turn, with nothing of ours between it and
//!                                              Anthropic; off: through the project's model route again. A running
//!                                              session restarts on its conversation once its turn has ended
//!   coordc account move NAME [ACCOUNT]         (the owner) NAME goes to ACCOUNT, else to the next by tier and turn
//!   coordc accounts                            the owner's accounts: id, label, tier, state, agents
//!   coordc account new ID [LABEL] [--tier N] [--key]   (the owner) a new account's folder, to be signed in to;
//!                                              --key: one that runs on an API key (coordc account key ID)
//!   coordc account key ID [--remove]           (the owner, in a terminal of his own) asks for ID's API key without
//!                                              showing it and puts it in ID's folder; --remove: deletes it
//!   coordc account tier ID N                   (the owner) its tier: a larger number is taken first
//!   coordc account signin ID [--email EMAIL]   (the owner) start Claude Code's sign-in for ID; prints the URL to open
//!   coordc account code ID [CODE]              (the owner) the code the browser showed (else read from stdin)
//!   coordc account check ID                    ask Claude Code whether ID is signed in, and record it
//!   coordc account cancel ID                   (the owner) end ID's running sign-in
//!   coordc account clear ID                    (the owner) ID can take requests again (out, reset unknown)
//!   coordc account remove ID                   (the owner) ID is no account any more: Claude Code signs it out and
//!                                              its folder is set aside; refused while an agent runs on it
//!   coordc account-for NAME                    roster's, at each start of a Claude Code agent: the folder its
//!                                              session runs with (CLAUDE_CONFIG_DIR), or nothing (the model route);
//!                                              of an opencode agent: its opencode account's folder and folders
//!                                              (XDG_*=, TMPDIR=), or nothing (no account of its provider)
//!   coordc mail [ID]                           old mail, read only (before tickets); new messages are tickets
//!   coordc ask [--on Q] SUBJECT...             a question ticket, the explanation on stdin; prints its id at once
//!                                              (its answer comes when it closes); --on Q: a post on my question Q
//!   coordc ask --drop Q WHY...                 withdraw my question Q (a cancel)
//!   coordc ask --drop T.pN WHY...              withdraw the asks of my post N of T (the owner is told nothing)
//!   coordc ask --list                          my questions: id, state, ask
//!   coordc ls                                  each agent: name, id, state, parent (also: state)
//!   coordc log NAME [LAST]                     an agent's session as text (agents below me), LAST entries (60)
//!   coordc item start ID | open TITLE | close ID WHY... | drop ID WHY...   (an item is a ticket: open, its close answered)
//!   coordc add NAME KIND MODEL [--renderings SET] [--from FROM]  register an agent; I become its parent
//!   coordc new NAME [--model M] TEXT...        a fresh agent NAME made from my turn, TEXT its first words (make-agent)
//!   coordc fork [FROM] [--name NEW] [--model M] [TEXT...]   a copy of FROM's conversation (mine if none) as NEW
//!   coordc go [ID]                             move me to layer ID (or ticket ID's layer) when my turn ends; none: where I stand and my steps' layers
//!   coordc start NAME [MODEL PROMPT-FILE DIR]  (the owner, roster in a boxed project) start NAME through coordd, its
//!                                              own user; no MODEL: again on its own conversation
//!   coordc role NAME ROLE                      (the owner) NAME's role, a word the policy's norms name
//!   coordc ended [--undo] NAME                 (the owner) end agent NAME for good: never started again, given no
//!                                              step, its sessions kept; --undo takes it back
//!   coordc jobs                                the runner's running jobs: who, wall time, memory, bound (RUNNER jobs)
//!   coordc store [ROW] [--from N]              what the store holds, as JSON (Home's Store): its rows, or ROW's entries
//!                                              from N, 200 at a time (the runner's service, `build-runner store`)
//!   coordc end JOB WHY...                      (the owner, or a lead) end the runner's job JOB; its step answers Ended
//!   coordc act IS TEXT [FIELD=VALUE...]       (an agent's tool) an act of the agent, recorded (the symbols server's
//!                                              edit: coordc act edit TEXT tool=rename symbol=S 'files=["a.rs"]')
//!   coordc check [--quick|--typecheck|--proofs] [--flip STAGE]... [REPO:BRANCH...]   (an agent) its own check, through coordd to the
//!                                              project's runner; none named: its layer's branch in every repository.
//!                                              Its answer comes as a message
//!   coordc team ask TEXT                       (an agent) ask to form a team: TEXT goes to its approvers (the norms' approve-team)
//!   coordc team grant NAME N                   NAME's team may have N agents (the owner, or an approver)
//!   coordc norm ACT [--thread T] ARG...        whether the norms enable ACT(ARG...) now, and why (e.g. send-mail A B none)
//!   coordc run NAME -- CMD...                  this process becomes agent NAME, then runs CMD
//!   coordc session NAME JSON                   session facts of NAME before its first hook (roster: a Codex thread)
//!   coordc hook EVENT                          a hook event (its input on stdin); it prints nothing
//!   coordc watch [FROM]                        the event stream, one JSON line per event
//!   coordc bridge                              yaaihh's pipe: stdin {"id":N,"req":REQUEST} lines are answered as
//!                                              {"id":N,"reply":REPLY}, between the stream's {"event":EVENT} lines
//!                                              and, for the terminals it attached, {"pty":PTYEVENT} lines
//!   coordc attach NAME                         NAME's terminal in this one (Ctrl-\ detaches)
//!   coordc keys NAME TEXT                      type TEXT into NAME's terminal
//!   coordc screen NAME                         NAME's screen as text
//!   coordc hangup NAME                         hang up NAME's terminal (SIGHUP to its program), wait for its end
//!   coordc terms                               each terminal: name, size, title, command
//!   coordc layers                              every layer of the project, open or ended, and the runner's running
//!                                              replays' (`kind` "run"), as JSON, with what lies under each (layer.rs)
//!   coordc layer-sync                          bring every open boxed layer's branch into the project's repository
//!   coordc open KIND [--under T] [--FIELD V]... [TEXT... | -]   open a ticket of KIND, its form's fields by flag,
//!                                              TEXT its first post (`-`: stdin; with no field and no TEXT, stdin
//!                                              when it is not a terminal); KIND alone prints the form
//!   coordc answer T HEAD [--FIELD V]... [TEXT... | -]   answer my step on T explicitly (a sole text field from TEXT;
//!                                              stdin as for open)
//!   coordc post [KIND] T TEXT...               a post on ticket T (TEXT or stdin); KIND, what it is:
//!                                              --ask NAME[,NAME...], --read NAME[,NAME...] HEAD, --blocker
//!                                              NAME[,NAME...], --hold I-N, --result or --fyi; none, untyped
//!   coordc say NAME TEXT...                    the owner's message to agent NAME, as the composer sends it: a thread
//!                                              (its @NAMEs tagged), or a one-off by the watcher's intake
//!   coordc move T N [TO]                       the owner moves post N of T to ticket TO, or to a new thread
//!   coordc keep T...                           the owner makes consecutive one-offs to one agent a thread
//!   coordc drop-thread T                       the owner makes resting thread T a one-off after all
//!   coordc take-back T.pN                      take back my post N of T while nobody has been handed it
//!   coordc pass T.pN NAME [WHY...]             pass the owner's @all (post N of T), handed to me, on to agent NAME
//!   coordc cancel T WHY...                     cancel ticket T: its step withdrawn, its open children cancelled
//!   coordc star ID / coordc unstar ID          star ticket ID (N-221) or a post (N-221.p5), or take my star off:
//!                                              a mark the owner's starred list shows, with who set it
//!   coordc archive T / coordc unarchive T      the owner: T is no longer of importance to him and leaves his lists;
//!                                              nothing at or under it is handed meanwhile (not an end)
//!   coordc slow T on|off                       the owner: thread T and the tickets under it halt before each step given,
//!                                              answer taken, post delivered, ticket opened, policy answer used
//!   coordc slow project on|off                 the owner: every ticket of the project halts so, those opened later too;
//!                                              off, what halted goes through in order (but under a slow ticket)
//!   coordc approve T N                         the owner lets halt N of T through (`coord show T`: its halts)
//!   coordc halt-edit T N JSON                  the owner edits halt N of T before approving it: {"prompt","worker"} for a
//!                                              step, {"head"} an answer, {"wakes"} a post, {"form"} an opened ticket,
//!                                              {"answer"} a policy decision
//!   coordc withdraw T 'PROGRAM' WHY...         withdraw T's step; PROGRAM (pattern text, one argument) goes on instead
//!   coordc edit T WHY... < PROGRAM             all T does after its awaited step's answer becomes PROGRAM (stdin)
//!   coordc repair T 'PROGRAM' WHY...           the failed ticket T runs PROGRAM (checked as its kind's pattern)
//!   coordc rekind T KIND [--FIELD V]... WHY... T becomes a KIND with that form and runs its pattern; posts stay
//!   coordc template T CALL step|tree WHY... < TEMPLATE   the owner's prompt for CALL's steps on T, once: the next step
//!                                              of CALL on T (step) or every later one, under T too (tree); TEMPLATE
//!                                              (stdin) in the call's own slots; the call's file is not changed
//!   coordc reload [--commit C] WHY...          the owner: coordd reads the kinds and calls again, taken only if all pass
//!   coordc show [--json|--program] T                     ticket T as a tree: its state, form, posts and the tickets under it
//!   coordc label ID...                                   what each reads as beside its id: a ticket, a step (C-1.s2), a post (N-1.p5), an agent, a commit (REPO:SHA)
//!   coordc mentions [--json] ID                       the posts whose words name ID: a ticket (or one of its posts), a post (N-1.p5), an agent
//!   coordc graph [ID] [--hops N] [--layer L]... [--who NAME]... [--from T] [--to T] [--limit N] [--json]
//!                                              the part of the one graph the Map draws around ID (a post, a ticket,
//!                                              an item, an agent by its name, commit:SHA…; none: the whole graph),
//!                                              as text: each edge a line, oldest first, then what each node is
//!   coordc tickets [--waiting|--mine|--kind K] the tickets I can see, one line each
//!
//! COORDD_SOCK overrides the socket (/run/coordd.sock). A host session's mail goes `--as NAME` through the owner side.
//! The terminals' sockets are COORD's pty/NAME.sock, served by their holders (coord-pty), not by coordd.

mod keyfile;
mod lsp;
// the mark's writing (`mark_mine`, `unmark_mine`) is coordd's alone (restart.rs)
#[allow(dead_code)]
mod switch;
mod terminal;

use switch::switching;
use coord_types::{Reply, Request, ESC_APART, SOCKET};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;

fn usage() -> ! {
    let doc = include_str!("client.rs");
    for line in doc.lines().take_while(|l| l.starts_with("//!")) {
        eprintln!("{}", line.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""));
    }
    std::process::exit(2)
}

fn connect() -> UnixStream {
    let path = std::env::var("COORDD_SOCK").unwrap_or_else(|_| SOCKET.into());
    UnixStream::connect(&path).unwrap_or_else(|e| {
        eprintln!("coord: coordd is not running ({path}: {e})");
        std::process::exit(3)
    })
}

/// A hook's request REQ to coordd at SOCK, and its answer line; none when coordd is not there and no coord-switch runs.
/// While one runs ([`switching`]) the hook waits for the new coordd, asking again every PAUSE, so no hook (a Stop above
/// all) is lost while coordd is down. An old coordd that took the request and ended before answering is asked again too.
fn hook_ask(sock: &str, req: &str, pause: std::time::Duration) -> Option<String> {
    loop {
        if let Ok(mut s) = UnixStream::connect(sock) {
            let mut line = String::new();
            if writeln!(s, "{req}").is_ok() && BufReader::new(s).read_line(&mut line).is_ok_and(|n| n > 0) {
                return Some(line);
            }
        }
        if !switching(sock) {
            return None;
        }
        std::thread::sleep(pause);
    }
}

/// REQ's answer from coordd. A post or msg with a kind goes as its typed op, and an older coordd's refusal of that op
/// is said as `KIND_UNTAKEN` ([`Request::line`], [`Request::answered`]). In an agent's session (AGENT_NAME), its name
/// goes beside the request ([`Request::declared_line`]). coordd reads it only from an agy agent's t3code server, whose
/// cgroup names no agent.
fn ask(req: &Request) -> Reply {
    let mut s = connect();
    let sent = match std::env::var("AGENT_NAME") {
        Ok(name) if !name.is_empty() => req.declared_line(&name),
        _ => req.line(),
    };
    writeln!(s, "{sent}").expect("coord: write");
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).expect("coord: read");
    req.answered(serde_json::from_str(&line).unwrap_or_else(|_| Reply { ok: false, out: format!("coord: bad reply {line:?}"), post: None }))
}

/// P, and all under it if a folder, readable by others where it is the caller's own (files 0644, folders 0755; links
/// left).
fn readable(p: &std::path::Path) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let Ok(m) = std::fs::symlink_metadata(p) else { return };
    if m.file_type().is_symlink() || m.uid() != unsafe { libc::getuid() } {
        return;
    }
    let want = if m.is_dir() { m.mode() | 0o755 } else { m.mode() | 0o644 } & 0o7777;
    if m.mode() & 0o7777 != want {
        let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(want));
    }
    if m.is_dir() {
        for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
            readable(&e.path());
        }
    }
}

/// A reload's answer (`{said, kinds, calls, tickets}`, `Core::ticket_reload`) as lines: what was read again, then a
/// line an open ticket of an edited kind.
fn reload_words(r: Reply) -> Reply {
    let Some(v) = r.ok.then(|| serde_json::from_str::<serde_json::Value>(&r.out).ok()).flatten() else { return r };
    let mut out = vec![v["said"].as_str().unwrap_or_default().to_string()];
    for t in v["tickets"].as_array().into_iter().flatten() {
        let id = t["id"].as_str().unwrap_or_default();
        out.push(match t["took"].as_bool() {
            Some(true) => format!("{id}: {}", t["said"].as_str().unwrap_or_default()),
            _ => format!("{id}: kept its old text: {}", t["why"].as_str().unwrap_or_default()),
        });
    }
    Reply { ok: true, out: out.join("\n"), post: None }
}

/// `coord account key ID [--remove]`: the owner's API key for account ID goes from his terminal to ID's key file and
/// nowhere else; coordd is asked where the file is and told that it is written, and the key is in neither request.
/// Refused before anything is read in an agent's session (`AGENT_NAME`), where coordd does not take the caller for the
/// owner, and without a terminal (a key in a pipe or on a command line stays in a shell's history).
fn account_key(id: &str, remove: bool) -> ! {
    use coord_types::KeyStep;
    let fail = |why: String| -> ! {
        eprintln!("coord account key: {why}");
        std::process::exit(1)
    };
    if std::env::var_os("AGENT_NAME").is_some() {
        eprintln!("{}", coord_types::KEY_NOT_HERE);
        std::process::exit(1)
    }
    let place = ask(&Request::AccountKey { id: id.into(), step: KeyStep::Where });
    if !place.ok {
        done(place)
    }
    let file = std::path::PathBuf::from(place.out);
    if remove {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => fail(format!("{}: {e}", file.display())),
        }
        done(ask(&Request::AccountKey { id: id.into(), step: KeyStep::Removed }))
    }
    let Ok(mut tty) = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty") else {
        fail("no terminal to ask in; the key is typed, not piped or given on the command line".into())
    };
    let _ = write!(tty, "Key for account {id} (not shown): ");
    let line = terminal::unseen_line(&tty);
    let _ = writeln!(tty);
    let line = match line {
        Ok(line) => line,
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => fail(format!("stopped; account {id}'s key is as it was")),
        Err(e) => fail(format!("the terminal: {e}")),
    };
    let key = keyfile::word(&line).unwrap_or_else(|why| fail(format!("{why}; account {id}'s key is as it was")));
    if let Err(e) = keyfile::write(&file, key) {
        fail(format!("{}: {e}", file.display()))
    }
    done(ask(&Request::AccountKey { id: id.into(), step: KeyStep::Set }))
}

/// `report-work [--refs REF]... [--item ITEM] [--] SUMMARY...` as its request.
fn report_work(args: &[String]) -> Result<Request, String> {
    let (mut refs, mut item, mut i) = (Vec::new(), None, 0);
    while i < args.len() {
        match args[i].as_str() {
            "--refs" | "--item" if i + 1 >= args.len() => return Err(format!("{} takes a value", args[i])),
            "--refs" => refs.push(args[i + 1].clone()),
            "--item" => item = Some(args[i + 1].clone()),
            "--" => {
                i += 1;
                break;
            }
            _ => break,
        }
        i += 2;
    }
    let summary = args[i..].join(" ");
    if summary.trim().is_empty() {
        return Err("say what was done: report-work [--refs REF]... [--item ITEM] SUMMARY...".into());
    }
    Ok(Request::ReportWork { summary, refs, item })
}

fn done(r: Reply) -> ! {
    if r.ok {
        if !r.out.is_empty() {
            println!("{}", r.out);
        }
        std::process::exit(0)
    }
    eprintln!("{}", r.out);
    std::process::exit(1)
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage()
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "msg" => {
            let (mut soon, mut as_host, mut re, mut thread, mut origin, mut under, mut label) = (false, None, None, false, None, None, None);
            let mut kind = None;
            while args.first().is_some_and(|a| a.starts_with("--")) {
                match kind_flag(&mut args) {
                    Ok(Some(k)) if kind.is_none() => {
                        kind = Some(k);
                        continue;
                    }
                    Ok(Some(_)) => {
                        eprintln!("coord msg: a post has one kind");
                        std::process::exit(2)
                    }
                    Err(f) => {
                        eprintln!("coord msg: {f} lacks its words ({KIND_FLAGS})");
                        std::process::exit(2)
                    }
                    Ok(None) => {}
                }
                match args.remove(0).as_str() {
                    // stopping a turn is its own act
                    "--esc" => {
                        eprintln!("coord msg: {}", ESC_APART);
                        std::process::exit(2)
                    }
                    // a post is delivered at the turn's end; --soon is taken and means nothing
                    "--soon" => soon = true,
                    "--as" if !args.is_empty() => as_host = Some(args.remove(0)),
                    "--re" if !args.is_empty() && re.is_none() => re = Some(args.remove(0)),
                    "--thread" if !args.is_empty() && re.is_none() => (re, thread) = (Some(args.remove(0)), true),
                    // the relay's, for a message from another project
                    "--origin" if !args.is_empty() => origin = Some(args.remove(0)),
                    // a subthread opened at post N of T, tied there from its open
                    "--under" if !args.is_empty() => under = Some(args.remove(0)),
                    "--label" if !args.is_empty() => label = Some(args.remove(0)),
                    f => {
                        eprintln!("coord msg: unknown or misplaced flag {f} (coord msg [--under T[.pN[:FROM+LEN]]] [--label TEXT] NAME[,NAME...] TEXT | msg --re ID NAME[,NAME...] TEXT | msg --thread ID TEXT)");
                        std::process::exit(2)
                    }
                }
            }
            // --thread names nobody here: coordd names everyone in the ticket but me; --re names NAME and the author of
            // the post replied to (coordd tells them apart by the names)
            if args.len() < if thread { 1 } else { 2 } {
                usage()
            }
            let to = if thread { Vec::new() } else { args.remove(0).split(',').filter(|s| !s.is_empty()).map(String::from).collect() };
            // TEXT `-`: read from stdin (a long text, or one with anything in it)
            let body = if args.len() == 1 && args[0] == "-" {
                let mut b = String::new();
                std::io::stdin().read_to_string(&mut b).expect("coord: stdin");
                b
            } else {
                args.join(" ")
            };
            done(ask(&Request::Msg { to, body, esc: false, soon, wait: false, as_host, re, quotes: Vec::new(), comments: Vec::new(), origin, under, label, kind }))
        }
        "effort" if args.len() == 2 => done(ask(&Request::Effort { name: args[0].clone(), level: args[1].clone() })),
        "can" => {
            // coord can [--json] [--as NAME] [THING...]: my acts now, at a ticket, at the names given (sent
            // comma-separated) or all (the owner names the agent)
            let (mut json, mut actor, mut thing): (bool, Option<String>, Option<String>) = (false, None, None);
            let mut it = args.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--json" => json = true,
                    "--as" => actor = Some(it.next().cloned().unwrap_or_else(|| usage())),
                    t if !t.starts_with('-') => thing = Some(thing.map_or(t.to_string(), |a| format!("{a},{t}"))),
                    _ => usage(),
                }
            }
            let r = ask(&Request::Enabled { thing, actor });
            if !r.ok || json {
                done(r)
            }
            for a in r.out.lines().filter_map(|l| serde_json::from_str::<coord_types::ActAnswer>(l).ok()) {
                let what = std::iter::once(a.act.as_str()).chain(a.objects.iter().map(String::as_str)).collect::<Vec<_>>().join(" ");
                match a.why {
                    None => println!("{what}"),
                    Some(why) => println!("{what}\t(not now: {why})"),
                }
            }
            std::process::exit(0)
        }
        "duties" => {
            // coord duties [--json] [--as NAME]: the duties I hold now (the owner: NAME's, or all)
            let (mut json, mut holder): (bool, Option<String>) = (false, None);
            let mut it = args.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--json" => json = true,
                    "--as" => holder = Some(it.next().cloned().unwrap_or_else(|| usage())),
                    _ => usage(),
                }
            }
            let r = ask(&Request::Duties { holder });
            if !r.ok || json {
                done(r)
            }
            for d in r.out.lines().filter_map(|l| serde_json::from_str::<coord_types::DutyAnswer>(l).ok()) {
                let what = std::iter::once(d.duty.as_str()).chain(d.objects.iter().map(String::as_str)).collect::<Vec<_>>().join(" ");
                let by = if d.ended_by.is_empty() { String::new() } else { format!("\t(ended by {})", d.ended_by.join(", ")) };
                println!("{what}{}{by}", if d.violated { "\tVIOLATED" } else { "" });
            }
            std::process::exit(0)
        }
        "compact" if args.len() == 2 => match args[1].parse::<u64>() {
            Ok(tokens) => done(ask(&Request::Compact { name: args[0].clone(), tokens })),
            Err(_) => usage(),
        },
        "model" if args.len() == 2 => done(ask(&Request::Model { name: args[0].clone(), model: args[1].clone() })),
        // coord mode NAME safe|yolo (0.2 line 25): the owner's only; coordd refuses it from an agent
        "mode" if args.len() == 2 => done(ask(&Request::Mode { name: args[0].clone(), mode: args[1].clone() })),
        "native" if args.len() == 2 && ["on", "off"].contains(&args[1].as_str()) => done(ask(&Request::Native { name: args[0].clone(), on: args[1] == "on" })),
        "account" if (args.len() == 2 || args.len() == 3) && args[0] == "move" => done(ask(&Request::MoveAgent { name: args[1].clone(), to: args.get(2).cloned() })),
        "accounts" if args.is_empty() => done(ask(&Request::Accounts { json: false })),
        "account-for" if args.len() == 1 => done(ask(&Request::AccountFor { name: args[0].clone() })),
        "account-bound" if args.len() == 1 => done(ask(&Request::AccountBound { name: args[0].clone() })),
        "account" if args.len() >= 2 && args[0] == "new" => {
            // ID [LABEL...] [--tier N]
            let mut rest: Vec<String> = args[1..].to_vec();
            let mut tier = 0;
            if let Some(i) = rest.iter().position(|a| a == "--tier") {
                match rest.get(i + 1).and_then(|n| n.parse().ok()) {
                    Some(n) => tier = n,
                    None => usage(),
                }
                rest.drain(i..i + 2);
            }
            let key = rest.iter().position(|a| a == "--key").map(|i| rest.remove(i)).is_some();
            if rest.is_empty() {
                usage()
            }
            let id = rest.remove(0);
            done(ask(&Request::AccountNew { id, label: (!rest.is_empty()).then(|| rest.join(" ")), tier, key, harness: None, provider: None }))
        }
        "account" if (args.len() == 2 || args.len() == 3 && args[2] == "--remove") && args[0] == "key" => account_key(&args[1], args.len() == 3),
        "account" if (args.len() == 2 || args.len() == 4 && args[2] == "--email") && args[0] == "signin" => done(ask(&Request::AccountSignin { id: args[1].clone(), email: args.get(3).cloned(), method: None })),
        "account" if (args.len() == 2 || args.len() == 3) && args[0] == "code" => {
            let code = args.get(2).cloned().unwrap_or_else(|| {
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                line.trim().to_string()
            });
            done(ask(&Request::AccountCode { id: args[1].clone(), code }))
        }
        "account" if args.len() == 2 && args[0] == "check" => done(ask(&Request::AccountCheck { id: args[1].clone() })),
        "account" if args.len() == 2 && args[0] == "cancel" => done(ask(&Request::AccountCancel { id: args[1].clone() })),
        "account" if args.len() == 2 && args[0] == "clear" => done(ask(&Request::AccountClear { id: args[1].clone() })),
        "account" if args.len() == 2 && args[0] == "remove" => done(ask(&Request::AccountRemove { id: args[1].clone() })),
        "account" if args.len() == 3 && args[0] == "tier" => match args[2].parse() {
            Ok(tier) => done(ask(&Request::AccountTier { id: args[1].clone(), tier })),
            Err(_) => usage(),
        },
        // every agent of the project in a turn at once
        "esc" if args.len() == 1 && args[0] == "--all" => done(ask(&Request::EscAll)),
        // the hold on agents' mail
        "hold-mail" if args.len() == 1 && (args[0] == "on" || args[0] == "off") => done(ask(&Request::MailHold { on: args[0] == "on" })),
        "esc" if !args.is_empty() => done(ask(&Request::Esc { to: args.iter().flat_map(|a| a.split(',')).filter(|s| !s.is_empty()).map(String::from).collect(), step: false })),
        "ask" => {
            // the flags lead; one it does not know is refused, never asked (8 Nov: `coord ask --list`, before there
            // was one, became the owner's question q7, "--list")
            let mut on = None;
            while args.first().is_some_and(|a| a.starts_with("--")) {
                match args.remove(0).as_str() {
                    "--list" if args.is_empty() => {
                        let r = ask(&Request::Tickets { filter: json!({"mine": true}) });
                        if !r.ok {
                            done(r)
                        }
                        for v in r.out.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["kind"] == "question") {
                            println!("{} · {} · {}", v["id"].as_str().unwrap_or("?"), state(&v), v["title"].as_str().unwrap_or(""));
                        }
                        std::process::exit(0)
                    }
                    // an old mail question (q12, or a mail in its thread), from before tickets: withdrawn as the old mail layer did, WHY
                    // optional
                    "--drop" if args.first().is_some_and(|a| old_mail_id(a)) => {
                        let id = args.remove(0);
                        let why = (!args.is_empty()).then(|| args.join(" "));
                        done(ask(&Request::Drop { id, why }))
                    }
                    // a post's asks: withdrawn, nobody told
                    "--drop" if args.len() >= 2 && post_of(&args[0]).is_some() => {
                        let (t, n) = post_of(&args.remove(0)).unwrap();
                        done(ask(&Request::WithdrawAsks { t, n, why: args.join(" ") }))
                    }
                    "--drop" if args.len() >= 2 => {
                        let t = args.remove(0);
                        done(ask(&Request::Cancel { t, why: args.join(" ") }))
                    }
                    "--on" if !args.is_empty() && on.is_none() => on = Some(args.remove(0)),
                    f => {
                        eprintln!("coord ask: unknown or misplaced flag {f} (coord ask [--on Q] SUBJECT... | ask --drop Q WHY... | ask --list)");
                        std::process::exit(2)
                    }
                }
            }
            if args.is_empty() {
                usage()
            }
            // the subject on the line, the whole explanation on stdin (a heredoc), when stdin is not a terminal
            let subject = args.join(" ");
            let mut more = String::new();
            if stdin_gives_text() {
                let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut more);
            }
            let more = more.trim_end().to_string();
            // S5: coordd opens the question ticket (its first line the ask, the rest its context), or posts on Q
            let body = if more.trim().is_empty() { subject } else { format!("{subject}\n\n{more}") };
            done(ask(&Request::Ask { body, on }))
        }
        "mail" | "log" if args.first().is_some_and(|a| a.starts_with("--")) => {
            eprintln!("coord {cmd}: unknown flag {} (coord mail [ID] | log NAME [LAST])", args[0]);
            std::process::exit(2)
        }
        "mail" => done(ask(&Request::Mail { id: args.first().cloned() })),
        "ls" | "state" => done(ask(&Request::State)),
        "log" => {
            let name = args.first().cloned().unwrap_or_else(|| usage());
            let last = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(60);
            done(ask(&Request::Log { name, last }))
        }
        "item" => {
            if args.len() < 2 {
                usage()
            }
            let action = args.remove(0);
            let (arg, why) = match action.as_str() {
                "close" | "drop" => (args.remove(0), Some(args.join(" "))),
                "open" => (args.join(" "), None),
                _ => (args.remove(0), None),
            };
            done(ask(&Request::Item { action, arg, why }))
        }
        "rename" => {
            if args.len() != 2 {
                usage()
            }
            done(ask(&Request::Rename { name: args[0].clone(), to: args[1].clone() }))
        }
        // an act of the calling agent (runner.rs): `coordc act IS TEXT [FIELD=VALUE...]`, a VALUE starting [ or { read as
        // JSON (the symbols server's `edit`: tool=rename symbol=report::twice 'files=["src/report.rs"]')
        "act" => {
            if args.len() < 2 {
                usage()
            }
            let (is, text) = (args.remove(0), args.remove(0));
            let mut more = serde_json::Map::new();
            for a in args {
                let Some((k, v)) = a.split_once('=') else {
                    eprintln!("coord act: {a}: want FIELD=VALUE");
                    std::process::exit(2)
                };
                let v = if v.starts_with(['[', '{']) { serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.into())) } else { serde_json::Value::String(v.into()) };
                more.insert(k.into(), v);
            }
            done(ask(&Request::Act { is, text, more }))
        }
        "jobs" => done(ask(&Request::Jobs)),
        "store" => {
            let (mut row, mut from) = (None, None);
            let mut it = args.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--from" => from = Some(it.next().and_then(|n| n.parse().ok()).unwrap_or_else(|| usage())),
                    r if row.is_none() && !r.starts_with('-') => row = Some(r.to_string()),
                    _ => usage(),
                }
            }
            done(ask(&Request::Store { row, from }))
        }
        // `coord end T [T...] done|dropped|kept WHY`: the threads end and leave the outline; anything else
        // (`coord end JOB WHY`) is the job end
        "end" if thread_end(&args).is_some() => {
            let (ts, how, why) = thread_end(&args).unwrap();
            done(ask(&Request::EndThread { ts, how, why }))
        }
        // thread ids and a how with no why: refused here, not taken as a job named like a ticket
        "end" if thread_end_without_why(&args) => {
            eprintln!("coord end: a thread's end needs a why: coord end {} WHY", args.join(" "));
            std::process::exit(2)
        }
        "end" => {
            if args.len() < 2 {
                usage()
            }
            let job = args.remove(0);
            done(ask(&Request::End { job, why: args.join(" ") }))
        }
        // an agent's own check through coordd to the project's runner (runner.rs)
        "check" => {
            let (mut mode, mut refs, mut flip) = (None, Vec::new(), Vec::new());
            let mut args = args.into_iter();
            while let Some(a) = args.next() {
                match a.as_str() {
                    "--quick" | "--typecheck" | "--proofs" if mode.is_none() => mode = Some(a[2..].to_string()),
                    // a stage held red on dev that this check may flip (the runner's --flip STAGE)
                    "--flip" => match args.next() {
                        Some(s) if !s.starts_with('-') => flip.push(s),
                        _ => {
                            eprintln!("coord check: --flip takes a stage (coord check [--quick|--typecheck|--proofs] [--flip STAGE]... [REPO:BRANCH...])");
                            std::process::exit(2)
                        }
                    },
                    f if f.starts_with('-') => {
                        eprintln!("coord check: unknown or second flag {f} (coord check [--quick|--typecheck|--proofs] [--flip STAGE]... [REPO:BRANCH...])");
                        std::process::exit(2)
                    }
                    _ => refs.push(a),
                }
            }
            done(ask(&Request::Check { mode, refs, flip }))
        }
        "team" => match (args.first().map(String::as_str), args.len()) {
            (Some("ask"), n) if n > 1 => done(ask(&Request::TeamAsk { text: args[1..].join(" ") })),
            (Some("grant"), 3) => match args[2].parse() {
                Ok(granted) => done(ask(&Request::TeamGrant { name: args[1].clone(), granted })),
                Err(_) => usage(),
            },
            _ => usage(),
        },
        "ended" => {
            let undo = args.first().is_some_and(|a| a == "--undo");
            if args.len() != 1 + undo as usize {
                usage()
            }
            done(ask(&Request::Ended { name: args[undo as usize].clone(), undo }))
        }
        "role" => {
            if args.len() != 2 {
                usage()
            }
            done(ask(&Request::Role { name: args[0].clone(), role: args[1].clone() }))
        }
        "norm" => {
            if args.is_empty() {
                usage()
            }
            let act = args.remove(0);
            let thread = match args.iter().position(|a| a == "--thread") {
                Some(i) if i + 1 < args.len() => {
                    let t = args.remove(i + 1);
                    args.remove(i);
                    t
                }
                _ => "none".to_string(),
            };
            done(ask(&Request::Norm { act, args, thread }))
        }
        // an agent made from my turn (item 11): `new NAME [--model M] TEXT`, `fork [FROM] [--name NEW] [--model M] TEXT`
        "new" | "fork" => {
            let (mut new, mut from, mut model, mut text) = (None, None, None, Vec::new());
            if cmd == "new" {
                new = Some(if args.is_empty() || args[0].starts_with("--") { usage() } else { args.remove(0) });
            } else if args.first().is_some_and(|a| !a.starts_with("--")) && args.get(1).is_some_and(|a| a == "--name" || a == "--model") {
                from = Some(args.remove(0));
            }
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--name" if cmd == "fork" && i + 1 < args.len() => {
                        new = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--model" if i + 1 < args.len() => {
                        model = Some(args[i + 1].clone());
                        i += 2;
                    }
                    a => {
                        text.push(a.to_string());
                        i += 1;
                    }
                }
            }
            done(ask(&Request::Make { new: new.unwrap_or_default(), from, model, text: text.join(" "), fork: cmd == "fork" }))
        }
        // `go [ID]`: move me to layer ID at my turn's end; none: where I stand and my steps' layers
        "go" => {
            if args.len() > 1 {
                usage()
            }
            done(ask(&Request::Go { to: args.first().cloned() }))
        }
        "start" => {
            // --force: a restart even while coordd holds a turn running (`roster restart NAME --force`)
            let force = args.iter().any(|a| a == "--force");
            let args: Vec<String> = args.iter().filter(|a| *a != "--force").cloned().collect();
            if args.len() != 1 && args.len() != 4 {
                usage()
            }
            let at = |i: usize| args.get(i).cloned();
            done(ask(&Request::Start { name: args[0].clone(), model: at(1), prompt: at(2), dir: at(3), force }))
        }
        // `add NAME KIND MODEL [--from FROM]`: FROM, the agent whose conversation NAME is a copy of (roster fork)
        "add" => {
            let (mut renderings, mut from) = (None, None);
            for pair in args.get(3..).unwrap_or_default().chunks(2) {
                match pair {
                    [f, v] if f == "--renderings" && renderings.is_none() => renderings = Some(v.clone()),
                    [f, v] if f == "--from" && from.is_none() => from = Some(v.clone()),
                    _ => usage(),
                }
            }
            if args.len() < 3 {
                usage()
            }
            done(ask(&Request::Add { name: args[0].clone(), kind: args[1].clone(), model: args[2].clone(), renderings, from }))
        }
        "run" => {
            let sep = args.iter().position(|a| a == "--").unwrap_or_else(|| usage());
            if sep != 1 || args.len() < 3 {
                usage()
            }
            let r = ask(&Request::Run { name: args[0].clone() });
            if !r.ok {
                done(r)
            }
            use std::os::unix::process::CommandExt;
            let e = std::process::Command::new(&args[2]).args(&args[3..]).exec();
            eprintln!("coord run: {}: {e}", args[2]);
            std::process::exit(127)
        }
        "session" => {
            if args.len() != 2 {
                usage()
            }
            let session: Value = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
                eprintln!("coord session: not JSON: {e}");
                std::process::exit(2)
            });
            done(ask(&Request::Session { name: args[0].clone(), session }))
        }
        "hook" => {
            let event = args.first().cloned().unwrap_or_else(|| usage());
            let mut input = String::new();
            let _ = std::io::stdin().read_to_string(&mut input);
            let mut input: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
            // a Codex agent's inbox is its app-server's socket, which the hook input does not carry
            if let (Some(o), Ok(sock)) = (input.as_object_mut(), std::env::var("CODEX_SOCK")) {
                o.entry("socket").or_insert(Value::String(sock));
            }
            // a session on one of the owner's accounts (accounts/mover.rs): which one, by the folder it runs under; what
            // it says of its limits is of that account, also when it arrives after its agent has moved to another
            if let (Some(o), Ok(dir)) = (input.as_object_mut(), std::env::var("CLAUDE_CONFIG_DIR")) {
                o.entry("claude_config_dir").or_insert(Value::String(dir));
            }
            // a boxed agent (coordd's sandbox.rs): Claude Code writes its transcript for itself alone (0600), and coordd
            // and the window read it; its folder (the conversations folder) shuts out everyone else
            if std::env::var("AGENT_BOX").is_ok_and(|b| !b.is_empty()) {
                if let Some(t) = input.get("transcript_path").and_then(Value::as_str) {
                    readable(std::path::Path::new(t));
                    readable(std::path::Path::new(t.strip_suffix(".jsonl").unwrap_or(t)));
                }
            }
            // coordd answers at once and the hook prints nothing: mail goes in through the channel after the turn, and coordd
            // reads the turn's end from the transcript
            let sock = std::env::var("COORDD_SOCK").unwrap_or_else(|_| SOCKET.into());
            let req = json!({"op": "hook", "event": event, "input": input}).to_string();
            let Some(line) = hook_ask(&sock, &req, std::time::Duration::from_millis(250)) else {
                std::process::exit(0) // a hook never blocks a session; coord watch shows coordd is down
            };
            // what coordd says back is for Claude Code: a refused subagent call (vault designs/teams-and-tiers-2026-11-10.md §4)
            if let Ok(r) = serde_json::from_str::<Reply>(&line) {
                if r.ok && !r.out.is_empty() {
                    println!("{}", r.out);
                }
            }
        }
        "watch" => {
            let from = args.first().and_then(|n| n.parse().ok());
            let mut s = connect();
            writeln!(s, "{}", serde_json::to_string(&Request::Watch { from, run: None, box_id: None }).unwrap()).expect("coord: write");
            let out = std::io::stdout();
            for line in BufReader::new(s).lines() {
                let Ok(line) = line else { break };
                let mut o = out.lock();
                if writeln!(o, "{line}").and_then(|_| o.flush()).is_err() {
                    break;
                }
            }
            std::process::exit(4) // the stream ended: coordd stopped
        }
        "bridge" => bridge(&args),
        "attach" | "screen" | "hangup" if args.len() == 1 => match cmd.as_str() {
            "attach" => terminal::attach(&args[0]),
            "screen" => terminal::screen(&args[0]),
            _ => terminal::hangup(&args[0]),
        },
        "keys" if args.len() >= 2 => terminal::keys(&args[0], &args[1..].join(" ")),
        "terms" => terminal::terms(),
        "layers" if args.is_empty() => done(ask(&Request::Layers)),
        // the owner's tools (release.sh coord-back --esc): deliveries held for a return, and coord's own notice
        "hold" if args.len() == 1 && (args[0] == "on" || args[0] == "off") => done(ask(&Request::Hold { on: args[0] == "on" })),
        "notice" if args.len() >= 2 => done(ask(&Request::Notice { to: args[0].clone(), text: args[1..].join(" ") })),
        "layer-sync" if args.is_empty() => done(ask(&Request::LayerSync)),
        // an agy agent's pull and any agent's report (pull.rs, N-1998); coord's read-brain and report-work on v1
        "read-brain" => done(ask(&Request::ReadBrain { note: args.join(" ") })),
        "report-work" => match report_work(&args) {
            Ok(req) => done(ask(&req)),
            Err(e) => {
                eprintln!("coordc report-work: {e}");
                std::process::exit(2)
            }
        },
        "open" | "answer" | "post" | "cancel" | "slow" | "approve" | "halt-edit" | "withdraw" | "edit" | "repair" | "rekind" | "template" | "reload" | "show" | "tickets" | "say" | "move" | "keep" | "drop-thread" | "take-back" | "pass" | "star" | "unstar" | "archive" | "unarchive" | "label" | "mentions" | "find" | "tie" | "title" | "outline" | "graph" => ticket_verb(&cmd, args),
        _ => usage(),
    }
}

/// One pipe for yaaihh (through `sbx exec`): the event stream, and each request on its own connection, so a slow one
/// holds up no other. Terminal requests are the bridge's own (it runs next to the holders' sockets) and are taken in
/// order, so keys arrive as typed. It ends when stdin closes or coordd stops.
///
/// Its first line is its hello (`{"hello": Hello}`: the protocol it speaks, the oldest it still speaks, its release);
/// an app's first line on stdin may be the app's own. One the bridge cannot speak with is answered with a hello that
/// says why (`refused`), and the bridge ends. An app that sends none is served as before.
///
/// `--from RUN:SEQ` (protocol 1): the app had the stream up to event SEQ of coordd's run RUN when its pipe went down,
/// and the stream resumes there (`Resumed`, then what was missed) instead of a whole snapshot, if coordd is still that
/// run and still has those events.
///
/// `--box ID`: the window start this bridge serves, fresh at each start, the same when it comes back with `--from`.
/// coordd counts the window open while its watch is up; an owner's step opened from that window (a `words` ticket's
/// form `window`) goes on by itself at once when it is gone.
fn bridge(args: &[String]) -> ! {
    use std::sync::{Arc, Mutex};
    let out = Arc::new(Mutex::new(std::io::stdout()));
    terminal::print(&out, json!({"hello": coord_types::Hello::ours(release())}));
    // the protocol the app's hello said (0: none yet, or none): a window from before ASKS_CLOSED_PROTOCOL is sent the ways
    // it can read, one from before MENTIONED_BODY no line back
    let app = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let hello = |h: &Value| {
        let why = match serde_json::from_value::<coord_types::Hello>(h.clone()) {
            Ok(h) => coord_types::Hello::mismatch(&h),
            Err(e) => Some(format!("the app's hello is not one: {e}")),
        };
        if let Some(why) = why {
            terminal::print(&out, json!({"hello": coord_types::Hello { refused: Some(why), ..coord_types::Hello::ours(release()) }}));
            std::process::exit(5)
        }
        if let Some(p) = h.get("protocol").and_then(Value::as_u64) {
            app.store(p.min(u32::MAX as u64) as u32, std::sync::atomic::Ordering::Relaxed);
        }
        // the release his window runs, from its first hello or the second one after it took over: one path to coordd for
        // both, on a connection of its own, its answer to nobody
        if let Some(req) = window_release(h) {
            std::thread::spawn(move || drop(ask(&req)));
        }
    };
    // the app writes its hello as it starts, so it is on the pipe by now and is read before the stream begins; one
    // that has sent nothing yet counts as 0 until it does (an app from before protocol 1 sends none, and its stream
    // never waits on it)
    let mut first: Option<String> = None;
    if stdin_ready() {
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).is_ok_and(|n| n > 0) {
            first = Some(line.trim_end_matches('\n').to_string());
        }
    }
    if let Some(h) = first.as_deref().and_then(|l| serde_json::from_str::<Value>(l).ok()).and_then(|v| v.get("hello").cloned()) {
        hello(&h);
        first = None;
    }
    let mut s = connect();
    let (from, run) = resume_point(args).map_or((None, None), |(r, n)| (Some(n), Some(r)));
    let box_id = box_arg(args);
    writeln!(s, "{}", serde_json::to_string(&Request::Watch { from, run, box_id }).unwrap()).expect("coord: write");
    let o = out.clone();
    let gate = Gate::default();
    let opens = gate.clone();
    let speaks = app.clone();
    std::thread::spawn(move || {
        pass_events(s, &opens, |mut event| {
            let app = speaks.load(std::sync::atomic::Ordering::Relaxed);
            if for_app(&event, app) {
                if app < coord_types::ASKS_CLOSED_PROTOCOL {
                    older_asks_closed(&mut event);
                }
                terminal::print(&o, json!({"event": event}));
            }
        });
        std::process::exit(4)
    });
    let relay = terminal::Relay::new(out.clone());
    let o = out.clone();
    let lsp = lsp::Lsp::new(std::env::var("COORD").unwrap_or_default(), std::sync::Arc::new(move |v| terminal::print(&o, v)));
    // a terminal's and a language server's requests are answered here, never by coordd
    let here = |req: Request| match req {
        req @ (Request::Attach { .. } | Request::Keys { .. } | Request::Resize { .. } | Request::Detach { .. }) => Ok(relay.handle(req)),
        Request::Lsp { lang, message } => Ok(lsp.handle(&lang, &message)),
        req => Err(req),
    };
    let o = out.clone();
    let lines = first.into_iter().map(Ok).chain(std::io::stdin().lock().lines());
    let speaks = app.clone();
    let ask = move |req: &Request| {
        let mut r = ask(req);
        if speaks.load(std::sync::atomic::Ordering::Relaxed) < coord_types::ASKS_CLOSED_PROTOCOL {
            r.out = older_asks_closed_text(&r.out);
        }
        r
    };
    let asked = serve(lines, &gate, hello, here, Arc::new(ask), Arc::new(move |v| terminal::print(&o, v)));
    asked.into_iter().for_each(|t| drop(t.join())); // the answers still owed
    std::process::exit(0)
}

/// Opened by the first line of coordd's stream, its snapshot: what goes to coordd waits for it, so a reply never speaks
/// of state the reader has not seen. A terminal's and a language server's requests never go to coordd and do not wait.
#[derive(Clone, Default)]
struct Gate(std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);

impl Gate {
    fn open(&self) {
        let (m, c) = &*self.0;
        *m.lock().unwrap() = true;
        c.notify_all();
    }

    fn wait(&self) {
        let (m, c) = &*self.0;
        let mut open = m.lock().unwrap();
        while !*open {
            open = c.wait(open).unwrap();
        }
    }
}

/// coordd's stream S, each event to EMIT; the first opens GATE.
fn pass_events(s: impl Read, gate: &Gate, mut emit: impl FnMut(Value)) {
    for line in BufReader::new(s).lines() {
        let Ok(line) = line else { break };
        emit(serde_json::from_str::<Value>(&line).unwrap_or(Value::Null));
        gate.open();
    }
}

/// How many of a bridge's requests are at coordd at once; the rest wait in the bridge, in order. Each holds a
/// connection open at coordd until answered, and a window asks all the texts a page shows at once (yaaihh texts.rs), so
/// without a bound they could take every file descriptor coordd has.
const ASKS_AT_ONCE: usize = 32;

/// The bridge's requests, one a line of LINES: an app's hello to HELLO; what HERE answers (a terminal's, a language
/// server's) at once; the rest asked of coordd by ASK once GATE is open, by at most [`ASKS_AT_ONCE`] threads, each
/// taking the next request waiting once it has its answer. Each reply goes to PRINT with its id. The threads of the
/// answers still owed: each ends once LINES has ended and no request is left waiting.
fn serve(
    lines: impl Iterator<Item = std::io::Result<String>>,
    gate: &Gate,
    mut hello: impl FnMut(&Value),
    here: impl Fn(Request) -> Result<Reply, Request>,
    ask: std::sync::Arc<dyn Fn(&Request) -> Reply + Send + Sync>,
    print: std::sync::Arc<dyn Fn(Value) + Send + Sync>,
) -> Vec<std::thread::JoinHandle<()>> {
    let mut asked = Vec::new();
    let (waiting, next) = std::sync::mpsc::channel::<(Value, Request)>();
    let next = std::sync::Arc::new(std::sync::Mutex::new(next));
    for line in lines {
        let Ok(line) = line else { break };
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        if let Some(h) = v.get("hello") {
            hello(h);
            continue;
        }
        let reply = match serde_json::from_value::<Request>(v["req"].clone()) {
            Ok(Request::Watch { .. }) => Reply { ok: false, out: "coord bridge: the stream is already here".into(), post: None },
            Ok(req) => match here(req) {
                Ok(r) => r,
                Err(req) => {
                    if asked.len() < ASKS_AT_ONCE {
                        let (gate, ask, print, next) = (gate.clone(), ask.clone(), print.clone(), next.clone());
                        asked.push(std::thread::spawn(move || {
                            gate.wait();
                            // the guard goes at the end of the `let`, so the others take theirs while this one asks
                            while let Ok((id, req)) = {
                                let taken = next.lock().unwrap().recv();
                                taken
                            } {
                                print(json!({"id": id, "reply": ask(&req)}))
                            }
                        }));
                    }
                    let _ = waiting.send((v["id"].clone(), req));
                    continue;
                }
            },
            Err(e) => Reply { ok: false, out: format!("coord bridge: not a request: {e}"), post: None },
        };
        print(json!({"id": v["id"], "reply": reply}));
    }
    drop(waiting);
    asked
}

/// The bridge's `--box ID`: the window start it serves, fresh at each start and the same when it comes back with
/// `--from`; none when not given.
fn box_arg(args: &[String]) -> Option<String> {
    let at = args.iter().position(|a| a == "--box")?;
    args.get(at + 1).filter(|b| !b.is_empty() && !b.starts_with("--")).cloned()
}

/// Whether stdin has a line waiting now (poll with no wait): the app's hello, written as it started.
fn stdin_ready() -> bool {
    let mut fds = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    // SAFETY: one pollfd for fd 0, a count of 1, no wait
    unsafe { libc::poll(&mut fds, 1, 0) == 1 && fds.revents & libc::POLLIN != 0 }
}

/// V, for a window from before `coord_types::ASKS_CLOSED_PROTOCOL`: each `AsksClosed` it cannot read as the closest it
/// can, `Withdrawn` as `NothingToAnswer`, `Subthread { ticket }` as `Followed { ticket }`.
fn older_asks_closed(v: &mut Value) {
    match v {
        Value::Object(o) => {
            match o.get("how").and_then(Value::as_str) {
                Some("Withdrawn") => {
                    *v = json!({"how": "NothingToAnswer"});
                    return;
                }
                Some("Subthread") => {
                    let ticket = o.get("ticket").cloned().unwrap_or(Value::Null);
                    *v = json!({"how": "Followed", "ticket": ticket});
                    return;
                }
                _ => {}
            }
            o.values_mut().for_each(older_asks_closed);
        }
        Value::Array(a) => a.iter_mut().for_each(older_asks_closed),
        _ => {}
    }
}

/// A reply's OUT, for a window from before ASKS_CLOSED_PROTOCOL: each of its lines that is JSON holding a new way, put
/// as `older_asks_closed` puts it; the rest as it is.
fn older_asks_closed_text(out: &str) -> String {
    if !out.contains("\"Withdrawn\"") && !out.contains("\"Subthread\"") {
        return out.to_string();
    }
    out.split('\n')
        .map(|l| match serde_json::from_str::<Value>(l) {
            Ok(mut v) => {
                older_asks_closed(&mut v);
                v.to_string()
            }
            Err(_) => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// ARGS of `coord end` as a thread end: one or more ticket ids (a capital letter, `-`, digits), then `done`, `dropped`
/// or `kept`, then a why; None when they are not that (a job's end, whose why may hold those words).
fn thread_end(args: &[String]) -> Option<(Vec<String>, String, String)> {
    let id = |a: &str| {
        let mut c = a.chars();
        c.next().is_some_and(|l| l.is_ascii_uppercase()) && c.next() == Some('-') && !c.as_str().is_empty() && c.as_str().chars().all(|d| d.is_ascii_digit())
    };
    let i = args.iter().position(|a| !id(a))?;
    (i > 0 && ["done", "dropped", "kept"].contains(&args[i].as_str()) && i + 1 < args.len()).then(|| (args[..i].to_vec(), args[i].clone(), args[i + 1..].join(" ")))
}

/// ARGS of `coord end` as ticket ids then a how and nothing after it: a thread end with no why.
fn thread_end_without_why(args: &[String]) -> bool {
    args.len() >= 2 && thread_end(&[args.to_vec(), vec![String::from("w")]].concat()).is_some_and(|(ts, _, _)| ts.len() == args.len() - 1)
}

/// Whether the bridge gives EVENT to an app whose hello said protocol APP (0: none yet, or none): every event but one it
/// could not decode (`Body::Mentioned` below `MENTIONED_BODY`, `Body::OwnerModel` below `OWNER_MODEL_PROTOCOL`).
fn for_app(event: &Value, app: u32) -> bool {
    (event.get("mentioned").is_none() || app >= coord_types::MENTIONED_BODY)
        && (event.get("ownermodel").is_none() || app >= coord_types::OWNER_MODEL_PROTOCOL)
        && (event.get("look").is_none() || app >= coord_types::LOOK_PROTOCOL)
}

/// The bridge's `--from RUN:SEQ` (the run is a time and a pid, so the number is after the last colon); none when not
/// given or not one.
fn resume_point(args: &[String]) -> Option<(String, u64)> {
    let at = args.iter().position(|a| a == "--from")?;
    let (run, seq) = args.get(at + 1)?.rsplit_once(':')?;
    Some((run.to_string(), seq.parse().ok()?)).filter(|(r, _)| !r.is_empty())
}

/// What a window's hello H asks of coordd: the release it names, with the time it says it became his window at
/// (`Hello::since`); a hello that names none asks nothing.
fn window_release(h: &Value) -> Option<Request> {
    let release = h.get("release")?.as_str().filter(|r| !r.is_empty())?;
    let since = h.get("since").and_then(Value::as_str).map(String::from);
    Some(Request::WindowRelease { release: release.into(), since })
}

/// The release this coordc runs from: the name of its folder when that folder is in a `releases` folder
/// (`releases/20261118-101752-e0eb4d1/coordc`, or agent-base/bin's link into it); none for a build run where it was
/// built.
fn release() -> Option<String> {
    release_of(&std::env::current_exe().ok()?.canonicalize().ok()?)
}

fn release_of(exe: &std::path::Path) -> Option<String> {
    let dir = exe.parent()?;
    (dir.parent()?.file_name()? == "releases").then(|| dir.file_name()?.to_str().map(String::from)).flatten()
}

/// An open's first post, or an answer's text: TEXT (the words left on the line), or stdin for `-`. With no TEXT,
/// stdin is read only when no field was given (FORMED false) and stdin is not a terminal, so an open with its form filled
/// never waits on a stdin left open.
fn given_text(args: &[String], formed: bool) -> Option<String> {
    if args.len() == 1 && args[0] == "-" {
        reading_stdin();
        let mut more = String::new();
        let _ = std::io::stdin().read_to_string(&mut more);
        return (!more.trim().is_empty()).then(|| more.trim_end().to_string());
    }
    if formed && args.is_empty() {
        return None;
    }
    text_or_stdin(args)
}

/// TEXT, an answer's text, when it is a JSON object and nothing else: its fields.
fn answer_object(text: &str) -> Option<serde_json::Map<String, Value>> {
    match serde_json::from_str::<Value>(text.trim()) {
        Ok(Value::Object(o)) => Some(o),
        _ => None,
    }
}

/// One line on stderr before stdin is read for a text, so a wait on a pipe that does not close explains itself.
fn reading_stdin() {
    eprintln!("coord: no text given; reading it from stdin until it ends");
}

/// Whether stdin carries a text to read when none is given: a pipe or a file (`echo ... | coord post T`, `< file`), not
/// a terminal, a socket or a device. Claude Code's Bash tool gives a command with a heredoc a socket as stdin that never
/// closes. An explicit `-` reads stdin whatever it is.
fn stdin_gives_text() -> bool {
    use std::os::fd::AsFd;
    std::io::stdin().as_fd().try_clone_to_owned().ok().map(std::fs::File::from).is_some_and(|f| gives_text(&f))
}

/// Whether F is a pipe or a regular file (`stdin_gives_text`).
fn gives_text(f: &std::fs::File) -> bool {
    use std::os::unix::fs::FileTypeExt;
    f.metadata().is_ok_and(|m| m.file_type().is_fifo() || m.is_file())
}

fn text_or_stdin(args: &[String]) -> Option<String> {
    if !args.is_empty() {
        return Some(args.join(" "));
    }
    if !stdin_gives_text() {
        return None;
    }
    reading_stdin();
    let mut more = String::new();
    let _ = std::io::stdin().read_to_string(&mut more);
    (!more.trim().is_empty()).then(|| more.trim_end().to_string())
}

/// `--FIELD V` pairs leading ARGS (removed from it), as an object of strings: coordd types them by the kind's form
/// or the step's head, and an ill-typed one is refused naming it.
fn fields(cmd: &str, args: &mut Vec<String>, under: &mut Option<String>) -> serde_json::Map<String, Value> {
    let mut form = serde_json::Map::new();
    while args.first().is_some_and(|a| a.starts_with("--") && a.len() > 2) {
        let f = args.remove(0)[2..].to_string();
        let Some(v) = (!args.is_empty()).then(|| args.remove(0)) else {
            eprintln!("coord {cmd}: --{f} needs a value");
            std::process::exit(2)
        };
        if f == "under" && cmd == "open" {
            *under = Some(v);
        } else {
            form.insert(f, Value::String(v));
        }
    }
    if args.first().is_some_and(|a| a == "--") {
        args.remove(0);
    }
    form
}

/// The ticket verbs (plan/interface.md §3): the requests coordd's ticket store answers.
fn ticket_verb(cmd: &str, mut args: Vec<String>) -> ! {
    let mut under = None;
    match cmd {
        // `coord open` alone: the kinds, one line each
        "open" if args.is_empty() => {
            let r = ask(&Request::Kinds);
            if !r.ok {
                done(r)
            }
            let kinds: Vec<Value> = serde_json::from_str(&r.out).unwrap_or_default();
            let mut out: Vec<String> = kinds
                .iter()
                .map(|k| {
                    let form: Vec<String> = k["form"].as_array().into_iter().flatten().filter_map(|f| f[0].as_str().map(str::to_string)).collect();
                    format!("  {:<10} {}", k["kind"].as_str().unwrap_or("?"), if form.is_empty() { "(no form)".to_string() } else { form.join(", ") })
                })
                .collect();
            out.insert(0, "The kinds you can open, each with its form's fields:".into());
            out.push("`coord open KIND` alone says what KIND needs.".into());
            done(Reply { ok: true, out: out.join("\n"), post: None })
        }
        "open" => {
            let kind = args.remove(0);
            let form = fields(cmd, &mut args, &mut under);
            let text = given_text(&args, !form.is_empty() || under.is_some());
            // `coord open KIND` alone (no field, no text, nothing on stdin): what KIND needs
            if form.is_empty() && under.is_none() && text.is_none() {
                done(ask(&Request::Kind { kind }))
            }
            done(ask(&Request::Open { kind, under, form: Value::Object(form), text }))
        }
        "answer" if args.len() >= 2 => {
            let t = args.remove(0);
            let head = args.remove(0);
            let mut v = fields(cmd, &mut args, &mut under);
            let mut text = given_text(&args, !v.is_empty());
            // the answer as a JSON object (`coord answer C-1 Design - < answer.json`): the head's fields, not a text
            if let Some(o) = text.as_deref().and_then(answer_object) {
                if let Some(h) = o.get("head").and_then(Value::as_str).filter(|h| *h != head) {
                    eprintln!("coord answer: the JSON's head is {h}, the command's {head}");
                    std::process::exit(2)
                }
                for (k, x) in o.into_iter().filter(|(k, _)| k != "head") {
                    v.entry(k).or_insert(x);
                }
                text = None;
            }
            v.insert("head".into(), Value::String(head));
            done(ask(&Request::Answer { t, head: Value::Object(v), text }))
        }
        "say" if !args.is_empty() => {
            if args.first().is_some_and(|a| a == "--esc") {
                eprintln!("coord say: {}", ESC_APART);
                std::process::exit(2)
            }
            if args.len() < 2 {
                usage()
            }
            let to = args.remove(0);
            let text = args.join(" ");
            // the @NAMEs in the text, as yaaihh's composer tags them (coordd checks they are agents)
            let mut names: Vec<String> = Vec::new();
            for w in text.split(|c: char| c.is_whitespace() || ",.;:!?()".contains(c)) {
                if let Some(n) = w.strip_prefix('@').filter(|n| !n.is_empty() && *n != to && !names.iter().any(|x| x == n)) {
                    names.push(n.to_string());
                }
            }
            done(ask(&Request::Say { to: Some(to), names, text, follows: None, wait: false, under: None, label: None, thread: false, typed: None, split_from: None, comments: Vec::new() }))
        }
        "move" if args.len() == 2 || args.len() == 3 => {
            let Ok(n) = args[1].trim_start_matches('p').parse() else { usage() };
            done(ask(&Request::Move { t: args[0].clone(), n, to: args.get(2).cloned() }))
        }
        "keep" if !args.is_empty() => done(ask(&Request::Keep { ts: args })),
        "drop-thread" if args.len() == 1 => done(ask(&Request::DropThread { t: args.remove(0) })),
        // my post N of T, before anyone was handed it
        "take-back" if args.len() == 1 => match post_of(&args[0]) {
            Some((t, n)) => done(ask(&Request::TakeBack { t, n })),
            _ => {
                eprintln!("usage: coord take-back T.pN (my post N of ticket T)");
                std::process::exit(2)
            }
        },
        // the owner's @all handed to me, on to another agent
        "pass" if args.len() >= 2 => match post_of(&args[0]) {
            Some((t, n)) => {
                let why = args[2..].join(" ");
                done(ask(&Request::Pass { t, n, to: args[1].clone(), why: (!why.trim().is_empty()).then_some(why) }))
            }
            _ => {
                eprintln!("usage: coord pass T.pN NAME [WHY...] (the owner's @all, post N of ticket T, on to agent NAME)");
                std::process::exit(2)
            }
        },
        "post" if !args.is_empty() => {
            let kind = match kind_flag(&mut args) {
                Ok(k) => k,
                Err(f) => {
                    eprintln!("coord post: {f} lacks its words ({KIND_FLAGS})");
                    std::process::exit(2)
                }
            };
            if kind.is_some() && !matches!(kind_flag(&mut args.clone()), Ok(None)) {
                eprintln!("coord post: a post has one kind");
                std::process::exit(2)
            }
            if args.is_empty() {
                usage()
            }
            let t = args.remove(0);
            // `-`: the text from stdin, as for open and answer (a heredoc of the command's own)
            let Some(text) = given_text(&args, false) else { usage() };
            done(ask(&Request::Post { t, text, to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), kind, comments: Vec::new() }))
        }
        "star" | "unstar" if args.len() == 1 => done(ask(&Request::Star { t: args.remove(0), on: cmd == "star" })),
        "archive" | "unarchive" if args.len() == 1 => done(ask(&Request::Archive { t: args.remove(0), on: cmd == "archive" })),
        // `coord tie T PARENT` (`-`: untie) and `coord title T TEXT`: the outline's ties and labels
        "tie" if args.len() == 2 => done(ask(&Request::Tie { t: args.remove(0), to: args.remove(0) })),
        "title" if args.len() >= 2 => {
            let t = args.remove(0);
            done(ask(&Request::Title { t, label: args.join(" ") }))
        }
        // `coord outline`: the open threads as a tree, each with who has it and its state
        "outline" if args.is_empty() => {
            let r = ask(&Request::Outline);
            if !r.ok {
                done(r)
            }
            print!("{}", outline_tree(&r.out));
            std::process::exit(0)
        }
        // `coord graph [ID] [--hops N] [--layer L]... [--who NAME]... [--from T] [--to T] [--limit N] [--json]`: the part of
        // the one graph the Map draws around ID, as coordd gives it to the window (`Request::Graph`): each edge a line, oldest
        // first, then what each node is (`graph_text`); --json: coordd's answer as it is
        "graph" => {
            let (q, json) = match graph_ask(&args) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("coord graph: {e}");
                    std::process::exit(2)
                }
            };
            let r = ask(&Request::Graph { ask: q.clone() });
            if !r.ok || json {
                done(r)
            }
            match serde_json::from_str::<coord_types::GraphAnswer>(&r.out) {
                Ok(a) => print!("{}", graph_text(&q, &a)),
                Err(e) => {
                    eprintln!("coord graph: coordd's answer reads as no graph ({e})");
                    std::process::exit(1)
                }
            }
            std::process::exit(0)
        }
        // `coord find WORDS`: the posts of this project's tickets, closed ones too, that hold every word, as the window's
        // Find lists them (`Request::Find`), and the mail and agents' conversations coordd reads on the host: newest first,
        // each by its id, where it is, who wrote it and the passage
        "find" if !args.is_empty() => {
            let r = ask(&Request::Find { q: args.join(" "), starred: false, wide: true });
            if !r.ok {
                done(r)
            }
            let hits: Vec<coord_types::FindHit> = r.out.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
            if hits.is_empty() {
                println!("No post holds every one of those words.");
            }
            for h in &hits {
                println!("{}", found_line(h));
            }
            if !hits.is_empty() {
                println!("coord show ID prints a whole ticket, coord mail ID a whole mail.");
            }
            std::process::exit(0)
        }
        "slow" if args.len() == 2 && (args[1] == "on" || args[1] == "off") => done(ask(&Request::Slow { t: args[0].clone(), on: args[1] == "on" })),
        "approve" if args.len() == 2 => {
            let Ok(n) = args[1].trim_start_matches('h').parse() else { usage() };
            done(ask(&Request::Approve { t: args[0].clone(), n }))
        }
        "halt-edit" if args.len() >= 3 => {
            let Ok(n) = args[1].trim_start_matches('h').parse() else { usage() };
            let Ok(value) = serde_json::from_str(&args[2..].join(" ")) else { usage() };
            done(ask(&Request::HaltEdit { t: args[0].clone(), n, value }))
        }
        "template" if args.len() >= 4 => {
            let (t, call, scope) = (args.remove(0), args.remove(0), args.remove(0));
            let Some(text) = text_or_stdin(&[]) else {
                eprintln!("coord template: the template comes on stdin (< FILE, or a pipe)");
                std::process::exit(2)
            };
            done(ask(&Request::Template { t, call, scope, text, why: args.join(" ") }))
        }
        "reload" if !args.is_empty() => {
            let commit = (args.len() >= 2 && args[0] == "--commit").then(|| {
                args.remove(0);
                args.remove(0)
            });
            done(reload_words(ask(&Request::Reload { commit, why: args.join(" ") })))
        }
        "cancel" if args.len() >= 2 => {
            let t = args.remove(0);
            done(ask(&Request::Cancel { t, why: args.join(" ") }))
        }
        "withdraw" if args.len() >= 3 => {
            let t = args.remove(0);
            let program = args.remove(0);
            done(ask(&Request::Withdraw { t, program, why: args.join(" ") }))
        }
        "edit" if args.len() >= 2 => {
            // the program on stdin, as `coord show --program T` printed it (its comments and the in-flight line may
            // stay: only what follows `answer <- (in flight);` is sent)
            let t = args.remove(0);
            if !stdin_gives_text() {
                usage()
            }
            let mut program = String::new();
            let _ = std::io::stdin().read_to_string(&mut program);
            let program = match program.split_once("answer <- (in flight);") {
                Some((_, rest)) => rest.trim_start_matches('\n').to_string(),
                None => program,
            };
            done(ask(&Request::Edit { t, program, why: args.join(" ") }))
        }
        "repair" if args.len() >= 3 => {
            let t = args.remove(0);
            let program = args.remove(0);
            done(ask(&Request::Repair { t, program, why: args.join(" ") }))
        }
        "rekind" if args.len() >= 3 => {
            let t = args.remove(0);
            let kind = args.remove(0);
            let mut under = None;
            let form = fields(cmd, &mut args, &mut under);
            if args.is_empty() {
                usage()
            }
            done(ask(&Request::Rekind { t, kind, form: Value::Object(form), why: args.join(" ") }))
        }
        "label" if !args.is_empty() => {
            let r = ask(&Request::Label { ids: std::mem::take(&mut args) });
            if !r.ok {
                done(r)
            }
            for l in r.out.lines().filter_map(|l| serde_json::from_str::<coord_types::Label>(l).ok()) {
                let on = l.on.map(|o| format!("  (on {o})")).unwrap_or_default();
                println!("{}\t{}\t{}{on}", l.id, if l.is.is_empty() { "?" } else { l.is.as_str() }, l.label);
            }
            std::process::exit(0)
        }
        "mentions" if args.len() == 1 || (args.len() == 2 && args[0] == "--json") => {
            let json = args[0] == "--json";
            let r = ask(&Request::Mentions { id: args.pop().unwrap_or_default() });
            if !r.ok || json {
                done(r)
            }
            for l in r.out.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
                let at = l["of"].as_u64().map(|n| format!("  (at p{n})")).unwrap_or_default();
                println!("{}.p{}\t{}\t{}\t{}{at}", l["by"].as_str().unwrap_or(""), l["post"], l["who"].as_str().unwrap_or(""), l["at"].as_str().unwrap_or(""), l["label"].as_str().unwrap_or(""));
            }
            std::process::exit(0)
        }
        "show" if args.len() == 1 || (args.len() == 2 && (args[0] == "--json" || args[0] == "--program")) => {
            if args[0] == "--json" {
                done(ask(&Request::Ticket { t: args.remove(1) }))
            }
            if args[0] == "--program" {
                // T's printed continuation, to edit and give back to `coord edit T WHY` on stdin
                let r = ask(&Request::Ticket { t: args.remove(1) });
                if !r.ok {
                    done(r)
                }
                match serde_json::from_str::<Value>(&r.out).ok().and_then(|v| v["continuation"].as_str().map(str::to_string)) {
                    Some(k) => println!("{k}"),
                    None => done(Reply { ok: false, out: "coord show --program: it awaits no step now, so there is nothing to edit".into(), post: None }),
                }
                std::process::exit(0)
            }
            // a post by its own reference (`N-101.p1033`): that post, whole
            if let Some((t, n)) = post_of(&args[0]) {
                done(post_whole(&t, n.into()))
            }
            let id = args.remove(0);
            let r = ask(&Request::Ticket { t: id.clone() });
            if !r.ok {
                done(r)
            }
            let mut out = String::new();
            match serde_json::from_str::<Value>(&r.out) {
                Ok(v) => {
                    tree(&v, 0, &mut out);
                    if v["posts"].as_array().is_some_and(|p| !p.is_empty()) {
                        out.push_str(&format!("Each post here is its first line; `coord show {id}.pN` prints post N whole.\n"));
                    }
                }
                Err(_) => out = r.out,
            }
            print!("{out}");
            std::process::exit(0)
        }
        "tickets" if args.len() <= 2 => {
            let filter = match (args.first().map(String::as_str), args.get(1)) {
                (None, None) => json!({}),
                (Some("--waiting"), None) => json!({"waiting": true}),
                (Some("--mine"), None) => json!({"mine": true}),
                (Some("--kind"), Some(k)) => json!({"kind": k}),
                _ => usage(),
            };
            // one line a ticket (its JSON lines are the reply): id, kind, state, title
            let r = ask(&Request::Tickets { filter });
            if !r.ok {
                done(r)
            }
            for l in r.out.lines() {
                match serde_json::from_str::<Value>(l) {
                    Ok(v) => println!("{} {} · {} · {}", v["id"].as_str().unwrap_or("?"), v["kind"].as_str().unwrap_or(""), state(&v), v["title"].as_str().unwrap_or("")),
                    Err(_) => println!("{l}"),
                }
            }
            std::process::exit(0)
        }
        _ => usage(),
    }
}

/// A value V of a ticket as a line: a text by reference as where it is, a variant as `Head: fields`.
/// Post N of ticket T, whole: a line of who and when, then its words (read through coordd when they are a reference
/// into a transcript), or its answer's fields.
fn post_whole(t: &str, n: u64) -> Reply {
    let r = ask(&Request::Ticket { t: t.to_string() });
    if !r.ok {
        return r;
    }
    let v: Value = serde_json::from_str(&r.out).unwrap_or(Value::Null);
    let Some(p) = v["posts"].as_array().into_iter().flatten().find(|p| p["n"].as_u64() == Some(n)) else {
        return Reply { ok: false, out: format!("coord show: {t} has no post {n}"), post: None };
    };
    let step = p["step"].as_u64().map(|s| format!(", the answer to step {s}{}{}", head_line(&p["head"]).map(|h| format!(": {h}")).unwrap_or_default(), lapsed_mark(&v, p))).unwrap_or_default();
    let at = p["at"].as_str().map(|a| format!(" at {a}")).unwrap_or_default();
    let words = match &p["text"] {
        Value::String(s) => s.clone(),
        Value::Object(o) if o.contains_key("loc") => match serde_json::from_value::<docket_machine::Loc>(o["loc"].clone()) {
            Ok(loc) => {
                let r = ask(&Request::Text { loc });
                if !r.ok {
                    return r;
                }
                r.out
            }
            Err(e) => return Reply { ok: false, out: format!("coord show: post {n}'s place: {e}"), post: None },
        },
        Value::Null => String::new(),
        x => serde_json::to_string_pretty(x).unwrap_or_default(),
    };
    Reply { ok: true, out: format!("{t}.p{n} by {}{at}{step}{}\n\n{}", p["by"].as_str().unwrap_or(""), cleaned_line(p), words.trim_end()), post: None }
}

/// Post P's line saying it is his words cleaned (`cleaned_from`), where what he typed is; else nothing.
fn cleaned_line(p: &Value) -> String {
    p["cleaned_from"].as_str().map(|w| format!("\nhis words cleaned; as typed: {w}'s form text (`coord show {w}`)")).unwrap_or_default()
}

/// Post P of ticket view V, when it is an owner's answer that went on by itself (`lapsed`): ", went on by itself
/// after 30 s", never read as his word; else nothing.
fn lapsed_mark(v: &Value, p: &Value) -> String {
    v["lapsed"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| serde_json::from_value::<docket_machine::ViewLapsed>(l.clone()).ok())
        .find(|l| l.post.is_some_and(|n| Some(u64::from(n)) == p["n"].as_u64()))
        .map(|l| format!(", {}", l.mark()))
        .unwrap_or_default()
}

fn shown(v: &Value) -> String {
    match v {
        Value::String(s) => s.lines().next().unwrap_or("").to_string() + if s.lines().nth(1).is_some() { " …" } else { "" },
        Value::Null => "-".into(),
        Value::Object(o) if o.contains_key("loc") => {
            let at = &o["loc"];
            let file = at["file"].as_str().unwrap_or("");
            format!("(in {}, byte {})", file.rsplit('/').next().unwrap_or(file), at["offset"])
        }
        Value::Object(o) if o.get("head").is_some_and(Value::is_string) => {
            // the fields' values in order, as a result reads (`No: not now`), an empty optional left out
            let fields: Vec<String> = o.iter().filter(|(k, x)| *k != "head" && !x.is_null()).map(|(_, x)| shown(x)).collect();
            let head = o["head"].as_str().unwrap_or("");
            if fields.is_empty() { head.into() } else { format!("{head}: {}", fields.join("; ")) }
        }
        // a raised value's record (`Cancelled {by, why}`): each field by name
        Value::Object(o) => o.iter().filter(|(_, x)| !x.is_null()).map(|(k, x)| format!("{k} {}", shown(x))).collect::<Vec<_>>().join("; "),
        _ => v.to_string(),
    }
}

/// Ticket V (a TicketView) as an indented tree at DEPTH, the tickets under it asked for and drawn beneath.
/// One hit of `coord find`, as a line: `N-12.p3  the label  the owner's, 2026-11-16 17:53: …the line…`.
fn found_line(h: &coord_types::FindHit) -> String {
    let by = if h.by == "owner" { String::from("the owner's") } else { format!("by {}", h.by) };
    let at = h.at.get(..16).unwrap_or(&h.at).replace('T', " ");
    let title = if h.title.is_empty() { String::new() } else { format!("  {}", h.title) };
    format!("{}{title}  {by}, {at}: {}", h.id, h.words)
}

fn tree(v: &Value, depth: usize, out: &mut String) {
    use std::fmt::Write as _;
    let pad = "  ".repeat(depth);
    let _ = writeln!(out, "{pad}{} {} · {}", v["id"].as_str().unwrap_or("?"), v["kind"].as_str().unwrap_or(""), state(v));
    tree_rest(v, depth, out);
}

/// Ticket V's state in words: what it waits on, or how it ended.
fn state(v: &Value) -> String {
    let st = &v["status"];
    match st["is"].as_str().unwrap_or("") {
        "asking" => format!("asking who fills {} for {} (step {})", st["role"].as_str().unwrap_or(""), st["call"].as_str().unwrap_or(""), st["step"]),
        // a hold: owed by its holder but never handed over; closed by an answer (`coord item close`)
        "awaits" if st["hold"] == true => format!("held by {} until {} (step {})", st["agent"].as_str().unwrap_or(""), st["call"].as_str().unwrap_or(""), st["step"]),
        "awaits" => format!(
            "awaits {} on {} (step {}{}{}{}){}",
            st["agent"].as_str().unwrap_or(""),
            st["call"].as_str().unwrap_or(""),
            st["step"],
            if st["taken"] == true { ", taken" } else { ", not yet taken" },
            if st["forming"] == true { ", reading the answer" } else { "" },
            if st["refusals"].as_u64().unwrap_or(0) > 0 { format!(", {} refused", st["refusals"]) } else { String::new() },
            // the step's layer and where its agent stands
            match (v["layer"].as_str(), st["stands"].as_str()) {
                (Some(l), Some(s)) => format!(" in layer {l}; {} stands in {s}", st["agent"].as_str().unwrap_or("")),
                (Some(l), None) => format!(" in layer {l}"),
                (None, Some(s)) => format!("; {} stands in {s}", st["agent"].as_str().unwrap_or("")),
                (None, None) => String::new(),
            }
        ),
        "waits" => format!("waits on {}", st["children"].as_array().map(|c| c.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default()),
        // an agents' thread (design part 9b): never done, it waits on whom it told a post, or is quiet
        "done" if st["result"]["head"] == "Between" => {
            let names: Vec<&str> = v["waits_on"].as_array().into_iter().flatten().filter_map(|w| w["agent"].as_str()).collect();
            if names.is_empty() { "quiet".to_string() } else { format!("waits on {}", names.join(", ")) }
        }
        "done" => format!("done: {}", shown(&st["result"])),
        "failed" => format!("failed: {} {}", st["raised"]["exc"].as_str().unwrap_or(""), shown(&st["raised"]["value"])),
        // a thread at its hole whose owner's post a handler holds: it waits on whom the handler asks, and must not read as
        // resting
        "resting" if v["waits_on"].as_array().is_some_and(|w| !w.is_empty()) => format!(
            "waits on {}",
            v["waits_on"].as_array().into_iter().flatten().map(|w| format!("{} on {}", w["agent"].as_str().unwrap_or("?"), w["call"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(", ")
        ),
        other => other.to_string(),
    }
}

/// The lines of ticket V under its first: who opened it, its form, its posts, the tickets under it.
fn tree_rest(v: &Value, depth: usize, out: &mut String) {
    use std::fmt::Write as _;
    let pad = "  ".repeat(depth);
    let _ = writeln!(out, "{pad}  opened by {} at {}{}", v["opener"].as_str().unwrap_or("-"), v["opened_at"].as_str().unwrap_or(""), v["closed_at"].as_str().map(|c| format!(", closed at {c}")).unwrap_or_default());
    // whom the open ticket waits on, down its tree (the steps owed at its leaves)
    let waits: Vec<String> = v["waits_on"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| {
            let layer = w["layer"].as_str().map(|l| format!(", in layer {l}")).unwrap_or_default();
            let stands = w["stands"].as_str().map(|s| format!("; stands in {s}")).unwrap_or_default();
            format!("{} ({}, {}{layer}{stands})", w["agent"].as_str().unwrap_or("?"), w["call"].as_str().unwrap_or(""), w["ticket"].as_str().unwrap_or(""))
        })
        .collect();
    if !waits.is_empty() && v["status"]["is"] == "waits" {
        let _ = writeln!(out, "{pad}  waits on {}", waits.join("; "));
    }
    if let Some(b) = v["broken"].as_str() {
        let _ = writeln!(out, "{pad}  BROKEN: {b}");
    }
    if let Some(form) = v["form"].as_object() {
        for (k, x) in form {
            let _ = writeln!(out, "{pad}  {k}: {}", shown(x));
        }
    }
    for p in v["posts"].as_array().into_iter().flatten() {
        let step = p["step"].as_u64().map(|s| format!(" (answer to step {s}{}{})", head_line(&p["head"]).map(|h| format!(": {h}")).unwrap_or_default(), lapsed_mark(v, p))).unwrap_or_default();
        let _ = writeln!(out, "{pad}  #{} {}{step}: {}", p["n"], p["by"].as_str().unwrap_or(""), shown(&p["text"]));
    }
    // its halts (slow mode), each what `coordc approve ID N` lets through
    for h in v["halts"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "{pad}  halt {}: {}", h["n"], halt_line(h));
    }
    // the tickets handling an event under it (a kind's `when`, a `during`'s look), with their own halts
    for hv in v["handlers"].as_array().into_iter().flatten() {
        tree(hv, depth + 1, out);
    }
    for c in v["children"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let r = ask(&Request::Ticket { t: c.to_string() });
        match serde_json::from_str::<Value>(&r.out) {
            Ok(cv) if r.ok => tree(&cv, depth + 1, out),
            _ => {
                let _ = writeln!(out, "{pad}  {c}: {}", r.out);
            }
        }
    }
}

/// A halt H (the view's `ViewHalt`) in a line: what it lets through when approved.
fn halt_line(h: &Value) -> String {
    let s = |k: &str| h[k].as_str().unwrap_or("").to_string();
    match h["what"].as_str().unwrap_or("") {
        "give" => format!("step {} to {} ({})", h["step"], s("worker"), s("call")),
        "take" => format!("{}'s answer to step {} ({}): {}", s("worker"), h["step"], s("call"), head_line(&h["head"]).unwrap_or_default()),
        // one card for one tell: his burst's posts with the first (`with`, E1b), all let through together
        "deliver" => {
            let with: Vec<String> = h["with"].as_array().into_iter().flatten().map(|n| n.to_string()).collect();
            let posts = if with.is_empty() { format!("post {}", h["post"]) } else { format!("posts {}, {}", h["post"], with.join(", ")) };
            format!("{posts} by {} to {}", s("by"), h["wakes"].as_array().map(|w| w.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default())
        }
        "open" => format!("the {} opened", s("kind")),
        "decide" => format!("the policy's answer to {}: {}", s("question"), shown(&h["answer"])),
        "notice" => format!("a {} notice to {}", s("notice"), s("to")),
        other => other.to_string(),
    }
}

/// An answer's head as a post's line shows it: its constructor, and the fields that are a word (a ticket, a commit) in
/// brackets, `Place(N-1)`; longer words are the post's own text, not repeated.
fn head_line(h: &serde_json::Value) -> Option<String> {
    let o = h.as_object()?;
    let head = o.get("head")?.as_str()?;
    let short: Vec<&str> = o.iter().filter(|(k, _)| *k != "head").filter_map(|(_, x)| x.as_str()).filter(|s| !s.is_empty() && s.len() <= 40 && !s.contains(char::is_whitespace)).collect();
    Some(if short.is_empty() { head.to_string() } else { format!("{head}({})", short.join(", ")) })
}

/// A post's kind flags, as `kind_flag` reads them.
const KIND_FLAGS: &str = "--ask NAME[,NAME...] | --read NAME[,NAME...] HEAD | --blocker NAME[,NAME...] | --hold I-N | --result | --fyi";

/// A kind flag at the front of ARGS, taken off it with its words (`KIND_FLAGS`): what a post is, on the request as
/// `{"is": "ask", "to": ["w2"]}` and the like. None when ARGS begin with no such flag; Err(the flag) when its words are
/// not there. coordd checks the names and the item (`kind_taken`).
fn kind_flag(args: &mut Vec<String>) -> Result<Option<docket_machine::PostKind>, String> {
    use docket_machine::PostKind;
    let words = match args.first().map(String::as_str) {
        Some("--ask" | "--blocker" | "--hold") => 1,
        Some("--read") => 2,
        Some("--result" | "--fyi") => 0,
        _ => return Ok(None),
    };
    let flag = args.remove(0);
    if args.len() < words || args[..words].iter().any(|w| w.trim().is_empty() || w.starts_with("--")) {
        return Err(flag);
    }
    let mut w = args.drain(..words);
    let mut next = || w.next().unwrap_or_default();
    let names = |s: String| s.split(',').filter(|n| !n.is_empty()).map(String::from).collect::<Vec<_>>();
    Ok(Some(match flag.as_str() {
        "--ask" => PostKind::Ask { to: names(next()) },
        "--read" => {
            let to = names(next());
            PostKind::Read { to, head: next() }
        }
        "--blocker" => PostKind::Blocker { to: names(next()) },
        "--hold" => PostKind::Hold { on: next() },
        "--result" => PostKind::Result,
        _ => PostKind::Fyi,
    }))
}

/// `N-412.p30`: the ticket as written and the post's number, read by `docket_machine::post_ref`, the one parse of a
/// post's id; none with no ticket (`.p3`).
fn post_of(id: &str) -> Option<(String, u32)> {
    docket_machine::post_ref(id).filter(|(t, _)| !t.is_empty()).map(|(t, n)| (t.to_string(), n))
}

/// Whether ID names an old mail (`q12`, `m40`): the mail layer's ids, before tickets (`N-12`, `Q-3`).
fn old_mail_id(id: &str) -> bool {
    id.len() > 1 && matches!(id.as_bytes()[0], b'q' | b'm') && id[1..].bytes().all(|b| b.is_ascii_digit())
}

/// `coord outline`'s rows (JSON lines of `ViewOutline`) as a tree: each thread under its parent in the order of the
/// posts it stands at, with who has it and its state; a row whose parent is not open stands at the top.
pub fn outline_tree(out: &str) -> String {
    let rows: Vec<Value> = out.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let ids: std::collections::BTreeSet<&str> = rows.iter().filter_map(|r| r["id"].as_str()).collect();
    fn walk(rows: &[Value], ids: &std::collections::BTreeSet<&str>, parent: Option<&str>, depth: usize, out: &mut String) {
        let mut mine: Vec<&Value> = rows.iter().filter(|r| match parent {
            Some(p) => r["parent"].as_str() == Some(p),
            None => r["parent"].as_str().is_none_or(|p| !ids.contains(p)),
        }).collect();
        mine.sort_by_key(|r| (r["at_post"].as_u64().unwrap_or(0), r["opened_at"].as_str().unwrap_or("").to_string()));
        for r in mine {
            let id = r["id"].as_str().unwrap_or("");
            let who = r["who"].as_str().map(|w| format!(" · {w}")).unwrap_or_default();
            out.push_str(&format!("{}{} {}{who} · {}\n", "  ".repeat(depth), r["label"].as_str().unwrap_or(""), id, r["state"].as_str().unwrap_or("")));
            walk(rows, ids, Some(id), depth + 1, out);
        }
    }
    let mut s = String::new();
    walk(&rows, &ids, None, 0, &mut s);
    s
}

/// `coord graph`'s words as the ask the window sends (`graph_slice::Ask`), and whether `--json` was given. ID: a post
/// (`N-1.p5`), a ticket or an item (`N-12`, `I-1478`), a node as the graph names it (`commit:1a2b3c4d`, `job:…`), or
/// an agent by its bare name (`agent:NAME`); none: the whole graph, its newest edges. Its layers: his messages, agent
/// to agent, deliveries and work, unless `--layer` names others (`all`: every one), as the calls carry prompts and the
/// files the paths an agent read; 2 hops and 200 edges unless given (the Map's are 3 and 6000: a text is read whole).
fn graph_ask(args: &[String]) -> Result<(docket_machine::graph_slice::Ask, bool), String> {
    use docket_machine::graph_slice::{Ask, Layer};
    let mut q = Ask { hops: 2, limit: 200, ..Ask::default() };
    let (mut json, mut layers, mut all) = (false, Vec::new(), false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or_else(|| format!("{flag} needs its value"));
        match a.as_str() {
            "--json" => json = true,
            "--hops" => q.hops = value("--hops")?.parse().map_err(|_| "--hops takes a number".to_string())?,
            "--limit" => q.limit = value("--limit")?.parse().map_err(|_| "--limit takes a number".to_string())?,
            "--layer" => match value("--layer")?.as_str() {
                "all" => all = true,
                // by its name as the window sends it: read as a Layer, any word is `other`
                l => layers.push(
                    Layer::ALL
                        .into_iter()
                        .chain([Layer::Other])
                        .find(|x| json!(x).as_str() == Some(l))
                        .ok_or_else(|| format!("there is no layer {l}: his, agents, deliveries, work, calls, files, other or all"))?,
                ),
            },
            "--who" => q.who.push(value("--who")?),
            "--from" => q.from = Some(value("--from")?),
            "--to" => q.to = Some(value("--to")?),
            f if f.starts_with("--") => return Err(format!("unknown flag {f}")),
            id if q.around.is_none() => q.around = Some(graph_node(id)),
            more => return Err(format!("one node at a time: {more} after {}", q.around.as_deref().unwrap_or(""))),
        }
    }
    q.layers = if all {
        Default::default()
    } else if layers.is_empty() {
        [Layer::His, Layer::Agents, Layer::Deliveries, Layer::Work].into_iter().collect()
    } else {
        layers.into_iter().collect()
    };
    Ok((q, json))
}

/// A node as `coord graph` is given it, named as the graph names it: a ticket, a post or an item as it is (`N-12`,
/// `N-12.p3`, `I-1478`), a node with its kind (`commit:…`) as it is, any other word an agent's name (`agent:NAME`).
fn graph_node(id: &str) -> String {
    let ticket = id.split_once('-').is_some_and(|(k, n)| {
        !k.is_empty() && k.chars().all(|c| c.is_ascii_uppercase()) && n.split(".p").next().is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
    });
    if ticket || id.contains(':') { id.to_string() } else { format!("agent:{id}") }
}

/// `coord graph`'s text for ASK's answer A: what it is around and how much it holds, each edge a line, oldest first
/// (`WHEN  LAYER  FROM KIND TO`), then each node with what it is, its writer and its label as coordd's `labels` read
/// it (the Map's, `coord show`'s), its first line, cut at 120 characters.
fn graph_text(q: &docket_machine::graph_slice::Ask, a: &coord_types::GraphAnswer) -> String {
    use std::fmt::Write as _;
    let word = |v: Value| v.as_str().unwrap_or("?").to_string();
    let s = &a.slice;
    let mut out = String::new();
    if s.unknown {
        let _ = writeln!(out, "The graph holds no node {}.", q.around.as_deref().unwrap_or(""));
        return out;
    }
    let around = q.around.as_deref().map_or("the whole graph".to_string(), |id| format!("around {id}, {} hops", q.hops));
    let layers = s.shown.iter().map(|(l, n)| format!("{} {n}", word(json!(l)))).collect::<Vec<_>>().join(", ");
    let left = if s.left > 0 { format!("; {} more left out by the limit (--limit)", s.left) } else { String::new() };
    let _ = writeln!(out, "{around}: {} edges ({}), {} nodes{left}", s.edges.len(), if layers.is_empty() { "none" } else { layers.as_str() }, s.nodes.len());
    for e in &s.edges {
        let _ = writeln!(out, "{:<20}  {:<10} {} {} {}", e.edge.when.as_deref().unwrap_or("(no time)"), word(json!(e.layer)), e.edge.from, word(json!(e.edge.kind)), e.edge.to);
    }
    if !s.nodes.is_empty() {
        out.push('\n');
    }
    let labels: std::collections::HashMap<&str, &coord_types::Label> = a.labels.iter().map(|l| (l.id.as_str(), l)).collect();
    for n in &s.nodes {
        let id = n.node.id.as_str();
        let (is, label) = match labels.get(id) {
            Some(l) if !l.is.is_empty() => (l.is.clone(), l.label.clone()),
            _ => (word(json!(n.node.kind)), n.node.what.clone()),
        };
        let by = n.by.as_deref().map(|b| format!(" by {b}")).unwrap_or_default();
        let label: String = label.lines().next().unwrap_or("").chars().take(120).collect();
        let _ = writeln!(out, "{id}  {is}{by}{}", if label.is_empty() { String::new() } else { format!(": {label}") });
    }
    out
}

#[cfg(test)]
mod tests {
    /// `coord post` and `coord msg`'s kind flags: taken off the front with their words, the rest left as it was; a flag
    /// without its words is refused by name.
    #[test]
    fn a_kind_flag_is_taken_off_the_front_with_its_words() {
        use docket_machine::PostKind;
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        let took = |v: &[&str]| {
            let mut args = a(v);
            (super::kind_flag(&mut args), args)
        };
        assert_eq!(took(&["--ask", "w2,w3", "N-12", "check the lock"]), (Ok(Some(PostKind::Ask { to: a(&["w2", "w3"]) })), a(&["N-12", "check the lock"])));
        assert_eq!(took(&["--read", "w2", "core2-pp 1a2b3c4d", "N-12", "-"]), (Ok(Some(PostKind::Read { to: a(&["w2"]), head: "core2-pp 1a2b3c4d".into() })), a(&["N-12", "-"])));
        assert_eq!(took(&["--blocker", "w2", "N-12", "x"]), (Ok(Some(PostKind::Blocker { to: a(&["w2"]) })), a(&["N-12", "x"])));
        assert_eq!(took(&["--hold", "I-1474", "N-12", "x"]), (Ok(Some(PostKind::Hold { on: "I-1474".into() })), a(&["N-12", "x"])));
        assert_eq!(took(&["--result", "N-12", "x"]), (Ok(Some(PostKind::Result)), a(&["N-12", "x"])));
        assert_eq!(took(&["--fyi", "N-12", "x"]), (Ok(Some(PostKind::Fyi)), a(&["N-12", "x"])));
        assert_eq!(took(&["N-12", "--ask", "w2"]), (Ok(None), a(&["N-12", "--ask", "w2"])), "only at the front");
        assert_eq!(took(&["--re", "N-12", "w2", "x"]), (Ok(None), a(&["--re", "N-12", "w2", "x"])), "msg's own flags are msg's");
        assert_eq!(took(&["--read", "w2"]).0, Err("--read".to_string()));
        assert_eq!(took(&["--ask", "--fyi", "N-12"]).0, Err("--ask".to_string()));
    }

    /// `coord`'s help (agent-base/bin/coord) gives `coord post`'s kind flags word for word as `KIND_FLAGS`, so a flag
    /// added to one and not the other is red.
    #[test]
    fn coords_help_gives_the_kind_flags_as_coordc_takes_them() {
        let help = std::fs::read_to_string(format!("{}/../../agent-base/bin/coord", env!("CARGO_MANIFEST_DIR"))).unwrap();
        // coord post's entry: its line and the lines under it, up to the next command's
        let mut lines = help.lines().skip_while(|l| !l.starts_with("#   coord post "));
        let entry: Vec<&str> = lines.next().into_iter().chain(lines.take_while(|l| l.starts_with("#    "))).collect();
        assert!(entry.iter().any(|l| l.contains(super::KIND_FLAGS)), "coord post's help holds {}: {entry:#?}", super::KIND_FLAGS);
    }

    #[test]
    fn a_hook_waits_while_a_coord_switch_runs_and_not_otherwise() {
        use super::hook_ask;
        use crate::switch::{started, switch_mark, switching};
        use std::io::{BufRead, BufReader, Write};
        use std::time::{Duration, Instant};
        let dir = std::env::temp_dir().join(format!("coordc-switch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("research.sock.d")).unwrap();
        let sock = dir.join("research.sock").display().to_string();
        // where the mark is: SOCK.d for the host's socket, the folder itself for a box's link into it
        assert_eq!(switch_mark(&sock), dir.join("research.sock.d/switching"));
        let boxed = dir.join(".coordd.sock");
        std::os::unix::fs::symlink("research.sock.d/coordd.sock", &boxed).unwrap();
        assert_eq!(switch_mark(&boxed.display().to_string()), dir.join("research.sock.d/switching"));
        // no coordd, no switch: nothing waited for
        let at = Instant::now();
        assert_eq!(hook_ask(&sock, "{}", Duration::from_millis(20)), None);
        assert!(at.elapsed() < Duration::from_secs(1));
        // a mark whose switch is gone (killed): nothing waited for either
        let mark = dir.join("research.sock.d/switching");
        let mut gone = std::process::Command::new("true").spawn().unwrap();
        let dead = gone.id();
        gone.wait().unwrap();
        std::fs::write(&mark, format!("{dead} 1\n")).unwrap();
        assert!(!switching(&sock));
        assert_eq!(hook_ask(&sock, "{}", Duration::from_millis(20)), None);
        // a live pid whose start time is not the switch's (the pid given again): nothing waited for
        let me = std::process::id().to_string();
        std::fs::write(&mark, format!("{me} 1\n")).unwrap();
        assert!(!switching(&sock), "another process with the pid holds nothing");
        // the switch running (this process, its own start time): the hook waits until the new coordd answers
        std::fs::write(&mark, format!("{me} {}\n", started(&me).unwrap())).unwrap();
        assert!(switching(&sock) && switching(&boxed.display().to_string()));
        let s2 = sock.clone();
        let up = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let l = std::os::unix::net::UnixListener::bind(&s2).unwrap();
            let (c, _) = l.accept().unwrap();
            let mut r = BufReader::new(c);
            let mut got = String::new();
            r.read_line(&mut got).unwrap();
            writeln!(r.get_mut(), "{{\"ok\":true,\"out\":\"\"}}").unwrap();
            got
        });
        let at = Instant::now();
        let answer = hook_ask(&sock, r#"{"op":"hook","event":"stop"}"#, Duration::from_millis(20));
        assert!(at.elapsed() >= Duration::from_millis(400), "it waited for the new coordd");
        assert_eq!(answer.as_deref(), Some("{\"ok\":true,\"out\":\"\"}\n"));
        assert_eq!(up.join().unwrap(), "{\"op\":\"hook\",\"event\":\"stop\"}\n", "the hook's request reached it whole");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coord_end_is_a_thread_end_only_after_ticket_ids() {
        let a = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        let ended = |s: &str| crate::thread_end(&a(s));
        assert_eq!(ended("N-12 done the lexer is merged"), Some((vec!["N-12".into()], "done".into(), "the lexer is merged".into())));
        assert_eq!(ended("N-12 Q-3 dropped not needed"), Some((vec!["N-12".into(), "Q-3".into()], "dropped".into(), "not needed".into())));
        // a job's end whose why holds the words: the job end, as before
        assert_eq!(ended("j12 build done and pushed"), None);
        assert_eq!(ended("N-12 build done and pushed"), None, "a word that is no id before the how");
        assert_eq!(ended("N-12 done"), None, "no why");
        let no_why = |s: &str| crate::thread_end_without_why(&a(s));
        assert!(no_why("N-12 done") && no_why("N-12 Q-3 kept"), "ids and a how, no why: refused, not a job's end");
        assert!(!no_why("j12 done") && !no_why("N-12 done w") && !no_why("N-12") && !no_why("N-12 build"));
        assert_eq!(ended("done because"), None, "no id");
        assert_eq!(ended("N-12 finished w"), None);
    }

    /// A window from before `ASKS_CLOSED_PROTOCOL` decodes every line about a record holding a withdrawal and an answer
    /// in a subthread, as NothingToAnswer and Followed; a newer window gets them as they are, and reads a newer way as
    /// Unknown.
    #[test]
    fn a_window_from_before_protocol_6_reads_a_withdrawal_and_a_subthread() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        #[serde(tag = "how")]
        enum Old {
            Answered { post: u32, by: String },
            Told { step: u32 },
            NothingToAnswer,
            Posted { post: u32 },
            Followed { ticket: String },
            Cancelled,
        }
        use docket_machine::line::AsksClosed;
        use serde_json::{Value, json};
        let posts = |a: AsksClosed, b: AsksClosed| json!({"t": "N-7", "posts": [{"n": 2, "asks_closed": a}, {"n": 3, "asks_closed": b}, {"n": 4, "asks_closed": {"how": "Posted", "post": 5}}]});
        let new = posts(AsksClosed::Withdrawn { by: "lead".into(), why: "overtaken".into() }, AsksClosed::Subthread { ticket: "N-9".into() });
        let olds = |v: &Value| v["posts"].as_array().unwrap().iter().map(|p| serde_json::from_value::<Old>(p["asks_closed"].clone())).collect::<Vec<_>>();
        assert!(olds(&new).iter().any(Result::is_err), "as they are, the old window cannot read them");
        let mut ev = json!({"TicketChange": new.clone()});
        super::older_asks_closed(&mut ev);
        let got: Vec<Old> = olds(&ev["TicketChange"]).into_iter().map(Result::unwrap).collect();
        assert_eq!(got, vec![Old::NothingToAnswer, Old::Followed { ticket: "N-9".into() }, Old::Posted { post: 5 }]);
        // a reply's lines (coord show --json, Ticket): each JSON line put the same way, the rest as it is
        let out = format!("{}\nnot json\n{}", new, json!({"x": 1}));
        let older = super::older_asks_closed_text(&out);
        let first: Value = serde_json::from_str(older.lines().next().unwrap()).unwrap();
        assert!(olds(&first).into_iter().all(|r| r.is_ok()) && older.lines().nth(1) == Some("not json"));
        assert_eq!(super::older_asks_closed_text("plain words"), "plain words");
        // a newer window reads them, and a way newer than it as Unknown
        let read: Vec<AsksClosed> = new["posts"].as_array().unwrap().iter().map(|p| serde_json::from_value(p["asks_closed"].clone()).unwrap()).collect();
        assert!(matches!(read[0], AsksClosed::Withdrawn { .. }) && matches!(read[1], AsksClosed::Subthread { .. }));
        assert_eq!(serde_json::from_value::<AsksClosed>(json!({"how": "Later", "x": 1})).unwrap(), AsksClosed::Unknown);
    }

    /// A stand-in coordd that sends its snapshot late: the bridge answers an attach before it, and asks coordd nothing
    /// until it has come.
    #[test]
    fn an_attach_is_answered_before_the_snapshot_and_a_coordd_request_after_it() {
        use super::*;
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};
        let gate = super::Gate::default();
        let (mut coordd, stream) = UnixStream::pair().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let (opens, e) = (gate.clone(), events.clone());
        std::thread::spawn(move || super::pass_events(stream, &opens, |v| e.lock().unwrap().push(v)));
        let (printed, asked) = (Arc::new(Mutex::new(Vec::<Value>::new())), Arc::new(Mutex::new(0)));
        let (p, a, g) = (printed.clone(), asked.clone(), gate.clone());
        let (mut app, input) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let here = |req: Request| match req {
                Request::Attach { .. } => Ok(Reply { ok: true, out: "attached".into(), post: None }),
                r => Err(r),
            };
            let ask = Arc::new(move |_: &Request| {
                *a.lock().unwrap() += 1;
                Reply { ok: true, out: "from coordd".into(), post: None }
            });
            super::serve(BufReader::new(input).lines(), &g, |_| {}, here, ask, Arc::new(move |v| p.lock().unwrap().push(v)))
        });
        let attach = Request::Attach { name: "x".into(), rows: 24, cols: 80 };
        writeln!(app, "{}", json!({"id": 1, "req": attach})).unwrap();
        writeln!(app, "{}", json!({"id": 2, "req": Request::Kinds})).unwrap();
        let replied = |id: u64| printed.lock().unwrap().iter().any(|v| v["id"] == id);
        let within = |f: &dyn Fn() -> bool| {
            let end = Instant::now() + Duration::from_secs(5);
            while !f() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(10));
            }
            f()
        };
        assert!(within(&|| replied(1)), "the attach is answered with no snapshot yet");
        std::thread::sleep(Duration::from_millis(200));
        assert!(!replied(2), "a coordd request is not answered before the snapshot");
        assert_eq!(*asked.lock().unwrap(), 0, "nor asked of coordd");
        writeln!(coordd, "{}", json!({"seq": 1, "snapshot": {}})).unwrap();
        assert!(within(&|| replied(2)), "it is, once the snapshot has come");
        assert_eq!(events.lock().unwrap().len(), 1);
        drop(app);
        for t in server.join().unwrap() {
            t.join().unwrap();
        }
    }

    /// A window that asks 200 texts at once while coordd is slow: no more than ASKS_AT_ONCE are at coordd at a time, and
    /// every one is answered once coordd answers again.
    #[test]
    fn a_window_asking_many_at_once_holds_at_most_asks_at_once_at_coordd() {
        use super::*;
        use std::sync::{Arc, Condvar, Mutex};
        use std::time::{Duration, Instant};
        let gate = super::Gate::default();
        gate.open();
        // coordd's lock held: each ask waits until `slow` is let go, and counts how many are at coordd now and at most
        let slow = Arc::new((Mutex::new((true, 0usize, 0usize)), Condvar::new()));
        let printed = Arc::new(Mutex::new(Vec::<Value>::new()));
        let (s, p, g) = (slow.clone(), printed.clone(), gate.clone());
        let (mut app, input) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let ask = Arc::new(move |_: &Request| {
                let (m, c) = &*s;
                let mut at = m.lock().unwrap();
                at.1 += 1;
                at.2 = at.2.max(at.1);
                while at.0 {
                    at = c.wait(at).unwrap();
                }
                at.1 -= 1;
                Reply { ok: true, out: "the text".into(), post: None }
            });
            super::serve(BufReader::new(input).lines(), &g, |_| {}, Err, ask, Arc::new(move |v| p.lock().unwrap().push(v)))
        });
        for id in 1..=200u64 {
            writeln!(app, "{}", json!({"id": id, "req": Request::Kinds})).unwrap();
        }
        let at_coordd = || slow.0.lock().unwrap().1;
        let end = Instant::now() + Duration::from_secs(5);
        while at_coordd() < super::ASKS_AT_ONCE && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(at_coordd(), super::ASKS_AT_ONCE, "as many as ASKS_AT_ONCE go to coordd at once, and no more");
        assert!(printed.lock().unwrap().is_empty());
        {
            let (m, c) = &*slow;
            m.lock().unwrap().0 = false;
            c.notify_all();
        }
        drop(app);
        let threads = server.join().unwrap();
        assert_eq!(threads.len(), super::ASKS_AT_ONCE);
        for t in threads {
            t.join().unwrap();
        }
        let mut ids: Vec<u64> = printed.lock().unwrap().iter().map(|v| v["id"].as_u64().unwrap()).collect();
        ids.sort();
        assert_eq!(ids, (1..=200).collect::<Vec<_>>(), "every request answered, once");
        assert_eq!(slow.0.lock().unwrap().2, super::ASKS_AT_ONCE);
    }

    /// The release a coordc runs from: its folder's name inside `releases`, nothing elsewhere (protocol 1's hello).
    #[test]
    fn the_bridge_resumes_from_a_run_and_a_number() {
        let a = |s: &[&str]| super::resume_point(&s.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&["--from", "2026-11-18T10:01:56Z-4242:1234"]), Some(("2026-11-18T10:01:56Z-4242".into(), 1234)));
        assert_eq!(a(&[]), None);
        assert_eq!(a(&["--from", "1234"]), None, "no run: never resumed");
        assert_eq!(a(&["--from", "r:x"]), None);
        let b = |s: &[&str]| super::box_arg(&s.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(b(&["--box", "w-7f3a", "--from", "r:1"]), Some("w-7f3a".into()));
        assert_eq!((b(&["--from", "r:1"]), b(&["--box"]), b(&["--box", "--from"])), (None, None, None));
    }

    #[test]
    fn a_cleaned_post_says_where_his_typed_words_are() {
        let p = serde_json::json!({"n": 3, "by": "owner", "text": "the fix", "cleaned_from": "W-12"});
        assert_eq!(super::cleaned_line(&p), "\nhis words cleaned; as typed: W-12's form text (`coord show W-12`)");
        assert_eq!(super::cleaned_line(&serde_json::json!({"n": 3})), "");
    }

    #[test]
    fn the_hello_names_the_release_coordc_runs_from() {
        let p = |s: &str| super::release_of(std::path::Path::new(s));
        assert_eq!(p("/home/t/Documents/papers/soft-swarm/releases/20261118-101752-e0eb4d1/coordc").as_deref(), Some("20261118-101752-e0eb4d1"));
        assert_eq!(p("/home/t/x/yaaihh/target/release/coordc"), None);
        assert_eq!(p("/coordc"), None);
    }

    /// A window's hello that names its release asks coordd to record it, the first hello and a taker's second alike, with
    /// the time it says it became his window; one naming none, or an empty name, asks nothing.
    #[test]
    fn a_window_hello_naming_its_release_asks_coordd_to_record_it() {
        let asks = |h: serde_json::Value| match super::window_release(&h) {
            Some(coord_types::Request::WindowRelease { release, since: None }) => Some(release),
            other => other.map(|r| format!("{r:?}")),
        };
        let at = serde_json::to_value(coord_types::Hello { since: Some("2026-11-21T09:34:49Z".into()), ..coord_types::Hello::ours(Some("20261121-101928-4ee5c14e".into())) }).unwrap();
        assert!(matches!(super::window_release(&at), Some(coord_types::Request::WindowRelease { release, since: Some(s) }) if release == "20261121-101928-4ee5c14e" && s == "2026-11-21T09:34:49Z"));
        let ours = |r: Option<&str>| serde_json::to_value(coord_types::Hello::ours(r.map(String::from))).unwrap();
        assert_eq!(asks(ours(Some("20261120-232357-a633aa96"))).as_deref(), Some("20261120-232357-a633aa96"));
        assert_eq!(asks(ours(None)), None);
        assert_eq!(asks(ours(Some(""))), None);
        assert_eq!(asks(serde_json::json!({"protocol": 30, "min": 1})), None);
    }

    /// `coord find`'s line: the owner's post marked as his, the time to the minute.
    #[test]
    fn a_find_hit_reads_as_one_line() {
        let h = coord_types::FindHit { id: "N-12.p3".into(), t: 12, post: Some(3), title: "the lexer".into(), by: "owner".into(), at: "2026-11-16T17:53:04Z".into(), words: "keep the lexer's tokens".into(), star: None, archived: false };
        assert_eq!(super::found_line(&h), "N-12.p3  the lexer  the owner's, 2026-11-16 17:53: keep the lexer's tokens");
        let h = coord_types::FindHit { by: "orch".into(), title: String::new(), ..h };
        assert_eq!(super::found_line(&h), "N-12.p3  by orch, 2026-11-16 17:53: keep the lexer's tokens");
    }

    /// `coord outline`'s rows as a tree: a child under its parent in the order of its tie post; a row whose parent is
    /// not among the open rows stands at the top.
    #[test]
    fn the_outline_prints_each_thread_under_its_parent_in_the_order_of_its_post() {
        let rows = [
            r#"{"id":"N-379","label":"the lexer","parent":"N-186","at_post":7,"who":"fixer","state":"at work","opened_at":"b"}"#,
            r#"{"id":"N-186","label":"the parser","parent":null,"at_post":null,"who":null,"state":"waits on you","opened_at":"a"}"#,
            r#"{"id":"N-378","label":"the tests","parent":"N-186","at_post":3,"who":null,"state":"quiet","opened_at":"c"}"#,
            r#"{"id":"N-380","label":"tokens","parent":"N-379","at_post":2,"who":"fixer","state":"waits on fixer","opened_at":"d"}"#,
            r#"{"id":"N-9","label":"orphan","parent":"N-1","at_post":1,"who":null,"state":"done","opened_at":"e"}"#,
        ];
        assert_eq!(
            super::outline_tree(&rows.join("\n")),
            "the parser N-186 · waits on you\n  the tests N-378 · quiet\n  the lexer N-379 · fixer · at work\n    tokens N-380 · fixer · waits on fixer\norphan N-9 · done\n"
        );
    }

    /// The new line back goes only to an app that can read it, and the owner model's change too; every other event to
    /// any app, one with no hello too.
    #[test]
    fn a_line_back_goes_only_to_an_app_that_reads_it() {
        let line = |b: coord_types::Body| serde_json::to_value(coord_types::Event { seq: 3, body: b }).unwrap();
        let m = line(coord_types::Body::Mentioned { id: "N-1".into(), lines: vec![] });
        assert!(m.get("mentioned").is_some(), "{m}");
        assert!(!super::for_app(&m, 0) && !super::for_app(&m, coord_types::MENTIONED_BODY - 1) && super::for_app(&m, coord_types::MENTIONED_BODY));
        let n = line(coord_types::Body::Notices(vec![]));
        assert!(super::for_app(&n, 0) && super::for_app(&n, coord_types::MENTIONED_BODY));
        // the owner model's change likewise, from OWNER_MODEL_PROTOCOL, a node or none
        for o in [None, Some(coord_types::OwnerModelView { node: "O-4".into(), ..Default::default() })] {
            let o = line(coord_types::Body::OwnerModel(o));
            assert!(o.get("ownermodel").is_some(), "{o}");
            assert!(!super::for_app(&o, 0) && !super::for_app(&o, coord_types::OWNER_MODEL_PROTOCOL - 1) && super::for_app(&o, coord_types::OWNER_MODEL_PROTOCOL), "{o}");
        }
        // the look loaded live: to a window that reads it, none below its number, a null look too
        for look in [serde_json::json!({"colour": {"attention": "#e5484d"}}), serde_json::Value::Null] {
            let l = line(coord_types::Body::Look(look));
            assert!(l.get("look").is_some(), "{l}");
            assert!(!super::for_app(&l, 0) && !super::for_app(&l, coord_types::LOOK_PROTOCOL - 1) && super::for_app(&l, coord_types::LOOK_PROTOCOL), "{l}");
        }
    }

    /// Every verb `ticket_verb` answers is sent to it by the dispatch, so no command prints the usage instead.
    #[test]
    fn every_ticket_verb_is_dispatched() {
        let src = include_str!("client.rs");
        let list = src.lines().find(|l| l.trim_end().ends_with("=> ticket_verb(&cmd, args),")).expect("the dispatch's list");
        let body = &src[src.find("fn ticket_verb(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        let verbs: Vec<&str> = body.lines().filter_map(|l| l.strip_prefix("        \"")).filter_map(|l| l.split_once('"').map(|(v, _)| v)).collect();
        assert!(verbs.contains(&"label") && verbs.contains(&"show"), "{verbs:?}");
        for v in verbs {
            assert!(list.contains(&format!("\"{v}\"")), "`{v}` is answered by ticket_verb but not sent to it: {list}");
        }
    }

    /// `coord graph`: its words as the window's ask, an agent named bare as the graph names it, the calls and files layers
    /// only when asked for; the answer as lines, each edge in coordd's order, then each node with what it is, its writer
    /// and its label's first line.
    #[test]
    fn coord_graph_asks_the_window_s_slice_and_prints_it_as_lines() {
        use docket_machine::graph_slice::Layer;
        use serde_json::json;
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        let (q, json) = super::graph_ask(&a(&["docket-edge8"])).unwrap();
        assert_eq!((q.around.as_deref(), q.hops, q.limit, json), (Some("agent:docket-edge8"), 2, 200, false));
        assert_eq!(q.layers.iter().copied().collect::<Vec<_>>(), [Layer::His, Layer::Agents, Layer::Deliveries, Layer::Work], "no calls, no files");
        for id in ["N-1446", "N-1446.p2717", "I-1478", "commit:1fca2905"] {
            assert_eq!(super::graph_ask(&a(&[id])).unwrap().0.around.as_deref(), Some(id));
        }
        let (q, json) = super::graph_ask(&a(&["--layer", "calls", "N-1446.p2717", "--hops", "1", "--who", "paper-research", "--from", "2026-11-24T00:00:00Z", "--json"])).unwrap();
        assert_eq!(
            (q.around.as_deref(), q.hops, q.who.as_slice(), q.from.as_deref(), json),
            (Some("N-1446.p2717"), 1, ["paper-research".to_string()].as_slice(), Some("2026-11-24T00:00:00Z"), true)
        );
        assert_eq!(q.layers.iter().copied().collect::<Vec<_>>(), [Layer::Calls]);
        assert!(super::graph_ask(&a(&["--layer", "all"])).unwrap().0.layers.is_empty(), "every layer");
        assert_eq!(super::graph_ask(&a(&[])).unwrap().0.around, None, "the whole graph");
        assert_eq!(super::graph_ask(&a(&["--layer", "prompts"])).unwrap_err(), "there is no layer prompts: his, agents, deliveries, work, calls, files, other or all");
        assert_eq!(super::graph_ask(&a(&["--hops"])).unwrap_err(), "--hops needs its value");
        assert_eq!(super::graph_ask(&a(&["N-1", "N-2"])).unwrap_err(), "one node at a time: N-2 after N-1");
        assert_eq!(super::graph_ask(&a(&["--depth", "2"])).unwrap_err(), "unknown flag --depth");

        let answer: coord_types::GraphAnswer = serde_json::from_value(json!({
            "slice": {
                "nodes": [
                    {"id": "agent:docket-edge8", "kind": "agent", "line": 10},
                    {"id": "N-1446.p2717", "kind": "post", "line": 12, "when": "2026-11-24T15:00:00Z", "by": "docket-edge8"},
                    {"id": "N-1446", "kind": "ticket", "line": 3, "what": "the 0.3 prototype"},
                ],
                "edges": [
                    {"from": "N-1446.p2717", "to": "N-1446", "kind": "posted_in", "line": 12, "layer": "agents"},
                    {"from": "agent:docket-edge8", "to": "N-1446.p2717", "kind": "wrote", "line": 12, "when": "2026-11-24T15:00:00Z", "layer": "agents"},
                ],
                "whole": {"agents": 900}, "shown": {"agents": 2}, "left": 3,
            },
            "labels": [{"id": "N-1446.p2717", "is": "post", "label": "Line 9: what an agent reaches from its home\nand the gaps"}],
        }))
        .unwrap();
        let (q, _) = super::graph_ask(&a(&["docket-edge8"])).unwrap();
        assert_eq!(
            super::graph_text(&q, &answer),
            "\
around agent:docket-edge8, 2 hops: 2 edges (agents 2), 3 nodes; 3 more left out by the limit (--limit)
(no time)             agents     N-1446.p2717 posted_in N-1446
2026-11-24T15:00:00Z  agents     agent:docket-edge8 wrote N-1446.p2717

agent:docket-edge8  agent
N-1446.p2717  post by docket-edge8: Line 9: what an agent reaches from its home
N-1446  ticket: the 0.3 prototype
"
        );
        let none: coord_types::GraphAnswer =
            serde_json::from_value(json!({"slice": {"nodes": [], "edges": [], "whole": {}, "shown": {}, "left": 0, "unknown": true}, "labels": []})).unwrap();
        let (q, _) = super::graph_ask(&a(&["N-99999"])).unwrap();
        assert_eq!(super::graph_text(&q, &none), "The graph holds no node N-99999.\n");
    }

    /// An agents' thread reads "waits on NAME" or "quiet", never "done".
    /// `show`'s text lists a ticket's halts, and a look under it with its own.

    /// What stdin may carry a text: a pipe and a file do; a socket (Claude Code's Bash tool with a heredoc), a device and
    /// so a terminal do not, so a text-less command never waits on them.
    #[test]
    fn a_text_is_read_from_a_pipe_or_a_file_never_from_a_socket() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(!super::gives_text(&std::fs::File::from(std::os::fd::OwnedFd::from(a))), "a socket");
        let (r, _w) = std::io::pipe().unwrap();
        assert!(super::gives_text(&std::fs::File::from(std::os::fd::OwnedFd::from(r))), "a pipe");
        let f = std::fs::File::open(env!("CARGO_MANIFEST_DIR").to_string() + "/Cargo.toml").unwrap();
        assert!(super::gives_text(&f), "a file");
        assert!(!super::gives_text(&std::fs::File::open("/dev/null").unwrap()), "a device");
    }
    /// `coord answer T HEAD -` with a JSON object on stdin: its fields; any other text stays the answer's text.
    #[test]
    fn an_answer_given_as_a_json_object_is_its_fields() {
        let o = super::answer_object("{\"head\": \"Design\", \"asked\": \"the words\", \"parts\": [{\"name\": \"a, b\"}]}\n").unwrap();
        assert_eq!(o["parts"][0]["name"], "a, b");
        assert!(super::answer_object("Design\n\nAsked\nthe words").is_none());
        assert!(super::answer_object("[1, 2]").is_none());
    }
    #[test]
    fn show_lists_halts_and_the_looks_under_a_ticket() {
        let look = serde_json::json!({"id": "A-1099511627776", "kind": "act", "status": {"is": "awaits", "agent": "f5-2", "call": "mind", "step": 1}, "halts": [{"what": "give", "n": 1, "step": 1, "worker": "f5-2", "call": "mind"}]});
        let v = serde_json::json!({"id": "P-5", "kind": "piece", "status": {"is": "awaits", "agent": "f5-1", "call": "implement", "step": 2}, "halts": [{"what": "take", "n": 5, "step": 2, "worker": "f5-1", "call": "implement", "head": {"head": "Commit", "commit": "4b9988a"}}], "handlers": [look]});
        let mut out = String::new();
        super::tree(&v, 0, &mut out);
        assert!(out.contains("  halt 5: f5-1's answer to step 2 (implement): Commit(4b9988a)"), "{out}");
        assert!(out.contains("  A-1099511627776 act · awaits f5-2 on mind") && out.contains("    halt 1: step 1 to f5-2 (mind)"), "{out}");
    }

    #[test]
    fn an_agents_thread_waits_or_is_quiet() {
        let v = |w: serde_json::Value| serde_json::json!({"status": {"is": "done", "result": {"head": "Between"}}, "waits_on": w});
        assert_eq!(super::state(&v(serde_json::json!([{"agent": "cone", "call": "post", "post": 1}]))), "waits on cone");
        assert_eq!(super::state(&v(serde_json::json!([]))), "quiet");
        assert_eq!(super::state(&serde_json::json!({"status": {"is": "done", "result": {"head": "Kept"}}})), "done: Kept");
    }

    /// `coord ask --drop ID`: an old mail id goes as Drop (the mail layer's withdrawal), a ticket id as Cancel.
    #[test]
    fn old_mail_ids_are_told_from_ticket_ids() {
        for id in ["q1", "q52", "m40"] {
            assert!(super::old_mail_id(id), "{id}");
        }
        for id in ["Q-3", "N-12", "q", "m", "q1a", "qx", "I-162", ""] {
            assert!(!super::old_mail_id(id), "{id}");
        }
        assert_eq!(super::post_of("N-412.p30"), Some(("N-412".into(), 30)));
        for id in ["Q-445", "N-412", ".p3", "N-412.px", "q12", "N-412.p+5", "N-412.p"] {
            assert_eq!(super::post_of(id), None, "{id}");
        }
    }

    /// A Deliver card in `coord show`: one post, or his burst's posts told together, one card (E1b's `with`).
    #[test]
    fn a_deliver_card_names_every_post_it_holds() {
        let card = |v: serde_json::Value| super::halt_line(&v);
        assert_eq!(card(serde_json::json!({"what": "deliver", "post": 5, "by": "owner", "wakes": ["lead"]})), "post 5 by owner to lead");
        assert_eq!(card(serde_json::json!({"what": "deliver", "post": 5, "with": [6, 7], "by": "owner", "wakes": ["lead", "fixer"]})), "posts 5, 6, 7 by owner to lead, fixer");
    }
}
