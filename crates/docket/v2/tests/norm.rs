//! The normative core over eFLINT (`norm::Core`) behind the seam: a step worked out and then committed, the closing
//! rule, a rule that counts, conditions with the host table, and what a load refuses (6.2, 8.2, 9, 14).

use docket_v2::check::{Loaded, load};
use docket_v2::norm::Core;
use docket_v2::run::explain::{Done, Explain, Holds, Premise};
use docket_v2::run::norms::{Norms, Why};
use docket_v2::run::value::{Env, Instance, Value};
use docket_v2::syntax::read;

const LENDING: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Lent(book: Book, to: Person, by: Person)
Duty Give-Back(holder: Person, claimant: Person, book: Book)
  Holds when Lent(book, holder, claimant)
Act Lend(by: Person, book: Book, to: Person)
  Conditioned by by != to && Not(Exists p, q : Lent(book, p, q))
  Creates Lent(book, to, by)
Act Keep(by: Person, book: Book)
  Conditioned by Exists q : Lent(book, by, q)
  Terminates Lent(book, by, q)
Act Take-Back(by: Person, book: Book)
  Conditioned by Exists p : Lent(book, p, by)
  Terminates Lent(book, p, by)
+Person("ann"). +Person("bob").
"#;

const NOTES: &str = r#"
module t
Fact Person Identified by String
Fact Note(n: String)
Fact Liked(n: String, by: Person)
Fact Likes(n: String, many: Int)
  Holds when Note(n) && many == Count(Foreach p : p When Liked(n, p))
Act Write(by: Person, n: String)
  Conditioned by str_len(n) > 2
  Creates Note(n)
Act Like(by: Person, n: String)
  Conditioned by Note(n)
  Creates Liked(n, by)
on Like(by, n) where Likes(n, k) && k > 1 do { return k }
on Like(by, n) where Exists p : Liked(n, p) && p != by do { return n }
on Write(by, n) where Liked(m, p) do { return m }
"#;

