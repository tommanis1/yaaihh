//! A match arm's `where` (N-101.p42) and a post that fills `{{ name }}` from the scope (N-101.p42).

use super::*;

const GUARD: &str = "kind mine\n  letter M\n  form c: Text\n  result Text\n  pattern\n    t <- step runner \"Whose red?\" -> Part(k: Text, why: Text, held: [Text]);\n    match t {\n      Part(k, why, held) where k == c => (post \"mine: {{ why }}; held: {{ held }}\"; return why)\n      Part(k, why, held) => return k\n    }\n";

/// The guarded arm is taken only when its condition holds; otherwise the next arm. The post fills a text and a list.
/// Printed with its `where`, read back the same; replay gives the same view.
#[test]
fn an_arm_with_where_is_taken_only_when_it_holds_and_a_post_fills_its_names() {
    let (d, errors) = defs_with(&[("mine.kind", GUARD)]);
    assert!(errors.is_empty(), "{errors:?}");
    let text = print::prog(&d.kinds["mine"].pattern, 0);
    assert!(text.contains("Part(k, why, held) where k == c =>"), "{text}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("mine", json!({"c": "abc"}));
    r.answer(200, "runner", json!({"head": "Part", "k": "abc", "why": "case 3", "held": ["check", "proofs"]}));
    let v = r.m.view(200).unwrap();
    assert!(matches!(&v.status, ViewStatus::Done { .. }), "{:?}", v.status);
    assert_eq!(v.posts.last().unwrap().text, Text::New("mine: case 3; held: check, proofs".into()));
    r.open_form("mine", json!({"c": "abc"}));
    r.answer(201, "runner", json!({"head": "Part", "k": "def", "why": "case 4", "held": []}));
    let v = r.m.view(201).unwrap();
    assert!(matches!(&v.status, ViewStatus::Done { .. }), "{:?}", v.status);
    assert!(v.posts.iter().all(|p| !matches!(&p.text, Text::New(s) if s.starts_with("mine"))));
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
    assert_eq!(m2.view(201), r.m.view(201));
}

/// A guarded arm covers nothing, so a match whose only arm for a constructor has a `where` is not complete; a guard
/// that is not a Bool and a post naming what is not in scope are refused.
#[test]
fn a_guarded_arm_covers_nothing_and_its_where_is_a_bool() {
    let errs = |src: String| defs_with(&[("mine.kind", src.as_str())]).1;
    let only = GUARD.replace("      Part(k, why, held) => return k\n", "");
    assert!(errs(only).iter().any(|e| e.contains("does not cover")), "a guarded arm alone");
    let not_bool = GUARD.replace("where k == c", "where k");
    assert!(errs(not_bool).iter().any(|e| e.contains("`where` takes a Bool")));
    let out_of_scope = GUARD.replace("{{ held }}", "{{ nothing }}");
    assert!(errs(out_of_scope).iter().any(|e| e.contains("not in scope")));
}
