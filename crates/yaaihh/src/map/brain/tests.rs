//! The brain's satellites: what each says, at a moment of the time bar, where the agents' nodes lie, and a frame's cost.

use super::*;

/// deep.rs's made-up passes (pr's that spoke, w2's that ended Quiet, pr's second still at work), as research's
/// snapshot holds them, with pr and w2 among its agents, each brain bound and on.
pub(crate) fn with_passes(s: &mut App) -> Vec<coord_types::TicketView> {
    let (_, passes) = super::super::deep::tests::brain_passes();
    let p = &mut s.projects[0];
    for name in ["pr", "w2"] {
        let mut a = p.live.agents[0].clone();
        a.name = name.to_string();
        // its last pass, as coordd says it (`Machine::brain_bound`): the newest of the agent's
        let last = passes.iter().filter(|v| v.form.get("agent").and_then(serde_json::Value::as_str) == Some(name)).max_by_key(|v| v.n);
        a.brain = Some(coord_types::BrainView { on: true, since: None, last_pass: last.map(|v| v.opened_at.clone()), last_id: last.map(|v| v.id.clone()), ready: Vec::new() });
        p.live.agents.push(a);
    }
    for v in &passes {
        p.live.tickets.insert(v.n, v.clone());
    }
    passes
}

fn app() -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = crate::view_cost::test_app();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let snap = super::super::made_up::research(chrono::Utc::now().timestamp());
    (p.live.tickets, p.live.agents) = (snap.tickets, snap.agents);
    s.projects = vec![p];
    (held, s)
}

/// Each pass as it stands: pr's first spoke to it, w2's ended with nothing said, pr's second is at work at the step
/// it waits on; and the satellite says the newest at work, else what the last came to, else that it is off.
#[test]
fn a_satellite_says_what_its_brain_is_doing_or_what_its_last_pass_came_to() {
    let (_held, mut s) = app();
    let passes = with_passes(&mut s);
    let sats = satellites(&s, 0);
    let by = |name: &str| sats.iter().find(|x| x.agent == name).unwrap_or_else(|| panic!("{name}'s satellite")).clone();
    let (pr, w2) = (by("pr"), by("w2"));
    assert_eq!(pr.passes.iter().map(|p| (p.id, p.came)).collect::<Vec<_>>(), [(passes[0].id.as_str(), Came::Intervened), (passes[2].id.as_str(), Came::Working)]);
    assert_eq!(w2.passes.iter().map(|p| p.came).collect::<Vec<_>>(), [Came::Quiet]);
    let step = passes[2].waits_on.first().map(|w| w.call.clone()).expect("pr's second pass waits on a step");
    assert_eq!(words(&pr), format!("brain: {step}…"));
    assert_eq!(words(&w2), "brain: quiet");
    // pr's at work gone: its first, which spoke, is what it says
    let done = Satellite { passes: pr.passes[..1].to_vec(), ..pr.clone() };
    assert_eq!(words(&done), "brain: intervened");
    // off: what it says once no pass is at work, and at work still while one is
    assert_eq!(words(&Satellite { on: false, ..done.clone() }), "brain: off");
    assert_eq!(words(&Satellite { on: false, ..pr.clone() }), format!("brain: {step}…"));
    assert_eq!(words(&Satellite { passes: Vec::new(), ..done }), "brain: no pass yet");
    // an agent with no brain bound has no satellite
    assert_eq!(sats.len(), 2, "pr's and w2's alone: {:?}", sats.iter().map(|x| &x.agent).collect::<Vec<_>>());
}

/// At a moment of the time bar: only the passes opened by then, each as it stood then.
#[test]
fn at_a_moment_a_satellite_counts_the_passes_opened_by_then_as_they_stood() {
    let (_held, mut s) = app();
    let passes = with_passes(&mut s);
    let secs = |t: &str| docket_machine::machine::secs(t).unwrap();
    // before any pass: none
    s.cloud.at = Some(secs(&passes[0].opened_at) - 1);
    assert!(satellites(&s, 0).iter().all(|x| x.passes.is_empty()));
    // at pr's first's opening: at work, its step not known then
    s.cloud.at = Some(secs(&passes[0].opened_at));
    let pr = satellites(&s, 0).into_iter().find(|x| x.agent == "pr").unwrap();
    assert_eq!(pr.passes.iter().map(|p| (p.came, p.step)).collect::<Vec<_>>(), [(Came::Working, None)]);
    assert_eq!(words(&pr), "brain: at work…");
}

