//! Derivation by knots and the well-founded model: every answer of a stratified text is the same whatever its types
//! are called; a text whose instances do not read themselves through `Not` is two-valued; a paradox or a tie leaves
//! only its own instances unknown, with what is said about it naming the cycle and where it is written.

use eflint::ast::{Phrase, Statement};
use eflint::ast::{Decl, Elem, Term, id};
use eflint::compile::{Reading, Spec};
use eflint::eval::{Through, graphs_built, polarity, undecided};
use eflint::interp::{Config, Output, report, step};
use eflint::parser::parse_statements;
use eflint::replay::{Event, Refusal, Replay, Value};

const HERE: &str = env!("CARGO_MANIFEST_DIR");

/// The phrases of TEXT.
fn phrases(text: &str) -> Vec<Phrase> {
    let statements = parse_statements(text).expect("the text reads");
    statements.into_iter().map(|s| match s {
        Statement::Phrase(p) => p,
        other => panic!("not a phrase: {other:?}"),
    }).collect()
}

/// Each phrase of TEXT run in turn from CFG, guided or not: for each, its full report's lines, sorted.
fn run(texts: &[String], guided: bool, file: &str) -> Vec<Vec<String>> {
    let mut cfg = Config::default();
    cfg.spec.reading.guided = guided;
    cfg.spec.file = file.into();
    let mut out = Vec::new();
    for p in texts.iter().flat_map(|t| phrases(t)) {
        let (outs, next) = step(&cfg, &p);
        let next = next.unwrap_or_else(|| cfg.clone());
        let mut lines: Vec<String> = report(&outs, &cfg, &next, false).lines().map(str::to_string).collect();
        lines.extend(next.state.contents().iter().map(|(te, i)| format!("{te} = {}", i.value)));
        lines.extend(next.state.unknown().iter().map(|te| format!("{te} = unknown")));
        lines.sort();
        out.push(lines);
        cfg = next;
    }
    out
}

