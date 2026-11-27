//! His burst (E1b; the owner, N-1445 p9: "hold them for 15s by default and ask a docket program to split them"):
//! base.kind's `gather p` holds his typed post (and a new thread's opening words, I-1474) while the owner's step of
//! `gather` waits, his next posts on the ticket join it, and the holder (`route`) decides them once, To whom or a Split
//! by whole posts. The policy's hold is on his post's line (`Line::Post`'s `hold`; his message's, its value's `hold`),
//! so a replay holds as it ran; with none, `gather` is today's lines. The model's
//! answers here are made up: no model is called in a test.

use super::*;

impl Run {
    /// The open `owner_post` handlers.
    fn handlers_open(&self) -> Vec<u64> {
        self.m.tickets.values().filter(|x| x.handling.as_deref() == Some("owner_post") && x.is_open()).map(|x| x.t).collect()
    }
    /// The call of the step H awaits, and of whom.
    fn awaits(&self, h: u64) -> (String, String) {
        match &self.m.tickets[&h].status {
            Status::Awaits { agent, owed, .. } => (agent.clone(), owed.call.clone()),
            s => panic!("{s:?}"),
        }
    }
    /// The burst's hold goes by itself, as coordd's lapse round answers it (15 s since his first post).
    fn hold_ends(&mut self, h: u64) -> Out {
        let Status::Awaits { owed, .. } = &self.m.tickets[&h].status else { panic!("{:?}", self.m.tickets[&h].status) };
        let step = owed.step;
        let out = self.go(Input::Lapsed { t: h, step, head: json!({"head": "Send"}), after: 15, because: "time".into() });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// His press "Send now" on the burst H holds: on a slowed thread the only way it goes (no lapse there, docket-core4).
    fn sends_now(&mut self, h: u64) -> Out {
        let out = self.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send"}), text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// The line of his post N of T (found by its words: his posts here all differ).
    fn post_line(&self, t: u64, n: u32) -> &Line {
        let Text::New(text) = &self.post(t, n).text else { panic!("{:?}", self.post(t, n).text) };
        self.log.iter().rev().find(|l| matches!(l, Line::Post { t: u, text: Text::New(w), .. } if *u == t && w == text)).expect("its post line")
    }
}

/// p590+p591 on 20 Nov (9 s apart, research's record): two posts, one decision, one delivery. His second post joins
/// the first while the hold waits, no handler of its own, nothing handed; at the hold's end the holder reads both, as
/// one message, and both go to whom it says, the second "joining" the first. The brain's half (proto-lead N-1446
/// p751): with a binding on `owner_to`, nothing is raised while the two are held, at either post or at the hold's end,
/// and the holder's one tell raises it once, for the agent the burst wakes (not the one it only informs), with both
/// posts in his order (`posts_reached`); the binding takes it, and the record replays.
#[test]
fn his_posts_held_together_are_one_burst_decided_once_and_told_together() {
    let mut r = Run::bound();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, out) = r.owner_posts(t, &[], "see 2 messages. should have been order / split properly");
    assert!(raised(&out).is_empty(), "held: nothing raised: {:?}", out.effects);
    assert!(matches!(r.post_line(t, a), Line::Post { hold: Some(15), .. }), "the hold is on his post's line: {:?}", r.post_line(t, a));
    let h = r.handler_of(t, a);
    assert_eq!(r.awaits(h), ("owner".to_string(), "gather".to_string()));
    assert!(r.m.due("watcher").is_empty() && r.m.due("lead").is_empty() && r.m.due("fixer").is_empty(), "held: no call yet, nobody given it");
    let (b, out) = r.owner_posts(t, &[], "read whats relevant from earlier chats");
    assert!(out.reply.as_ref().is_ok_and(|s| s.ends_with(&format!("held with his post {a}."))), "{:?}", out.reply);
    assert!(raised(&out).is_empty(), "held: nothing raised: {:?}", out.effects);
    assert_eq!(r.handlers_open(), vec![h], "no handler of its own");
    assert_eq!(r.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![a, b]);
    assert_eq!(r.post(t, b).held, Some(r.m.id(h)));
    assert_eq!((r.told_lines(t, a), r.told_lines(t, b)), (0, 0), "his second post hands nothing (not `hand_unanswered`)");
    // the hold ends: the holder is asked once, for both; his posts are still held, so still nothing raised
    let out = r.hold_ends(h);
    assert!(raised(&out).is_empty(), "with the holder: nothing raised: {:?}", out.effects);
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()));
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    let both = Text::Joined { joined: vec![Text::Post { post: PostRef { t, n: a } }, Text::Post { post: PostRef { t, n: b } }] };
    assert_eq!(owed.args["text"], both.value(), "route reads his burst");
    let steps = |r: &Run| r.log.iter().filter(|l| matches!(l, Line::Step { call, .. } if call == "route")).count();
    assert_eq!(steps(&r), 1, "one decision");
    let out = r.go(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "To", "agents": ["lead"]}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    // its one tell: raised once, for lead, with the whole burst; fixer, only informed, not at all
    let ids: Vec<String> = [a, b].iter().map(|n| format!("{}.p{n}", r.m.id(t))).collect();
    let told = raised(&out);
    assert_eq!(told, vec![json!({"agent": "lead", "posts": ids, "tied_at": ids[1]})], "{:?}", out.effects);
    assert_eq!((r.told_lines(t, a), r.told_lines(t, b)), (1, 1));
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["lead".to_string()]));
    assert_eq!((r.post(t, a).joins, r.post(t, b).joins), (None, Some(a)), "the second goes on from the first");
    assert!(r.woken("lead", t) && !r.woken("fixer", t) && r.informed("fixer", t));
    assert_eq!(r.m.due("lead").iter().filter(|d| d.t == t).count(), 1, "one delivery: {:?}", r.m.due("lead"));
    assert!(r.handler().is_none());
    // coordd sends it back: the binding opens one ticket, for lead, and the record replays
    assert_eq!(r.back(told), [json!("lead")]);
}

/// paper-research N-1446 p40, made up: in one burst his first post is about the build and his second about the
/// brain; the holder splits it by whole posts, and each goes to its own agent (lead and fixer stand in for docket-core
/// and paper-research-brain), the other informed.
#[test]
fn a_split_burst_tells_each_part_to_its_own_agent() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "the build: 36 fails on the runner, look at the lock");
    let (b, _) = r.owner_posts(t, &[], "and the brain: the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    let split = json!({"head": "Split", "parts": [{"posts": [a], "to": ["lead"]}, {"posts": [b], "to": ["fixer"]}]});
    let out = r.go(Input::Answer { t: h, by: "watcher".into(), head: split, text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { n, to, inform, .. } if *n == a && to == &["lead".to_string()] && inform == &["fixer".to_string()])), "{:?}", out.lines);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { n, to, inform, .. } if *n == b && to == &["fixer".to_string()] && inform == &["lead".to_string()])), "{:?}", out.lines);
    assert_eq!((r.post(t, a).joins, r.post(t, b).joins), (None, None), "two parts, two thoughts");
    assert!(r.woken("lead", t) && r.woken("fixer", t));
    assert!(r.handler().is_none(), "every post told: the handler is done");
    r.replays();
}

/// His own @NAME in one post of a burst (E1b hold 1; consult-3 N-1446 p281, proto-lead p325): the burst does not go
/// to that agent as a whole, it goes to the holder with the post he chose for and whom (`chosen`); and the burst's
/// names, answer and `@all` are none, its `naming` whom the post with no choice was typed to. A burst whose posts all
/// name the same agent goes there with no call, as one post does.
#[test]
fn a_burst_with_his_own_name_in_one_post_goes_to_the_holder_with_his_choice() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &["lead"], "the build: 36 fails on the runner");
    let (b, _) = r.owner_posts(t, &["fixer"], "@fixer and the brain: the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()), "not To(naming) for both");
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    assert_eq!((owed.args["to"].clone(), owed.args["others"].clone()), (json!(["lead"]), json!(["fixer"])), "typed to: the post with no choice's");
    assert_eq!(r.m.his_choices(t, &[a, b]), vec![(b, vec!["fixer".to_string()])]);
    let split = json!({"head": "Split", "parts": [{"posts": [a], "to": ["lead"]}, {"posts": [b], "to": ["fixer"]}]});
    r.route_of(t, a, split);
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["fixer".to_string()]));
    r.replays();
    // both posts name fixer: his one choice, To(fixer) with no call
    let (c, _) = r.owner_posts(t, &["fixer"], "@fixer one more on the ledger");
    let (d, _) = r.owner_posts(t, &["fixer"], "@fixer and p617");
    let h = r.handler_of(t, c);
    assert!(r.m.his_choices(t, &[c, d]).is_empty());
    let steps = |r: &Run| r.log.iter().filter(|l| matches!(l, Line::Step { call, .. } if call == "route")).count();
    let before = steps(&r);
    r.hold_ends(h);
    assert_eq!(steps(&r), before, "no call");
    assert_eq!((r.post(t, c).told.clone(), r.post(t, d).told.clone()), (vec!["fixer".to_string()], vec!["fixer".to_string()]));
    r.replays();
}

/// The holder cannot tell whom his post with no choice is for (proto-reader N-1446 p445, made up): its Unclear is taken.
/// His post to fixer goes to fixer at once, and he is asked only about the other, its words alone; his press sends it.
#[test]
fn an_unclear_burst_sends_his_chosen_post_first_and_asks_him_only_about_the_rest() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
    let (b, _) = r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()));
    r.route_of(t, a, json!({"head": "Unclear", "why": "which work is the median?"}));
    assert_eq!((r.post(t, b).told.clone(), r.post(t, a).told.clone()), (vec!["fixer".to_string()], Vec::<String>::new()), "his @fixer goes, the other waits on him");
    assert_eq!(r.awaits(h), ("owner".to_string(), "clarify".to_string()));
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    assert_eq!(owed.args["text"], Text::Post { post: PostRef { t, n: a } }.value(), "he is asked about the post left, its words alone");
    r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send", "to": ["lead"]}), text: Text::New(String::new()) }).reply.unwrap();
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["fixer".to_string()]));
    assert!(r.handler().is_none());
    r.replays();
}

