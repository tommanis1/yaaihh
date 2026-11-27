//! What an agent's post asks the owner, as passages of the post (design-two-systems §16, 2): the watcher's call
//! copies each ask and the options offered for it from the post; they are looked up here and stored as where they
//! stand. The post's text is never changed; a passage not found is thrown away.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An ask marked on a post: AT, the ask's span; OPTIONS, the spans of the choices offered for it. Offsets are bytes
/// of the post's words as resolved (what the place says, decoded; a new text's own bytes), half-open: what a surface
/// slices the resolved text by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub at: [usize; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<[usize; 2]>,
}

/// The marks for ITEMS (`[{ask, options: [..]}]`, the call's passages) in TEXT, and the passages not found. Each ask
/// is looked up in the whole text (its first occurrence); each option inside its ask first, then anywhere. An item
/// whose ask is not found goes, options and all; an empty passage is not found. The options of the post never overlap
/// (one that overlaps an option kept before it is dropped, as not found), and each mark's are in order of place
/// (paper-research m1396: the call gives options at a different grain from run to run).
pub fn spans(text: &str, items: &Value) -> (Vec<Mark>, Vec<String>) {
    let mut marks = Vec::new();
    let mut lost = Vec::new();
    let mut kept: Vec<[usize; 2]> = Vec::new();
    for it in items.as_array().into_iter().flatten() {
        let ask = it.get("ask").and_then(Value::as_str).unwrap_or("");
        let Some(a) = text.find(ask).filter(|_| !ask.is_empty()) else {
            lost.push(ask.to_string());
            continue;
        };
        let end = a + ask.len();
        let mut options = Vec::new();
        for o in it.get("options").and_then(Value::as_array).into_iter().flatten() {
            let o = o.as_str().unwrap_or("");
            let b = if o.is_empty() { None } else { text[a..end].find(o).map(|b| b + a).or_else(|| text.find(o)) };
            match b.map(|b| [b, b + o.len()]).filter(|s| !kept.iter().any(|k| s[0] < k[1] && k[0] < s[1])) {
                Some(s) => {
                    kept.push(s);
                    options.push(s);
                }
                None => lost.push(o.to_string()),
            }
        }
        options.sort();
        marks.push(Mark { at: [a, end], options });
    }
    (marks, lost)
}