fn loaded(text: &str) -> Result<(Loaded, Core), String> {
    let l = load(vec![read(text).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    let mut core = Core::default();
    core.load(&l.norms)?;
    Ok((l, core))
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn inst(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: format!("t.{ty}"), args }
}

/// A head field set equal to a count stands for that count in the rest of its rule too, so a rule by cases on the
/// count holds only in the case the count gives; under a quantifier that binds a variable of the count it is refused.
#[test]
fn a_head_field_set_equal_stands_for_its_expression_in_the_rest_of_the_rule() {
    let moods = r#"
module t
Fact Person Identified by String
Fact Note(n: String)
Fact Mood(state: String, notes: Int)
  Holds when notes == Count(Foreach n : n When Note(n))
             && ((notes == 0 && state == "calm") || (notes > 0 && state == "busy"))
Act Write(by: Person, n: String)
  Creates Note(n)
+Person("ann").
"#;
    let (_, mut core) = loaded(moods).unwrap();
    let mood = |s: &str, n: i64| inst("Mood", vec![t(s), Value::Int(n)]);
    assert_eq!([core.holds(&mood("calm", 0)), core.holds(&mood("busy", 0))], [true, false]);
    let s = core.step(&inst("Write", vec![t("ann"), t("x")]), Some(&t("ann"))).unwrap();
    core.commit(s);
    let after = [mood("busy", 1), mood("calm", 1), mood("busy", 0), mood("calm", 0)];
    assert_eq!(after.map(|m| core.holds(&m)), [true, false, false, false]);

    let caught = "module t\nFact Note(n: String)\nFact Long(n: String, k: Int)\n  Holds when Note(n) && k == str_len(n) && Exists n : Note(n) && k > 1";
    let why = loaded(caught).err().expect("refused");
    assert!(why.contains("where a quantifier binds n again"), "{why}");
}

/// A head field set equal to an expression that names another such field reads that field's term, whatever order
/// they are written or declared in.
#[test]
fn a_head_field_set_from_another_reads_that_fields_term() {
    let pairs = r#"
module t
Fact Person Identified by String
Fact Note(n: String)
Fact Pair(a: Int, b: Int)
  Holds when a == Count(Foreach n : n When Note(n)) && b == a + 1
Fact Back(b: Int, a: Int)
  Holds when b == a + 1 && a == Count(Foreach n : n When Note(n))
Act Write(by: Person, n: String)
  Creates Note(n)
+Person("ann").
"#;
    let (_, mut core) = loaded(pairs).unwrap();
    let two = |ty: &str, x: i64, y: i64| inst(ty, vec![Value::Int(x), Value::Int(y)]);
    assert_eq!([core.holds(&two("Pair", 0, 1)), core.holds(&two("Back", 1, 0))], [true, true]);
    let s = core.step(&inst("Write", vec![t("ann"), t("x")]), Some(&t("ann"))).unwrap();
    core.commit(s);
    let after = [two("Pair", 1, 2), two("Back", 2, 1), two("Pair", 0, 1), two("Back", 1, 0)];
    assert_eq!(after.map(|p| core.holds(&p)), [true, true, false, false]);

}

/// A step changes nothing until it is committed and is read whether kept or refused; the holder of a duty cannot end
/// it, the claimant can; a condition that fails refuses the step.
#[test]
fn a_step_is_worked_out_then_committed_and_never_closes_its_actors_duty() {
    let (_, mut core) = loaded(LENDING).unwrap();
    let lend = inst("Lend", vec![t("ann"), t("b1"), t("bob")]);
    let lent = inst("Lent", vec![t("b1"), t("bob"), t("ann")]);
    let s = core.step(&lend, Some(&t("ann"))).unwrap();
    assert_eq!((s.created.as_slice(), s.terminated.len()), (std::slice::from_ref(&lent), 0));
    assert!(!core.holds(&lent));
    core.commit(s);
    assert!(core.holds(&lent));
    assert!(core.holds(&inst("Give-Back", vec![t("bob"), t("ann"), t("b1")])));

    let keep = inst("Keep", vec![t("bob"), t("b1")]);
    let nothing = Core::default().reads();
    core.reads();
    assert_eq!(core.step(&keep, Some(&t("bob"))), Err(Why::Closes("t.Give-Back(\"bob\", \"ann\", \"b1\")".into())));
    assert_ne!(core.reads(), nothing, "a refused step is read");
    assert!(core.holds(&lent));
    assert_eq!(core.step(&inst("Lend", vec![t("bob"), t("b1"), t("ann")]), Some(&t("bob"))), Err(Why::Policy("not enabled".into())));

    let back = core.step(&inst("Take-Back", vec![t("ann"), t("b1")]), Some(&t("ann"))).unwrap();
    assert_eq!(back.terminated, [lent.clone()]);
    core.commit(back);
    assert!(!core.holds(&lent));
}

/// A head field set equal to a count is worked out from the body; a host function conditions an act; a `where`
/// binds its free variables each way it holds, in the order of values.
#[test]
fn a_counted_rule_and_conditions_are_read_as_eflint_reads_them() {
    let (l, mut core) = loaded(NOTES).unwrap();
    let act = |core: &mut Core, ty: &str, by: &str, n: &str| core.step(&inst(ty, vec![t(by), t(n)]), Some(&t(by))).map(|s| core.commit(s));
    assert_eq!(act(&mut core, "Write", "ann", "no"), Err(Why::Policy("not enabled".into())));
    act(&mut core, "Write", "ann", "long").unwrap();
    act(&mut core, "Like", "ann", "long").unwrap();
    act(&mut core, "Like", "bob", "long").unwrap();
    assert!(core.holds(&inst("Likes", vec![t("long"), Value::Int(2)])));

    let env = |kv: &[(&str, Value)]| kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<Env>();
    let given = env(&[("by", t("bob")), ("n", t("long"))]);
    let cond = |k: usize| l.ons[k].cond.clone().unwrap();
    assert_eq!(core.each(&cond(0), &given).unwrap(), [env(&[("by", t("bob")), ("n", t("long")), ("k", Value::Int(2))])]);
    assert!(core.cond(&cond(1), &given).unwrap());
    let rows = core.each(&cond(2), &env(&[("by", t("ann")), ("n", t("x"))])).unwrap();
    let pairs: Vec<(Value, Value)> = rows.iter().map(|e| (e["m"].clone(), e["p"].clone())).collect();
    assert_eq!(pairs, [(t("long"), t("ann")), (t("long"), t("bob"))]);
}

/// A field that is not first-order, a Duty without a claimant and a test that is enumerated (also as a rule relying on
/// it, or by a variable bound only under `Not`) are refused when the text is loaded (14, checks 1 and 5); a test used
/// with its fields given loads; rules that read each other under `Not` load, their tie left unknown (6.2).
#[test]
fn a_load_refuses_what_6_2_and_5_3_rule_out() {
    let head = "module t\nFact Person Identified by String\n";
    let refused = |rest: &str, why: &str| match loaded(&format!("{head}{rest}")) {
        Err(e) => assert!(e.contains(why), "{rest}: {e}"),
        Ok(_) => panic!("{rest}: loaded"),
    };
    refused("Fact Many(xs: List<Int>)", "no fact type, Int, String or Bool");
    refused("Duty Owed(holder: Person)", "a Duty has a holder and a claimant");
    let big = "Fact Big(n: Int) Holds when n > 3\n";
    refused(&format!("{big}Act Probe(by: Person) Conditioned by Exists n : Big(n)"), "t.Big is a test");
    refused(&format!("{big}Act Grow(by: Person, n: Int) Creates Big(n)"), "t.Big is a test");
    refused(&format!("{big}Fact Large(n: Int) Holds when Big(n)\nAct Probe(by: Person) Conditioned by Exists n : Large(n)"), "t.Large is a test");
    refused(&format!("{big}Fact Seen(n: Int)\nAct Probe(by: Person) Conditioned by Exists n : Not(Seen(n)) && Big(n)"), "t.Big is a test");
    let tie = "+Person(\"ann\").\nFact P(x: Person) Holds when Person(x) && Not(Q(x))\nFact Q(x: Person) Holds when Person(x) && Not(P(x))";
    let (_, mut core) = loaded(&format!("{head}{tie}")).unwrap();
    let (_, state) = core.eflint().unwrap().unwrap();
    let open: Vec<String> = state.unknown().iter().map(|te| te.to_string()).collect();
    assert_eq!(open, ["t.P(t.Person(\"ann\"))", "t.Q(t.Person(\"ann\"))"]);
    assert!(!core.holds(&inst("P", vec![t("ann")])) && !core.holds(&inst("Q", vec![t("ann")])));
    let (_, mut core) = loaded(&format!("{head}{big}Act Check(by: Person, n: Int) Conditioned by Big(n)")).unwrap();
    let check = |core: &mut Core, n| core.step(&inst("Check", vec![t("ann"), Value::Int(n)]), None).map(|_| ());
    assert_eq!((check(&mut core, 5), check(&mut core, 2)), (Ok(()), Err(Why::Policy("not enabled".into()))));
}

/// Rules that read each other under `Not` load (6.2); an act and a condition that turn on their tie are refused as
/// undecided, naming the cycle and where it is written; an act on a decided fact beside them is kept.
#[test]
fn an_act_or_a_condition_that_turns_on_a_tie_is_refused_as_undecided() {
    let text = r#"module t
Fact Person Identified by String
+Person("ann").
Fact P(x: Person) Holds when Person(x) && Not(Q(x))
Fact Q(x: Person) Holds when Person(x) && Not(P(x))
Act Take-P(by: Person) Conditioned by Not(P(by))
Act Take-Q(by: Person) Conditioned by Not(Q(by))
Act Greet(by: Person) Conditioned by Person(by)
on Greet(by) where Not(P(by)) do { return by }
"#;
    let (l, mut core) = loaded(text).unwrap();
    for act in ["Take-P", "Take-Q"] {
        match core.step(&inst(act, vec![t("ann")]), None).map(|_| ()) {
            Err(Why::Policy(w)) => assert!(
                w.starts_with("undecided, unknown: [t.P, t.Q]") && w.contains("t.P's rule (t:4) reads t.Q through Not"),
                "{act}: {w}"
            ),
            other => panic!("{act}: {other:?}"),
        }
    }
    core.step(&inst("Greet", vec![t("ann")]), None).unwrap();
    let given: Env = [("by".to_string(), t("ann"))].into_iter().collect();
    let w = core.cond(&l.ons[0].cond.clone().unwrap(), &given).unwrap_err();
    assert!(w.starts_with("undecided, unknown: [t.P, t.Q]"), "{w}");
}

/// A rule by cases binds its head in each case, so its type is no test and may be counted; the instance of a type
/// identified by a primitive is that value (5.4); a `Bool` field set equal stands in the head as its text; two counts
/// bind one name to two types.
#[test]
fn rules_by_cases_primitive_instances_and_scoped_names_load_as_eflint_reads_them() {
    let text = r#"
module t
Fact Person Identified by String
Fact Tag Identified by String
Fact Box(n: Int)
Fact Lid(n: Int)
Fact Owned(n: Int, by: Person)
Fact Size(tag: Tag, n: Int)
  Holds when Box(n) && (tag == "small" || tag == "any")
Fact Shut(n: Int)
  Holds when Box(n) || Lid(n)
Fact Lost(n: Int)
  Holds when Owned(n, w) && Not(Person(w))
Fact Shown(by: Person, lit: Bool)
  Holds when Person(by) && lit == true
Fact Total(k: Int)
  Holds when k == Count(Foreach x : x When Shut(x)) + Count(Foreach x : x When Person(x))
Act Put(by: Person, n: Int, to: Person)
  Creates Box(n), Owned(n, to)
Act Pick(by: Person)
  Conditioned by Exists x, y, z, w : x == y && y == z && w == x && Box(z)
+Person("ann").
"#;
    let (_, mut core) = loaded(text).unwrap();
    for (n, to) in [(1, "ann"), (2, "cy")] {
        let s = core.step(&inst("Put", vec![t("ann"), Value::Int(n), t(to)]), Some(&t("ann"))).unwrap();
        core.commit(s);
    }
    let mut holds = |ty: &str, args: Vec<Value>| core.holds(&inst(ty, args));
    assert!(holds("Size", vec![t("any"), Value::Int(1)]) && !holds("Size", vec![t("big"), Value::Int(1)]));
    assert!(holds("Lost", vec![Value::Int(2)]) && !holds("Lost", vec![Value::Int(1)]));
    assert!(holds("Shown", vec![t("ann"), Value::Bool(true)]) && !holds("Shown", vec![t("ann"), Value::Bool(false)]));
    assert!(holds("Total", vec![Value::Int(3)]), "two boxes shut and one person stated");
    assert!(core.step(&inst("Pick", vec![t("ann")]), None).is_ok(), "a chain of equalities types each of its variables");
}

const LIBRARY: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Member(p: Person)
Fact Banned(p: Person)
Fact Shelved(b: Book)
Fact Lent(b: Book)
Fact Open Identified by String
Fact Trusted(p: Person) Holds when Member(p) && Not(Banned(p))
Fact May-Borrow(p: Person, b: Book) Holds when Trusted(p) && Shelved(b)
Act Join(by: Person) Creates Member(by)
Act Shelve(by: Person, b: Book) Creates Shelved(b)
Act Ban(by: Person, who: Person) Creates Banned(who)
Act Close(by: Person, d: String) Terminates Open(d)
Act Borrow(by: Person, b: Book)
  Conditioned by May-Borrow(by, b)
  Conditioned by Member(by) && Not(Lent(b))
  Creates Lent(b)
+Person("ann"). +Person("bob"). +Person("carl"). +Open("desk").
"#;

/// The line of TEXT that NEEDLE first stands on, counted from 1.
fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).map(|k| k as u32 + 1).unwrap_or_else(|| panic!("{needle} is not in the text"))
}

