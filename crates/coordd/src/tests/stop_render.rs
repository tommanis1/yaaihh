//! The step a stopped ticket gives (paper-research N-101 p934; docket-machine's machine/stop.rs), as the one asked is
//! handed it, on a dev coord's record as it is. Ignored: it reads DOCKET_STOP_COORD (read only) with the project folder
//! DOCKET_STOP_PROJECT, and prints each stopped ticket's step, rendered.

#[test]
#[ignore]
fn the_steps_of_the_stopped_tickets_of_a_coords_record() {
    let coord = std::env::var("DOCKET_STOP_COORD").expect("DOCKET_STOP_COORD");
    let project = std::env::var("DOCKET_STOP_PROJECT").ok();
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let t = crate::ticket::Tickets::load(&coord, &base, project.as_deref(), 1);
    for e in &t.errors {
        println!("load error: {e}");
    }
    let m = &t.m;
    let places = crate::policy::Places { base, project, coord, ..Default::default() };
    for x in m.tickets.values() {
        let docket_machine::Status::Stopped { raised, to, by, .. } = &x.status else { continue };
        println!("{} stopped at {}; asked: {to}, in {}", m.id(x.t), raised.exc, m.id(*by));
        for d in m.due(to).into_iter().filter(|d| d.t == *by) {
            let view = m.view(d.t).expect("the stop's ticket");
            let (text, _) = crate::render::delivery(&places, &d, &view, false, false, false, false, None).expect("rendered");
            println!("--- {} to {to}:\n{text}\n---", d.id);
        }
    }
}
