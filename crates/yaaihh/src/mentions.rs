//! Mentions (the owner N-446.p1, paper-research's read-back post 2 there): an id written in a post's words, a ticket
//! (`N-412`, `Q-445`) or a post (`N-412.p30`), is drawn as its label and its id, and a click goes there. The words are
//! never changed; only what is drawn of them. Inside code (a fence, an indented block, `inline code`) and inside a
//! link already made (a path) an id stays as written. An id the app has no label for stays as written too.

use regex::Regex;
use std::sync::OnceLock;

/// The scheme of a mention's link, which `open` takes.
const SCHEME: &str = "yaaihh-ref:";
/// The end of the link that draws a mention's id (small and dim), after its label's.
const ID: &str = "#id";

/// How a Markdown draws mentions: the label in the text colour with a thin dim underline, its id small and dim; a
/// hover underlines them more. Other links keep the default look.
pub fn look(k: &rune_kit::Kit) -> impl Fn(&str) -> Option<(rune::Span, rune::LinkPaint)> + 'static {
    let (base, faint, thin, under, small) = (k.c.text_base, k.c.text_faint, k.c.border_weak, k.c.border_strong, k.ty.small.size);
    move |url: &str| {
        let id = url.strip_prefix(SCHEME)?;
        // the label keeps a thin dim underline, so it has an edge in running words (paper-research N-447.p88)
        let span = if id.ends_with(ID) { rune::Span::new(0, 0).color(faint).size(small) } else { rune::Span::new(0, 0).color(base).underline(thin) };
        Some((span, rune::LinkPaint::new().underline(under)))
    }
}

fn id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([A-Z]-[0-9]+)(\.p[0-9]+)?\b").unwrap())
}

fn agent_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"@([A-Za-z][A-Za-z0-9_-]*[A-Za-z0-9])").unwrap())
}

fn link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]\n]*\]\([^)\n]*\)").unwrap())
}

/// TEXT (Markdown) with each id LABEL knows made a link: `[label · N-412](yaaihh-ref:N-412)`. LABEL gets the
/// ticket's id (a post's thread) and answers its label.
pub fn link(text: &str, label: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            out.push_str(line);
            continue;
        }
        if fence || line.starts_with("    ") || line.starts_with('\t') {
            out.push_str(line);
            continue;
        }
        // outside inline code: the even pieces between backticks
        for (i, piece) in line.split('`').enumerate() {
            if i > 0 {
                out.push('`');
            }
            if i % 2 == 1 {
                out.push_str(piece);
            } else {
                outside_links(piece, &label, &mut out);
            }
        }
    }
    out
}

/// PIECE with its ids made links, but not inside a link it already has. A link the writer gave an id or an agent as
/// its target (`[x](N-12.p3)`, `[x](@name)`) stays one only when LABEL knows it, as the window's own mentions do; else
/// it is its words with the target after them, plain (consult-4, N-585.p34: `[x](@nobody)` opened an agent that is not
/// there; N-585.p44: an id from another project is the one thing he could look up).
fn outside_links(piece: &str, label: &impl Fn(&str) -> Option<String>, out: &mut String) {
    let mut at = 0;
    for m in link_re().find_iter(piece) {
        ids(&piece[at..m.start()], label, out);
        let whole = m.as_str();
        match whole.rsplit_once("](").map(|(words, t)| (&words[1..], t.trim_end_matches(')').trim())) {
            Some((words, t)) if named(t) && label(t.split_once(".p").map_or(t, |(id, _)| id)).is_none() => out.push_str(&format!("{words} ({t})")),
            _ => out.push_str(whole),
        }
        at = m.end();
    }
    ids(&piece[at..], label, out);
}

fn ids(s: &str, label: &impl Fn(&str) -> Option<String>, out: &mut String) {
    let mut at = 0;
    for c in id_re().captures_iter(s) {
        let whole = c.get(0).unwrap();
        // not part of a longer word (`x-N-412`, `N-412-b`) nor an @name
        let before = s[..whole.start()].chars().next_back();
        let after = s[whole.end()..].chars().next();
        if before.is_some_and(|ch| ch == '-' || ch == '@' || ch == '/' || ch.is_alphanumeric()) || after.is_some_and(|ch| ch == '-' || ch == '/' || ch.is_alphanumeric()) {
            continue;
        }
        let Some(l) = label(&c[1]) else { continue };
        out.push_str(&agents(&s[at..whole.start()], label));
        let id = whole.as_str();
        let l: String = l.chars().map(|ch| if matches!(ch, '[' | ']') { ' ' } else { ch }).collect();
        // the label in the text colour, the id small and dim after it (N-466 rule 1; `look`); a `|` in the label
        // escaped, so an id in a table's cell does not split the cell (N-1271)
        out.push_str(&format!("[{}]({SCHEME}{id}) [{id}]({SCHEME}{id}{ID})", l.trim().replace('|', "\\|")));
        at = whole.end();
    }
    out.push_str(&agents(&s[at..], label));
}