fn by(core: &mut Core, ty: &str, args: &[&str]) {
    let s = core.step(&inst(ty, args.iter().map(|a| t(a)).collect()), Some(&t(args[0]))).unwrap();
    core.commit(s);
}

/// Committed step K, which performed act TY with ARGS.
fn done(k: usize, ty: &str, args: &[&str]) -> Done {
    Done { step: k, act: Some(inst(ty, args.iter().map(|a| t(a)).collect())) }
}

fn premise_texts(ps: &[Premise]) -> Vec<String> {
    ps.iter().map(|p| p.text.clone()).collect()
}

/// A fact two rules from what was stated is proved down to its steps, `Not` by a fact never held; after a ban it fails there.
#[test]
fn a_derived_fact_is_proved_down_to_stated_facts_and_refuted_through_negation_as_failure() {
    let (_, mut core) = loaded(LIBRARY).unwrap();
    by(&mut core, "Join", &["ann"]);
    by(&mut core, "Shelve", &["ann", "b1"]);
    let may = inst("May-Borrow", vec![t("ann"), t("b1")]);
    let Explain::True { by: Holds::Rule(rule), .. } = core.explain(&may).unwrap() else { panic!("May-Borrow holds by its rule") };
    assert_eq!((rule.at.line, rule.text.contains("Trusted")), (line_of(LIBRARY, "Fact May-Borrow"), true));
    assert_eq!(premise_texts(&rule.premises), ["t.Trusted(p)", "t.Shelved(b)"]);
    assert_eq!(rule.env, [("b".to_string(), t("b1")), ("p".to_string(), t("ann"))].into_iter().collect::<Env>());
    let [Explain::True { by: Holds::Rule(trusted), .. }] = rule.premises[0].under.as_slice() else { panic!("Trusted holds by its rule: {:?}", rule.premises[0]) };
    let [member, not_banned] = trusted.premises.as_slice() else { panic!("two premises") };
    assert_eq!(member.under, [Explain::True { inst: inst("Member", vec![t("ann")]), by: Holds::Stated(done(1, "Join", &["ann"])) }]);
    assert!(not_banned.holds && !not_banned.parts[0].holds, "Not(Banned(p)) holds since Banned(p) does not");
    let Explain::False { ended: None, rules, .. } = &not_banned.parts[0].under[0] else { panic!("Banned(ann) never held") };
    assert!(rules.is_empty(), "Banned has no rule: it was never stated");
    assert_eq!(rule.premises[1].under, [Explain::True { inst: inst("Shelved", vec![t("b1")]), by: Holds::Stated(done(2, "Shelve", &["ann", "b1"])) }]);

    by(&mut core, "Ban", &["bob", "ann"]);
    let Explain::False { rules, .. } = core.explain(&inst("Trusted", vec![t("ann")])).unwrap() else { panic!("Trusted(ann) fails after the ban") };
    let [rule] = rules.as_slice() else { panic!("one rule") };
    assert_eq!(premise_texts(&rule.premises), ["Not(t.Banned(p))"], "only the failing conjunct");
    assert_eq!(rule.premises[0].parts[0].under, [Explain::True { inst: inst("Banned", vec![t("ann")]), by: Holds::Stated(done(3, "Ban", &["bob", "ann"])) }]);
}