/// What the brain's step costs on a frame where the snapshot or the moment changed (canvas.rs `brain_step`: `satellites`,
/// then `changes`; on any other frame it reads nothing), on research's made-up snapshot with 2,000 passes of pr's and
/// w2's brains besides: the mean and the worst of 500, printed.
/// `yaaihh-tests map::brain::tests::a_frames_brain_step_costs --ignored --exact --nocapture`.
#[test]
#[ignore]
fn a_frames_brain_step_costs() {
    let (_held, mut s) = app();
    let passes = with_passes(&mut s);
    let p = &mut s.projects[0];
    let top = p.live.tickets.keys().max().copied().unwrap_or(0);
    for k in 0..2_000u64 {
        let mut v = passes[(k % 2) as usize].clone();
        v.n = top + 1 + k;
        v.id = format!("B-{}", v.n);
        p.live.tickets.insert(v.n, v);
    }
    let base = super::super::cloud::tests::base();
    let (mut g, _) = super::super::cloud::cloud(&base, None, "project:research", "research", &mut Default::default()).unwrap();
    place(&mut g, 0, &satellites(&s, 0)).unwrap();
    println!("{} tickets in the snapshot, {} nodes", s.projects[0].live.tickets.len(), g.nodes().len());
    let mut took = Vec::new();
    for _ in 0..500 {
        let t = std::time::Instant::now();
        let sats = satellites(&s, 0);
        assert!(!changes(&g, 0, &sats));
        took.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let mean = took.iter().sum::<f64>() / took.len() as f64;
    println!("the brain's step on a change: {mean:.3} ms mean, {:.3} ms worst, of {} steps", took.iter().copied().fold(0.0, f64::max), took.len());
}

/// The agents' nodes lie in the room left of the threads, in a column, inside the project, and move no thread; the pass
/// each satellite speaks of lies inside its agent's node; placed again with nothing new, nothing changes.
#[test]
fn the_agents_nodes_lie_left_of_the_threads_and_move_none() {
    let (_held, mut s) = app();
    with_passes(&mut s);
    let base = super::super::cloud::tests::base();
    let (mut g, _) = super::super::cloud::cloud(&base, None, "project:research", "research", &mut Default::default()).unwrap();
    let before: Vec<Rect> = g.nodes().iter().map(|n| n.rect).collect();
    let sats = satellites(&s, 0);
    assert!(changes(&g, 0, &sats));
    assert!(place(&mut g, 0, &sats).unwrap());
    assert_eq!(g.nodes()[..before.len()].iter().map(|n| n.rect).collect::<Vec<_>>(), before, "no node moved");
    let left = g.children(0).iter().filter(|c| !ours(&g, **c)).map(|c| g.node(*c).rect.x).fold(f64::INFINITY, f64::min);
    let agents: Vec<u32> = g.children(0).iter().copied().filter(|c| ours(&g, *c)).collect();
    assert_eq!(agents.iter().map(|a| g.node(*a).id.as_str()).collect::<Vec<_>>(), ["agent:pr", "agent:w2"]);
    for a in &agents {
        let r = g.node(*a).rect;
        assert!(r.right() < left && g.node(0).rect.holds(&r), "{} left of the threads, in the project", g.node(*a).id);
    }
    // inside each, a node for each of its passes from when it opened; the agent's history is their coming
    let secs = |t: &str| docket_machine::machine::secs(t).unwrap();
    for (a, sat) in agents.iter().zip(&sats) {
        let kids = g.children(*a);
        assert_eq!(kids.iter().map(|c| g.node(*c).id.clone()).collect::<Vec<_>>(), sat.passes.iter().map(|p| format!("brain:{}", p.id)).collect::<Vec<_>>());
        assert!(kids.iter().all(|c| ours(&g, *c) && g.node(*c).kind == Kind::Brain && g.node(*a).rect.holds(&g.node(*c).rect)));
        assert_eq!(kids.iter().map(|c| g.node(*c).born).collect::<Vec<_>>(), sat.passes.iter().map(|p| secs(p.opened)).collect::<Vec<_>>());
        assert_eq!(g.node(*a).events, kids.iter().map(|c| NodeEvent { at: g.node(*c).born, what: Change::Child(*c) }).collect::<Vec<_>>());
    }
    assert_eq!(g.node(agents[0]).note, words(&sats[0]));
    assert!(!changes(&g, 0, &sats), "placed again with nothing new, nothing changes");
    assert!(!place(&mut g, 0, &sats).unwrap());
}