/// TEXT with each name of NAMES given as its pair's second, decorated (`x1`, `x'`) or not; string literals and
/// comments left as they are.
fn rename(text: &str, names: &[(String, String)]) -> String {
    let (mut out, cs) = (String::new(), text.chars().collect::<Vec<_>>());
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '"' {
            let j = (i + 1..cs.len()).find(|&j| cs[j] == '"' && cs[j - 1] != '\\').map_or(cs.len(), |j| j + 1);
            out.extend(&cs[i..j]);
            i = j;
        } else if c == '/' && cs.get(i + 1) == Some(&'/') {
            let j = (i..cs.len()).find(|&j| cs[j] == '\n').unwrap_or(cs.len());
            out.extend(&cs[i..j]);
            i = j;
        } else if word(c) && (i == 0 || !word(cs[i - 1])) {
            let j = (i..cs.len()).find(|&j| !word(cs[j])).unwrap_or(cs.len());
            let w: String = cs[i..j].iter().collect();
            let base = w.trim_end_matches(|c: char| c.is_ascii_digit());
            let hit = names.iter().find(|(n, _)| *n == w).or_else(|| names.iter().find(|(n, _)| n == base));
            out.push_str(&match hit {
                Some((n, m)) => format!("{m}{}", &w[n.len()..]),
                None => w,
            });
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// I as a name of two letters that sort as I does.
fn letters(i: usize) -> String {
    [b'a' + (i / 26) as u8, b'a' + (i % 26) as u8].iter().map(|&b| b as char).collect()
}

/// The types TEXTS declare, the built-in ones aside.
fn declared(texts: &[String]) -> Vec<String> {
    let mut cfg = Config::default();
    for p in texts.iter().flat_map(|t| phrases(t)) {
        if let (_, Some(next)) = step(&cfg, &p) {
            cfg = next;
        }
    }
    cfg.spec.decls.keys().map(|d| d.to_string()).filter(|d| !["int", "string", "actor"].contains(&d.as_str())).collect()
}

/// A conformance case: the reference's file alone, or a benchmark's specification and the statements of its scenario.
fn case(path: &str) -> Vec<String> {
    let read = |p: &str| std::fs::read_to_string(format!("{HERE}/conformance/{p}")).expect("the case is there");
    match path.split_once('+') {
        Some((spec, scenario)) => vec![read(spec), read(scenario)],
        None => vec![read(path)],
    }
}

const STRATIFIED: [&str; 6] = [
    "benchmark/tests/functional/diff_1_naf.spec.eflint+benchmark/tests/functional/diff_1_naf.diff0.eflint",
    "reference/tests/derivation-rules/from_not_enabled2.eflint",
    "reference/tests/derivation-rules/from_not_enabled3.eflint",
    "reference/tests/derivation-rules/deriv_sum1a.eflint",
    "reference/tests/derivation-rules/deriv_sum1b.eflint",
    "benchmark/tests/functional/22_forall_exists.spec.eflint+benchmark/tests/functional/22_forall_exists.same0.eflint",
];

#[test]
fn renaming_every_type_of_a_stratified_text_changes_no_answer() {
    for path in STRATIFIED {
        let texts = case(path);
        let mut names = declared(&texts);
        names.sort();
        let n = names.len();
        // the order of the names turned round, and turned by half: each type now sorts where another did
        let turns: [fn(usize, usize) -> usize; 2] = [|i, n| n - 1 - i, |i, n| (i + n / 2) % n];
        for turn in turns {
            let to: Vec<(String, String)> = names.iter().enumerate().map(|(i, d)| (d.clone(), format!("q{}-{d}", letters(turn(i, n))))).collect();
            let back: Vec<(String, String)> = to.iter().map(|(a, b)| (b.clone(), a.clone())).collect();
            let renamed: Vec<String> = texts.iter().map(|t| rename(t, &to)).collect();
            for guided in [false, true] {
                let want = run(&texts, guided, "case");
                let got: Vec<Vec<String>> = run(&renamed, guided, "case")
                    .into_iter()
                    .map(|ls| {
                        let mut ls: Vec<String> = ls.iter().map(|l| rename(l, &back)).collect();
                        ls.sort();
                        ls
                    })
                    .collect();
                assert_eq!(got, want, "{path}, guided {guided}, renamed {to:?}");
                assert!(want.iter().flatten().all(|l| !l.ends_with("unknown")), "{path} is stratified");
            }
        }
    }
}

/// The instances unknown at the end of TEXT, and the queries' answers in order (`None`: unknown).
fn answers(text: &str, file: &str) -> (Vec<String>, Vec<Option<bool>>, Config) {
    let mut cfg = Config::default();
    cfg.spec.file = file.into();
    let mut queries = Vec::new();
    for p in phrases(text) {
        let (outs, next) = step(&cfg, &p);
        for o in outs {
            match o {
                Output::Query(b) => queries.push(Some(b)),
                Output::Unknown => queries.push(None),
                Output::Runtime(e) => panic!("{}", e.message()),
                _ => {}
            }
        }
        cfg = next.expect("the phrase is taken");
    }
    (cfg.state.unknown().iter().map(|te| te.to_string()).collect(), queries, cfg)
}

/// Reflections §5.3: the least price of an object, read through `Not` of itself (OP `<` or `<=`).
fn min_price(op: &str) -> String {
    format!(
        "Fact object Identified by String
Fact price Identified by Int
Fact bid Identified by object * price
Fact min-price-of Identified by object * price
  Holds when bid(object, price) && Not(Exists price' : min-price-of(object, price') && price' {op} price)
Fact listed Identified by object Holds when (Exists price : bid(object, price))
.
+object(\"watch\"). +object(\"vase\"). +object(\"lamp\"). +price(90). +price(100). +price(120).
+bid(\"watch\", 100). +bid(\"watch\", 90). +bid(\"watch\", 120). +bid(\"vase\", 120).
?min-price-of(\"watch\", 90). ?min-price-of(\"vase\", 120). ?!min-price-of(\"watch\", 100). ?!min-price-of(\"watch\", 120).
?listed(\"watch\"). ?listed(\"vase\"). ?!listed(\"lamp\")."
    )
}

#[test]
fn the_least_price_by_a_strict_comparison_has_one_two_valued_answer() {
    let (unknown, queries, _) = answers(&min_price("<"), "min-price.eflint");
    assert!(unknown.is_empty(), "{unknown:?}");
    assert_eq!(queries, vec![Some(true); 7]);
}

#[test]
fn the_least_price_below_or_equal_to_itself_leaves_only_its_own_instances_unknown() {
    let (unknown, queries, cfg) = answers(&min_price("<="), "min-price.eflint");
    let mp = |o: &str, p: u32| format!("min-price-of(object(\"{o}\"),price({p}))");
    assert_eq!(unknown, vec![mp("vase", 120), mp("watch", 90), mp("watch", 100), mp("watch", 120)]);
    assert_eq!(queries, [vec![None; 4], vec![Some(true); 3]].concat());
    let said = undecided(&cfg.spec, &cfg.state);
    assert_eq!(said.len(), 1);
    assert_eq!(said[0].cycle.iter().map(|r| (r.reader.to_string(), r.through, r.at.clone())).collect::<Vec<_>>(), vec![(
        "min-price-of".to_string(),
        Through::Not,
        Some("min-price.eflint:5".to_string())
    )]);
}

/// GPCE 2025's Example 10 (`p` holds when it does not) and Example 11 (`f(0)` and `f(1)` each when the other does
/// not), with types beside them that read them and that do not.
const PARADOX_AND_TIE: &str = "Fact p Identified by String
  Derived from p(\"\") Where Not(Holds(p(\"\")))
Fact f Identified by Int
  Derived from f(0) Where Not(Holds(f(1)))
  Derived from f(1) Where Not(Holds(f(0)))
  Derived from f(2)
Fact q Identified by String Derived from q(\"a\")
Fact r Identified by String Derived from r(\"\") Where Not(Holds(q(\"b\")))
Fact s Identified by String Derived from s(\"\") Where Holds(p(\"\"))
Fact g Identified by String Derived from g(\"\") Where Holds(f(2)) && Not(Holds(r(\"x\")))
.
?p(\"\"). ?!p(\"\"). ?f(0). ?f(1). ?f(2). ?!f(3). ?q(\"a\"). ?!q(\"b\"). ?r(\"\"). ?s(\"\"). ?g(\"\").
?p(\"\") || q(\"a\"). ?p(\"\") && q(\"b\").";

#[test]
fn a_paradox_and_a_tie_leave_only_their_own_instances_unknown() {
    let (mut unknown, queries, cfg) = answers(PARADOX_AND_TIE, "gpce.eflint");
    unknown.sort();
    assert_eq!(unknown, ["f(0)", "f(1)", "p(\"\")", "s(\"\")"]);
    let (t, u) = (Some(true), None);
    assert_eq!(queries, vec![u, u, u, u, t, t, t, t, t, u, t, t, Some(false)]);
    let said = undecided(&cfg.spec, &cfg.state);
    let cycles: Vec<(String, Vec<Option<String>>)> =
        said.iter().map(|u| (u.types.iter().map(|d| d.to_string()).collect(), u.cycle.iter().map(|r| r.at.clone()).collect())).collect();
    assert_eq!(cycles, vec![
        ("f".into(), vec![Some("gpce.eflint:4".into()), Some("gpce.eflint:5".into())]),
        ("p".into(), vec![Some("gpce.eflint:2".into())]),
        ("s".into(), vec![]),
    ]);
    assert!(said[0].message().contains("neither holding nor failing: f(0), f(1)"), "{}", said[0].message());
}

#[test]
fn a_tie_between_two_bidders_leaves_both_unknown_and_the_rest_decided() {
    let text = "Fact bidder Identified by String
Fact auctioneer Identified by bidder
  Holds when Not(Exists bidder' : auctioneer(bidder') && bidder' != bidder)
Fact seller Identified by String
Fact paid Identified by seller Holds when seller == \"s1\"
.
+bidder(\"ann\"). +bidder(\"bob\"). +seller(\"s1\"). +seller(\"s2\").
?auctioneer(\"ann\"). ?auctioneer(\"bob\"). ?paid(\"s1\"). ?!paid(\"s2\").";
    let (unknown, queries, _) = answers(text, "auction.eflint");
    assert_eq!(unknown, ["auctioneer(bidder(\"ann\"))", "auctioneer(bidder(\"bob\"))"]);
    assert_eq!(queries, vec![None, None, Some(true), Some(true)]);
}

#[test]
fn the_graph_marks_each_read_and_puts_a_read_through_not_in_a_later_stratum() {
    let mut cfg = Config::default();
    cfg.spec.file = "from_not_enabled2.eflint".into();
    for p in case("reference/tests/derivation-rules/from_not_enabled2.eflint").iter().flat_map(|t| phrases(t)) {
        cfg = step(&cfg, &p).1.unwrap_or(cfg);
    }
    let g = polarity(&cfg.spec);
    let reads: Vec<(String, String, Through, Option<String>)> =
        g.reads.iter().map(|r| (r.reader.to_string(), r.read.to_string(), r.through, r.at.clone())).collect();
    assert!(reads.contains(&("prohibition".into(), "read".into(), Through::Not, Some("from_not_enabled2.eflint:14".into()))), "{reads:?}");
    let stratum = |d: &str| g.knots.iter().find(|k| k.members.iter().any(|&i| &*g.types[i] == d)).map(|k| k.stratum);
    assert_eq!((stratum("read"), stratum("prohibition")), (Some(0), Some(1)));
    assert!(g.knots.iter().all(|k| k.falling.is_empty()));
}

/// A tie, acts conditioned on each side of it and one on neither, and a duty violated on a side and one surely.
const TIE_ACTS: &str = "Fact p Identified by String Derived from p(\"ann\") Where Not(Holds(q(\"ann\")))
Fact q Identified by String Derived from q(\"ann\") Where Not(Holds(p(\"ann\")))
Fact person Identified by String
Fact claimant Identified by String
Duty owe Holder person Claimant claimant Violated when Not(Holds(q(\"ann\")))
Duty due Holder person Claimant claimant Violated when person == person
Act open Actor person Creates owe(person, claimant(\"c\")), due(person, claimant(\"c\"))
Act take-p Actor person Conditioned by Not(Holds(p(\"ann\")))
Act take-q Actor person Conditioned by Not(Holds(q(\"ann\")))
Act greet Actor person.";

fn decls(text: &str) -> Vec<Decl> {
    phrases(text).into_iter().flat_map(|p| match p {
        Phrase::Decls(ds) => ds,
        other => panic!("not a declaration: {other:?}"),
    }).collect()
}

/// An act whose condition turns on a tie is refused as undecided, naming the cycle, and one on a decided fact is
/// kept; a duty whose violation turns on it is neither violated nor kept; a condition asked on it is an error; and no
/// step makes the graph again.
#[test]
fn an_act_or_a_duty_that_turns_on_a_tie_is_undecided_and_a_step_makes_no_graph() {
    let spec = Spec { reading: Reading { guided: true, ranging: true }, ..Spec::new() };
    let mut r = Replay::declare(decls(TIE_ACTS), spec).unwrap();
    let act = |n: &str| Event::Act { name: n.into(), actor: Value::Str("ann".into()), objects: vec![] };
    r.step(&act("open"), None).unwrap();
    let built = graphs_built();
    for n in ["take-p", "take-q"] {
        match r.step(&act(n), None) {
            Err(Refusal::Undecided(why)) => assert!(
                why.starts_with("undecided, unknown: [p, q] read each other in a cycle through a read that can turn false")
                    && why.ends_with("neither holding nor failing: p(\"ann\"), q(\"ann\")"),
                "{why}"
            ),
            other => panic!("{n}: {:?}", other.map(|_| ())),
        }
    }
    r.step(&act("greet"), None).unwrap();
    let ann = Elem::Str(id("ann"));
    r.step(&act("greet"), Some(&ann)).unwrap();
    let duties: Vec<(String, bool, bool)> = r.duties().iter().map(|d| (d.duty.dom.to_string(), d.violated, d.undecided)).collect();
    assert_eq!(duties, [("due".into(), true, false), ("owe".into(), false, true)]);
    let asked = |t: &str| match &phrases(&format!("?{t}."))[..] {
        [Phrase::Query(t)] => t.clone(),
        other => panic!("{other:?}"),
    };
    let not_p: Term = asked("Not(Holds(p(\"ann\")))");
    assert!(r.ask(&not_p).unwrap_err().starts_with("undecided, unknown: [p, q]"));
    assert_eq!(r.ask(&asked("Holds(owe(person(\"ann\"), claimant(\"c\")))")), Ok(true));
    assert_eq!(graphs_built(), built, "a step on the text made its graph again");
}

/// A tie whose facts an act turns on and off, worked out again at each step, beside N stored facts nothing reads.
fn tie_beside(n: i64) -> Replay {
    let text = "Fact n Identified by Int
Fact on Identified by String
Fact person Identified by String
Fact p Identified by String Derived from p(\"ann\") Where Holds(on(\"ann\")) && Not(Holds(q(\"ann\")))
Fact q Identified by String Derived from q(\"ann\") Where Holds(on(\"ann\")) && Not(Holds(p(\"ann\")))
Act switch-on Actor person Creates on(\"ann\")
Act switch-off Actor person Terminates on(\"ann\").";
    let spec = Spec { reading: Reading { guided: true, ranging: true }, ..Spec::new() };
    let mut r = Replay::declare(decls(text), spec).unwrap();
    for i in 0..n {
        r.cfg.state.insert(eflint::ast::Tagged { elem: Elem::Int(i), dom: id("n") }, eflint::eval::Info { value: true, derived: false });
    }
    r
}

/// The alternating fixpoint stays inside its knot: a step that works the tie out again takes as long beside 50,000
/// stored facts as beside none (each mean over 200 steps, said on standard error; within a factor of 3, for a
/// loaded machine).
#[test]
fn working_a_tie_out_again_does_not_grow_with_the_facts_beside_it() {
    let mean = |n: i64| {
        let mut r = tie_beside(n);
        let act = |a: &str| Event::Act { name: a.into(), actor: Value::Str("ann".into()), objects: vec![] };
        let p = eflint::ast::Tagged { elem: Elem::Str(id("ann")), dom: id("p") };
        let start = std::time::Instant::now();
        for _ in 0..100 {
            r.step(&act("switch-on"), None).unwrap();
            assert!(r.holds(&p) == Ok(false) && r.cfg.state.unknown().contains(&p));
            r.step(&act("switch-off"), None).unwrap();
            assert!(r.holds(&p) == Ok(false) && r.cfg.state.unknown().is_empty());
        }
        start.elapsed().as_secs_f64() * 1000.0 / 200.0
    };
    let (none, many) = (mean(0), mean(50_000));
    eprintln!("a step working the tie out again: {none:.3} ms beside no fact, {many:.3} ms beside 50,000");
    assert!(many < 3.0 * none.max(0.05), "{many:.3} ms beside 50,000 facts, {none:.3} ms beside none");
}

/// The counterexamples of the owner's audit (N-1913): beside a paradox `p("a")`, a count of `p`, an `Enabled` and a
/// `Violated` of what reads it, and `==`/`!=` on its truth decide nothing; nor does a `Creates ... When` on it.
const COUNTER: &str = "Fact person Identified by String
Fact claimant Identified by String
Fact flag Identified by String
Fact p Identified by String Derived from p(\"a\") Where Not(Holds(p(\"a\")))
Fact held Identified by String Derived from held(\"b\")
Act secret Actor person Conditioned by Holds(p(\"a\"))
Act steal Actor person Conditioned by Count(Foreach p : p When Holds(p)) >= 1
Act launch Actor person Conditioned by Enabled(secret(person))
Act abort Actor person Conditioned by Not(Enabled(secret(person)))
Act same Actor person Conditioned by Holds(p(\"a\")) == False
Act other Actor person Conditioned by Holds(p(\"a\")) != True
Act counted Actor person Conditioned by Count(Foreach held : held When Holds(held)) >= 1
Act mark Actor person Creates flag(\"x\") When Holds(p(\"a\"))
Duty owe Holder person Claimant claimant Violated when Count(Foreach p : p When Holds(p)) >= 1
Duty due Holder person Claimant claimant Violated when Enabled(secret(person))
Act open Actor person Creates owe(person, claimant(\"c\")), due(person, claimant(\"c\")).";

/// Each of the audit's counterexamples is refused as undecided, ranging as docket-v2 reads, guided or not; a
/// count of decided facts beside them enables its act; the duties violated on them are undecided, not violated.
#[test]
fn a_count_an_enabled_a_violated_and_an_equality_on_an_unknown_instance_decide_nothing() {
    for guided in [true, false] {
        let spec = Spec { reading: Reading { guided, ranging: true }, ..Spec::new() };
        let mut r = Replay::declare(decls(COUNTER), spec).unwrap();
        let act = |n: &str| Event::Act { name: n.into(), actor: Value::Str("ann".into()), objects: vec![] };
        r.step(&act("open"), None).unwrap();
        for n in ["secret", "steal", "launch", "abort", "same", "other", "mark"] {
            match r.step(&act(n), None) {
                Err(Refusal::Undecided(why)) => assert!(why.contains("neither holding nor failing: p(\"a\")"), "{n}: {why}"),
                other => panic!("{n}, guided {guided}: {:?}", other.map(|_| ())),
            }
        }
        r.step(&act("counted"), None).unwrap();
        let duties: Vec<(String, bool, bool)> = r.duties().iter().map(|d| (d.duty.dom.to_string(), d.violated, d.undecided)).collect();
        assert_eq!(duties, [("due".into(), false, true), ("owe".into(), false, true)], "guided {guided}");
    }
}

/// A knot that counts itself keeps what is sure among what may hold (the fixpoint's own check) and leaves its
/// instance unknown; a count of another knot's settled instances beside it stays decided.
#[test]
fn a_knot_that_counts_itself_leaves_its_instance_unknown() {
    let text = "Fact q Identified by String Derived from q(\"x\") Where Count(Foreach q : q When Holds(q)) == 0
Fact r Identified by String Derived from r(\"y\")
Fact s Identified by String Derived from s(\"z\") Where Count(Foreach r : r When Holds(r)) == 1
.
?q(\"x\"). ?s(\"z\").";
    let (unknown, queries, _) = answers(text, "count.eflint");
    assert_eq!(unknown, ["q(\"x\")"]);
    assert_eq!(queries, vec![None, Some(true)]);
}


/// The term of a query phrase.
fn query(t: &str) -> eflint::ast::Term {
    match &phrases(&format!("?{t}."))[..] {
        [Phrase::Query(t)] => t.clone(),
        other => panic!("not a query: {other:?}"),
    }
}

/// Acts enabled through each other, and an event synced to one, end the graph's walk: no stack overflow, and the
/// fact read through them is worked out.
#[test]
fn acts_enabled_through_each_other_end_the_walk() {
    let text = "Fact person Identified by String
Act ping Actor person Conditioned by Enabled(pong(person))
Act pong Actor person Conditioned by Enabled(ping(person))
Event tick Syncs with ping(person(\"ann\"))
Fact ready Identified by String Derived from ready(\"r\") Where Enabled(ping(person(\"ann\"))) || Enabled(tick())";
    let mut r = Replay::declare(decls(text), Spec::new()).unwrap();
    assert_eq!(r.ask(&query("Holds(ready(\"r\"))")), Ok(false));
}

/// A round that errors beside another knot's unknown instance leaves that instance unknown, not holding.
#[test]
fn an_error_in_a_round_leaves_the_other_knots_unknown_instances_unknown() {
    let text = "Fact person Identified by String
Fact go Identified by String
Fact p Identified by String Derived from p(\"a\") Where Not(Holds(p(\"a\")))
Fact z Identified by String Derived from z(\"x\") Where Holds(p(\"a\")) && Holds(go(\"g\")) && 1 / 0 == 0
Act start Actor person Creates go(\"g\")";
    let spec = Spec { reading: Reading { guided: true, ranging: true }, ..Spec::new() };
    let mut r = Replay::declare(decls(text), spec).unwrap();
    let start = Event::Act { name: "start".into(), actor: Value::Str("ann".into()), objects: vec![] };
    let stepped = r.step(&start, None).map(|_| ());
    let z = r.ask(&query("Holds(z(\"x\"))"));
    assert!(matches!(stepped, Err(Refusal::Error(_))) || z.as_ref().is_err_and(|e| e.contains("divide by zero")), "{stepped:?} {z:?}");
    let p = r.ask(&query("Holds(p(\"a\"))"));
    assert!(p.as_ref().is_err_and(|why| why.contains("neither holding nor failing: p(\"a\")")), "{p:?}");
}