/// A fact its module states holds by that; once a step ends it, it fails by that step, naming the statement it overrides.
#[test]
fn a_stated_fact_ended_by_a_step_is_explained_by_that_step_over_its_statement() {
    let (_, mut core) = loaded(LIBRARY).unwrap();
    let desk = Instance { ty: "t.Open".into(), args: vec![t("desk")] };
    let at = |e: &Explain| match e {
        Explain::True { by: Holds::Postulated { at }, .. } => at.line,
        Explain::False { overridden: Some(h), .. } => match &**h {
            Holds::Postulated { at } => at.line,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let before = core.explain(&desk).unwrap();
    assert_eq!(at(&before), line_of(LIBRARY, "+Open"));
    by(&mut core, "Join", &["ann"]);
    by(&mut core, "Close", &["ann", "desk"]);
    assert!(!core.holds(&desk));
    let after = core.explain(&desk).unwrap();
    let Explain::False { ended: Some(ended), .. } = &after else { panic!("{after:?}") };
    assert_eq!(*ended, done(2, "Close", &["ann", "desk"]));
    assert_eq!(at(&after), line_of(LIBRARY, "+Open"));
}

/// A refused Act names each failing `Conditioned by` conjunct, its place, values and facts beneath; asking reads nothing.
#[test]
fn a_refused_act_names_each_failing_conjunct_with_its_place_values_and_the_facts_beneath() {
    let (_, mut core) = loaded(LIBRARY).unwrap();
    by(&mut core, "Join", &["ann"]);
    by(&mut core, "Shelve", &["ann", "b1"]);
    by(&mut core, "Ban", &["bob", "ann"]);
    let borrow = |who: &str| inst("Borrow", vec![t(who), t("b1")]);
    assert_eq!(core.step(&borrow("ann"), Some(&t("ann"))), Err(Why::Policy("not enabled".into())), "the step's refusal is as it was");
    core.reads();
    let w = core.why_refused(&borrow("ann")).unwrap();
    assert_eq!(core.reads(), Core::default().reads(), "an explanation is read as no run reads");
    assert_eq!(premise_texts(&w.failing), ["t.May-Borrow(by, b)"], "the second clause holds");
    let p = &w.failing[0];
    assert_eq!((p.at.module.as_str(), p.at.line, p.holds), ("t", line_of(LIBRARY, "Conditioned by May-Borrow"), false));
    assert_eq!(p.env, [("b".to_string(), t("b1")), ("by".to_string(), t("ann"))].into_iter().collect::<Env>());
    let [Explain::False { rules, .. }] = p.under.as_slice() else { panic!("May-Borrow(ann, b1) fails: {p:?}") };
    let [Explain::False { rules: trusted, .. }] = rules[0].premises[0].under.as_slice() else { panic!("through Trusted(ann)") };
    assert_eq!(trusted[0].premises[0].parts[0].under, [Explain::True { inst: inst("Banned", vec![t("ann")]), by: Holds::Stated(done(3, "Ban", &["bob", "ann"])) }]);

    let carl = core.why_refused(&borrow("carl")).unwrap();
    assert_eq!(premise_texts(&carl.failing), ["t.May-Borrow(by, b)", "t.Member(by)"], "one conjunct of each clause; Not(Lent(b)) holds");
    assert_eq!(carl.failing[1].at.line, line_of(LIBRARY, "Conditioned by Member"));

    by(&mut core, "Join", &["bob"]);
    let open = core.why_refused(&inst("Shelve", vec![t("bob"), t("b2")])).unwrap();
    assert!(open.failing.is_empty() && open.syncs.is_empty(), "{open:?}");
}

/// Rules that read each other under `Not` load, the tie left unknown (N-1913): asked, either instance has no single
/// answer and its cycle is witnessed, finite, each link placed; an act conditioned on one is refused and says so.
#[test]
fn a_negative_cycle_is_witnessed_as_a_finite_cycle_of_its_rules() {
    let text = r#"module t
Fact Person Identified by String
Fact P(x: Person) Holds when Person(x) && Not(Q(x))
Fact Q(x: Person) Holds when Person(x) && Not(P(x))
Act Go(by: Person) Conditioned by P(by)
+Person("ann").
"#;
    let l = load(vec![read(text).unwrap()]).unwrap();
    let mut core = Core::default();
    core.load(&l.norms).unwrap();
    let p = inst("P", vec![t("ann")]);
    let Explain::Unknown { inst: asked, cycle } = core.explain(&p).unwrap() else { panic!("no single answer") };
    assert_eq!(asked, p);
    let links: Vec<(Instance, bool, u32)> = cycle.iter().map(|k| (k.inst.clone(), k.negated, k.at.line)).collect();
    assert_eq!(links, [(p.clone(), true, 3), (inst("Q", vec![t("ann")]), true, 4)]);
    assert!(cycle[0].text.contains("Not(t.Q(x))"), "{}", cycle[0].text);
    assert_eq!(docket_v2::norm::cycle(&l.norms, &p), Some(cycle));
    assert_eq!(docket_v2::norm::cycle(&l.norms, &inst("Person", vec![t("ann")])), None);
    assert!(matches!(core.explain(&inst("Person", vec![t("ann")])).unwrap(), Explain::True { .. }));
    let go = inst("Go", vec![t("ann")]);
    assert!(core.step(&go, Some(&t("ann"))).is_err(), "an unknown condition enables nothing");
    let w = core.why_refused(&go).unwrap();
    let [failing] = w.failing.as_slice() else { panic!("{w:?}") };
    assert!(matches!(failing.under.as_slice(), [Explain::Unknown { inst, cycle }] if *inst == p && cycle.len() == 2), "{failing:?}");
}