/// Every post of his burst carries his own choice, not all one (proto-reader N-1446 p445, consult-3 p446, made up): the
/// holder has nothing to decide, so nothing is asked, of it or of him; each post goes where he chose.
#[test]
fn a_burst_whose_posts_all_carry_his_choices_goes_by_them_with_no_call() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &["lead"], "@lead the median on the dashboard looks off");
    let (b, _) = r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    let steps = |r: &Run| r.log.iter().filter(|l| matches!(l, Line::Step { call, .. } if call == "route" || call == "clarify")).count();
    r.hold_ends(h);
    assert_eq!(steps(&r), 0, "no call, no press");
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["fixer".to_string()]));
    assert!(r.handler().is_none());
    r.replays();
}

/// His `@all` in a burst beside a post of his with no choice (proto-lead N-1446 p475; N-1134: "of whom one takes it";
/// made up): the `@all` part goes as a lone `@all` does, before anything else. The handler asks the policy's `claims`
/// for it, by name, over every agent on the ticket and its words alone; the one it says is handed it and the other is
/// informed, owing nothing; the first answer is the claim. Only then is the holder asked, about the post with no
/// choice; nothing is handed past that post while it is held, so both go once it is decided, in his order. Beside a
/// post of his @NAME, nothing is left to the holder: the `@all` goes by claims, the other where he chose, with no
/// route call.
#[test]
fn his_all_in_a_burst_goes_through_claims_to_one_agent_as_a_lone_all_does() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
    let (b, _) = r.owner_posts(t, &[], "@all who has the lock?");
    let h = r.handler_of(t, a);
    assert_eq!(r.m.his_claimed(t, &[a, b]), vec![b]);
    r.hold_ends(h);
    // claims, for b alone
    let Status::Asking { owed, role } = &r.m.tickets[&h].status else { panic!("{:?}", r.m.tickets[&h].status) };
    assert_eq!((role.as_str(), owed.call.as_str()), (machine::POLICY, format!("{}claims", syntax::DECIDE).as_str()));
    let mut agents: Vec<String> = serde_json::from_value(owed.args["agents"].clone()).unwrap();
    agents.sort();
    assert_eq!((agents, owed.args["text"].clone()), (vec!["fixer".to_string(), "lead".to_string()], Text::Post { post: PostRef { t, n: b } }.value()));
    let step = owed.step;
    let out = r.go(Input::Decided { t: h, step, question: "decide".into(), answer: json!("fixer"), provenance: json!({"decision": "claims"}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { n, to, inform, .. } if *n == b && to == &["fixer".to_string()] && inform == &["lead".to_string()])), "{:?}", out.lines);
    let st = r.m.all_state(t, b).unwrap();
    assert_eq!((st.handed, st.claim), (vec!["fixer".to_string()], None), "handed to one, claimed by nobody yet");
    assert_eq!(r.post(t, a).told, Vec::<String>::new(), "a still waits on the holder");
    assert!(r.m.due("fixer").is_empty() && r.m.due("lead").is_empty(), "nothing is handed past his post still held, a");
    // then the holder, for a: its step names a's words alone, what route reads (proto-reader N-1446 p526)
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()));
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    assert_eq!(owed.args["text"], Text::Post { post: PostRef { t, n: a } }.value());
    r.route_of(t, a, json!({"head": "To", "agents": ["lead"]}));
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["fixer".to_string()]), "b is not told again");
    assert_eq!(r.log.iter().filter(|l| matches!(l, Line::Told { n, .. } if *n == b)).count(), 1);
    assert!(r.woken("fixer", t) && r.woken("lead", t), "fixer for b, lead for a");
    assert!(r.handler().is_none());
    r.replays();
    // beside his @NAME: claims for the `@all`, his @lead where he chose, and no route call
    let (c, _) = r.owner_posts(t, &[], "@all is the runner back?");
    let (d, _) = r.owner_posts(t, &["lead"], "@lead and the median, once more");
    let h = r.handler_of(t, c);
    let routes = |r: &Run| r.log.iter().filter(|l| matches!(l, Line::Step { call, .. } if call == "route" || call == "clarify")).count();
    let before = routes(&r);
    r.hold_ends(h);
    let Status::Asking { owed, .. } = &r.m.tickets[&h].status else { panic!("{:?}", r.m.tickets[&h].status) };
    let step = owed.step;
    r.go(Input::Decided { t: h, step, question: "decide".into(), answer: json!("lead"), provenance: json!({"decision": "claims"}) }).reply.unwrap();
    assert_eq!(routes(&r), before, "nothing left to the holder");
    assert_eq!((r.post(t, c).told.clone(), r.post(t, d).told.clone()), (vec!["lead".to_string()], vec!["lead".to_string()]));
    assert_eq!(r.m.all_state(t, c).unwrap().handed, vec!["lead".to_string()]);
    assert!(r.handler().is_none());
    r.replays();
}

