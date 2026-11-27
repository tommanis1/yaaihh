//! The type check on small texts (spec 7; load checks 3, 6, 8): what it takes, what it refuses, the rows it finds.

use std::collections::BTreeSet;

const HEAD: &str = "module t\nEffect Ask() -> Int\nEffect Out() -> Int\nEffect Pick<T>(x: T) -> T\nFact Moment Identified by String\nEffect Hook(m: Moment) -> Int\n";

fn typed(t: &str) -> Result<docket_v2::check::typing::Rows, String> {
    let l = docket_v2::check::names::load(vec![docket_v2::syntax::read(&format!("{HEAD}{t}")).unwrap_or_else(|e| panic!("{e}\n{t}"))]).unwrap();
    docket_v2::check::typing::check(&l).map_err(|es| es.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))
}

fn refused(t: &str, why: &str) {
    let e = typed(t).err().unwrap_or_else(|| panic!("typed, and should not be:\n{t}"));
    assert!(e.contains(why), "{e}\n{t}");
}

fn row(t: &str, act: &str) -> BTreeSet<String> {
    typed(t).unwrap_or_else(|e| panic!("{e}\n{t}")).acts[act].clone()
}

#[test]
fn a_program_a_fn_builds_runs_only_with_do() {
    typed("fn p() -> Program<Int> { perform Ask() }\nfn q() -> Program<Int> { do p() }").unwrap();
    refused("fn p() -> Program<Int> { perform Ask() }\nfn q() -> Program<Int> { p() }", "Int wanted, Program<Int");
    refused("Act A(by: String) Does { perform Ask() }", "Unit wanted, Int given");
}

#[test]
fn an_if_without_else_gives_unit_and_par_gives_a_tuple_or_a_list() {
    refused("fn f(b: Bool) -> Program<Int> { if b { return 1 } }", "an `if` without `else` gives ()");
    typed("fn f() -> Program<(Int, Int)> { par(perform Ask(), perform Out()) }\nfn g() -> Program<List<Int>> { par x in [1, 2] up to 1 { return x } }").unwrap();
    refused("fn g() -> Program<Unit> { par x in [1, 2] { return x } }", "Unit wanted, List<Int> given");
    refused("fn g() -> Program<Unit> {\n  x <- perform Ask();\n  par y in [x] { return y }\n}", "t:9:3: Unit wanted, List<Int> given");
}

#[test]
fn an_arm_keeps_its_effects_type_parameter_abstract_and_names_an_effect() {
    typed("fn f(p: Program<Int>) -> Program<Int> { handle do p with { | Pick(x), k => resume k(x) } }").unwrap();
    refused("fn f(p: Program<Int>) -> Program<Int> { handle do p with { | Pick(x), k => resume k(1) } }", "Int given");
    refused("Act Go(by: String)\nfn f(p: Program<Int>) -> Program<Int> { handle do p with { | Go(b), k => resume k(()) } }", "which is no Effect");
}

#[test]
fn k_is_resumed_at_most_once_on_a_path_and_never_inside_par() {
    refused("fn f() -> Program<Int> { handle { perform Ask() } with { | Ask(), k => { x <- resume k(1); resume k(x) } } }", "twice on one path");
    typed("fn f(b: Bool) -> Program<Int> { handle { perform Ask() } with { | Ask(), k => if b { resume k(1) } else { resume k(2) } } }").unwrap();
    refused("fn f() -> Program<(Int, Int)> { handle { par(perform Ask(), return 0) } with { | Ask(), k => par(resume k(1), return 2) } }", "inside a `par`");
}

#[test]
fn no_fn_stands_on_a_cycle_and_no_program_performs_charge() {
    refused("fn f() -> Program<Int> { do g() }\nfn g() -> Program<Int> { do f() }", "calls itself");
    refused("fn f() -> Program<Unit> { perform Charge(\"tokens\", 1) }", "performs no Charge");
}

#[test]
fn a_handlers_row_loses_the_effects_its_arms_take_whole_and_keeps_those_under_a_where() {
    assert_eq!(row("Act A(by: String) Returns Int Does { handle { x <- perform Ask(); perform Out() } with { | Ask(), k => resume k(1) } }", "t.A"), ["t.Out".to_string()].into());
    let kept = row("Act A(by: String) Returns Int Does { handle { x <- perform Ask(); perform Out() } with { | Ask(), k where 1 > 0 => resume k(1) } }", "t.A");
    assert_eq!(kept, ["t.Ask".to_string(), "t.Out".to_string()].into());
}

#[test]
fn a_fn_given_a_program_has_that_programs_row_less_what_it_handles() {
    let t = "fn wrap(p: Program<Int>) -> Program<Int> { handle do p with { | Ask(), k => resume k(1) } }\nAct A(by: String) Returns Int Does { do wrap({ x <- perform Ask(); perform Out() }) }\nAct B(by: String) Returns Int Does { do wrap({ perform Hook(\"tool_called\") }) }";
    assert_eq!(row(t, "t.A"), ["t.Out".to_string()].into());
    assert_eq!(row(t, "t.B"), ["t.Hook".to_string()].into());
    refused("fn only(p: Program<Int ! {Ask}>) -> Program<Int> { do p }\nAct A(by: String) Returns Int Does { do only({ perform Out() }) }", "may perform t.Out");
    refused("Act A(by: String) Returns Int Does { perform Hook(1) }", "t.Moment wanted, Int given");
}

#[test]
fn never_given_fits_any_type_and_a_value_given_where_never_is_wanted_does_not() {
    refused("fn f() -> Program<Never> { return \"x\" }", "Never wanted, String given");
    refused("type T = A | B | C\nfn f(x: T) -> Program<Int> { match x { | A => perform Fault(\"no\") | B => return 1 | C => return \"s\" } }", "String given");
    refused("type T = A | B | C\nfn f(x: T) -> Program<Int> { match x { | B => return 1 | C => return \"s\" | A => perform Fault(\"no\") } }", "String given");
    typed("fn f(b: Bool) -> Program<Int> { if b { perform Fault(\"no\") } else { return 1 } }").unwrap();
    refused("fn f(p: Program<Int>) -> Program<Int> { handle do p with { | Fault(w), k => resume k(5) } }", "Never wanted, Int given");
}

#[test]
fn a_program_built_and_dropped_is_refused() {
    refused("fn note(x: Int) -> Program<Int> { perform Ask() }\nfn f() -> Program<Unit> { note(1); return () }", "a program built and dropped");
    typed("fn note(x: Int) -> Program<Int> { perform Ask() }\nfn f() -> Program<Unit> { do note(1); return () }").unwrap();
}

#[test]
fn a_condition_the_core_reads_whole_calls_no_fn() {
    let head = "fn f(x: Int) -> Int { return x }\nFact Seen(n: Int)\n";
    refused(&format!("{head}fn g(n: Int) -> Program<Int> {{\n  if Seen(n) && f(n) > 0 {{ return 1 }} else {{ return 0 }}\n}}"), "t:10:17: t.f is called in a condition the normative core reads whole");
    refused(&format!("{head}fn g(n: Int) -> Program<Bool> {{ return Exists m : Seen(m) && f(m) > n }}"), "t.f is called in a condition");
    typed(&format!("{head}fn g(n: Int) -> Program<Int> {{ x <- return f(n); if Seen(n) && x > 0 {{ return 1 }} else {{ if f(n) > 0 {{ return 2 }} else {{ return 0 }} }} }}")).unwrap();
}