/// S with each `@NAME` LABEL knows (as `@NAME`) made a link to the agent's page.
fn agents(s: &str, label: &impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    for c in agent_re().captures_iter(s) {
        let whole = c.get(0).unwrap();
        let before = s[..whole.start()].chars().next_back();
        if before.is_some_and(|ch| ch.is_alphanumeric() || ch == '.' || ch == '/') || label(whole.as_str()).is_none() {
            continue;
        }
        out.push_str(&s[at..whole.start()]);
        out.push_str(&format!("[{}]({SCHEME}{})", whole.as_str(), whole.as_str()));
        at = whole.end();
    }
    out.push_str(&s[at..]);
    out
}

/// Whether URL is a bare ticket or post id (`N-12`, `N-12.p3`) or `@agent`, as a writer may give a link's target.
pub fn named(url: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:[A-Z]-[0-9]+(?:\.p[0-9]+)?|@[A-Za-z][A-Za-z0-9_-]*[A-Za-z0-9])$").unwrap()).is_match(url)
}

/// The mention URL names: its ticket's id, number and post. None for another link.
pub fn target(url: &str) -> Option<(String, u64, Option<u32>)> {
    let id = url.strip_prefix(SCHEME)?;
    let id = id.strip_suffix(ID).unwrap_or(id);
    let (t, post) = match id.split_once(".p") {
        Some((t, n)) => (t, Some(n.parse().ok()?)),
        None => (id, None),
    };
    let n = t.split_once('-')?.1.parse().ok()?;
    Some((id.to_string(), n, post))
}

/// A click on URL: a mention opens its thread (at its post); anything else is not one (false).
pub fn open(s: &mut crate::App, url: &str) -> bool {
    // an id or an agent the writer gave as a link's target (`[x](N-12.p3)`, `[x](@docket-core)`): opened as a mention
    // is, on every system (N-763.p1)
    if named(url) {
        return open(s, &format!("{SCHEME}{url}"));
    }
    if let Some(name) = url.strip_prefix(SCHEME).and_then(|r| r.strip_prefix('@')) {
        s.sel = crate::Sel::Agent(name.to_string());
        return true;
    }
    let Some((id, n, post)) = target(url) else { return false };
    crate::find::open_at(s, n, &id, post);
    true
}

/// The tickets that mention one, oldest first, each once: its id, its label, how many of its posts mention it, and the
/// newest of those.
fn by_ticket(lines: &[docket_machine::ViewMention]) -> Vec<(&str, &str, usize, u32)> {
    let mut out: Vec<(&str, &str, usize, u32)> = Vec::new();
    for b in lines {
        match out.iter_mut().find(|x| x.0 == b.by) {
            Some(x) => {
                x.2 += 1;
                x.3 = x.3.max(b.post);
            }
            None => out.push((&b.by, &b.label, 1, b.post)),
        }
    }
    out
}

