//! The order derived types are worked out in: each after what its rule reads, and rules with no single answer left
//! unknown, said with the cycle.

use eflint::session::Session;

/// What TEXT's phrases print in test mode (errors and failed queries only), the derivation guided or not.
fn test_mode(text: &str, guided: bool) -> String {
    let mut out = Vec::new();
    let mut s = Session::new(&mut out, true);
    s.cfg.spec.reading.guided = guided;
    s.run_text(text);
    String::from_utf8(out).unwrap()
}

/// A `Not` of a derived fact is asked when that fact is complete, whatever the two types are called: `n` sorts
/// before `zd` and `m` after `ad`, and neither holds; nor does `e`, through an `Exists` under the `Not`.
#[test]
fn a_not_of_a_derived_fact_answers_the_same_whatever_the_names() {
    let text = "
Fact a Identified by Int
Fact zd Identified by a Holds when a
Fact n Identified by a Holds when a && Not(zd(a))
Fact ad Identified by a Holds when a
Fact m Identified by a Holds when a && Not(ad(a))
Fact e Identified by a Holds when a && Not(Exists a' : zd(a'))
.
+a(1).
?!n(1).
?!m(1).
?!e(1).
?zd(1).
?ad(1).
";
    for guided in [false, true] {
        assert_eq!(test_mode(text, guided), "", "guided {guided}");
    }
}

/// Two derived types each holding when the other does not have no single answer: both instances are left unknown, and
/// what is said names both types, the reads that close the cycle and the instances.
#[test]
fn rules_that_read_each_other_under_not_leave_their_instances_unknown() {
    let text = "
Fact a Identified by Int
Fact p Identified by a Holds when a && Not(q(a))
Fact q Identified by a Holds when a && Not(p(a))
.
+a(1).
";
    for guided in [false, true] {
        let out = test_mode(text, guided);
        assert!(
            out.contains("unknown: [p, q] read each other in a cycle through a read that can turn false (p's rule (:3) reads q through Not; q's rule (:4) reads p through Not); neither holding nor failing: p(a(1)), q(a(1))"),
            "guided {guided}: {out}"
        );
    }
}

/// Rules that read each other with no `Not`, `Forall` or count among them load and are worked out together.
#[test]
fn rules_that_read_each_other_plainly_still_load() {
    let text = "
Fact a Identified by Int
Fact r Identified by a Holds when a || s(a)
Fact s Identified by a Holds when r(a)
.
+a(1).
?r(1).
?s(1).
";
    for guided in [false, true] {
        assert_eq!(test_mode(text, guided), "", "guided {guided}");
    }
}

/// Applying a type whose instances are plain numbers reads it as applying any other type does: `even` and `x` sort
/// before the types they read, and each still gets its instances (the reference's bottom_up_derived_primitive).
#[test]
fn a_rule_reading_a_type_of_plain_numbers_comes_after_it_whatever_the_names() {
    let text = "
Fact y Identified by 1..10 Holds when y > 3.
Fact x Identified by 1..10 Holds when y(x) && x % 2 == 0.
Fact even Identified by 1..10 Holds when x(even).
?even(4).
?even(10).
?!even(2).
?!even(5).
";
    for guided in [false, true] {
        assert_eq!(test_mode(text, guided), "", "guided {guided}");
    }
}

/// Two types of plain numbers each holding when the other does not are left unknown as other types are.
#[test]
fn types_of_plain_numbers_that_read_each_other_under_not_leave_their_instances_unknown() {
    let text = "
Fact p Identified by 1..3 Holds when Not(q(p))
Fact q Identified by 1..3 Holds when Not(p(q))
.
";
    for guided in [false, true] {
        let out = test_mode(text, guided);
        assert!(out.contains("unknown: [p, q] read each other in a cycle") && out.contains("neither holding nor failing: p(1), q(1), p(2), q(2), p(3), q(3)"), "guided {guided}: {out}");
    }
}

/// A rule that divides is tried on every combination, guided too, so it meets the errors the reference meets. Here
/// `+b(0)` makes the combination of a 2 and b 0, which no `pair` names: it divides by zero there, the line is refused
/// and b 0 is not held, both ways.
#[test]
fn a_division_by_zero_no_fact_gives_is_met_both_ways() {
    let text = "
Fact a Identified by Int
Fact b Identified by Int
Fact pair Identified by a * b
Fact ratio Identified by a * b Holds when a / b > 0 && pair(a, b)
.
+a(2). +b(0). +b(1).
+pair(a(2), b(1)).
?ratio(a(2), b(1)).
?b(0).
";
    for guided in [false, true] {
        assert_eq!(test_mode(text, guided), "INTERNAL ERROR divide by zero\nquery failed\n", "guided {guided}");
    }
}
