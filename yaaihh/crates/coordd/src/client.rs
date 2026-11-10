//! coordc: coordd's client (design §5, §7). `coord` runs it for the mail and agent commands; the hooks and roster run
//! it directly.
//!
//!   coordc msg [--esc] [--re ID] NAME[,NAME...] TEXT...   mail; the first line is its subject; --re: a reply in
//!                                              the thread of mail or question ID
//!   coordc msg --thread ID TEXT...             a reply in ID's thread to everyone in it (who wrote or was sent
//!                                              mail in it)
//!   coordc mail [ID]                           my recent mail, or one mail
//!   coordc ask [--on QID] SUBJECT...           a question to the owner, the explanation on stdin; prints its id
//!                                              at once (the answer comes as mail); --on follows up in QID's thread
//!   coordc ask --drop QID [WHY...]             withdraw my question
//!   coordc ask --list                          my questions: state (open, answered, dropped), thread, answers in full
//!   coordc ls                                  each agent: name, id, state, parent (also: state)
//!   coordc log NAME [LAST]                     an agent's session as text (agents below me), LAST entries (60)
//!   coordc item start ID | open TITLE | close ID WHY...
//!   coordc add NAME KIND MODEL                 register an agent; I become its parent
//!   coordc role NAME ROLE                      (the owner) NAME's role, a word the policy's norms name
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
//!
//! COORDD_SOCK overrides the socket (/run/coordd.sock). A host session's mail goes `--as NAME` through the owner side.
//! The terminals' sockets are COORD's pty/NAME.sock, served by their holders (coord-pty), not by coordd.

mod terminal;

use coord_types::{Reply, Request, SOCKET};
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

fn ask(req: &Request) -> Reply {
    let mut s = connect();
    writeln!(s, "{}", serde_json::to_string(req).unwrap()).expect("coord: write");
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).expect("coord: read");
    serde_json::from_str(&line).unwrap_or_else(|_| Reply { ok: false, out: format!("coord: bad reply {line:?}") })
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
            let (mut esc, mut soon, mut as_host, mut re, mut thread) = (false, false, None, None, false);
            while args.first().is_some_and(|a| a.starts_with("--")) {
                match args.remove(0).as_str() {
                    "--esc" => esc = true,
                    "--soon" => soon = true,
                    "--as" if !args.is_empty() => as_host = Some(args.remove(0)),
                    "--re" if !args.is_empty() && re.is_none() => re = Some(args.remove(0)),
                    "--thread" if !args.is_empty() && re.is_none() => (re, thread) = (Some(args.remove(0)), true),
                    f => {
                        eprintln!("coord msg: unknown or misplaced flag {f} (coord msg [--esc|--soon] [--re ID] NAME[,NAME...] TEXT | msg --thread ID TEXT)");
                        std::process::exit(2)
                    }
                }
            }
            // --thread: no names, the thread's participants (coordd's)
            if args.len() < if thread { 1 } else { 2 } {
                usage()
            }
            let to = if thread { Vec::new() } else { args.remove(0).split(',').filter(|s| !s.is_empty()).map(String::from).collect() };
            done(ask(&Request::Msg { to, body: args.join(" "), esc, soon, as_host, re, quotes: Vec::new() }))
        }
        "ask" => {
            // the flags lead; one it does not know is refused, never asked (8 Nov: `coord ask --list`, before there
            // was one, became the owner's question q7, "--list")
            let mut on = None;
            while args.first().is_some_and(|a| a.starts_with("--")) {
                match args.remove(0).as_str() {
                    "--list" if args.is_empty() => done(ask(&Request::Questions)),
                    "--drop" if !args.is_empty() => {
                        let id = args.remove(0);
                        let why = (!args.is_empty()).then(|| args.join(" "));
                        done(ask(&Request::Drop { id, why }))
                    }
                    "--on" if !args.is_empty() && on.is_none() => on = Some(args.remove(0)),
                    f => {
                        eprintln!("coord ask: unknown or misplaced flag {f} (coord ask [--on QID] SUBJECT... | ask --drop QID [WHY...] | ask --list)");
                        std::process::exit(2)
                    }
                }
            }
            if args.is_empty() {
                usage()
            }
            // the subject on the line, the whole explanation on stdin (a heredoc), when stdin is not a terminal
            let mut body = args.join(" ");
            if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                let mut more = String::new();
                let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut more);
                if !more.trim().is_empty() {
                    body = format!("{body}\n\n{}", more.trim_end());
                }
            }
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
                "close" => (args.remove(0), Some(args.join(" "))),
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
        "team" => match (args.first().map(String::as_str), args.len()) {
            (Some("ask"), n) if n > 1 => done(ask(&Request::TeamAsk { text: args[1..].join(" ") })),
            (Some("grant"), 3) => match args[2].parse() {
                Ok(granted) => done(ask(&Request::TeamGrant { name: args[1].clone(), granted })),
                Err(_) => usage(),
            },
            _ => usage(),
        },
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
        "add" => {
            if args.len() != 3 {
                usage()
            }
            done(ask(&Request::Add { name: args[0].clone(), kind: args[1].clone(), model: args[2].clone() }))
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
            let mut s = match UnixStream::connect(std::env::var("COORDD_SOCK").unwrap_or_else(|_| SOCKET.into())) {
                Ok(s) => s,
                Err(_) => std::process::exit(0), // a hook never blocks a session; coord watch shows coordd is down
            };
            let _ = writeln!(s, "{}", json!({"op": "hook", "event": event, "input": input}));
            // coordd answers at once and the hook prints nothing: mail goes in through the channel after the turn,
            // and coordd reads the turn's end from the transcript (owner m254, 9 Nov)
            let mut line = String::new();
            let _ = BufReader::new(s).read_line(&mut line);
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
            writeln!(s, "{}", serde_json::to_string(&Request::Watch { from }).unwrap()).expect("coord: write");
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
        "bridge" => bridge(),
        "attach" | "screen" | "hangup" if args.len() == 1 => match cmd.as_str() {
            "attach" => terminal::attach(&args[0]),
            "screen" => terminal::screen(&args[0]),
            _ => terminal::hangup(&args[0]),
        },
        "keys" if args.len() >= 2 => terminal::keys(&args[0], &args[1..].join(" ")),
        "terms" => terminal::terms(),
        _ => usage(),
    }
}