/// I-1459, `posts_reached` (docket-core4 N-1446 p527, proto-lead p530, p569; made up): the brain's `owner_to` is
/// raised where his post reaches the agent it wakes, when that agent may first be shown it (`seen_upto`), not when
/// told. A post of his told while one before it still waits on the holder is not shown before that one goes, and nor
/// is it raised for: his `@all` that claims hands to fixer. When that one goes, each agent is raised for once, with
/// its own posts in his order; his first post told to fixer as well, fixer is raised for once, with both. A post his
/// own question holds (`clarify`, (c2)) is untold and unshown and holds back nothing: his @fixer part an Unclear tells
/// reaches fixer at once, alone. His Send then tells the first to each agent he chose by a delivery of its own
/// (`p{n}.told`), whatever its mark, once, and raises that agent for it alone (p576); his Drop raises nothing and
/// nobody is ever shown it. A record from before, whose question's line has no `asked`, replays held back as it ran.
/// The binding takes each, and each case replays.
#[test]
fn owner_to_is_raised_when_his_post_may_be_shown_not_while_a_post_before_it_still_waits() {
    let run = || {
        let mut r = Run::bound();
        let t = r.two_on_a_thread();
        r.m.owner_hold = Some(15);
        (r, t)
    };
    let to = |r: &Run, t: u64, a: &str, ns: &[u32]| {
        let ids: Vec<String> = ns.iter().map(|n| format!("{}.p{n}", r.m.id(t))).collect();
        json!({"agent": a, "posts": ids, "tied_at": ids.last().unwrap()})
    };
    let answer = |r: &mut Run, h: u64, by: &str, head: Value| {
        let out = r.go(Input::Answer { t: h, by: by.into(), head, text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    };
    // his `@all` beside a post with no choice: claims hands it to fixer, told and not shown, nothing raised
    let claimed = |r: &mut Run, t: u64| -> (u32, u32, u64) {
        let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
        let (b, _) = r.owner_posts(t, &[], "@all who has the lock?");
        let h = r.handler_of(t, a);
        assert!(raised(&r.hold_ends(h)).is_empty());
        let Status::Asking { owed, .. } = &r.m.tickets[&h].status else { panic!("{:?}", r.m.tickets[&h].status) };
        let step = owed.step;
        let out = r.go(Input::Decided { t: h, step, question: "decide".into(), answer: json!("fixer"), provenance: json!({"decision": "claims"}) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        assert_eq!(r.post(t, b).told, vec!["fixer".to_string()]);
        assert!(r.m.due("fixer").is_empty() && raised(&out).is_empty(), "told, not shown, not raised for: {:?}", out.effects);
        (a, b, h)
    };
    // the holder tells the first to lead: lead raised for it, fixer for the `@all`, once each, in his order
    let (mut r, t) = run();
    let (a, b, h) = claimed(&mut r, t);
    let out = answer(&mut r, h, "watcher", json!({"head": "To", "agents": ["lead"]}));
    assert_eq!(raised(&out), vec![to(&r, t, "lead", &[a]), to(&r, t, "fixer", &[b])], "{:?}", out.effects);
    assert!(r.woken("lead", t) && r.woken("fixer", t));
    assert_eq!(r.back(raised(&out)), [json!("lead"), json!("fixer")]);
    // the holder tells the first to fixer too: fixer raised for once, with both
    let (mut r, t) = run();
    let (a, b, h) = claimed(&mut r, t);
    let out = answer(&mut r, h, "watcher", json!({"head": "To", "agents": ["fixer"]}));
    assert_eq!(raised(&out), vec![to(&r, t, "fixer", &[a, b])], "{:?}", out.effects);
    assert_eq!(r.back(raised(&out)), [json!("fixer")]);
    // an Unclear tells his @fixer part first and asks him about the first: while that waits on him the first is
    // untold and unshown and holds nothing back, so fixer is raised for its part at once and is due it alone
    let holds = |r: &Run, t: u64, n: u32| {
        ["lead", "fixer"].iter().any(|a| r.m.due(a).iter().any(|d| d.t == t && matches!(&d.what, What::Posts { unread } | What::Step { unread, .. } | What::Closed { unread, .. } if unread.iter().any(|p| p.n == n))))
    };
    let ns = |d: &Delivery| match &d.what {
        What::Posts { unread } => unread.iter().map(|p| p.n).collect::<Vec<u32>>(),
        w => panic!("{w:?}"),
    };
    let unclear = |r: &mut Run, t: u64| -> (u32, u32, u64, Out) {
        let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
        let (b, _) = r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
        let h = r.handler_of(t, a);
        r.hold_ends(h);
        let out = answer(&mut *r, h, "watcher", json!({"head": "Unclear", "why": "which work is the median?"}));
        assert_eq!(r.post(t, b).told, vec!["fixer".to_string()]);
        assert_eq!(r.awaits(h), ("owner".to_string(), "clarify".to_string()));
        assert!(r.log.iter().any(|l| matches!(l, Line::Step { t: u, call, asked: true, .. } if *u == h && call == "clarify")), "his question's line says so");
        assert_eq!(raised(&out), vec![to(&*r, t, "fixer", &[b])], "{:?}", out.effects);
        assert_eq!(r.m.due("fixer").iter().map(ns).collect::<Vec<_>>(), vec![vec![b]], "fixer is due its part alone");
        assert!(!holds(&*r, t, a), "nobody is shown the first");
        (a, b, h, out)
    };
    let told = |r: &Run, t: u64, n: u32| format!("{}.p{n}.{TOLD}", r.m.id(t));
    // his press sends the first to lead: lead has it by a delivery of its own, whatever its mark, and is raised for it
    // alone; lead's ordinary delivery, his part that informs it, leaves it out
    let (mut r, t) = run();
    let (a, b, h, first) = unclear(&mut r, t);
    let out = answer(&mut r, h, "owner", json!({"head": "Send", "to": ["lead"]}));
    assert_eq!(raised(&out), vec![to(&r, t, "lead", &[a])], "{:?}", out.effects);
    let due: Vec<(String, bool, Vec<u32>)> = r.m.due("lead").iter().map(|d| (d.id.clone(), d.inform, ns(d))).collect();
    assert_eq!(due, vec![(format!("{}.p{b}", r.m.id(t)), true, vec![b]), (told(&r, t, a), false, vec![a])]);
    // where the first is for lead (`whereabouts`): waiting while lead holds only the part that informs it, then handed
    // by `p{a}.told`, the row that hands it (`Ticket::hands`; proto-reader N-1446 p595)
    let waits = |r: &Run| r.where_is(t, a, "lead").is_some_and(|w| w.waiting.is_some() && w.handed.is_none());
    assert!(waits(&r));
    let lead = r.m.due("lead");
    r.go(Input::Given { t, to: "lead".into(), delivery: lead[0].id.clone(), upto: lead[0].upto, skipped: Default::default(), place: None }).reply.unwrap();
    assert!(waits(&r), "its part informs lead and does not hand the first");
    r.go(Input::Given { t, to: "lead".into(), delivery: lead[1].id.clone(), upto: lead[1].upto, skipped: Default::default(), place: None }).reply.unwrap();
    let (d, at) = r.m.tickets[&t].given.last().map(|g| (g.delivery.clone(), g.at.clone())).unwrap();
    assert_eq!(d, told(&r, t, a));
    assert!(r.where_is(t, a, "lead").is_some_and(|w| w.waiting.is_none() && w.handed == Some(at.clone())), "handed by {d}: {:?}", r.where_is(t, a, "lead"));
    assert_eq!(r.back([raised(&first), raised(&out)].concat()), [json!("fixer"), json!("lead")]);
    // his press drops the first: nothing raised, and nobody is ever shown it
    let (mut r, t) = run();
    let (a, _, h, first) = unclear(&mut r, t);
    let out = answer(&mut r, h, "owner", json!({"head": "Drop"}));
    assert!(r.post(t, a).dropped && raised(&out).is_empty(), "{:?}", out.effects);
    assert!(!holds(&r, t, a));
    assert_eq!(r.back(raised(&first)), [json!("fixer")]);
    // the read mark: fixer, given its part, read past the first; his press sends the first to fixer, which still wakes
    // fixer, by `p{a}.told`, and fixer's turn that holds it replies on the thread
    let (mut r, t) = run();
    let (a, b, h, first) = unclear(&mut r, t);
    assert_eq!(r.give("fixer", t), format!("{}.p{b}", r.m.id(t)));
    assert!(!r.woken("fixer", t));
    let out = answer(&mut r, h, "owner", json!({"head": "Send", "to": ["fixer"]}));
    assert_eq!(raised(&out), vec![to(&r, t, "fixer", &[a])], "{:?}", out.effects);
    assert!(r.woken("fixer", t));
    assert_eq!(r.m.due("fixer").iter().map(|d| d.id.clone()).collect::<Vec<_>>(), vec![told(&r, t, a)]);
    let n = r.m.tickets[&t].last();
    r.handed("fixer", 40);
    let x = &r.m.tickets[&t];
    assert!(x.last() == n + 1 && x.posts.last().is_some_and(|p| p.by == "fixer" && p.auto), "fixer's reply on the thread");
    assert!(!r.woken("fixer", t));
    assert_eq!(r.back([raised(&first), raised(&out)].concat()), [json!("fixer"), json!("fixer")]);
    // his press lands between fixer's `p{b}` being handed and its `given` (consult-3 p576): fixer ends holding the
    // first once, by its own delivery, after its part
    let (mut r, t) = run();
    let (a, b, h, first) = unclear(&mut r, t);
    let handed = r.m.due("fixer");
    let out = answer(&mut r, h, "owner", json!({"head": "Send", "to": ["fixer"]}));
    assert_eq!(raised(&out), vec![to(&r, t, "fixer", &[a])], "{:?}", out.effects);
    let give = |r: &mut Run, d: &Delivery| r.go(Input::Given { t: d.t, to: "fixer".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    give(&mut r, &handed[0]);
    let late = r.m.due("fixer");
    assert_eq!(late.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), vec![told(&r, t, a)]);
    give(&mut r, &late[0]);
    assert!(r.m.due("fixer").is_empty());
    assert_eq!(handed.iter().chain(&late).flat_map(ns).collect::<Vec<_>>(), vec![b, a], "the first once, after its part");
    assert_eq!(r.back([raised(&first), raised(&out)].concat()), [json!("fixer"), json!("fixer")]);
    // a record from before (c2), whose question's line has no `asked`: the first held back his part, and the replay
    // holds it back so
    let (mut r, t) = run();
    unclear(&mut r, t);
    let text: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    assert_eq!(text.iter().filter(|l| l.contains(",\"asked\":true")).count(), 1);
    let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(&s.replace(",\"asked\":true", "")).unwrap()).collect();
    let mut m2 = Machine::load(defs_with(BOUND).0);
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    assert!(m2.due("fixer").is_empty() && m2.tickets[&t].asked.is_empty(), "held back, as it ran");
    assert!(!r.m.due("fixer").is_empty());
    // his take-back of the first tells whom a `given` row handed it (`Ticket::hands`; proto-reader's cases, N-1446
    // p595): its TAKEN_BACK line's `to`, or nobody had it
    let had = |out: &Out| {
        out.lines.iter().find_map(|l| match l {
            Line::Event { name, value, .. } if name == TAKEN_BACK => Some(value["to"].clone()),
            Line::TakenBack { .. } => Some(json!("nobody had it")),
            _ => None,
        })
    };
    // held under his question still, fixer given its part past it: nobody was shown the first
    let (mut r, t) = run();
    let (a, b, _, _) = unclear(&mut r, t);
    assert_eq!(r.give("fixer", t), format!("{}.p{b}", r.m.id(t)));
    let out = r.go(Input::TakeBack { t, n: a, by: "owner".into() });
    assert_eq!(had(&out), Some(json!("nobody had it")), "{:?}", out.reply);
    // told to lead alone after fixer's mark passed it: lead had it, by `p{a}.told`; fixer never did
    let (mut r, t) = run();
    let (a, b, h, _) = unclear(&mut r, t);
    assert_eq!(r.give("fixer", t), format!("{}.p{b}", r.m.id(t)));
    answer(&mut r, h, "owner", json!({"head": "Send", "to": ["lead"]}));
    for d in r.m.due("lead") {
        r.go(Input::Given { t: d.t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    }
    let out = r.go(Input::TakeBack { t, n: a, by: "owner".into() });
    assert_eq!(had(&out), Some(json!(["lead"])), "{:?}", out.reply);
    // lead given `p{a}.told`, not its part: no row of lead's reaches the first by its mark, the told one's the mark as it
    // stood, and lead had it all the same
    let (mut r, t) = run();
    let (a, _, h, _) = unclear(&mut r, t);
    answer(&mut r, h, "owner", json!({"head": "Send", "to": ["lead"]}));
    let d = r.m.due("lead").into_iter().find(|d| d.id == told(&r, t, a)).unwrap();
    r.go(Input::Given { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    let rows: Vec<(String, u32)> = r.m.tickets[&t].given.iter().filter(|g| g.to == "lead").map(|g| (g.delivery.clone(), g.upto)).collect();
    assert!(rows.iter().all(|(_, upto)| *upto < a) && rows.last().is_some_and(|(d, _)| *d == told(&r, t, a)), "{rows:?}");
    let out = r.go(Input::TakeBack { t, n: a, by: "owner".into() });
    assert_eq!(had(&out), Some(json!(["lead"])), "{:?}", out.reply);
    // a delivery handed before his Send and given after it (proto-reader N-1446 p621, red at reader-c2-race c767d66c;
    // proto-lead p631): lead's part is handed while the first is asked, his Send of the first to fixer alone lands,
    // then lead's `given`. What the hand-over left out (`Machine::left_out`, which coordd keeps with it) is on that
    // line, so lead's row does not hand the first, his take-back tells fixer only, and a replay reads it the same
    {
        let (mut r, t) = run();
        let (a, b, h, _) = unclear(&mut r, t);
        let on_way = r.m.due("lead");
        let carried: Vec<u32> = on_way.iter().flat_map(ns).collect();
        let left: Vec<Option<std::collections::BTreeSet<u32>>> = on_way.iter().map(|d| r.m.left_out("lead", d)).collect();
        assert_eq!(left, vec![Some(std::collections::BTreeSet::from([a]))], "the hand-over leaves the first out");
        answer(&mut r, h, "owner", json!({"head": "Send", "to": ["fixer"]}));
        for (d, skipped) in on_way.iter().zip(left) {
            r.go(Input::Given { t: d.t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped, place: None }).reply.unwrap();
        }
        assert!(r.log.iter().any(|l| matches!(l, Line::Given { to, skipped: Some(s), .. } if to == "lead" && s.contains(&a))), "on lead's `given` line");
        let lead_had = r.m.tickets[&t].handed("lead", a);
        let fixer = r.m.due("fixer");
        for d in &fixer {
            let skipped = r.m.left_out("fixer", d);
            r.go(Input::Given { t: d.t, to: "fixer".into(), delivery: d.id.clone(), upto: d.upto, skipped, place: None }).reply.unwrap();
        }
        let out = r.go(Input::TakeBack { t, n: a, by: "owner".into() });
        assert_eq!((carried, lead_had, had(&out)), (vec![b], false, Some(json!(["fixer"]))), "lead's part carried only b; only fixer had the first");
        r.replays_with(BOUND);
    }
    // the other way round (consult-3 N-1446 p661, proto-lead p661): his Send of the first to lead lands, fixer's part is
    // handed carrying it (nothing left out), he takes the first back while lead's and fixer's are both on their way,
    // then fixer's `given`. The hand-over's set, empty, is on its line: fixer's row hands the first, as fixer, told of
    // the take-back (coordd's `backs`), had it. Red on 8d9a3f42, which worked an empty set out again at the given and
    // found the first taken back
    {
        let (mut r, t) = run();
        let (a, b, h, _) = unclear(&mut r, t);
        answer(&mut r, h, "owner", json!({"head": "Send", "to": ["lead"]}));
        let id = format!("{}.p{b}", r.m.id(t));
        let part = r.m.due("fixer").into_iter().find(|d| d.id == id).expect("fixer's part");
        assert_eq!(ns(&part), vec![a, b], "it carries the first");
        let skipped = r.m.left_out("fixer", &part);
        assert_eq!(skipped, Some(std::collections::BTreeSet::new()), "nothing left out, and the line says so");
        let out = r.go(Input::TakeBack { t, n: a, by: "owner".into() });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        r.go(Input::Given { t, to: "fixer".into(), delivery: part.id.clone(), upto: part.upto, skipped, place: None }).reply.unwrap();
        assert!(r.log.iter().any(|l| matches!(l, Line::Given { to, delivery, skipped: Some(s), .. } if to == "fixer" && *delivery == id && s.is_empty())), "`\"skipped\":[]` on fixer's line");
        assert!(r.m.tickets[&t].handed("fixer", a), "{:?}", r.m.tickets[&t].given);
        r.replays_with(BOUND);
    }
    // an agents' thread (Between; consult-3 N-1446 p601, proto-lead p601): his post there, typed to lead with no @ in
    // its words (one naming nobody is refused on a closed thread), runs base's `when owner_post`; route reads it, as
    // fixer is on the thread too, and an Unclear asks him about it. His Send tells it to lead, its mark below it, by
    // `p{a}.told`: lead waits on it while the turn that holds it runs (`waits_between`), and not once that turn ends
    // (`Ticket::hands`)
    let mut r = Run::bound();
    r.m.owner_hold = Some(15);
    let form = json!({"to": "lead", "typed": ["lead"], "names": ["lead"], "text": "", "addressed": {"head": "Sent", "by": "fixer"}, "follows": null});
    r.go(Input::Open { kind: "thread".into(), under: None, opener: "fixer".into(), form, text: Some(Text::New("@lead the ledger marks p614 kept".into())), names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(r.m.tickets[&t].between(), "{:?}", r.m.tickets[&t].status);
    let g = r.give("lead", t);
    r.go(Input::TurnEnded { agent: "lead".into(), delivery: Some(g), text: Loc { len: 0, ..Run::loc(50) }, later: vec![], stood: None, go: None });
    let (a, _) = r.owner_posts(t, &["lead"], "the median on the dashboard looks off");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()));
    answer(&mut r, h, "watcher", json!({"head": "Unclear", "why": "which work is the median?"}));
    assert_eq!(r.awaits(h), ("owner".to_string(), "clarify".to_string()));
    let waits = |r: &Run| r.m.waits_between(t).contains(&("lead".to_string(), a));
    assert!(!waits(&r), "held under his question, it wakes nobody: {:?}", r.m.waits_between(t));
    answer(&mut r, h, "owner", json!({"head": "Send", "to": ["lead"]}));
    assert!(waits(&r), "told, not handed yet: {:?}", r.m.waits_between(t));
    let d = r.m.due("lead").into_iter().find(|d| d.t == t).unwrap();
    assert!(d.id == told(&r, t, a) && d.upto < a, "{d:?}");
    r.go(Input::Given { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    assert!(waits(&r), "the turn that holds it runs: {:?}", r.m.waits_between(t));
    r.go(Input::TurnEnded { agent: "lead".into(), delivery: Some(d.id.clone()), text: Loc { len: 0, ..Run::loc(60) }, later: vec![], stood: None, go: None });
    assert!(!waits(&r), "that turn ended: {:?}", r.m.waits_between(t));
    r.replays_with(BOUND);
}

/// His press "Send now" (paper-research N-1446 p36: a press goes at once): the burst goes to the holder at once, and
/// the hold's later lapse is refused and writes nothing.
#[test]
fn send_now_ends_the_hold_at_once_and_the_later_lapse_writes_nothing() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "go");
    let h = r.handler_of(t, a);
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    let step = owed.step;
    let out = r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send"}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()));
    let before = r.log.len();
    assert!(r.go(Input::Lapsed { t: h, step, head: json!({"head": "Send"}), after: 15, because: "time".into() }).reply.is_err());
    assert_eq!(r.log.len(), before, "the lapse writes nothing");
    r.route_of(t, a, json!({"head": "To", "agents": ["fixer"]}));
    assert!(r.woken("fixer", t));
    r.replays();
}

/// Off is today (proto-reader N-1446 p62): with no hold in the policy his post's line has none, `gather` waits for
/// nothing and writes nothing, and the holder is asked at once, as on 20 Nov.
#[test]
fn with_no_hold_gather_is_his_post_at_once_and_writes_nothing() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "eh yeah but wait lets analyze where we need special actions vs not");
    assert!(matches!(r.post_line(t, a), Line::Post { hold: None, .. }));
    let h = r.handler_of(t, a);
    assert_eq!(r.awaits(h), ("watcher".to_string(), "route".to_string()), "asked at once");
    assert!(!r.log.iter().any(|l| matches!(l, Line::Step { call, .. } if call == "gather")), "no step of `gather`");
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    assert_eq!(owed.args["text"], Text::Post { post: PostRef { t, n: a } }.value(), "his post, as before");
    // his next post while the first is with the holder: a post of its own, as before
    let (b, _) = r.owner_posts(t, &[], "like ofc for reset apply can be special action");
    assert_eq!(r.handlers_open().len(), 2);
    assert_ne!(r.handler_of(t, b), h);
    r.replays();
}

/// p586 and p587 (22 s apart) under the 15 s hold: p586's hold ended before p587 came, so p587 is a burst of its own
/// with its own hold (the window is 15 s from the first post, proto-lead p61.7); it does not join a handler that is
/// past its hold.
#[test]
fn a_post_after_the_hold_ended_is_a_burst_of_its_own() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "eh yeah but wait lets analyze where we need special actions vs not");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    let (b, _) = r.owner_posts(t, &[], "like ofc for reset apply can be special action");
    let g = r.handler_of(t, b);
    assert_ne!(g, h);
    assert_eq!(r.awaits(g), ("owner".to_string(), "gather".to_string()));
    assert_eq!(r.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![a]);
    r.route_of(t, a, json!({"head": "To", "agents": ["lead"]}));
    r.hold_ends(g);
    r.go(Input::Answer { t: g, by: "watcher".into(), head: json!({"head": "To", "agents": ["lead"], "joins": a}), text: Text::New(String::new()) }).reply.unwrap();
    assert_eq!(r.post(t, b).joins, Some(a));
    r.replays();
}

/// One post of his burst taken back while it is held: it leaves the burst, which goes on with the rest; the first
/// taken back, the next is the burst's first.
#[test]
fn a_post_taken_back_leaves_the_burst() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "one");
    let (b, _) = r.owner_posts(t, &[], "two");
    let (c, _) = r.owner_posts(t, &[], "three");
    let h = r.handler_of(t, a);
    r.go(Input::TakeBack { t, n: b, by: "owner".into() }).reply.unwrap();
    assert_eq!(r.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![a, c]);
    r.go(Input::TakeBack { t, n: a, by: "owner".into() }).reply.unwrap();
    assert_eq!(r.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![c]);
    assert!(r.m.tickets[&h].is_open(), "the burst goes on");
    r.hold_ends(h);
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    assert_eq!(owed.args["text"], Text::Post { post: PostRef { t, n: c } }.value());
    r.route_of(t, c, json!({"head": "To", "agents": ["lead"]}));
    assert_eq!((r.told_lines(t, a), r.told_lines(t, b), r.told_lines(t, c)), (0, 0, 1));
    r.replays();
}

/// Where his wait comes from (`Machine::owner_steps` and `owner_waits_else`, which coordd reads with its clock,
/// proto-reader N-1446 p136): his held burst's `gather` step, named under the thread's id as its `waits_on` names it,
/// and nothing else; on the handler, under its own. Once the holder has it, nothing of his is waited on.
#[test]
fn his_held_burst_is_the_one_step_he_is_waited_on_for() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "the build: 36 fails on the runner");
    let h = r.handler_of(t, a);
    assert_eq!((r.m.owner_steps(&r.m.tickets[&t]), r.m.owner_waits_else(&r.m.tickets[&t])), (vec![(h, "gather".to_string(), r.m.id(t))], false));
    assert_eq!((r.m.owner_steps(&r.m.tickets[&h]), r.m.owner_waits_else(&r.m.tickets[&h])), (vec![(h, "gather".to_string(), r.m.id(h))], false));
    let v = r.m.view(t).unwrap();
    assert!(v.waits_on.iter().any(|w| w.agent == "owner" && w.call == "gather" && w.ticket == r.m.id(t)), "the line the view has for it: {:?}", v.waits_on);
    r.hold_ends(h);
    assert_eq!((r.m.owner_steps(&r.m.tickets[&t]), r.m.owner_waits_else(&r.m.tickets[&t])), (Vec::new(), false));
}

