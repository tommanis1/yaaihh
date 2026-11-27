//! A call's template choosing its text by `{{ match }}` (paper-research N-101 p722): checked when the file is loaded
//! (the matched value is of a type with alternatives, each alternative has exactly one arm, an arm's names are in
//! scope in it only, a hole reading another arm's is refused with its line), and the minder's `mind.call`
//! (tickets-patterns plan/minder/moment/) rendered for each of its four moments against the design's examples.

use crate::policy::call::Calls;
use serde_json::{Value, json};

/// A folder holding FILE with TEXT, loaded; it is removed after.
fn load(name: &str, text: &str) -> Calls {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("coordd-moment-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.call"), text).unwrap();
    let calls = Calls::load(&[dir.display().to_string()], &Default::default()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    calls
}

const MOMENT: &str = "type Moment =
  | Dispatch                /// before
  | Check(ran: Text)        /// a check
  | HandIn(report: Text, commit: Ref)
";

/// The call file: MOMENT, then a call whose template is TEMPLATE (each line indented under `template`).
fn with(template: &str) -> String {
    let body: String = template.lines().map(|l| format!("    {l}\n")).collect();
    format!("{MOMENT}\ncall mind(moment: Moment, after: Text, n: Text?) -> Rest | Say(text: Text)\n  task mind\n  template\n{body}")
}

/// The line of the call file `with(TEMPLATE)` that holds TEXT.
fn line_of(template: &str, text: &str) -> usize {
    with(template).lines().position(|l| l.contains(text)).unwrap() + 1
}

/// Why `with(TEMPLATE)` is refused.
fn refused(template: &str) -> String {
    let calls = load("refused", &with(template));
    assert!(!calls.defs.contains_key("mind"), "taken: {template}");
    calls.errors.join("\n")
}

const GOOD: &str = "Look.
{{ match moment }}
{{ Dispatch }}
Nothing yet.
{{ Check(ran) }}
Ran: {{ ran }}
{{ HandIn(report, commit) }}
{{ commit }}: {{ report }}
{{ end }}
After: {{ after }}";

#[test]
fn a_match_renders_the_arm_of_the_values_alternative_its_lines_taken_with_its_tags() {
    let calls = load("good", &with(GOOD));
    assert!(calls.errors.is_empty(), "{:?}", calls.errors);
    let r = |moment: Value| calls.render_step("mind", &json!({"moment": moment, "after": "built", "n": null})).unwrap();
    assert_eq!(r(json!({"head": "Dispatch"})), "Look.\nNothing yet.\nAfter: built");
    assert_eq!(r(json!({"head": "Check", "ran": "cargo test: ok"})), "Look.\nRan: cargo test: ok\nAfter: built");
    assert_eq!(r(json!({"head": "HandIn", "report": "done", "commit": "a=8f28eb2"})), "Look.\na=8f28eb2: done\nAfter: built");
    // an arm names its alternative's first fields, in order, as a program's pattern does, by any names
    let calls = load("names", &with(&GOOD.replace("{{ HandIn(report, commit) }}\n{{ commit }}: {{ report }}", "{{ HandIn(r) }}\nreport {{ r }}").replace("{{ Check(ran) }}\nRan: {{ ran }}", "{{ Check }}\nRan")));
    assert!(calls.errors.is_empty(), "{:?}", calls.errors);
    let r = |moment: Value| calls.render_step("mind", &json!({"moment": moment, "after": "built", "n": null})).unwrap();
    assert_eq!(r(json!({"head": "HandIn", "report": "done", "commit": "a=8f28eb2"})), "Look.\nreport done\nAfter: built");
    assert_eq!(r(json!({"head": "Check", "ran": "x"})), "Look.\nRan\nAfter: built");
    // inline, the tags take nothing with them
    let calls = load("inline", &with("[{{ match moment }}{{ Dispatch }}d{{ Check(ran) }}c {{ ran }}{{ HandIn }}h{{ end }}]"));
    assert!(calls.errors.is_empty(), "{:?}", calls.errors);
    assert_eq!(calls.render_step("mind", &json!({"moment": {"head": "Check", "ran": "x"}, "after": "", "n": null})).unwrap(), "[c x]");
}

