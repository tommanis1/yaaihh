//! The export for Clingo (`eflint::clingo::export`) run by Clingo itself: each text's stable models set beside the
//! engine's well-founded model, both read guided and ranging as docket-v2 reads. Clingo is the command CLINGO names
//! (words split at spaces, e.g. `python3 -m clingo`), else `clingo`; a test fails where it does not start.

use eflint::ast::Statement;
use eflint::clingo::{export, instance};
use eflint::compile::Reading;
use eflint::eval::Info;
use eflint::interp::{Config, step};
use eflint::parser::parse_statements;
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

type Model = BTreeSet<String>;

/// TEXTS taken in turn as the file FILE, guided and ranging.
fn config(texts: &[&str], file: &str) -> Config {
    let mut cfg = Config::default();
    cfg.spec.file = file.into();
    cfg.spec.reading = Reading { guided: true, ranging: true };
    for text in texts {
        for s in parse_statements(text).expect("the text reads") {
            let Statement::Phrase(p) = s else { panic!("not a phrase: {s:?}") };
            cfg = step(&cfg, &p).1.expect("the phrase is taken");
        }
    }
    cfg
}

/// The engine's well-founded model of CFG: what holds and what is left unknown, as Clingo writes instances.
fn engine(cfg: &Config) -> (Model, Model) {
    let sure = cfg.state.contents().iter().filter(|(_, i)| i.value).map(|(te, _)| plain(&instance(te))).collect();
    (sure, cfg.state.unknown().iter().map(|te| plain(&instance(te))).collect())
}

/// An instance as Clingo prints it: no space outside a string.
fn plain(i: &str) -> String {
    let (mut out, mut quoted, mut escaped) = (String::new(), false, false);
    for c in i.chars() {
        if c == '"' && !escaped {
            quoted = !quoted;
        }
        escaped = c == '\\' && !escaped;
        if quoted || c != ' ' {
            out.push(c);
        }
    }
    out
}