/// More than 20 parts (base.kind's `for part in parts up to 20`, which raises `Exhausted` before its first round when
/// the list is longer, then `Exhausted => tell p.agents p`; proto-reader N-1446 p294): the split is not made, and
/// every post of the burst goes to every agent on the ticket, each told once.
#[test]
fn a_split_of_more_than_20_parts_tells_every_post_to_every_agent_once() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let ns: Vec<u32> = (1..=21).map(|i| r.owner_posts(t, &[], &format!("point {i} of the release")).0).collect();
    let h = r.handler_of(t, ns[0]);
    r.hold_ends(h);
    let to = |i: usize| if i % 2 == 0 { "lead" } else { "fixer" };
    let parts: Vec<Value> = ns.iter().enumerate().map(|(i, n)| json!({"posts": [n], "to": [to(i)]})).collect();
    let out = r.go(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "Split", "parts": parts}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    for n in &ns {
        assert_eq!((r.told_lines(t, *n), r.post(t, *n).told.clone()), (1, vec!["lead".to_string(), "fixer".to_string()]), "#{n}");
    }
    assert!(r.handler().is_none(), "every post told: the handler is done");
    r.replays();
}

/// Slow mode's cards for his burst (docket-core4 N-1446 p184, consult-3's small 2, proto-reader p302): a slowed
/// thread, his two posts held together until his "Send now" (no lapse on a slowed thread), the holder chose whom (the
/// watcher's `route`): one tell, so one card holding
/// both, and no `owner_to` while it holds them; his card lets both go at once, and the brain's `owner_to` is raised
/// once, with both posts. His card edited to wake another moves both (`wakes`, one choice), and raises once, to the
/// one he chose. The binding takes each, and each replays. Replayed where its handler is not loaded (the way back), the
/// tell and its card are taken as recorded: the card holds both until he lets them go (proto-reader N-1446 p947), and
/// a program's post given just before the tell, which wakes nobody, is shown and is not on it (consult-3 N-1446 p1026).
#[test]
fn his_burst_on_a_slowed_thread_is_one_card_and_raises_owner_to_once_with_all_its_posts() {
    let burst = || {
        let mut r = Run::bound();
        let t = r.two_on_a_thread();
        r.go(Input::Slow { t, on: true, by: "owner".into() }).reply.unwrap();
        r.m.owner_hold = Some(15);
        let (a, _) = r.owner_posts(t, &[], "the build: 36 fails on the runner, look at the lock");
        let (b, _) = r.owner_posts(t, &[], "and the release notes too");
        let h = r.handler_of(t, a);
        r.sends_now(h);
        let out = r.go(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "To", "agents": ["fixer"]}), text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        (r, t, a, b, out)
    };
    let ids = |r: &Run, t: u64, ns: &[u32]| ns.iter().map(|n| format!("{}.p{n}", r.m.id(t))).collect::<Vec<_>>();
    // one card holding both, nothing raised, nobody woken
    let (mut r, t, a, b, out) = burst();
    assert!(raised(&out).is_empty() && !r.woken("fixer", t), "held at his card: {:?}", out.effects);
    let cards: Vec<(u32, Halting)> = r.m.tickets[&t].halts.iter().map(|h| (h.n, h.what.clone())).collect();
    assert_eq!(cards, vec![(1, Halting::Deliver { post: a, with: vec![b], edited: false, chose: Some("watcher".into()) })]);
    assert!(r.post(t, a).halted && r.post(t, b).halted);
    assert_eq!(out.lines.iter().filter(|l| matches!(l, Line::Halted { .. })).count(), 1, "one card: {:?}", out.lines);
    // the order the way back reads it by (`as_recorded`): its card right after the tell's `told` lines
    let h = out.lines.iter().position(|l| matches!(l, Line::Halted { .. })).unwrap();
    assert!(h >= 2 && matches!(&out.lines[h - 2..h], [Line::Told { n: x, .. }, Line::Told { n: y, .. }] if *x == a && *y == b), "{:#?}", out.lines);
    let v = r.m.view(t).unwrap();
    assert!(matches!(&v.halts[..], [crate::ViewHalt::Deliver { post, with, .. }] if *post == a && with == &vec![b]), "{:?}", v.halts);
    // the way back: a later release's record, whose handler of his burst this release cannot parse
    let mut rec = r.log.clone();
    let mut handler = String::new();
    for l in rec.iter_mut() {
        if let Line::Opened { t: h, kind, program, .. } = l {
            if kind == "owner_post" && *h >= HIDDEN {
                *program = format!("p <- @@ a later release's word @@;\n{program}");
                handler = r.m.id(*h);
            }
        }
    }
    // in the input of the tell, before its `told` lines, a post of the handler's that names and informs nobody
    let told = rec.iter().position(|l| matches!(l, Line::Told { t: u, n, .. } if *u == t && *n == a)).unwrap();
    let Line::Told { at, .. } = rec[told].clone() else { unreachable!() };
    let c = b + 1;
    rec.insert(told, Line::Posted { at, t, n: c, by: handler, step: None, names: vec![], inform: vec![], into_turn: vec![] });
    let mut back = Run::bound();
    let problems = back.m.replay(&rec);
    assert!(problems.iter().any(|p| p.ends_with("is not loaded: this release cannot parse its recorded program.")), "{problems:#?}");
    let cards: Vec<(u32, Halting)> = back.m.tickets[&t].halts.iter().map(|h| (h.n, h.what.clone())).collect();
    assert_eq!(cards, vec![(1, Halting::Deliver { post: a, with: vec![b], edited: false, chose: None })], "{problems:#?}");
    assert!(back.post(t, a).halted && back.post(t, b).halted && !back.woken("fixer", t), "held at his card: {:?}", back.m.due("fixer"));
    assert!(back.post(t, c).visible(), "shown, not on his card: {:?}", back.post(t, c));
    back.approve(t, 1);
    assert!(!back.post(t, a).halted && !back.post(t, b).halted && back.woken("fixer", t));
    // his card lets both go: raised once, both posts, for the fixer
    let out = r.approve(t, 1);
    let both = ids(&r, t, &[a, b]);
    assert_eq!(raised(&out), vec![json!({"agent": "fixer", "posts": both, "tied_at": both[1]})], "{:?}", out.effects);
    assert!(!r.post(t, a).halted && !r.post(t, b).halted && r.woken("fixer", t));
    assert_eq!(out.reply.as_deref(), Ok(format!("Posts {a}, {b} of {} are delivered (fixer, lead).", r.m.id(t)).as_str()));
    r.replays_with(BOUND);
    // his card edited to wake the lead: both posts move, and his card raises once, both posts, for the lead
    let (mut r, t, a, b, _) = burst();
    let out = r.go(Input::HaltEdit { t, n: 1, by: "owner".into(), value: json!({"wakes": ["lead"]}) });
    assert_eq!(out.reply.as_deref(), Ok(format!("Posts {a}, {b} of {} wake lead once approved.", r.m.id(t)).as_str()));
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["lead".to_string()]));
    let out = r.approve(t, 1);
    let both = ids(&r, t, &[a, b]);
    assert_eq!(raised(&out), vec![json!({"agent": "lead", "posts": both, "tied_at": both[1]})], "{:?}", out.effects);
    assert!(r.woken("lead", t) && !r.woken("fixer", t));
    r.replays_with(BOUND);
}

