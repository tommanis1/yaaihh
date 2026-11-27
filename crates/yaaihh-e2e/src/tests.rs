//! The runner's own reading: steps, a tree's shown texts, where a press lands, and what an expectation holds.

use crate::look::{find, shown};
use crate::{expect, Step, Target};
use serde_json::json;

#[test]
fn steps_read_as_a_person_says_them_and_others_are_refused() {
    assert_eq!("tap Settings".parse::<Step>(), Ok(Step::Tap(Target::Text("Settings".into()))));
    assert_eq!("tap  This project's settings ".parse::<Step>(), Ok(Step::Tap(Target::Text("This project's settings".into()))));
    assert_eq!("tap @new-agent".parse::<Step>(), Ok(Step::Tap(Target::Key("new-agent".into()))));
    assert_eq!("type hello there".parse::<Step>(), Ok(Step::Type("hello there".into())));
    assert_eq!("key Enter".parse::<Step>(), Ok(Step::Key("Enter".into())));
    assert_eq!("wait 1.5".parse::<Step>(), Ok(Step::Wait(1.5)));
    assert_eq!("look".parse::<Step>(), Ok(Step::Look));
    for bad in ["tap", "tap @", "wait", "wait 0", "wait 500", "wait soon", "look again", "open the window", ""] {
        assert!(bad.parse::<Step>().is_err(), "{bad:?} was read as a step");
    }
}

#[test]
fn only_texts_on_screen_are_seen() {
    let tree = json!({ "viewport": [800.0, 600.0], "elements": [
        { "text": "Settings", "rect": [10.0, 20.0, 100.0, 20.0], "opacity": 1.0 },
        { "text": "  ", "rect": [10.0, 40.0, 100.0, 20.0], "opacity": 1.0 },
        { "text": "Faded", "rect": [10.0, 60.0, 100.0, 20.0], "opacity": 0.0 },
        { "text": "Below", "rect": [10.0, 900.0, 100.0, 20.0], "opacity": 1.0 },
        { "text": "Leaving", "rect": [10.0, 80.0, 100.0, 20.0], "opacity": 1.0, "exiting": true },
        { "text": "Who may do what", "key": "policy-head", "rect": [200.0, 100.0, 300.0, 30.0], "opacity": 1.0 },
        { "rect": [0.0, 0.0, 800.0, 600.0] }
    ]});
    let s = shown(&tree);
    let texts: Vec<&str> = s.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, ["Settings", "Who may do what"]);
    assert_eq!(s[1].center, (350.0, 115.0));
    assert_eq!(s[1].key.as_deref(), Some("policy-head"));
}

#[test]
fn a_press_lands_on_the_exact_text_first_then_one_containing_it() {
    let tree = json!({ "viewport": [800.0, 600.0], "elements": [
        { "text": "This project's settings", "rect": [0.0, 0.0, 200.0, 20.0] },
        { "text": "Settings", "rect": [0.0, 100.0, 100.0, 20.0], "key": "nav-settings" },
    ]});
    let s = shown(&tree);
    assert_eq!(find(&s, &Target::Text("Settings".into())), Some((50.0, 110.0)));
    assert_eq!(find(&s, &Target::Text("project's".into())), Some((100.0, 10.0)));
    assert_eq!(find(&s, &Target::Key("nav-settings".into())), Some((50.0, 110.0)));
    assert_eq!(find(&s, &Target::Text("Map".into())), None);
}

#[test]
fn an_expectation_holds_only_when_every_line_is_shown_and_no_absent_one_is() {
    let seen = "Settings\nWho may do what\nOn";
    assert_eq!(expect(seen, "Settings\nWho may do what"), Ok(()));
    assert_eq!(expect(seen, "Settings\n!Reading…"), Ok(()));
    assert_eq!(expect("Settings\nReading…", "Settings\n!Reading…"), Err(vec!["!Reading…".to_string()]));
    assert_eq!(expect(seen, "Map\nOn"), Err(vec!["Map".to_string()]));
    assert!(expect(seen, "  \n").is_err(), "an expectation of nothing never holds");
    assert!(expect(seen, "!Reading…").is_err(), "nor one of absences alone, which a blank screen meets");
}
