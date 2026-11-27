use super::*;
use serde_json::json;

fn i(ty: &str, a: &str) -> Json {
    json!({"inst": ty, "args": [a]})
}

fn view(facts: &[Json], duties: &[Json], violated: &[Json]) -> View {
    let by = |xs: &[Json]| {
        let mut m = ByType::new();
        xs.iter().for_each(|x| drop(m.entry(ty(x).to_string()).or_default().insert(x.to_string(), x.clone())));
        m
    };
    View { facts: by(facts), duties: by(duties), violated: by(violated) }
}

/// DS merged into one frame, as a subscriber's slot merges them.
fn merged(ds: &[&StepDelta]) -> StepDelta {
    let mut f = Frame::default();
    ds.iter().for_each(|d| f.push(d));
    f.delta()
}

#[test]
fn between_two_views_is_what_moved_alone() {
    let a = view(&[i("t.F", "a"), i("t.F", "b")], &[i("t.D", "1")], &[]);
    let b = view(&[i("t.F", "b"), i("t.F", "c")], &[i("t.D", "2")], &[i("t.D", "2")]);
    let d = between(&a, &b, 7);
    assert_eq!((d.seq, d.created_facts, d.terminated_facts), (7, vec![i("t.F", "c")], vec![i("t.F", "a")]));
    assert_eq!((d.new_duties, d.terminated_duties, d.violations), (vec![i("t.D", "2")], vec![i("t.D", "1")], vec![i("t.D", "2")]));
    assert!(between(&b, &b, 8).is_empty());
}

#[test]
fn merged_deltas_drop_what_came_and_went_and_applied_make_the_later_view() {
    let a = view(&[i("t.F", "a")], &[], &[]);
    let d1 = StepDelta { seq: 1, created_facts: vec![i("t.F", "b")], new_duties: vec![i("t.D", "1")], violations: vec![i("t.D", "1")], ..StepDelta::default() };
    let d2 = StepDelta { seq: 2, terminated_facts: vec![i("t.F", "b"), i("t.F", "a")], terminated_duties: vec![i("t.D", "1")], created_facts: vec![i("t.G", "x")], ..StepDelta::default() };
    let m = merged(&[&d1, &d2]);
    assert_eq!(m, StepDelta { seq: 2, created_facts: vec![i("t.G", "x")], terminated_facts: vec![i("t.F", "a")], ..StepDelta::default() }, "b and the duty came and went");
    let (mut one, mut two) = (a.clone(), a.clone());
    apply(&mut one, &d1);
    apply(&mut one, &d2);
    apply(&mut two, &m);
    assert_eq!(between(&one, &two, 0), StepDelta::default(), "the merged delta makes the same view");
    let only_g = only(&m, Some(&BTreeSet::from(["t.G".to_string()])));
    assert_eq!((only_g.created_facts.len(), only_g.terminated_facts.len()), (1, 0));
    assert_eq!(narrowed(&one, Some(&BTreeSet::from(["t.G".to_string()]))).facts.keys().collect::<Vec<_>>(), ["t.G"]);
}

/// A duty held before the frame, violated and then ended in it, leaves no violation.
#[test]
fn a_held_duty_violated_then_ended_in_one_frame_leaves_no_violation() {
    let base = view(&[], &[i("t.D", "1")], &[]);
    let d1 = StepDelta { seq: 1, violations: vec![i("t.D", "1")], ..StepDelta::default() };
    let d2 = StepDelta { seq: 2, terminated_duties: vec![i("t.D", "1")], ..StepDelta::default() };
    let m = merged(&[&d1, &d2]);
    assert_eq!(m, StepDelta { seq: 2, terminated_duties: vec![i("t.D", "1")], ..StepDelta::default() });
    let (mut one, mut two) = (base.clone(), base.clone());
    apply(&mut one, &d1);
    apply(&mut one, &d2);
    apply(&mut two, &m);
    assert_eq!(between(&one, &two, 0), StepDelta::default(), "the merged frame makes the same view");
    assert!(two.violated.values().all(Held::is_empty) && two.duties.values().all(Held::is_empty));
}

