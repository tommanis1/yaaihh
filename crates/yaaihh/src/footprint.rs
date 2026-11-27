//! An agent's footprint, in its page's head (N-1933.p1: "When I click an agent I should be able to see its
//! memory and the size it's holding in storage somewhere small"): its process's memory while it runs, its session's
//! transcript and its own folder on disk as coordd reads them (a stat and a walk every 30 s), and its context.

use coord_types::AgentView;
use rune::{El, text};
use rune_kit::Kit;

/// The strip's parts, each only when coordd gave it.
pub fn parts(a: &AgentView) -> Vec<String> {
    let mut out = Vec::new();
    out.extend(a.mem_mb.map(|m| format!("RAM {}", bytes(m.saturating_mul(1 << 20)))));
    out.extend(a.transcript_bytes.map(|b| format!("session {}", bytes(b))));
    out.extend(a.folder_bytes.map(|b| format!("folder {}", bytes(b))));
    let of = crate::circle::point(a).unwrap_or_else(|| crate::circle::whole(a));
    out.extend(a.context.as_ref().map(|c| format!("context {} / {}", crate::circle::tokens(c.used), crate::circle::tokens(of))));
    out
}

/// Bytes as Explorer counts them (of 1024): "812 B", "14 KB", "3.1 MB", "1.5 GB".
pub fn bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let (mut v, mut unit) = (n as f64 / 1024.0, 0);
    // the unit and the decimals by the figure as it is written, rounded: 1023.6 KB reads "1.0 MB", 9.96 KB "10 KB"
    while v >= 1023.5 && unit < 3 {
        (v, unit) = (v / 1024.0, unit + 1);
    }
    let unit = ["KB", "MB", "GB", "TB"][unit];
    if v < 9.95 { format!("{v:.1} {unit}") } else { format!("{v:.0} {unit}") }
}

/// The strip: its parts on one quiet line; none when coordd gave none.
pub fn strip(a: &AgentView, pi: usize, k: &Kit) -> Option<El> {
    let parts = parts(a);
    (!parts.is_empty()).then(|| text(parts.join(" · ")).font(k.ty.small).color(k.c.text_faint).truncate().shrink().key(format!("footprint-{pi}-{}", a.name)))
}

#[cfg(test)]
mod tests {
    use super::bytes;

    #[test]
    fn bytes_read_as_explorer_counts_them() {
        assert_eq!([0, 812, 14 << 10, 3_250_000, 412 << 20, 1500 << 20].map(bytes), ["0 B", "812 B", "14 KB", "3.1 MB", "412 MB", "1.5 GB"]);
        assert_eq!([1_048_575, 10_239, 10_240].map(bytes), ["1.0 MB", "10 KB", "10 KB"]);
    }

    /// N-1933.p1: an agent's page, opened, shows its RAM, its session's and its folder's size and its context
    /// in one line under the head; a stopped one shows what it holds on disk, and no RAM.
    #[test]
    fn an_agents_page_shows_its_footprint_in_its_head() {
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev"]);
        let a = &mut s.projects[0].live.agents[0];
        (a.mem_mb, a.transcript_bytes, a.folder_bytes) = (Some(412), Some(3_250_000), Some(18 << 20));
        a.context = Some(coord_types::AgentContext { used: 84_000, window: Some(350_000), ..Default::default() });
        s.select_agent("dev");
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        let shown = |rt: &rune::runtime::Runtime<crate::App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("footprint-0-dev")).and_then(|e| e.text);
        assert_eq!(shown(&rt).as_deref(), Some("RAM 412 MB · session 3.1 MB · folder 18 MB · context 84k / 350k"));
        let a = &mut rt.state.projects[0].live.agents[0];
        (a.mem_mb, a.context) = (None, None);
        rt.invalidate();
        rt.frame(0.016);
        assert_eq!(shown(&rt).as_deref(), Some("session 3.1 MB · folder 18 MB"));
    }
}
