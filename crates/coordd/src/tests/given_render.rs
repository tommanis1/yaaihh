//! The steps due on a coord's record, as their agents are handed them (`agents` filled in): for reading a step's
//! gives on a real record (paper-research N-101 p980, the assign steps of the stand-in's C-1). Ignored: it reads the
//! coord DOCKET_GIVEN_COORD (a copy: nothing in it is changed but what loading it writes) and prints each step due.

#[test]
#[ignore]
fn the_steps_due_on_a_coords_record_as_handed() {
    let coord = std::env::var("DOCKET_GIVEN_COORD").expect("DOCKET_GIVEN_COORD");
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let mut core = crate::core::Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    for e in &core.tickets.errors {
        println!("load error: {e}");
    }
    let names: Vec<String> = core.agents.keys().cloned().collect();
    for n in names {
        let due = core.tickets.m.due(&n);
        if due.is_empty() {
            continue;
        }
        if let Some(a) = core.agents.get_mut(&n) {
            a.live.pid = Some(1);
        }
        for (d, view) in core.ticket_handing(&n, due) {
            let (text, _) = crate::render::delivery(&core.places(), &d, &view, false, false, false, false, None).expect("rendered");
            println!("--- {} to {n}:\n{text}\n---", d.id);
        }
    }
}