/// A violation cleared while its duty holds leaves every view; cleared and violated again in one frame, nothing moved.
#[test]
fn a_cleared_violation_leaves_the_view_and_one_that_comes_back_in_the_frame_is_no_change() {
    let base = view(&[], &[i("t.D", "1")], &[i("t.D", "1")]);
    let clear = StepDelta { seq: 1, cleared_violations: vec![i("t.D", "1")], ..StepDelta::default() };
    let again = StepDelta { seq: 2, violations: vec![i("t.D", "1")], ..StepDelta::default() };
    let mut one = base.clone();
    apply(&mut one, &clear);
    assert!(one.violated["t.D"].is_empty() && one.duties["t.D"].len() == 1);
    assert_eq!(between(&base, &one, 1), clear, "a whole read says the same");
    let m = merged(&[&clear, &again]);
    assert!(m.is_empty(), "cleared and violated again: {m:?}");
    let m = merged(&[&again, &StepDelta { seq: 3, cleared_violations: vec![i("t.D", "1")], ..StepDelta::default() }]);
    assert!(m.is_empty(), "violated and cleared again: {m:?}");
}

/// A violated duty that ends and comes back unviolated in one frame is cleared, whether it was cleared before it ended
/// or not; one that comes back violated is still violated.
#[test]
fn a_violated_duty_that_ends_and_comes_back_in_one_frame_comes_back_as_it_now_is() {
    let d = i("t.D", "1");
    let base = view(&[], std::slice::from_ref(&d), std::slice::from_ref(&d));
    let end = StepDelta { seq: 1, terminated_duties: vec![d.clone()], ..StepDelta::default() };
    let back = StepDelta { seq: 2, new_duties: vec![d.clone()], ..StepDelta::default() };
    let back_violated = StepDelta { seq: 2, new_duties: vec![d.clone()], violations: vec![d.clone()], ..StepDelta::default() };
    let clear = StepDelta { seq: 1, cleared_violations: vec![d.clone()], ..StepDelta::default() };
    let end2 = StepDelta { seq: 2, terminated_duties: vec![d.clone()], ..StepDelta::default() };
    let back3 = StepDelta { seq: 3, new_duties: vec![d.clone()], ..StepDelta::default() };
    for ds in [vec![&end, &back], vec![&end, &back_violated], vec![&clear, &end2, &back3]] {
        let (mut one, mut two) = (base.clone(), base.clone());
        ds.iter().for_each(|x| apply(&mut one, x));
        apply(&mut two, &merged(&ds));
        assert_eq!(between(&one, &two, 0), StepDelta::default(), "{:?}", merged(&ds));
    }
    assert_eq!(merged(&[&end, &back]).cleared_violations, [d.clone()]);
}

/// A fixed-seed draw, so a failing run comes back the same.
struct Draw(u64);

impl Draw {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

/// Over seeded random states of four facts and four duties, any of them violated, a client given every change in
/// frames of one to six merged has the state each frame ends at, and one given every change alone has each state.
#[test]
fn frames_merged_from_random_changes_make_the_state_each_ends_at() {
    let mut r = Draw(1915);
    let pick = |r: &mut Draw, ty: &str| (0..4).filter(|_| r.below(2) == 1).map(|k| i(ty, &k.to_string())).collect::<Vec<_>>();
    let state = |r: &mut Draw| {
        let duties = pick(r, "t.D");
        let violated: Vec<Json> = duties.iter().filter(|_| r.below(2) == 1).cloned().collect();
        view(&pick(r, "t.F"), &duties, &violated)
    };
    let (mut now, mut seq) = (state(&mut r), 0);
    let (mut one, mut client) = (now.clone(), now.clone());
    let mut cleared = 0;
    for _ in 0..2000 {
        let mut f = Frame::default();
        for _ in 0..=r.below(6) {
            let next = state(&mut r);
            seq += 1;
            let d = between(&now, &next, seq);
            apply(&mut one, &d);
            assert_eq!(between(&one, &next, 0), StepDelta::default(), "each change alone, at {seq}");
            f.push(&d);
            now = next;
        }
        let m = f.delta();
        cleared += usize::from(!m.cleared_violations.is_empty());
        apply(&mut client, &m);
        assert_eq!(between(&client, &now, 0), StepDelta::default(), "the frame up to {seq}: {m:?}");
    }
    assert!(cleared > 100, "only {cleared} of 2000 frames cleared a violation");
}