/// docket-core4's case (N-1446 p184): his burst split in two parts on a slowed thread is two tells, so two cards, each
/// holding its own part's posts, and each agent is raised for once, with only its own part's posts, when they reach
/// it: the later part's card let go first is still behind the earlier part's, so it is shown, and raised for, when
/// that card goes (`posts_reached`, I-1459).
#[test]
fn a_split_burst_on_a_slowed_thread_is_a_card_for_each_part() {
    let mut r = Run::bound();
    let t = r.two_on_a_thread();
    r.go(Input::Slow { t, on: true, by: "owner".into() }).reply.unwrap();
    r.m.owner_hold = Some(15);
    let (a, _) = r.owner_posts(t, &[], "the build: 36 fails on the runner, look at the lock");
    let (b, _) = r.owner_posts(t, &[], "and the release notes too");
    let (c, _) = r.owner_posts(t, &[], "and the brain: the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    r.sends_now(h);
    let split = json!({"head": "Split", "parts": [{"posts": [a, b], "to": ["lead"]}, {"posts": [c], "to": ["fixer"]}]});
    let out = r.go(Input::Answer { t: h, by: "watcher".into(), head: split, text: Text::New(String::new()) });
    assert!(out.reply.is_ok() && raised(&out).is_empty(), "{:?} {:?}", out.reply, out.effects);
    let cards: Vec<(u32, u32, Vec<u32>)> = r.m.tickets[&t].halts.iter().filter_map(|h| match &h.what { Halting::Deliver { post, with, .. } => Some((h.n, *post, with.clone())), _ => None }).collect();
    assert_eq!(cards, vec![(1, a, vec![b]), (2, c, vec![])]);
    let ticket = r.m.id(t);
    let id = |n: u32| format!("{ticket}.p{n}");
    let out = r.approve(t, 2);
    assert!(raised(&out).is_empty() && !r.woken("fixer", t), "let through, still behind a and b: {:?}", out.effects);
    assert!(r.post(t, a).halted && r.post(t, b).halted && !r.post(t, c).halted);
    let out = r.approve(t, 1);
    assert_eq!(raised(&out), vec![json!({"agent": "lead", "posts": [id(a), id(b)], "tied_at": id(b)}), json!({"agent": "fixer", "posts": [id(c)], "tied_at": id(c)})], "{:?}", out.effects);
    assert!(r.woken("lead", t) && r.woken("fixer", t));
    r.replays_with(BOUND);
}

/// The way in from the release before 0.3 (proto-reader N-1446 p302 and p348's 1): his `Approve` lines name a card by
/// its number on the ticket, and that release has no bursts, so each tell is one post and one card. Its own slowed
/// record (proto-mind's wb_slow.jsonl, made by train-two 982349ee, byte for byte: two of his posts routed by the
/// watcher, cards 1 and 2; lead's post naming fixer halted, card 3; approved 3, then 1, then 2) replays here with no
/// problem line: the same cards, numbered alike, let through in the same order, each post told as it was.
#[test]
fn a_slowed_record_from_the_release_before_keeps_its_card_numbers() {
    let lines: Vec<Line> = include_str!("slow_before.jsonl").lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
    let mut m = Machine::load(defs());
    assert_eq!(m.replay(&lines), Vec::<String>::new());
    let x = &m.tickets[&200];
    let posts: Vec<Value> = x.posts.iter().map(|p| json!([p.n, p.by, p.told, p.halted])).collect();
    assert_eq!(posts, vec![json!([1, "owner", ["lead", "fixer"], false]), json!([2, "owner", ["lead"], false]), json!([3, "owner", ["fixer"], false]), json!([4, "lead", [], false])]);
    assert_eq!((x.halted, x.halts.len()), (3, 0), "three cards made, none left");
}

/// The way back to build 36 (hold 2; proto-reader N-1446 p276/p286, consult-3 p282): E1b's records of his posts on
/// base.kind's real `owner_post` program, for 36's harness (`replays_from_after`, proto-mind's mind-way-back). Each
/// line is printed after `REC SCENE `, and each post as E1b made it after `POSTS SCENE ` (`[t, n, by, names, told,
/// halted]`, the harness's `.posts`), since the runner drops lines that look like JSON. `off`: no hold, one ordinary
/// post. `on`: the 15 s hold, a held burst of two, one post alone, a burst sent at once by his "Send now" and split.
/// `cut`: the hold on and the record cut while a burst of two is held. `skipped`: his take-back between fixer's
/// hand-over and its `given` (the reverse race, consult-3 N-1446 p661), so fixer's `given` line carries `"skipped":[]`,
/// which a release from before does not know and works out as it did (proto-lead p661). Each scene twice: on today's kinds, and with
/// thread.kind's `drawn line` (L5, 06d870e5) left out, so what 36 makes of E1b's own lines shows past the thread's
/// open (36 cannot read `drawn`). The answers are made up.
#[test]
#[ignore]
fn records_for_the_way_back() {
    let print = |scene: &str, r: &Run| {
        r.replays();
        for l in &r.log {
            println!("REC {scene} {}", serde_json::to_string(l).unwrap());
        }
        for (t, x) in r.m.tickets.range(..HIDDEN) {
            for p in &x.posts {
                println!("POSTS {scene} {}", json!([t, p.n, p.by, p.names, p.told, p.halted]));
            }
        }
    };
    let without_drawn: String = KINDS.iter().find(|(f, _)| *f == "thread.kind").unwrap().1.lines().filter(|l| !l.trim_start().starts_with("drawn ")).collect::<Vec<_>>().join("\n");
    let l5_out = || Run { m: Machine::load(defs_before(&[("thread.kind", without_drawn.as_str())])), log: Vec::new(), clock: 0 };
    let runs: [(&str, &dyn Fn() -> Run); 2] = [("", &Run::new), ("-l5out", &l5_out)];
    for (also, new) in runs {
        let mut r = new();
        let t = r.two_on_a_thread();
        let (a, _) = r.owner_posts(t, &[], "the lock first");
        r.route_of(t, a, json!({"head": "To", "agents": ["lead"]}));
        assert!(r.handler().is_none());
        print(&format!("off{also}"), &r);

        let mut r = new();
        let t = r.two_on_a_thread();
        r.m.owner_hold = Some(15);
        let (a, _) = r.owner_posts(t, &[], "see 2 messages. should have been order / split properly");
        r.owner_posts(t, &[], "read whats relevant from earlier chats");
        let h = r.handler_of(t, a);
        r.hold_ends(h);
        r.route_of(t, a, json!({"head": "To", "agents": ["lead"]}));
        let (c, _) = r.owner_posts(t, &[], "and the median: fixer's");
        let g = r.handler_of(t, c);
        r.hold_ends(g);
        r.route_of(t, c, json!({"head": "To", "agents": ["fixer"]}));
        let (d, _) = r.owner_posts(t, &[], "the build: look at the lock");
        let (e, _) = r.owner_posts(t, &[], "and the brain: mark p614 kept");
        let k = r.handler_of(t, d);
        r.go(Input::Answer { t: k, by: "owner".into(), head: json!({"head": "Send"}), text: Text::New(String::new()) }).reply.unwrap();
        r.route_of(t, d, json!({"head": "Split", "parts": [{"posts": [d], "to": ["lead"]}, {"posts": [e], "to": ["fixer"]}]}));
        assert!(r.handler().is_none());
        print(&format!("on{also}"), &r);

        // fixer's part is handed carrying both his posts while his question on the first waits: nothing left out, and
        // its `given`, after he took the first back, says so
        let mut r = new();
        let t = r.two_on_a_thread();
        r.m.owner_hold = Some(15);
        let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
        let (b, _) = r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
        let h = r.handler_of(t, a);
        r.hold_ends(h);
        for (by, head) in [("watcher", json!({"head": "Unclear", "why": "which work is the median?"})), ("owner", json!({"head": "Send", "to": ["lead"]}))] {
            r.go(Input::Answer { t: h, by: by.into(), head, text: Text::New(String::new()) }).reply.unwrap();
        }
        let id = format!("{}.p{b}", r.m.id(t));
        let part = r.m.due("fixer").into_iter().find(|d| d.id == id).expect("fixer's part");
        let skipped = r.m.left_out("fixer", &part);
        assert_eq!(skipped, Some(std::collections::BTreeSet::new()));
        r.go(Input::TakeBack { t, n: a, by: "owner".into() }).reply.unwrap();
        r.go(Input::Given { t, to: "fixer".into(), delivery: part.id.clone(), upto: part.upto, skipped, place: None }).reply.unwrap();
        assert!(r.log.iter().any(|l| matches!(l, Line::Given { delivery, skipped: Some(s), .. } if *delivery == id && s.is_empty())));
        print(&format!("skipped{also}"), &r);

        let mut r = new();
        let t = r.two_on_a_thread();
        r.m.owner_hold = Some(15);
        r.owner_posts(t, &[], "see 2 messages. should have been order / split properly");
        r.owner_posts(t, &[], "read whats relevant from earlier chats");
        print(&format!("cut{also}"), &r);
    }
}

/// His post told late, put into a running turn (consult-3 N-1446 p717, the hold on core4-head 367fbb84): what a turn
/// must answer of his posts put into it (paper-research N-412.p2194, p2197) reaches his post a delivery of its own told
/// late (`N-T.pN.told`, given with lead's mark as it stood). Held under his question and sent to lead while lead's turn
/// on another thread runs, it goes into that turn; the turn ends on its own thread without answering it, so it comes
/// again once, alone, said to be handed again; its own turn ends with Nothing, so it is owed, `owed` on its row; lead's
/// answer ends that. The record replays so.
#[test]
fn his_post_told_late_into_a_running_turn_and_left_comes_again_then_is_owed() {
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    // lead's turn on another thread runs
    let o = r.note("fixer", "lead", "the stills?");
    let d1 = r.give("lead", o);
    // his post typed to nobody, held under his question while one to fixer goes past it; his press sends it to lead
    let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
    r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    for (by, head) in [("watcher", json!({"head": "Unclear", "why": "which work is the median?"})), ("owner", json!({"head": "Send", "to": ["lead"]}))] {
        r.go(Input::Answer { t: h, by: by.into(), head, text: Text::New(String::new()) }).reply.unwrap();
    }
    let told = format!("{}.p{a}.{TOLD}", r.m.id(t));
    let d = r.m.due("lead").into_iter().find(|d| d.id == told).expect("his post told late to lead");
    r.go(Input::GivenInto { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    // the turn ends on the other thread: his post was not answered, so it comes again once, alone, said so
    let out = r.turn("lead", Some(&d1), 1);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t: u, .. } if *u == o)), "its last message is on the other thread: {:?}", out.lines);
    assert!(out.effects.contains(&Effect::Deliver { to: "lead".into() }), "{out:?}");
    let again = r.m.due("lead").into_iter().find(|e| e.id == told).unwrap_or_else(|| panic!("due again: {:?}", r.m.due("lead")));
    assert!(r.m.handed_before("lead", &told), "said to be handed again");
    let What::Posts { unread } = &again.what else { panic!("{again:?}") };
    assert_eq!(unread.iter().map(|p| p.n).collect::<Vec<_>>(), vec![a], "alone");
    assert!(r.where_is(t, a, "lead").unwrap().read.is_none(), "not read");
    // handed again; its own turn ends with Nothing: owed, not handed a third time, its row says so
    r.go(Input::Given { t, to: "lead".into(), delivery: told.clone(), upto: again.upto, skipped: Default::default(), place: None }).reply.unwrap();
    r.go(Input::TurnEnded { agent: "lead".into(), delivery: Some(told.clone()), text: Loc { len: 0, ..Run::loc(2) }, later: vec![], stood: None, go: None });
    let id = format!("{}.p{a}", r.m.id(t));
    assert_eq!(r.m.owed_posts("lead"), vec![id.clone()]);
    assert!(!r.m.due("lead").iter().any(|e| e.id == told), "not handed a third time");
    assert!(r.where_is(t, a, "lead").unwrap().read.is_none(), "not read");
    let rows: Vec<bool> = r.m.view(t).unwrap().given.iter().filter(|g| g.to == "lead" && g.delivery == told).map(|g| g.owed).collect();
    assert_eq!(rows, [false, true], "the last row of it owed");
    // lead answers it: owed no more
    r.go(Input::Post { t, by: "lead".into(), text: Text::New("the release page's".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(a), kind: None }).reply.unwrap();
    assert!(r.m.owed_posts("lead").is_empty());
    r.replays();
}

