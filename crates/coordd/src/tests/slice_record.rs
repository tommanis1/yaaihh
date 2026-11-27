//! A post's way into a session on a coord's record, read by coordd's own path for `Request::Raw` (paper-research N-1446
//! p1376, 3: "his p767 → given to paper-research → the 1,604 bytes at that offset, and those bytes hold his words";
//! p1380). Ignored: it reads the coord DOCKET_SLICE_COORD (a copy: nothing in it is changed but what loading it writes)
//! and, for post DOCKET_SLICE_POST and agent DOCKET_SLICE_TO, asks `Core::ticket_slice` and `ticket::slice_words` for
//! each giving in the record that brought that post to that agent. It prints each one's record line, delivery, place
//! (offset and length, not its file), and whether it was served and holds the post's words; never a word of a session
//! or of the post.

#[test]
#[ignore]
fn a_posts_givings_on_a_coords_record_are_served_with_its_words() {
    use std::io::BufRead;
    let (Ok(coord), Ok(post), Ok(to)) = (std::env::var("DOCKET_SLICE_COORD"), std::env::var("DOCKET_SLICE_POST"), std::env::var("DOCKET_SLICE_TO")) else { return };
    let base = std::env::var("DOCKET_SLICE_BASE").unwrap_or_else(|_| format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    let mut core = crate::core::Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    println!("loaded {coord}: {} problem lines", core.tickets.errors.len());
    let (t, n) = docket_machine::post_id(&post).expect("a post's id");
    let p = core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).expect("the post").clone();
    let words = crate::label::read(&p.text).expect("its words").trim().to_string();
    println!("{post}: by {}, {} bytes of words", p.by, words.len());
    let file = core.record_file();
    let mut found = 0;
    for (i, l) in std::io::BufReader::new(std::fs::File::open(&file).unwrap()).lines().enumerate() {
        let l = l.unwrap();
        if !l.contains("\"ev\":\"given\"") || !l.contains("\"where\"") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&l).unwrap();
        if v["t"].as_u64() != Some(t) || v["to"] != to.as_str() {
            continue;
        }
        let delivery = v["delivery"].as_str().unwrap_or_default().to_string();
        let place: docket_machine::Loc = serde_json::from_value(v["where"].clone()).unwrap();
        let id = docket_machine::graph::slice_id(&to, &delivery, i + 1);
        let scope = match core.ticket_slice(&crate::core::Who::Owner, &id) {
            Ok(s) => s,
            Err(e) => {
                println!("line {}: {delivery}: not served: {e}", i + 1);
                continue;
            }
        };
        if !scope.givings.iter().any(|(l, ps)| *l == place && ps.iter().any(|(pid, _)| *pid == post)) {
            continue;
        }
        found += 1;
        match crate::ticket::slice_words(&scope) {
            Ok(text) => println!("line {}: {delivery} to {to}, {} bytes at offset {} of its session: served, {} bytes of text; holds {post}'s words: {}", i + 1, place.len, place.offset, text.len(), text.contains(&words)),
            Err(e) => println!("line {}: {delivery}: refused: {e}", i + 1),
        }
    }
    println!("{found} giving(s) in the record brought {post} to {to}");
    assert!(found > 0, "no giving brought {post} to {to}");
}