#[test]
fn a_match_on_a_value_with_no_alternatives_is_refused() {
    let t = GOOD.replace("{{ match moment }}", "{{ match after }}");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ match after }}}} is on a Text: it takes a value of a type with alternatives", line_of(&t, "{{ match after }}"))), "{e}");
}

#[test]
fn a_match_without_an_arm_for_each_alternative_is_refused() {
    let t = GOOD.replace("{{ HandIn(report, commit) }}\n{{ commit }}: {{ report }}\n", "");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ match moment }}}} has no arm for HandIn", line_of(&t, "{{ match moment }}"))), "{e}");
}

#[test]
fn a_second_arm_for_one_alternative_is_refused() {
    let t = GOOD.replace("{{ end }}", "{{ Dispatch }}\nagain\n{{ end }}");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ Dispatch }}}}: a second arm for Dispatch in {{{{ match moment }}}}", line_of(&t, "again") - 1)), "{e}");
}

#[test]
fn an_arm_for_no_alternative_is_refused() {
    let t = GOOD.replace("{{ Dispatch }}", "{{ Asking(question) }}");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ Asking }}}}: Asking is not an alternative of `moment` (its alternatives: Dispatch, Check, HandIn)", line_of(&t, "{{ Asking(question) }}"))), "{e}");
}

#[test]
fn an_arm_naming_more_fields_than_its_alternative_has_is_refused() {
    let t = GOOD.replace("{{ Check(ran) }}", "{{ Check(ran, more) }}");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ Check }}}}: Check has 1 fields, the arm names 2", line_of(&t, "{{ Check(ran, more) }}"))), "{e}");
}

#[test]
fn a_hole_reading_another_arms_field_is_refused_with_its_line() {
    let t = GOOD.replace("Ran: {{ ran }}", "Ran: {{ ran }}, for {{ commit }}");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ commit }}}} reads a field of the HandIn arm, in the Check arm of {{{{ match moment }}}}", line_of(&t, "for {{ commit }}"))), "{e}");
    // after the match, no arm's name is in scope
    let t = GOOD.replace("After: {{ after }}", "After: {{ ran }}");
    let e = refused(&t);
    assert!(e.contains("{{ ran }}: no `ran` here (the call's parameters: after, moment, n)"), "{e}");
}

#[test]
fn text_before_a_matchs_first_arm_and_a_match_without_end_are_refused() {
    let t = GOOD.replace("{{ match moment }}\n", "{{ match moment }}\nstray\n");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: text between {{{{ match }}}} and its first arm", line_of(&t, "{{ Dispatch }}"))), "{e}");
    let e = refused(&GOOD.replace("{{ end }}\n", ""));
    assert!(e.contains("an {{ if }}, {{ for }} or {{ match }} without its {{ end }}"), "{e}");
}

/// The text of TEXT between A and B.
fn between<'a>(text: &'a str, a: &str, b: &str) -> &'a str {
    let from = text.find(a).unwrap_or_else(|| panic!("{a}")) + a.len();
    let to = text[from..].find(b).unwrap_or_else(|| panic!("{b}")) + from;
    &text[from..to]
}