/// One pipe for yaaihh (through `sbx exec`): the event stream, and each request on its own connection, so a slow one
/// holds up no other. Terminal requests are the bridge's own (it runs next to the holders' sockets) and are taken in
/// order, so keys arrive as typed. It ends when stdin closes or coordd stops.
fn bridge() -> ! {
    use std::sync::{Arc, Mutex};
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let mut s = connect();
    writeln!(s, "{}", serde_json::to_string(&Request::Watch { from: None }).unwrap()).expect("coord: write");
    let o = out.clone();
    let (first, started) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(s).lines() {
            let Ok(line) = line else { break };
            terminal::print(&o, json!({"event": serde_json::from_str::<Value>(&line).unwrap_or(Value::Null)}));
            let _ = first.send(());
        }
        std::process::exit(4)
    });
    // the snapshot comes first, so a reply never speaks of state the reader has not seen yet
    let _ = started.recv();
    let relay = terminal::Relay::new(out.clone());
    let mut asked = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let reply = match serde_json::from_value::<Request>(v["req"].clone()) {
            Ok(Request::Watch { .. }) => Reply { ok: false, out: "coord bridge: the stream is already here".into() },
            Ok(req @ (Request::Attach { .. } | Request::Keys { .. } | Request::Resize { .. } | Request::Detach { .. })) => relay.handle(req),
            Ok(req) => {
                let o = out.clone();
                asked.push(std::thread::spawn(move || terminal::print(&o, json!({"id": v["id"], "reply": ask(&req)}))));
                continue;
            }
            Err(e) => Reply { ok: false, out: format!("coord bridge: not a request: {e}") },
        };
        terminal::print(&out, json!({"id": v["id"], "reply": reply}));
    }
    asked.into_iter().for_each(|t| drop(t.join())); // the answers still owed
    std::process::exit(0)
}