/// The lines back of a mentioned ticket (coordd's `TicketView.mentioned`, docket-core5), one per ticket that mentions
/// it, oldest first: "↖ mentioned in label N-446.p2", a click to that post; a ticket whose posts mention it more than
/// once is one line, "↖ mentioned 38× in label N-1446 ›", a click to the newest of them (proto-lead N-1446 p87.2: a
/// busy thread opened with forty such lines before its first post). Dim: it does not need him (N-466 rule 1).
pub fn back(lines: &[docket_machine::ViewMention], k: &rune_kit::Kit) -> Option<rune::El> {
    use rune::{col, row, text};
    if lines.is_empty() {
        return None;
    }
    let mut c = col().key("mentioned").w_full().gap(2.0);
    for (by, label, n, last) in by_ticket(lines) {
        let (said, id) = if n == 1 { ("↖ mentioned in".to_string(), format!("{by}.p{last}")) } else { (format!("↖ mentioned {n}× in"), by.to_string()) };
        let r = crate::refs::Ref { label: crate::refs::cut(label, 50), id };
        let mut line = row().gap(6.0).center_y().child(text(said).font(k.ty.small).color(k.c.text_faint)).child(crate::refs::el(&r, k.ty.small, k.c.text_muted, k));
        if n > 1 {
            line = line.child(text("›").font(k.ty.small).color(k.c.text_faint));
        }
        let url = format!("{SCHEME}{by}.p{last}");
        c = c.child(k.interactive(line.key(format!("mentioned-{by}"))).on_press(move |s: &mut crate::App| {
            open(s, &url);
        }));
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(id: &str) -> Option<String> {
        (id == "N-412" || id == "Q-445").then(|| format!("about {id}"))
    }

    /// N-1445's forty "mentioned in" lines (proto-lead N-1446 p87.2) are a line per ticket: N-1104's three posts one
    /// line, N-1446's many another, each opening the newest post that mentions it; a ticket mentioning it once keeps
    /// its post.
    #[test]
    fn a_ticket_mentioning_one_many_times_is_one_line() {
        let m = |by: &str, post: u32, label: &str| serde_json::from_value::<docket_machine::ViewMention>(serde_json::json!({"by": by, "post": post, "label": label, "who": "paper-research", "at": "2026-11-20T19:10:00Z"})).unwrap();
        let mut lines = vec![m("N-1104", 630, "Your one group chat"), m("N-1104", 631, "Your one group chat"), m("N-1104", 632, "Your one group chat")];
        lines.extend((1..=38).map(|p| m("N-1446", p * 3, "0.3 prototype: the owner's fundamentals")));
        lines.push(m("N-1449", 1, "Two of the owner's for you"));
        assert_eq!(by_ticket(&lines), [("N-1104", "Your one group chat", 3, 632), ("N-1446", "0.3 prototype: the owner's fundamentals", 38, 114), ("N-1449", "Two of the owner's for you", 1, 1)]);
    }

    #[test]
    fn an_id_is_drawn_with_its_label_and_code_and_links_are_left() {
        assert_eq!(link("see N-412 and N-412.p30.", known), "see [about N-412](yaaihh-ref:N-412) [N-412](yaaihh-ref:N-412#id) and [about N-412](yaaihh-ref:N-412.p30) [N-412.p30](yaaihh-ref:N-412.p30#id).");
        assert_eq!(link("`N-412` x-N-412 N-999 Q-445", known), "`N-412` x-N-412 N-999 [about Q-445](yaaihh-ref:Q-445) [Q-445](yaaihh-ref:Q-445#id)");
        assert_eq!(link("```\nN-412\n```\n[N-412](a/b.rs)", known), "```\nN-412\n```\n[N-412](a/b.rs)");
    }

    #[test]
    fn a_mention_opens_its_thread_at_its_post() {
        assert_eq!(target("yaaihh-ref:N-412.p30"), Some(("N-412.p30".to_string(), 412, Some(30))));
        assert_eq!(target("yaaihh-ref:Q-445#id"), Some(("Q-445".to_string(), 445, None)));
        assert_eq!(target("src/a.rs"), None);
        let lab = |id: &str| (id == "@docket-core4" || id == "N-412").then(|| "x".to_string());
        assert_eq!(link("ask @docket-core4, not a@b or @nobody", lab), "ask [@docket-core4](yaaihh-ref:@docket-core4), not a@b or @nobody");
    }

    #[test]
    fn an_id_or_agent_given_as_a_link_target_is_a_mention() {
        assert!(named("N-12") && named("N-12.p3") && named("@docket-core"));
        assert!(!named("C-1.s2") && !named("src/a.rs") && !named("yaaihh-ref:N-12") && !named("@") && !named("N-12.p"));
        // one the window does not know is its words alone; one it knows stays a link
        let lab = |id: &str| (id == "@docket-core4" || id == "N-412").then(|| "x".to_string());
        assert_eq!(link("[a](@nobody) [b](N-99999.p2) [c](@docket-core4) [d](N-412.p3) [e](src/a.rs)", lab), "a (@nobody) b (N-99999.p2) [c](@docket-core4) [d](N-412.p3) [e](src/a.rs)");
    }

    #[test]
    fn a_mentioned_ticket_draws_one_line_back_per_mention() {
        let lines: Vec<docket_machine::ViewMention> = serde_json::from_value(serde_json::json!([
            {"by": "N-446", "post": 2, "label": "Track topics first", "who": "paper-research", "at": "2026-11-18T10:00:00Z", "of": 30},
            {"by": "N-447", "post": 1, "label": "Threads: the engine half", "who": "paper-research", "at": "2026-11-18T10:05:00Z"}])).unwrap();
        let k = rune_kit::Kit::neutral(true);
        assert!(back(&lines, &k).is_some());
        assert!(back(&[], &k).is_none());
    }
}
