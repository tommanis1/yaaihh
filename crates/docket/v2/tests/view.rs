//! `view`: what holds as it stood after the first k committed steps, stated falses given back with stated trues, the
//! past kept across a reload and read under the rules now loaded, what a reload left out taken back and made again
//! with the steps before it, no commit while a past state is viewed (8.1, 11).

use docket_v2::check::load;
use docket_v2::norm::Core;
use docket_v2::run::norms::Norms;
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::read;

/// `Lit("l1")` is the module's own instance: `Snuff` states it false, `Clear` takes away what is stated of it.
const LAMPS: &str = r#"
module t
Fact Person Identified by String
Fact Lit(lamp: String)
Fact Watched(lamp: String, by: Person)
Fact Seen(lamp: String)
  Holds when Lit(lamp) && Exists p : Watched(lamp, p)
Act Watch(by: Person, lamp: String)
  Creates Watched(lamp, by)
Act Snuff(by: Person, lamp: String)
  Terminates Lit(lamp)
Act Clear(by: Person, lamp: String)
  Obfuscates Lit(lamp)
+Person("ann"). +Person("bob"). +Lit("l1").
"#;

const STEPS: [(&str, &str, &str); 4] = [("Watch", "ann", "l1"), ("Snuff", "ann", "l1"), ("Clear", "ann", "l1"), ("Watch", "bob", "l2")];

fn inst(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("t.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

fn core_of(text: &str) -> Core {
    core_as(text, Core::checked())
}

fn core_as(text: &str, mut core: Core) -> Core {
    let l = load(vec![read(text).unwrap()]).unwrap();
    core.load(&l.norms).unwrap();
    core
}

fn perform(core: &mut Core, (act, by, lamp): (&str, &str, &str)) {
    let s = core.step(&inst(act, &[by, lamp]), Some(&Value::text(by))).unwrap();
    core.commit(s);
}

/// Whether each instance the steps make, end or work out holds.
fn answers(core: &mut Core) -> Vec<bool> {
    let asked = [inst("Lit", &["l1"]), inst("Seen", &["l1"]), inst("Watched", &["l1", "ann"]), inst("Watched", &["l2", "bob"])];
    asked.iter().map(|i| core.holds(i)).collect()
}

/// The answers of a core of TEXT that committed the first K steps and nothing else.
fn rebuilt(text: &str, k: usize) -> Vec<bool> {
    let mut core = core_of(text);
    for s in &STEPS[..k] {
        perform(&mut core, *s);
    }
    answers(&mut core)
}

/// Viewed back and forth, the core answers as one rebuilt from its first k steps: the module's own instance stated
/// false after `Snuff` and holding again after `Clear`. Reloaded with another rule while a past is viewed, it goes on
/// from the present and reads every past under the new rule.
#[test]
fn a_viewed_state_answers_as_one_rebuilt_from_its_first_steps() {
    assert_eq!((rebuilt(LAMPS, 2)[0], rebuilt(LAMPS, 3)[0]), (false, true), "a Terminates wins over the + line");
    let mut core = core_of(LAMPS);
    for s in STEPS {
        perform(&mut core, s);
    }
    for k in [2, 0, 3, 1, 4, 2] {
        core.view(Some(k));
        assert_eq!(answers(&mut core), rebuilt(LAMPS, k), "after {k} steps");
    }

    let changed = LAMPS.replace("Holds when Lit(lamp) && Exists p : Watched(lamp, p)", "Holds when Lit(lamp)");
    core.load(&load(vec![read(&changed).unwrap()]).unwrap().norms).unwrap();
    assert_eq!(answers(&mut core), rebuilt(&changed, 4), "the reload goes on from the present");
    for k in [0, 2, 1, 3] {
        core.view(Some(k));
        assert_eq!(answers(&mut core), rebuilt(&changed, k), "after {k} steps, reloaded");
    }
    core.view(None);
    perform(&mut core, ("Snuff", "bob", "l1"));
    assert!(!core.holds(&inst("Seen", &["l1"])));
}

/// A type a reload's text does not declare loses its instances at that reload, and declared again later starts with
/// none: viewed before that reload, the instances the steps stored are there; back in the present, they are not.
#[test]
fn a_type_left_out_by_a_reload_and_declared_again_starts_empty_whatever_is_viewed() {
    let mut core = core_of(LAMPS);
    perform(&mut core, STEPS[0]);
    perform(&mut core, STEPS[1]);
    let without = LAMPS
        .replace("Fact Watched(lamp: String, by: Person)\n", "")
        .replace("  Holds when Lit(lamp) && Exists p : Watched(lamp, p)\n", "")
        .replace("Act Watch(by: Person, lamp: String)\n  Creates Watched(lamp, by)\n", "");
    for text in [without.as_str(), LAMPS] {
        core.load(&load(vec![read(text).unwrap()]).unwrap().norms).unwrap();
    }
    perform(&mut core, STEPS[3]);
    let present = answers(&mut core);
    assert_eq!(present, [false, false, false, true], "Watched starts empty when declared again");
    let seen = [1, 0, 2, 1, 3].map(|k| {
        core.view(Some(k));
        answers(&mut core)
    });
    let before = vec![true, true, true, false];
    assert_eq!(seen, [before.clone(), vec![true, false, false, false], vec![false; 4], before, present.clone()]);
    core.view(None);
    assert_eq!(answers(&mut core), present, "looking at a past changes nothing in the present");
}

/// Checked against the whole worked out guided, the core answers as one checked against every binding, viewed too.
#[test]
fn a_core_checked_guided_answers_as_one_checked_every_way() {
    let (mut guided, mut every) = (core_as(LAMPS, Core::checked_guided()), core_of(LAMPS));
    for s in STEPS {
        perform(&mut guided, s);
        perform(&mut every, s);
        assert_eq!(answers(&mut guided), answers(&mut every), "after {s:?}");
    }
    guided.view(Some(1));
    every.view(Some(1));
    assert_eq!(answers(&mut guided), answers(&mut every), "viewed after one step");
}

#[test]
#[should_panic(expected = "while a past state is viewed")]
fn a_commit_while_a_past_state_is_viewed_is_refused() {
    let mut core = core_of(LAMPS);
    perform(&mut core, STEPS[0]);
    core.view(Some(0));
    perform(&mut core, STEPS[1]);
}

#[test]
fn reads_are_zeros_when_nothing_was_read() {
    let mut core = core_of(LAMPS);
    assert_eq!(core.reads(), [0; 32]);
    core.holds(&inst("Lit", &["l1"]));
    assert_ne!(core.reads(), [0; 32]);
    assert_eq!(core.reads(), [0; 32]);
}