/// The stable models of PROGRAM, each the instances its `holds` atoms name, in Clingo's order; Clingo's warnings.
fn models(program: &str) -> (Vec<Model>, String) {
    let cmd = std::env::var("CLINGO").unwrap_or_else(|_| "clingo".into());
    let words: Vec<&str> = cmd.split_whitespace().collect();
    let mut child = Command::new(words[0])
        .args(&words[1..])
        .args(["-", "0", "-V0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("Clingo ({cmd}) does not start: {e}; CLINGO names its command"));
    child.stdin.take().expect("piped").write_all(program.as_bytes()).expect("Clingo reads the program");
    let out = child.wait_with_output().expect("Clingo ends");
    let (text, warned) = (String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string());
    let lines: Vec<&str> = text.lines().collect();
    let Some((last, found)) = lines.split_last() else { panic!("Clingo said nothing: {warned}\n{program}") };
    match *last {
        "SATISFIABLE" => (sorted(found.iter().map(|l| atoms(l)).collect()), warned),
        "UNSATISFIABLE" => (Vec::new(), warned),
        other => panic!("Clingo ended with {other:?}: {warned}\n{program}"),
    }
}

/// The instances the `holds` atoms of a model's line name, split at the spaces outside strings.
fn atoms(line: &str) -> Model {
    let (mut out, mut word, mut quoted, mut escaped) = (Model::new(), String::new(), false, false);
    for c in line.chars().chain([' ']) {
        if c == ' ' && !quoted {
            if let Some(i) = word.strip_prefix("holds(").and_then(|w| w.strip_suffix(')')) {
                out.insert(i.to_string());
            }
            word.clear();
            continue;
        }
        if c == '"' && !escaped {
            quoted = !quoted;
        }
        escaped = c == '\\' && !escaped;
        word.push(c);
    }
    out
}

/// The well-founded model of CFG within each stable model of its export (what holds is in each, what fails in none),
/// and where it leaves nothing unknown, the one stable model; the stable models, what is sure and what unknown.
fn parity(cfg: &Config) -> (Vec<Model>, Model, Model) {
    let program = export(&cfg.spec, &cfg.state);
    let (models, _) = models(&program);
    let (sure, unknown) = engine(cfg);
    let may: Model = sure.union(&unknown).cloned().collect();
    for m in &models {
        assert!(sure.is_subset(m), "sure {:?} not in a stable model {m:?}\n{program}", sure.difference(m).collect::<Vec<_>>());
        assert!(m.is_subset(&may), "a stable model holds what fails: {:?}\n{program}", m.difference(&may).collect::<Vec<_>>());
    }
    if unknown.is_empty() {
        assert_eq!(models, vec![sure.clone()], "\n{program}");
    }
    (models, sure, unknown)
}

fn set(xs: &[&str]) -> Model {
    xs.iter().map(|x| plain(x)).collect()
}

fn union(a: &Model, b: &Model) -> Model {
    a.union(b).cloned().collect()
}

fn sorted(mut ms: Vec<Model>) -> Vec<Model> {
    ms.sort();
    ms
}

/// Bids: a comparison of a field with a number, a count of composite instances, the greatest of a set and of an empty
/// one, and the engine's division and remainder, which round towards negative infinity.
const BIDS: &str = "Fact object Identified by String
Fact price Identified by Int
Fact bid Identified by object * price
Fact cheap Identified by object Holds when (Exists bid : bid.object == object && bid.price < 100 && bid)
Fact busy Identified by object Holds when Count(Foreach bid : bid When bid.object == object) >= 2
Fact top Identified by object * price Holds when bid(object, price) && price == Max(Foreach bid : bid.price When bid.object == object)
Fact low Identified by object Holds when Max(Foreach bid : bid.price When bid.object == object) == 0
Fact half Identified by Int Derived from (Foreach price : half((price - 95) / 2))
Fact rest Identified by Int Derived from (Foreach price : rest((price - 95) % 7))
.
+object(\"watch\"). +object(\"vase\"). +object(\"lamp\").
+bid(\"watch\", 90). +bid(\"watch\", 100). +bid(\"vase\", 120).";

#[test]
fn bids_have_one_stable_model_and_it_is_the_engines() {
    let (models, sure, _) = parity(&config(&[BIDS], "bids.eflint"));
    let want = [
        "e_cheap(e_object(\"watch\"))",
        "e_busy(e_object(\"watch\"))",
        "e_top(e_object(\"watch\"), e_price(100))",
        "e_top(e_object(\"vase\"), e_price(120))",
        "e_low(e_object(\"lamp\"))",
        // (90 - 95) / 2 is -3 and (90 - 95) % 7 is 2, where Clingo's own `/` and `\` give -2 and -5
        "e_half(-3)",
        "e_rest(2)",
    ];
    for w in want {
        assert!(sure.contains(&plain(w)), "{w} not in {sure:?}");
    }
    assert!(!sure.contains(&plain("e_cheap(e_object(\"vase\"))")) && !sure.contains(&plain("e_low(e_object(\"watch\"))")), "{sure:?}");
    assert_eq!(models.len(), 1);
}

/// A tie of two bidders, each the auctioneer where the other is not.
const AUCTION: &str = "Fact bidder Identified by String
Fact auctioneer Identified by bidder
  Holds when Not(Exists bidder' : auctioneer(bidder') && bidder' != bidder)
Fact seller Identified by String
Fact paid Identified by seller Holds when seller == \"s1\"
.
+bidder(\"ann\"). +bidder(\"bob\"). +seller(\"s1\"). +seller(\"s2\").";

#[test]
fn a_tie_between_two_bidders_has_a_stable_model_for_each_and_the_engine_leaves_both_unknown() {
    let (models, sure, unknown) = parity(&config(&[AUCTION], "auction.eflint"));
    let (ann, bob) = ("e_auctioneer(e_bidder(\"ann\"))", "e_auctioneer(e_bidder(\"bob\"))");
    assert_eq!(unknown, set(&[ann, bob]));
    assert!(sure.contains(&plain("e_paid(e_seller(\"s1\"))")), "{sure:?}");
    assert_eq!(models, sorted(vec![union(&sure, &set(&[ann])), union(&sure, &set(&[bob]))]));
}

/// Reflections §5.3: the least price of an object, read through `Not` of itself, by OP.
fn min_price(op: &str) -> String {
    format!(
        "Fact object Identified by String
Fact price Identified by Int
Fact bid Identified by object * price
Fact min-price-of Identified by object * price
  Holds when bid(object, price) && Not(Exists price' : min-price-of(object, price') && price' {op} price)
.
+object(\"watch\"). +object(\"vase\").
+bid(\"watch\", 100). +bid(\"watch\", 90). +bid(\"watch\", 120). +bid(\"vase\", 120)."
    )
}

#[test]
fn the_least_price_by_a_strict_comparison_has_one_stable_model_and_by_its_own_price_none() {
    let (models, sure, _) = parity(&config(&[&min_price("<")], "min-price.eflint"));
    let mp = |o: &str, p: u32| format!("e_min_dprice_dof(e_object(\"{o}\"),e_price({p}))");
    assert!(sure.contains(&mp("watch", 90)) && sure.contains(&mp("vase", 120)) && !sure.contains(&mp("watch", 100)), "{sure:?}");
    assert_eq!(models.len(), 1);
    // each least price reads itself through `Not` once it may equal itself: no stable model, all four unknown
    let (models, _, unknown) = parity(&config(&[&min_price("<=")], "min-price.eflint"));
    assert_eq!((models.len(), unknown.len()), (0, 4), "{unknown:?}");
}

/// A text of the benchmark's GPCE 2025 functional suite with its scenario.
fn benchmark(name: &str) -> Config {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/conformance/benchmark/tests/functional/");
    let read = |part: &str| std::fs::read_to_string(format!("{dir}{name}.{part}.eflint")).expect("the benchmark is there");
    config(&[&read("spec"), &read("diff0")], &format!("{name}.eflint"))
}

#[test]
fn the_gpce_examples_have_the_stable_models_the_paper_gives() {
    // Example 9, a read through `Not` of what holds: y("") alone
    let (models, sure, _) = parity(&benchmark("diff_1_naf"));
    assert_eq!((models, sure), (vec![set(&["e_y(\"\")"])], set(&["e_y(\"\")"])));
    // Example 10, p("") when it does not hold: no stable model, and p("") unknown
    let (models, _, unknown) = parity(&benchmark("diff_2_no_stable"));
    assert_eq!((models, unknown), (Vec::new(), set(&["e_p(\"\")"])));
    // Example 11, f(0) and f(1) each when the other does not: one stable model for each, both unknown
    let (models, _, unknown) = parity(&benchmark("diff_3_2_stable"));
    assert_eq!(models, sorted(vec![set(&["e_f(0)"]), set(&["e_f(1)"])]));
    assert_eq!(unknown, set(&["e_f(0)", "e_f(1)"]));
    // the suite's fourth such text, a composite read through `Not` of itself under the scenario's x(0), x(1)
    // y(x, x, x, x) for each x and nothing more sure; no stable model, and the ten other y of 0 and 1 unknown
    let (models, sure, unknown) = parity(&benchmark("diff_clingo_no_model"));
    let y = |a: [u8; 4]| format!("e_y({})", a.map(|i| format!("e_x({i})")).join(","));
    assert_eq!(sure, set(&["e_x(0)", "e_x(1)", &y([0; 4]), &y([1; 4])]));
    assert_eq!((models.len(), unknown.len()), (0, 10), "{unknown:?}");
}

#[test]
fn a_closure_with_a_domain_constraint_has_the_engines_one_stable_model() {
    let text = include_str!("../conformance/reference/tests/derivation-rules/transitive1.eflint");
    let (_, sure, _) = parity(&config(&[text], "transitive1.eflint"));
    let fc = |a: &str, b: &str| format!("e_family_dof_dclosure(e_person(\"{a}\"),e_person(\"{b}\"))");
    assert!(sure.contains(&fc("Alice", "Chloe")) && sure.contains(&fc("Chloe", "Alice")), "{sure:?}");
    assert!(!sure.contains(&fc("Alice", "Alice")), "the constraint keeps a person from their own family: {sure:?}");
}

/// A knot that counts itself, `Not(False)`, an `Exists` binding again a variable the rule ranges over, and a rule
/// whose condition is a `When` with alternatives.
#[test]
fn a_self_count_a_not_false_a_shadowed_exists_and_a_when_of_alternatives_agree_with_the_engine() {
    let text = "Fact object Identified by String
Fact bid Identified by object1 * object2
Fact q Identified by String Derived from q(\"x\") Where Count(Foreach q : q When Holds(q)) == 0
Fact t Identified by String Derived from t(\"y\") Where Not(False)
Fact u Identified by bid Holds when bid && (Exists bid : bid.object1 == \"b\" && bid)
Fact v Identified by String Derived from (Foreach object : v(object) When (object == \"a\" || object == \"b\"))
.
+bid(\"a\", \"c\"). +bid(\"b\", \"d\").";
    let cfg = config(&[text], "knots.eflint");
    let program = export(&cfg.spec, &cfg.state);
    let (_, warned) = models(&program);
    assert!(!warned.contains("aux"), "an atom no rule makes: {warned}\n{program}");
    let (models, sure, unknown) = parity(&cfg);
    assert_eq!(unknown, set(&["e_q(\"x\")"]));
    assert!(models.is_empty(), "q(\"x\") holds just where none does: {models:?}");
    for w in ["e_t(\"y\")", "e_v(\"a\")", "e_v(\"b\")"] {
        assert!(sure.contains(w), "{w} not in {sure:?}");
    }
    // the inner bid is its own: each holding bid is a u, the outer bid's fields untouched by the inner's
    let u = |a: &str, b: &str| format!("e_u(e_bid(e_object(\"{a}\"),e_object(\"{b}\")))");
    assert!(sure.contains(&u("a", "c")) && sure.contains(&u("b", "d")), "{sure:?}");
}

/// A string with a line break in a stated fact is one string to Clingo, and a place with one stays in its comment.
#[test]
fn a_line_break_in_a_string_or_a_place_is_no_rule() {
    let text = "Fact name Identified by String\nAct go Actor name\nFact ready Identified by String Derived from ready(\"\") Where Enabled(go(name(\"a\")))";
    let mut cfg = config(&[text], "go.eflint\nholds(e_injected).");
    let te = eflint::ast::Tagged { elem: eflint::ast::Elem::Str("one\ntwo \"three\" \\".into()), dom: "name".into() };
    cfg.state.insert(te.clone(), Info { value: true, derived: false });
    let program = export(&cfg.spec, &cfg.state);
    assert!(program.contains("% not exported: a rule of ready (go.eflint holds(e_injected).:3): Violated and Enabled are not exported"), "{program}");
    let (models, _) = models(&program);
    assert_eq!(models, vec![set(&[&instance(&te)])], "\n{program}");
}
