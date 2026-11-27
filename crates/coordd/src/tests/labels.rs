//! The labels `coord label` gives, on a dev coord's record as it is (paper-research N-101 p800: every ticket, three
//! steps, three posts, each agent, the hand-in's commit). Ignored: it reads DOCKET_LABEL_COORD (read only) with the
//! project folder DOCKET_LABEL_PROJECT, and the repositories DOCKET_LABEL_REPOS (`a=/path,b=/path`).

#[test]
#[ignore]
fn the_labels_of_a_coords_record() {
    let coord = std::env::var("DOCKET_LABEL_COORD").expect("DOCKET_LABEL_COORD");
    let project = std::env::var("DOCKET_LABEL_PROJECT").ok();
    let repos: Vec<(String, String)> = std::env::var("DOCKET_LABEL_REPOS").unwrap_or_default().split(',').filter_map(|r| r.split_once('=').map(|(n, p)| (n.to_string(), p.to_string()))).collect();
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let t = crate::ticket::Tickets::load(&coord, &base, project.as_deref(), 1);
    println!("load errors: {}", t.errors.len());
    let m = &t.m;
    let mut ids: Vec<String> = m.tickets.keys().map(|n| m.id(*n)).collect();
    let mut agents = Vec::new();
    for x in m.tickets.values() {
        for (n, (_, w)) in &x.step_calls {
            ids.push(format!("{}.s{n}", m.id(x.t)));
            if !agents.contains(w) {
                agents.push(w.clone());
            }
        }
        ids.extend(x.posts.iter().map(|p| format!("{}.p{}", m.id(x.t), p.n)));
    }
    ids.extend(agents.iter().cloned());
    ids.extend(std::env::var("DOCKET_LABEL_COMMITS").unwrap_or_default().split(',').filter(|c| !c.is_empty()).map(str::to_string));
    let mut out: Vec<coord_types::Label> = ids.iter().map(|id| crate::label::of(m, &agents, id)).collect();
    let commits: Vec<(usize, Option<String>, String)> = out.iter().enumerate().filter(|(_, l)| l.is.is_empty()).map(|(i, l)| match l.id.split_once(':') {
        Some((r, s)) => (i, Some(r.to_string()), s.to_string()),
        None => (i, None, l.id.clone()),
    }).collect();
    crate::label::fill_commits(&mut out, &crate::label::subjects(&commits, &repos));
    for l in out {
        println!("{}\t{}\t{}{}", l.id, l.is, l.label, l.on.map(|o| format!("   (on {o})")).unwrap_or_default());
    }
}