/// The minder's call as designed, rendered for each moment: the three examples word for word, and the fourth,
/// `Asking`, from the same inputs as the check's; and with errors (the `if` over three lines, paper-research N-101
/// p773).
#[test]
fn the_minders_call_renders_each_moment_as_the_design_shows() {
    let root = format!("{}/testdata/moment", env!("CARGO_MANIFEST_DIR"));
    let calls = Calls::load(&[root.clone()], &Default::default()).unwrap();
    assert!(calls.errors.is_empty(), "{:?}", calls.errors);
    let example = |f: &str| std::fs::read_to_string(format!("{root}/{f}")).unwrap();
    let args = |text: &str, moment: Value| {
        json!({
            "moment": moment,
            "after": between(text, "What will be true when it is built (the design's After):\n", "\n\nThe changes it is making, in order:"),
            "changes": between(text, "The changes it is making, in order:\n", "\n\nWhat each change serves:"),
            "why": between(text, "What each change serves:\n", "\n\nGive the file"),
            "errors": null,
        })
    };
    let render = |a: Value| calls.render_step("mind", &a).unwrap() + "\n";
    let mut wrong = Vec::new();
    let mut compare = |what: &str, got: String, want: &str| {
        if got != want {
            let at = got.lines().zip(want.lines()).position(|(g, w)| g != w).unwrap_or(got.lines().count().min(want.lines().count()));
            wrong.push(format!("{what}: differs from line {}:\n got: {:?}\nwant: {:?}", at + 1, got.lines().skip(at).take(3).collect::<Vec<_>>(), want.lines().skip(at).take(3).collect::<Vec<_>>()));
        }
    };
    let dispatch = example("example-dispatch-M17.txt");
    compare("Dispatch", render(args(&dispatch, json!({"head": "Dispatch"}))), &dispatch);
    let check = example("example-check-M01.txt");
    let ran = between(&check, "The implementer has just run a check:\n", "\nRead the changes so far.");
    compare("Check", render(args(&check, json!({"head": "Check", "ran": ran}))), &check);
    let hand_in = example("example-hand-in-M05.txt");
    let report = between(&hand_in, "you answer. The implementer's report:\n", "\nLook for:");
    compare("HandIn", render(args(&hand_in, json!({"head": "HandIn", "report": report, "commit": "829c313b"}))), &hand_in);
    // the fourth: the check's, with the question's lines in place of the check's
    let at = check.find("The implementer has just run a check:").unwrap();
    let back = check.find("\n\nWhat will be true").unwrap();
    let question = "Is a successor kept where its path condition is unsat?";
    let want = format!("{}The implementer is about to ask the owner:\n{question}\nLook for: a question to the owner that is already answered.{}", &check[..at], &check[back..]);
    compare("Asking", render(args(&check, json!({"head": "Asking", "question": question}))), &want);
    // with errors: they stand after the why, a blank line before the closing sentence
    let mut a = args(&dispatch, json!({"head": "Dispatch"}));
    a["errors"] = json!("cargo test: 2 failed");
    let why = between(&dispatch, "What each change serves:\n", "\n\nGive the file");
    compare("errors", render(a), &dispatch.replace(&format!("{why}\n\nGive"), &format!("{why}\nWhat came back from its last hand-in:\ncargo test: 2 failed\n\nGive")));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A `Ref` in a template reads as an answer writes it, not as JSON (paper-research N-101 p850; the live run's hand-in
/// step read `The commit {"a":"02e0410"} is about to go on the line`): one commit as it is, commits by repository
/// `a 02e0410, b 1a2b3c4`, a list of them one per line, an optional one inside its `if`.
#[test]
fn a_ref_in_a_template_reads_as_an_answer_writes_it() {
    let t = "call show(c: Ref, cs: [Ref], o: Ref?, m: Moment) -> Rest\n  task show\n  template\n    one {{ c }}\n    {{ cs }}\n    {{ if o }}o {{ o }}{{ end }}\n    {{ match m }}{{ Dispatch }}{{ Check(ran) }}{{ HandIn(r, commit) }}at {{ commit }}{{ end }}\n";
    let calls = load("ref", &format!("{MOMENT}\n{t}"));
    assert!(calls.errors.is_empty(), "{:?}", calls.errors);
    let two = json!({"a": "02e0410", "b": "1a2b3c4"});
    let r = calls.render_step("show", &json!({"c": two, "cs": ["9f8e7d6", two], "o": {"a": "02e0410"}, "m": {"head": "HandIn", "report": "r", "commit": {"a": "02e0410"}}})).unwrap();
    assert_eq!(r, "one a 02e0410, b 1a2b3c4\n9f8e7d6\na 02e0410, b 1a2b3c4\no a 02e0410\nat a 02e0410");
    let r = calls.render_step("show", &json!({"c": "02e0410", "cs": [], "o": null, "m": {"head": "Dispatch"}})).unwrap();
    assert_eq!(r, "one 02e0410\n\n\n");
}

/// A value with alternatives put whole in a hole is refused at load, with its line: `{{ match }}` chooses its text
/// (paper-research N-101 p880).
#[test]
fn a_hole_holding_a_sum_whole_is_refused_with_its_line() {
    let t = GOOD.replace("After: {{ after }}", "After: {{ moment }}");
    let e = refused(&t);
    assert!(e.contains(&format!("line {}: {{{{ moment }}}}: a value with alternatives is not written whole: name one of its fields, or choose by `match`", line_of(&t, "After: {{ moment }}"))), "{e}");
}