/// The skipped half of a giving (proto-reader N-1446 p727, 2; `Ticket::gave`): as above, with his third post of the
/// burst, "@lead and the release page" (a+2), which the Unclear tells to lead while his first waits on his press. Its
/// giving G1 goes into lead's running turn past the first, so its `skipped` holds the first; his press then sends the
/// first to lead late, by `.told`, into the same turn. The turn ends on the other thread answering neither: G1 comes
/// again for what it carried (his @fixer part, which informs lead, and a+2), without the first, and the first only by
/// its `.told`. Its reply on the other thread answers the first too, by the reply tool's `to` (`answered_from`): G1
/// still comes again, and the first does not. Read without `skipped`, G1 would carry the first too, and that answer
/// would answer G1. The record replays so.
#[test]
fn his_post_a_giving_passed_comes_again_only_by_its_told() {
    let setup = || {
        let mut r = Run::new();
        r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
        let t = r.two_on_a_thread();
        r.m.owner_hold = Some(15);
        let o = r.note("fixer", "lead", "the stills?");
        let d1 = r.give("lead", o);
        let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
        r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
        let (c, _) = r.owner_posts(t, &["lead"], "@lead and the release page");
        let h = r.handler_of(t, a);
        r.hold_ends(h);
        r.go(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "Unclear", "why": "which work is the median?"}), text: Text::New(String::new()) }).reply.unwrap();
        // his @lead part goes into lead's running turn past the first: its giving skips it
        let g1 = r.m.due("lead").into_iter().find(|d| d.t == t).expect("his @lead part");
        let skipped = r.m.left_out("lead", &g1);
        let What::Posts { unread } = &g1.what else { panic!("{g1:?}") };
        let carried: Vec<u32> = unread.iter().map(|p| p.n).collect();
        assert_eq!((g1.id.clone(), skipped.clone(), carried.contains(&a), carried.last()), (format!("{}.p{c}", r.m.id(t)), Some(std::collections::BTreeSet::from([a])), false, Some(&c)));
        r.go(Input::GivenInto { t, to: "lead".into(), delivery: g1.id.clone(), upto: g1.upto, skipped, place: None }).reply.unwrap();
        // his press sends the first to lead late, into the same turn
        r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send", "to": ["lead"]}), text: Text::New(String::new()) }).reply.unwrap();
        let told = format!("{}.p{a}.{TOLD}", r.m.id(t));
        let d = r.m.due("lead").into_iter().find(|d| d.id == told).expect("his first told late to lead");
        r.go(Input::GivenInto { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        (r, t, a, d1, g1.id.clone(), carried, told)
    };
    let due_on = |r: &Run, t: u64| -> Vec<(String, Vec<u32>)> {
        let mut out: Vec<(String, Vec<u32>)> = r
            .m
            .due("lead")
            .into_iter()
            .filter(|e| e.t == t)
            .map(|e| match &e.what {
                What::Posts { unread } => (e.id.clone(), unread.iter().map(|p| p.n).collect()),
                w => panic!("{w:?}"),
            })
            .collect();
        out.sort();
        out
    };
    // the turn ends on the other thread answering neither: G1 again for what it carried, the first by its `.told`
    let (mut r, t, a, d1, g1, carried, told) = setup();
    r.turn("lead", Some(&d1), 1);
    let mut want = vec![(g1, carried), (told, vec![a])];
    want.sort();
    assert_eq!(due_on(&r, t), want, "G1 again for what it carried, without the first; the first by its `.told` alone");
    r.replays();
    // its reply on the other thread answers the first too (the reply tool's `to`, `answered_from`): G1 still comes
    // again, for what it carried; the first does not
    let (mut r, t, a, d1, g1, carried, _) = setup();
    let o = parse_delivery(&d1).unwrap().0;
    r.go(Input::Post { t: o, by: "lead".into(), text: Text::New("the stills are in; the dashboard's median is the weekly one".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    let n = r.m.tickets[&o].last();
    r.go(Input::AnswersAlso { t: o, n, also: vec![format!("{}.p{a}", r.m.id(t))] }).reply.unwrap();
    r.turn("lead", Some(&d1), 1);
    assert_eq!(due_on(&r, t), vec![(g1, carried)], "the answer to the first does not answer G1");
    r.replays();
}

/// His post told late and left comes again alone; a later giving on its thread stays read (consult-3 N-1446 p733,
/// proto-lead p734; `hand_his_again`). His post goes into lead's running turn by `.told`; then fixer's post to lead on
/// the same thread goes into it too (G2), and the turn's reply on the other thread answers it, by the reply tool's
/// `to`. The turn ends without answering his post: it comes again by its `.told`, alone, said to be handed again, and
/// G2 does not come again, the read mark where G2 left it. The `.told` row's giving moved no mark, so making it due
/// again puts back none; before, the mark went back to where it stood then, and G2's post came again with it. The
/// record replays so.
#[test]
fn his_post_told_late_left_comes_again_alone_and_a_later_giving_stays_read() {
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let o = r.note("fixer", "lead", "the stills?");
    let d1 = r.give("lead", o);
    // his post, held under his question, sent to lead by his press: into lead's running turn by `.told`
    let (a, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
    r.owner_posts(t, &["fixer"], "@fixer and the ledger should mark p614 kept");
    let h = r.handler_of(t, a);
    r.hold_ends(h);
    for (by, head) in [("watcher", json!({"head": "Unclear", "why": "which work is the median?"})), ("owner", json!({"head": "Send", "to": ["lead"]}))] {
        r.go(Input::Answer { t: h, by: by.into(), head, text: Text::New(String::new()) }).reply.unwrap();
    }
    let told = format!("{}.p{a}.{TOLD}", r.m.id(t));
    let d = r.m.due("lead").into_iter().find(|d| d.id == told).expect("his post told late to lead");
    r.go(Input::GivenInto { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    // fixer's post to lead on the same thread, G2, into the same turn: past his post's place
    r.post_on(t, "fixer", "the ledger marks p614 kept now", &["lead"]).reply.unwrap();
    let f = r.m.tickets[&t].last();
    let g2 = r.m.due("lead").into_iter().find(|e| e.t == t && e.id != told).expect("fixer's post");
    let What::Posts { unread } = &g2.what else { panic!("{g2:?}") };
    assert!(unread.iter().any(|p| p.n == f) && !unread.iter().any(|p| p.n == a) && g2.upto > a, "{g2:?}");
    let skipped = r.m.left_out("lead", &g2);
    r.go(Input::GivenInto { t, to: "lead".into(), delivery: g2.id.clone(), upto: g2.upto, skipped, place: None }).reply.unwrap();
    // the turn's reply on the other thread answers fixer's post too, not his; the turn ends
    r.go(Input::Post { t: o, by: "lead".into(), text: Text::New("the stills are in, and p614 is kept".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    let n = r.m.tickets[&o].last();
    r.go(Input::AnswersAlso { t: o, n, also: vec![format!("{}.p{f}", r.m.id(t))] }).reply.unwrap();
    r.turn("lead", Some(&d1), 1);
    // his post comes again by its `.told`, alone; G2 does not
    let again: Vec<(String, Vec<u32>)> = r
        .m
        .due("lead")
        .into_iter()
        .filter(|e| e.t == t)
        .map(|e| match &e.what {
            What::Posts { unread } => (e.id.clone(), unread.iter().map(|p| p.n).collect()),
            w => panic!("{w:?}"),
        })
        .collect();
    assert_eq!(again, vec![(told.clone(), vec![a])], "his post alone, by its `.told`; G2 stays read");
    assert!(r.m.handed_before("lead", &told), "said to be handed again");
    r.replays();
}

/// The way back onto 0.3 (proto-lead N-1446 p819 (b); consult-3 p803's case, made up): his two posts, typed 5 s apart
/// under a later release that held them as one burst, neither told when its record ends, its handler a program this
/// release cannot parse. Sent on after the replay (UNTOLD) under the policy's 15 s hold, each is held as his typed post
/// is: the first opens the one handler, which gathers, the second joins it, the holder is asked once at the hold's end,
/// and one tell takes both. The hold is on each UNTOLD line, so the record replays as it ran with no hold in the
/// policy; and an UNTOLD line with none (57's, or one sent under a policy with none) reads as none under a policy that
/// holds: each post goes to a handler of its own, routed at once.
#[test]
fn his_untold_posts_sent_on_under_the_hold_are_one_burst_decided_once() {
    let mut r = Run::new();
    let a = r.say("lead", &["fixer"], "the release");
    let from = r.log.len();
    for text in ["fix the lock", "then the window"] {
        let out = r.go(Input::Post { t: a, by: "owner".into(), text: Text::New(text.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    let (p2, p3) = (r.m.tickets[&a].last() - 1, r.m.tickets[&a].last());
    // the later release's record: his first post's handler holds a program this release cannot parse, and his second
    // has no line of its own (that handler's `gather` took it)
    let mut rec = r.log.clone();
    let second = rec.iter().rposition(|l| matches!(l, Line::Post { .. })).unwrap();
    rec.truncate(second + 1);
    for l in rec[from..].iter_mut() {
        if let Line::Opened { t, kind, program, .. } = l {
            if kind == "owner_post" && *t >= HIDDEN {
                *program = format!("p <- @@ a later release's word @@;\n{program}");
            }
        }
    }
    // a machine replaying LINES with the policy's hold HOLD told to it
    let load = |hold: Option<u64>, lines: &[Line]| {
        let mut m = Machine::load(defs());
        m.owner_hold = hold;
        let problems = m.replay(lines);
        (m, problems)
    };
    let (m, problems) = load(None, &rec);
    assert!(problems.len() == 1 && problems[0].ends_with("is not loaded: this release cannot parse its recorded program."), "{problems:#?}");
    assert_eq!(m.untold_posts(), vec![(a, p2), (a, p3)]);
    // coordd's first live moment under the policy's hold: each is sent on with the hold on its line
    let mut s = Run { m, log: rec.clone(), clock: r.clock };
    s.m.owner_hold = Some(15);
    let untold = |s: &mut Run, n: u32| {
        let out = s.go(Input::Event { name: UNTOLD.into(), value: json!({"t": a, "n": n}) });
        assert!(out.reply.is_ok(), "{out:?}");
        assert!(matches!(out.lines.first(), Some(Line::Event { name, value, .. }) if name == UNTOLD && value["hold"] == json!(15)), "the hold on its line: {:?}", out.lines);
        out
    };
    untold(&mut s, p2);
    let h = s.handler_of(a, p2);
    assert_eq!(s.awaits(h), ("owner".to_string(), "gather".to_string()));
    assert!(s.m.due("watcher").is_empty(), "held: no call yet");
    let out = untold(&mut s, p3);
    assert!(out.reply.as_ref().is_ok_and(|w| w.ends_with(&format!("held with his post {p2}."))), "{:?}", out.reply);
    assert_eq!(s.handlers_open(), vec![h], "no handler of its own");
    assert_eq!(s.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![p2, p3]);
    // the hold ends: the holder is asked once, for both, and one tell takes both
    s.hold_ends(h);
    assert_eq!(s.awaits(h), ("watcher".to_string(), "route".to_string()));
    assert_eq!(s.log[rec.len()..].iter().filter(|l| matches!(l, Line::Step { call, .. } if call == "route")).count(), 1, "one decision");
    let out = s.go(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "To", "agents": ["lead"]}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!((s.told_lines(a, p2), s.told_lines(a, p3)), (1, 1));
    assert_eq!((s.post(a, p2).joins, s.post(a, p3).joins), (None, Some(p2)), "the second goes on from the first");
    assert_eq!(s.m.due("lead").iter().filter(|d| d.t == a).count(), 1, "one delivery: {:?}", s.m.due("lead"));
    assert!(s.m.untold_posts().is_empty());
    // the record replays as it ran, the hold read from its lines, with none in the policy
    let (m3, again) = load(None, &s.log);
    assert_eq!(again, problems);
    assert!(m3.untold_posts().is_empty());
    assert_eq!((m3.view(a), m3.view(h)), (s.m.view(a), s.m.view(h)));
    // 57's lines, with no hold: under a policy that holds now, each still goes to a handler of its own, routed at once
    let (m, _) = load(None, &rec);
    let mut n = Run { m, log: rec.clone(), clock: r.clock };
    for p in [p2, p3] {
        let out = n.go(Input::Event { name: UNTOLD.into(), value: json!({"t": a, "n": p}) });
        assert!(matches!(out.lines.first(), Some(Line::Event { value, .. }) if value.get("hold").is_none()), "{:?}", out.lines);
    }
    let (m4, again) = load(Some(15), &n.log);
    assert_eq!(again, problems);
    let hs: Vec<u64> = [p2, p3].iter().map(|p| m4.tickets.values().find(|x| x.is_open() && x.post_of.as_ref().is_some_and(|o| o.t == a && o.n == *p)).map(|x| x.t).expect("a handler of its own")).collect();
    assert_ne!(hs[0], hs[1]);
    for h in hs {
        assert!(matches!(&m4.tickets[&h].status, Status::Awaits { agent, owed, .. } if agent == "watcher" && owed.call == "route"), "{:?}", m4.tickets[&h].status);
    }
}

/// His opening words on a new thread are gathered before intake sorts them (N-1104 p935/p936; I-1474; N-1445 p9: "hold
/// them for 15s by default"): a line carrying the policy's 15 s hold (`hold`, which the machine writes there, never a
/// sender's) holds the thread at its `gather` step before `intake` is asked, nobody is given it, his next `owner_message`
/// to the same agent in the 15 s joins it (`on: Some(t)`), and at the hold's end `intake` reads the whole burst once and
/// both go to the agent the thread is for, together, without a second 15 s wait. His "Send now" lets a held opener go to
/// `intake` at once. His words placed by `intake` on a ticket whose handler gathers a burst of his join that burst. An
/// older thread opened on 64's `thread.kind` (`Run::before_asks`, without `gather` before `intake`) still holds its first
/// post at its handler's `gather` after `intake` and replays as it ran. The words are made up.
#[test]
fn his_opening_words_wait_out_the_hold_as_his_typed_post_does() {
    // his message to lead, its line carrying HOLD: the thread it opens (or joins)
    fn says(r: &mut Run, hold: Option<u64>, text: &str) -> (u64, Out) {
        let mut v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": text, "addressed": {"head": "Unnamed"}});
        if let Some(s) = hold {
            v["hold"] = json!(s);
        }
        let out = r.go(Input::Event { name: "owner_message".into(), value: v });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        (*r.m.tickets.range(..HIDDEN).next_back().unwrap().0, out)
    }
    let message = |r: &Run| r.log.iter().find_map(|l| match l { Line::Event { name, value, .. } if name == "owner_message" => Some(value.get("hold").cloned()), _ => None }).expect("his message's line");
    let given = |r: &Run, t: u64| r.m.due("lead").iter().filter(|d| d.t == t).count();
    let intakes = |r: &Run| r.log.iter().filter(|l| matches!(l, Line::Step { call, .. } if call == "intake")).count();
    // held before intake: on his message's line, and the thread waits at `gather`, no intake yet, given to nobody
    let mut r = Run::new();
    r.m.owner_hold = Some(15);
    let (t, _) = says(&mut r, Some(15), "the lock on the runner fails again");
    assert_eq!(message(&r), Some(json!(15)));
    assert_eq!(r.awaits(t), ("owner".to_string(), "gather".to_string()));
    assert_eq!((intakes(&r), given(&r, t)), (0, 0), "held before intake: no intake yet, lead is given nothing");
    // his next message to lead in the 15 s joins the burst on T: no second thread, one intake at the hold's end
    let (t2, out) = says(&mut r, Some(15), "and the window build too");
    assert_eq!(t2, t, "no second thread opened");
    assert!(out.reply.as_ref().is_ok_and(|w| w == &format!("Held with {}.", r.m.id(t))), "{:?}", out.reply);
    assert!(matches!(out.lines.as_slice(), [Line::Event { name, on: Some(u), fired, .. }] if name == "owner_message" && *u == t && fired.is_empty()), "{:?}", out.lines);
    assert_eq!((intakes(&r), given(&r, t)), (0, 0));
    r.hold_ends(t);
    assert_eq!(r.awaits(t), ("watcher".to_string(), "intake".to_string()));
    assert_eq!(intakes(&r), 1, "one intake for the burst");
    let Status::Awaits { owed, .. } = &r.m.tickets[&t].status else { panic!() };
    assert_eq!(owed.args["text"], json!("the lock on the runner fails again\nand the window build too"));
    r.intake(t, json!({"head": "Thread"}));
    let a = r.m.tickets[&t].last();
    assert_eq!(r.post(t, a).text, Text::New("the lock on the runner fails again\nand the window build too".into()));
    assert!(r.handlers_open().is_empty(), "already gathered before intake: no second hold after intake");
    assert_eq!((r.told_lines(t, a), given(&r, t)), (1, 1), "one delivery: {:?}", r.m.due("lead"));
    r.replays();
    // his "Send now": a held opener goes to intake at once, and on Thread delivers at once
    let mut r = Run::new();
    r.m.owner_hold = Some(15);
    let (t, _) = says(&mut r, Some(15), "the lock on the runner fails again");
    r.sends_now(t);
    assert_eq!(r.awaits(t), ("watcher".to_string(), "intake".to_string()));
    r.intake(t, json!({"head": "Thread"}));
    let a = r.m.tickets[&t].last();
    assert!(r.handlers_open().is_empty());
    assert_eq!((r.told_lines(t, a), given(&r, t)), (1, 1));
    r.replays();
    // placed by intake on a ticket whose handler gathers his burst: it joins that burst
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.m.owner_hold = Some(15);
    let (c, _) = r.owner_posts(t, &[], "the median on the dashboard looks off");
    let g = r.handler_of(t, c);
    let (m, _) = says(&mut r, Some(15), "and the median's tooltip");
    r.hold_ends(m);
    r.intake(m, json!({"head": "Place", "ticket": r.m.id(t)}));
    let d = r.m.tickets[&t].last();
    assert_eq!(r.handlers_open(), vec![g], "no handler of its own");
    assert_eq!(r.m.tickets[&g].post_of.as_ref().unwrap().posts(), vec![c, d]);
    r.replays();
    // no hold on his message's line: the opener goes to intake at once, under a policy that holds now too
    let mut r = Run::new();
    let (t, _) = says(&mut r, None, "the lock on the runner fails again");
    assert_eq!(message(&r), None);
    assert_eq!(r.awaits(t), ("watcher".to_string(), "intake".to_string()));
    r.intake(t, json!({"head": "Thread"}));
    let a = r.m.tickets[&t].last();
    assert!(r.handlers_open().is_empty());
    assert_eq!((r.told_lines(t, a), given(&r, t)), (1, 1));
    let mut m2 = Machine::load(defs());
    m2.owner_hold = Some(15);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(t), r.m.view(t));
    // a hold the sender wrote on his message is not taken: only the policy's goes on its line, none here
    let mut r = Run::new();
    let (t, _) = says(&mut r, Some(15), "the lock on the runner fails again");
    r.intake(t, json!({"head": "Thread"}));
    assert_eq!(message(&r), None);
    assert!(r.handlers_open().is_empty());
    // an older thread opened on 64's thread.kind (no `gather` before `intake`): holds its first post at its handler's
    // `gather` after `intake` and replays as it ran (I-1474)
    let mut r = Run::before_asks();
    r.m.owner_hold = Some(15);
    let (t, _) = says(&mut r, Some(15), "the lock on the runner fails again");
    r.intake(t, json!({"head": "Thread"}));
    let a = r.m.tickets[&t].last();
    let h = r.handler_of(t, a);
    assert_eq!(r.awaits(h), ("owner".to_string(), "gather".to_string()));
    let (b, _) = r.owner_posts(t, &[], "and the window build too");
    assert_eq!(r.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![a, b]);
    r.hold_ends(h);
    assert_eq!((r.told_lines(t, a), r.told_lines(t, b), given(&r, t)), (1, 1, 1));
    r.replays_today();
}

/// His posts the era ERA_HIS_HANDED's line tells late are each handed by the giving that shows it, and by no other
/// (consult-3 N-1479 p262, proto-reader p264): his a and b to lead, both passed by lead's own post before any giving
/// reached them, and the era line tells them late. Each giving leaves out what it does not show (`Machine::left_out`,
/// as coordd writes it on the line), so `hands` is exact for them. a's `.told` alone leaves b owed late, its `.told` due;
/// his later c's own giving alone, past both, leaves a's and b's due. The record replays so.
#[test]
fn his_posts_the_era_tells_late_are_handed_each_by_the_giving_that_shows_it() {
    let setup = || {
        let mut r = Run::new();
        let t = r.two_on_a_thread();
        let (a, _) = r.owner_posts(t, &["lead"], "@lead the median is off");
        let (b, _) = r.owner_posts(t, &["lead"], "@lead and the mean");
        // lead's own post passes both before any giving reached them: so read before the era
        r.post_on(t, "lead", "on it", &[]).reply.unwrap();
        r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_HIS_HANDED, "passed": [[t, a, "lead"], [t, b, "lead"]]}) }).reply.unwrap();
        (r, t, a, b)
    };
    let told = |r: &Run, t: u64, n: u32| format!("{}.p{n}.{TOLD}", r.m.id(t));
    let due = |r: &Run| r.m.due("lead").into_iter().map(|d| d.id).collect::<Vec<_>>();
    // given as coordd gives it: what it leaves out on its line
    let give = |r: &mut Run, id: &str| {
        let d = r.m.due("lead").into_iter().find(|d| d.id == id).unwrap_or_else(|| panic!("{id} due: {:?}", r.m.due("lead")));
        let skipped = r.m.left_out("lead", &d);
        r.go(Input::Given { t: d.t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped, place: None }).reply.unwrap();
    };
    // a's `.told` alone: it showed a, so b stays owed late and its `.told` due
    let (mut r, t, a, b) = setup();
    assert!(due(&r).contains(&told(&r, t, a)) && due(&r).contains(&told(&r, t, b)), "both told late: {:?}", due(&r));
    let ta = told(&r, t, a);
    give(&mut r, &ta);
    let x = &r.m.tickets[&t];
    assert_eq!((x.owes_late("lead", a), x.owes_late("lead", b)), (false, true), "a given, b still owed late");
    assert_eq!((due(&r).contains(&told(&r, t, a)), due(&r).contains(&told(&r, t, b))), (false, true), "{:?}", due(&r));
    r.replays();
    // his later c's own giving alone: past both, it showed neither, so both stay owed late and due
    let (mut r, t, a, b) = setup();
    let (c, _) = r.owner_posts(t, &["lead"], "@lead the dashboard too");
    let tc = format!("{}.p{c}", r.m.id(t));
    give(&mut r, &tc);
    let x = &r.m.tickets[&t];
    assert_eq!((x.owes_late("lead", a), x.owes_late("lead", b)), (true, true), "neither given by c's giving");
    assert!(due(&r).contains(&told(&r, t, a)) && due(&r).contains(&told(&r, t, b)), "{:?}", due(&r));
    r.replays();
}
