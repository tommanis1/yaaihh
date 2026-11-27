//! The window's look (the flavour's `look.toml`, paper-research N-866 and N-412 post 1107): how the window is drawn,
//! as data the running window takes when the file changes, so an agent or the owner changes how it looks, and what it
//! says, by a checked commit of text and without a new window. One look for the whole window (N-866.p6, 6): the
//! window's own controls are not a project's.
//!
//! The vocabulary is closed: each key names one part, each value is a word from a short list, a number in bounds, a
//! colour as `#rrggbb`, or one line of plain text. Nothing in it is a path, a link, markup, a command or a request.
//! **A look says how a thing is drawn, never whether it is there** (N-866.p6, 1): no `none`, no size or colour that
//! draws a thing below what he can see.
//!
//! One check, [`check`], with no IO: coordd's text-change runs it before a look is written, and the window runs it on
//! what coordd sends. A key the check does not know is refused, so a look written for a newer window says so instead
//! of being half drawn. A key left out keeps the built-in value, and the built-in look is the window as it is without
//! a look.
//!
//! ```toml
//! [colour]              # both modes; [colour.dark] and [colour.light] for one of them. A mode that gives one
//!                       # colour gives all seven, so each is checked against what it is drawn with
//! page = "#101114"      # the page under the text
//! read = "#e8e8ea"      # what he reads (body text): contrast 4.5 against page and against layer
//! support = "#a0a1a6"   # who, when, a count: 3 against page and against layer
//! quiet = "#6c6d72"     # a hint, a dim id: 2 against page
//! layer = "#1a1b1f"     # a picked row, a card, a popover
//! border = "#2a2b30"    # 1.1 against page, so a line drawn stays a line
//! attention = "#e5484d" # the one accent, "this needs you": 3 against page, and apart from every text colour
//! [type]
//! body = 14             # the read level's size, 11–20
//! small = 12            # support and quiet, 9–16, at most body
//! [words]               # what the window says, key by key (`WORDS`): one plain line, only the key's {placeholders}
//! to_agent = "To {agent}"
//! [context.mark]        # the compaction mark on an agent's context circle
//! shape = "line"        # line | dot | tick
//! from = 26             # where a line or tick starts, 0–50 from the circle's centre (its box is 100)
//! to = 50               # where it ends, 0–50, at least 4 past from
//! width = 10            # 2–16
//! colour = "attention"  # attention (the one accent) | text | faint
//! [context.click]
//! opens = "context"     # context (the panel under the circle)
//! [list]                # the threads list (its topics and the threads under them)
//! row = ["mark", "label", "id"]  # the order of a row's fields: each of mark, label and id once
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The look as checked; every part is there, the built-in value where the text left it out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Look {
    pub context: Context,
    pub colour: Colours,
    pub size: Sizes,
    /// What the window says, by key of [`WORDS`]: the look's text where it gives one.
    pub words: BTreeMap<String, String>,
    #[serde(default)]
    pub list: List,
}

/// `[list]`: the threads list's rows. A look orders a row's fields; it never leaves one out (N-866.p6, 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub row: Vec<Field>,
}

/// A field of a row of the threads list: its mark (the dot where it waits on him or is new), its label, its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    Mark,
    Label,
    Id,
}

impl Default for List {
    /// The built-in row: the mark, the label, the id.
    fn default() -> Self {
        List { row: vec![Field::Mark, Field::Label, Field::Id] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Context {
    pub mark: Mark,
    pub click: Opens,
}

/// The compaction mark on the context circle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub shape: Shape,
    pub from: f32,
    pub to: f32,
    pub width: f32,
    pub colour: Colour,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    /// A line across the ring, from `from` to `to`.
    Line,
    /// A round dot on the ring, `width` across.
    Dot,
    /// A short line outside the ring, from `from` to `to`, drawn thin.
    Tick,
}

/// The mark's colour as a word of the window's look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Colour {
    /// The one accent: this needs you.
    Attention,
    /// The text's own colour.
    Text,
    /// Faint, as the ring.
    Faint,
}

/// What a click on the circle opens, among what the window already does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Opens {
    Context,
}

/// A colour: red, green, blue.
pub type Rgb = [u8; 3];

/// The colours of one mode, each one the look gives (none: the window's own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tones {
    pub page: Option<Rgb>,
    pub read: Option<Rgb>,
    pub support: Option<Rgb>,
    pub quiet: Option<Rgb>,
    pub layer: Option<Rgb>,
    pub border: Option<Rgb>,
    pub attention: Option<Rgb>,
}

/// `[colour]` for both modes, and `[colour.dark]` / `[colour.light]` over it for one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Colours {
    pub both: Tones,
    pub dark: Tones,
    pub light: Tones,
}

impl Tones {
    /// These tones with OVER's given ones in their place.
    fn under(self, over: Tones) -> Tones {
        Tones {
            page: over.page.or(self.page),
            read: over.read.or(self.read),
            support: over.support.or(self.support),
            quiet: over.quiet.or(self.quiet),
            layer: over.layer.or(self.layer),
            border: over.border.or(self.border),
            attention: over.attention.or(self.attention),
        }
    }
}

impl Colours {
    /// The tones the window draws in its dark mode (DARK) or its light one.
    pub fn of(&self, dark: bool) -> Tones {
        self.both.under(if dark { self.dark } else { self.light })
    }
}

/// `[type]`: the two sizes of the window's text (window-rules §2: no other sizes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sizes {
    pub body: Option<f32>,
    pub small: Option<f32>,
}

/// What the window says that a look may word (`[words]`): its key, the placeholders its text may hold, and the
/// window's own text. A key is added here and drawn through [`say`] in the same build; from then on its wording is
/// live. The window's test `every_word_is_said` finds a key no page says, so none is a word he changes that changes
/// nothing.
pub const WORDS: &[(&str, &[&str], &str)] = &[
    ("version", &["version", "build"], "{version} · build {build}"),
    ("waits_on_you", &[], "Waits on you"),
    ("asked_no_answer", &[], "Asked, no answer yet"),
    ("with_you", &[], "With you"),
    ("all_active", &[], "All active"),
    ("ended_today", &[], "Ended today"),
    ("to_agent", &["agent"], "To {agent}"),
    ("new_thread_with", &["agent"], "New thread with {agent}"),
    ("nothing_with", &["agent"], "Nothing between you and {agent} yet."),
];

/// The longest a text of `[words]` may be, in characters.
pub const WORDS_MOST: usize = 200;

/// What the window says for KEY: the look's text, else its own (`WORDS`), with each `{name}` of ARGS put in once. A
/// value put in is never read again for placeholders, so no text can bring in another key's, nor grow by itself
/// (docket-core3, N-412 post 1119, 3). A key not in `WORDS` says itself.
pub fn say(look: &Look, key: &str, args: &[(&str, &str)]) -> String {
    let own = WORDS.iter().find(|w| w.0 == key).map(|w| w.2);
    let Some(text) = look.words.get(key).map(String::as_str).or(own) else { return key.to_string() };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}').and_then(|j| args.iter().find(|a| a.0 == &after[..j]).map(|a| (a.1, j))) {
            Some((v, j)) => {
                out.push_str(v);
                rest = &after[j + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

impl Default for Context {
    /// The built-in look: the window as drawn without a look (circle.rs, the owner's N-804.p16 red line).
    fn default() -> Self {
        Context { mark: Mark { shape: Shape::Line, from: 26.0, to: 50.0, width: 10.0, colour: Colour::Attention }, click: Opens::Context }
    }
}

/// Check the look V (the file as JSON, or as TOML read into JSON): the look with the built-in values where V left a
/// key out, or every fault found, each naming its key.
pub fn check(v: &Value) -> Result<Look, Vec<String>> {
    let mut look = Look::default();
    let mut faults = Vec::new();
    let Some(top) = v.as_object() else {
        return Err(vec!["the look is not a table".into()]);
    };
    for (k, v) in top {
        match k.as_str() {
            "context" => context(v, &mut look.context, &mut faults),
            "colour" => colours(v, &mut look.colour, &mut faults),
            "type" => sizes(v, &mut look.size, &mut faults),
            "words" => words(v, &mut look.words, &mut faults),
            "list" => list(v, &mut look.list, &mut faults),
            _ => faults.push(format!("`{k}`: no such part of the look (known: colour, type, words, context, list)")),
        }
    }
    if faults.is_empty() { Ok(look) } else { Err(faults) }
}

fn colours(v: &Value, c: &mut Colours, faults: &mut Vec<String>) {
    let Some(t) = table("colour", v, faults) else { return };
    for (k, v) in t {
        match k.as_str() {
            "dark" => tones("colour.dark", v, &mut c.dark, faults),
            "light" => tones("colour.light", v, &mut c.light, faults),
            _ => tone("colour", k, v, &mut c.both, faults),
        }
    }
    for (dark, mode) in [(true, "dark"), (false, "light")] {
        seen(&c.of(dark), mode, faults);
    }
}

fn tones(key: &str, v: &Value, t: &mut Tones, faults: &mut Vec<String>) {
    let Some(m) = table(key, v, faults) else { return };
    for (k, v) in m {
        tone(key, k, v, t, faults);
    }
}

fn tone(key: &str, k: &str, v: &Value, t: &mut Tones, faults: &mut Vec<String>) {
    let at = format!("{key}.{k}");
    let slot = match k {
        "page" => &mut t.page,
        "read" => &mut t.read,
        "support" => &mut t.support,
        "quiet" => &mut t.quiet,
        "layer" => &mut t.layer,
        "border" => &mut t.border,
        "attention" => &mut t.attention,
        _ => {
            let tables = if key == "colour" { ", and the tables dark and light" } else { "" };
            faults.push(format!("`{at}`: no such colour (known: page, read, support, quiet, layer, border, attention{tables})"));
            return;
        }
    };
    match v.as_str().and_then(rgb) {
        Some(c) => *slot = Some(c),
        None => faults.push(format!("`{at}`: a colour as #rrggbb is wanted, not {v}")),
    }
}

/// `#rrggbb` as its three numbers.
pub fn rgb(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([n(0)?, n(2)?, n(4)?])
}

/// The contrast of A and B, 1 to 21 (WCAG's relative luminance).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let lum = |c: Rgb| {
        let ch = |x: u8| {
            let x = x as f32 / 255.0;
            if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// What the tones T of MODE draw must be seen (window-rules §2). A mode that gives a colour gives all seven: this
/// check knows only what the look says, so the window's own text on the look's page, or the look's text on the
/// window's own layer, would go unchecked (consult-5, N-412 post 2013). Each text colour stands with the page at its
/// level's contrast, `read` and `support` with the layer too; the border stands apart from the page; the accent stands
/// apart from every text colour so it keeps its one meaning.
fn seen(t: &Tones, mode: &str, faults: &mut Vec<String>) {
    let all = [("page", t.page), ("read", t.read), ("support", t.support), ("quiet", t.quiet), ("layer", t.layer), ("border", t.border), ("attention", t.attention)];
    if all.iter().all(|(_, c)| c.is_none()) {
        return;
    }
    let (Some(page), Some(read), Some(support), Some(quiet), Some(layer), Some(border), Some(a)) = (t.page, t.read, t.support, t.quiet, t.layer, t.border, t.attention) else {
        let left: Vec<String> = all.iter().filter(|(_, c)| c.is_none()).map(|(n, _)| format!("`{n}`")).collect();
        faults.push(format!("`colour` ({mode}): a mode that gives a colour gives all seven; {} left out would be the window's own, drawn with yours and not checked", left.join(", ")));
        return;
    };
    let against = [("read", read, "page", page, 4.5), ("support", support, "page", page, 3.0), ("quiet", quiet, "page", page, 2.0), ("attention", a, "page", page, 3.0), ("read", read, "layer", layer, 4.5), ("support", support, "layer", layer, 3.0), ("border", border, "page", page, 1.1)];
    for (name, c, under, u, least) in against {
        if contrast(c, u) < least {
            faults.push(format!("`colour.{name}` ({mode}): its contrast with `{under}` is {:.2}, and at least {least} is wanted so he can see it", contrast(c, u)));
        }
    }
    for (name, c) in [("read", read), ("support", support), ("quiet", quiet)] {
        if a.iter().zip(c.iter()).map(|(x, y)| x.abs_diff(*y) as u32).sum::<u32>() < 90 {
            faults.push(format!("`colour.attention` ({mode}): too near `{name}`; the accent means \"this needs you\" and must stand apart from the text"));
        }
    }
}

fn sizes(v: &Value, s: &mut Sizes, faults: &mut Vec<String>) {
    let Some(t) = table("type", v, faults) else { return };
    for (k, v) in t {
        let key = format!("type.{k}");
        match k.as_str() {
            "body" => s.body = number(&key, v, 11.0, 20.0, faults),
            "small" => s.small = number(&key, v, 9.0, 16.0, faults),
            _ => faults.push(format!("`{key}`: no such size (known: body, small)")),
        }
    }
    if let (Some(b), Some(sm)) = (s.body, s.small) {
        if sm > b {
            faults.push(format!("`type.small` ({sm}) must be at most `type.body` ({b})"));
        }
    }
}

fn words(v: &Value, w: &mut BTreeMap<String, String>, faults: &mut Vec<String>) {
    let Some(t) = table("words", v, faults) else { return };
    for (k, v) in t {
        let key = format!("words.{k}");
        let Some((_, names, _)) = WORDS.iter().find(|x| x.0 == k) else {
            faults.push(format!("`{key}`: the window says nothing by that name"));
            continue;
        };
        let Some(text) = v.as_str() else {
            faults.push(format!("`{key}`: a line of text is wanted, not {v}"));
            continue;
        };
        if text.trim().is_empty() {
            faults.push(format!("`{key}`: empty; a look words a thing, it does not take it away"));
        } else if text.chars().count() > WORDS_MOST {
            faults.push(format!("`{key}`: {} characters, and at most {WORDS_MOST} are wanted", text.chars().count()));
        } else if text.chars().any(char::is_control) {
            faults.push(format!("`{key}`: one line of plain text is wanted, with no line breaks or control characters"));
        } else if let Some(bad) = holes(text).into_iter().find(|h| !h.as_deref().is_some_and(|h| names.contains(&h))) {
            let known = if names.is_empty() { "it takes none".to_string() } else { format!("it takes {}", names.iter().map(|n| format!("{{{n}}}")).collect::<Vec<_>>().join(", ")) };
            faults.push(format!("`{key}`: {} is not one of its placeholders ({known})", bad.map_or("a lone { or }".to_string(), |b| format!("{{{b}}}"))));
        } else {
            w.insert(k.clone(), text.to_string());
        }
    }
}

/// The placeholders in TEXT, in order: `Some(name)` for each `{name}`, None for a brace that opens or closes none.
fn holes(text: &str) -> Vec<Option<String>> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        match (rest.find('{'), rest.find('}')) {
            (None, None) => return out,
            (Some(i), Some(j)) if i < j => {
                let name = &rest[i + 1..j];
                out.push((!name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')).then(|| name.to_string()));
                rest = &rest[j + 1..];
            }
            _ => {
                out.push(None);
                return out;
            }
        }
    }
}

fn list(v: &Value, l: &mut List, faults: &mut Vec<String>) {
    let Some(t) = table("list", v, faults) else { return };
    for (k, v) in t {
        if k != "row" {
            faults.push(format!("`list.{k}`: no such key (known: row)"));
            continue;
        }
        let Some(a) = v.as_array() else {
            faults.push(format!("`list.row`: a list of the row's fields is wanted (mark, label, id), not {v}"));
            continue;
        };
        // each field once, and all of them: a look orders a row, it does not take a field away; each fault names its
        // field (paper-research N-1446 p1799)
        let before = faults.len();
        let mut row = Vec::new();
        for x in a {
            let f = match word("list.row", x, &["mark", "label", "id"], faults) {
                Some("mark") => Field::Mark,
                Some("label") => Field::Label,
                Some("id") => Field::Id,
                _ => continue,
            };
            if row.contains(&f) {
                faults.push(format!("`list.row`: `{}` twice; each field is drawn once", x.as_str().unwrap_or_default()));
            } else {
                row.push(f);
            }
        }
        for (name, f) in [("mark", Field::Mark), ("label", Field::Label), ("id", Field::Id)] {
            if !row.contains(&f) {
                faults.push(format!("`list.row`: `{name}` left out; a look orders the row's fields, it does not take one away (each of mark, label, id once)"));
            }
        }
        if faults.len() == before {
            l.row = row;
        }
    }
}

fn context(v: &Value, c: &mut Context, faults: &mut Vec<String>) {
    let Some(t) = table("context", v, faults) else { return };
    for (k, v) in t {
        match k.as_str() {
            "mark" => mark(v, &mut c.mark, faults),
            "click" => {
                let Some(t) = table("context.click", v, faults) else { continue };
                for (k, v) in t {
                    match k.as_str() {
                        "opens" => {
                            word("context.click.opens", v, &["context"], faults);
                        }
                        _ => faults.push(format!("`context.click.{k}`: no such key (known: opens)")),
                    }
                }
            }
            _ => faults.push(format!("`context.{k}`: no such part (known: mark, click)")),
        }
    }
}

fn mark(v: &Value, m: &mut Mark, faults: &mut Vec<String>) {
    let Some(t) = table("context.mark", v, faults) else { return };
    for (k, v) in t {
        let key = format!("context.mark.{k}");
        match k.as_str() {
            "shape" => {
                if let Some(w) = word(&key, v, &["line", "dot", "tick"], faults) {
                    m.shape = match w {
                        "line" => Shape::Line,
                        "dot" => Shape::Dot,
                        _ => Shape::Tick,
                    };
                }
            }
            "from" => m.from = number(&key, v, 0.0, 50.0, faults).unwrap_or(m.from),
            "to" => m.to = number(&key, v, 0.0, 50.0, faults).unwrap_or(m.to),
            "width" => m.width = number(&key, v, 2.0, 16.0, faults).unwrap_or(m.width),
            "colour" => {
                if let Some(w) = word(&key, v, &["attention", "text", "faint"], faults) {
                    m.colour = match w {
                        "attention" => Colour::Attention,
                        "text" => Colour::Text,
                        _ => Colour::Faint,
                    };
                }
            }
            _ => faults.push(format!("`{key}`: no such key (known: shape, from, to, width, colour)")),
        }
    }
    // a line or tick shorter than he can see is no mark (N-866.p6, 1)
    if matches!(m.shape, Shape::Line | Shape::Tick) && m.to - m.from < 4.0 {
        faults.push(format!("`context.mark`: `to` ({}) must be at least 4 past `from` ({}), so the mark is seen", m.to, m.from));
    }
}

fn table<'a>(key: &str, v: &'a Value, faults: &mut Vec<String>) -> Option<&'a serde_json::Map<String, Value>> {
    let t = v.as_object();
    if t.is_none() {
        faults.push(format!("`{key}`: a table is wanted"));
    }
    t
}

fn word<'a>(key: &str, v: &Value, known: &[&'a str], faults: &mut Vec<String>) -> Option<&'a str> {
    let w = v.as_str().and_then(|s| known.iter().find(|k| **k == s).copied());
    if w.is_none() {
        faults.push(format!("`{key}`: one of {} is wanted, not {v}", known.join(", ")));
    }
    w
}

fn number(key: &str, v: &Value, least: f32, most: f32, faults: &mut Vec<String>) -> Option<f32> {
    match v.as_f64().map(|n| n as f32) {
        Some(n) if (least..=most).contains(&n) => Some(n),
        _ => {
            faults.push(format!("`{key}`: a number from {least} to {most} is wanted, not {v}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_look_is_the_built_in_look() {
        assert_eq!(check(&json!({})), Ok(Look::default()));
    }

    #[test]
    fn a_dot_in_the_texts_colour_checks_and_keeps_what_it_leaves_out() {
        let l = check(&json!({"context": {"mark": {"shape": "dot", "colour": "text", "width": 12}}})).unwrap();
        assert_eq!((l.context.mark.shape, l.context.mark.colour, l.context.mark.width, l.context.mark.from), (Shape::Dot, Colour::Text, 12.0, 26.0));
        assert_eq!(l.context.click, Opens::Context);
    }

    #[test]
    fn an_unknown_key_a_word_off_the_list_and_a_number_out_of_bounds_are_each_named() {
        let e = check(&json!({"context": {"mark": {"shape": "star", "width": 40, "svg": "<path/>"}, "click": {"opens": "https://x"}}, "theme": {}})).unwrap_err();
        for k in ["`context.mark.shape`", "`context.mark.width`", "`context.mark.svg`", "`context.click.opens`", "`theme`"] {
            assert!(e.iter().any(|f| f.starts_with(k)), "{k} in {e:?}");
        }
    }

    /// A look says how a thing is drawn, never whether it is there (N-866.p6, 1).
    #[test]
    fn nothing_and_none_and_a_mark_too_short_to_see_are_refused() {
        assert!(check(&json!({"context": {"mark": {"shape": "none"}}})).is_err());
        assert!(check(&json!({"context": {"click": {"opens": "nothing"}}})).is_err());
        assert!(check(&json!({"context": {"mark": {"from": 40, "to": 30}}})).is_err());
        assert!(check(&json!({"context": {"mark": {"from": 30, "to": 32}}})).is_err());
        assert!(check(&json!({"context": {"mark": {"shape": "dot", "from": 40, "to": 30}}})).is_ok());
        assert!(check(&json!({"words": {"with_you": "  "}})).is_err(), "an empty text takes a thing away");
        assert!(check(&json!({"list": {"row": ["mark", "label"]}})).is_err(), "a row with its id left out");
        assert!(check(&json!({"list": {"row": ["label", "label", "id"]}})).is_err(), "a field twice in place of one");
    }

    /// A whole palette for each mode, every contrast met: the doc's dark one and a light one.
    fn dark() -> Value {
        json!({"page": "#101114", "read": "#e8e8ea", "support": "#a0a1a6", "quiet": "#6c6d72", "layer": "#1a1b1f", "border": "#2a2b30", "attention": "#e5484d"})
    }
    fn light() -> Value {
        json!({"page": "#ffffff", "read": "#111111", "support": "#555555", "quiet": "#8a8a8a", "layer": "#f2f2f4", "border": "#d0d0d4", "attention": "#c8102e"})
    }
    /// V with KEY set to HEX, or taken out.
    fn with(mut v: Value, key: &str, hex: &str) -> Value {
        v[key] = json!(hex);
        v
    }
    fn without(mut v: Value, key: &str) -> Value {
        v.as_object_mut().unwrap().remove(key);
        v
    }

    #[test]
    fn colours_check_against_the_page_and_the_accent_stands_apart() {
        let mut both = dark();
        both["light"] = light();
        let l = check(&json!({"colour": both})).unwrap();
        assert_eq!(l.colour.of(true).read, Some([0xe8, 0xe8, 0xea]));
        assert_eq!(l.colour.of(false).read, Some([0x11, 0x11, 0x11]), "the light table over both");
        assert_eq!(l.colour.of(false).attention, Some([0xc8, 0x10, 0x2e]));
        let l = check(&json!({"colour": {"dark": dark()}})).unwrap();
        assert_eq!(l.colour.of(false), Tones::default(), "a mode the look gives nothing for is the window's own");
        let e = check(&json!({"colour": with(dark(), "read", "#202124")})).unwrap_err();
        assert!(e.iter().any(|f| f.starts_with("`colour.read` (dark): its contrast with `page`")), "{e:?}");
        assert!(check(&json!({"colour": with(dark(), "attention", "#e8e8e0")})).unwrap_err().iter().any(|f| f.contains("too near `read`")));
        assert!(check(&json!({"colour": {"page": "red"}})).unwrap_err().iter().any(|f| f.starts_with("`colour.page`: a colour as #rrggbb")));
        assert!(check(&json!({"colour": {"accent": "#ff0000"}})).is_err());
    }

    /// consult-5's wrong on 02de3136 (N-412 post 2013): a page alone, or a text alone, leaves the window's own colours
    /// drawn with it unchecked, so it is refused in each mode it reaches; a light layer under light text is refused,
    /// and so is a border in the page's own colour.
    #[test]
    fn a_page_alone_a_layer_he_cannot_read_on_and_a_border_not_seen_are_refused() {
        let e = check(&json!({"colour": {"page": "#ffffff"}})).unwrap_err();
        assert!(e.iter().any(|f| f.starts_with("`colour` (dark): a mode that gives a colour gives all seven") && f.contains("`read`") && f.contains("`layer`")), "{e:?}");
        assert!(e.iter().any(|f| f.starts_with("`colour` (light)")), "both modes: {e:?}");
        assert!(check(&json!({"colour": {"read": "#e8e8ea"}})).is_err(), "a text alone");
        assert!(check(&json!({"colour": {"dark": without(dark(), "support")}})).unwrap_err().iter().any(|f| f.contains("`support`")));
        let e = check(&json!({"colour": {"dark": with(dark(), "layer", "#f2f2f4")}})).unwrap_err();
        assert!(e.iter().any(|f| f.starts_with("`colour.read` (dark): its contrast with `layer`")), "{e:?}");
        let e = check(&json!({"colour": {"dark": with(dark(), "border", "#101114")}})).unwrap_err();
        assert!(e.iter().any(|f| f.starts_with("`colour.border` (dark): its contrast with `page`")), "{e:?}");
    }

    #[test]
    fn sizes_are_in_bounds_and_small_is_at_most_body() {
        assert_eq!(check(&json!({"type": {"body": 15, "small": 12}})).unwrap().size, Sizes { body: Some(15.0), small: Some(12.0) });
        assert!(check(&json!({"type": {"body": 4}})).is_err());
        assert!(check(&json!({"type": {"body": 12, "small": 14}})).is_err());
        assert!(check(&json!({"type": {"huge": 30}})).is_err());
    }

    /// A text holds only its key's placeholders, each put in once; a value put in is never read again
    /// (docket-core3, N-412 post 1119, 3).
    #[test]
    fn words_take_only_their_placeholders_and_a_value_is_put_in_once() {
        let l = check(&json!({"words": {"to_agent": "Write to {agent}", "with_you": "Mine"}})).unwrap();
        assert_eq!(say(&l, "to_agent", &[("agent", "{agent}{agent}")]), "Write to {agent}{agent}");
        assert_eq!(say(&l, "with_you", &[]), "Mine");
        assert_eq!(say(&Look::default(), "version", &[("version", "0.1"), ("build", "21")]), "0.1 · build 21");
        for bad in [json!({"to_agent": "To {who}"}), json!({"with_you": "{agent} and you"}), json!({"with_you": "a { b"}), json!({"with_you": "two\nlines"}), json!({"nowhere": "x"}), json!({"with_you": 3})] {
            assert!(check(&json!({ "words": bad })).is_err(), "{bad}");
        }
        assert!(check(&json!({"words": {"with_you": "x".repeat(WORDS_MOST + 1)}})).is_err());
    }

    /// `[list] row` orders the threads list's fields (N-1104 p113, head 4): any order of the three, each once; a word
    /// that is no field is named, and so is a field left out or given twice.
    #[test]
    fn a_row_is_its_three_fields_in_the_order_the_look_gives() {
        assert_eq!(Look::default().list.row, [Field::Mark, Field::Label, Field::Id]);
        assert_eq!(check(&json!({"list": {"row": ["id", "mark", "label"]}})).unwrap().list.row, [Field::Id, Field::Mark, Field::Label]);
        let e = check(&json!({"list": {"row": ["mark", "label", "when"]}})).unwrap_err();
        assert!(e.iter().any(|f| f.starts_with("`list.row`: one of mark, label, id is wanted, not \"when\"")), "{e:?}");
        assert!(check(&json!({"list": {"gap": 4}})).unwrap_err().iter().any(|f| f.starts_with("`list.gap`")));
        assert!(check(&json!({"list": {"row": "id"}})).is_err());
        // each fault names its field: the one left out, the one given twice, the word that is none
        let named = |row: Value, says: &str| check(&json!({"list": {"row": row}})).unwrap_err().iter().any(|f| f.starts_with(says));
        assert!(named(json!(["mark", "label"]), "`list.row`: `id` left out"));
        assert!(named(json!(["mark", "label", "id", "label"]), "`list.row`: `label` twice"));
        let e = check(&json!({"list": {"row": ["when", "mark", "mark"]}})).unwrap_err();
        for says in ["`list.row`: one of mark, label, id is wanted, not \"when\"", "`list.row`: `mark` twice", "`list.row`: `label` left out", "`list.row`: `id` left out"] {
            assert!(e.iter().any(|f| f.starts_with(says)), "{says}: {e:?}");
        }
    }

    /// docket-core3's sixth condition (N-412 post 1119): the look goes whole through an `Event`, alone and in a
    /// snapshot; a snapshot with none carries no `look` key, and one from a coordd before it reads as none.
    #[test]
    fn the_look_goes_whole_through_an_event_and_a_snapshot_without_one_has_no_key() {
        let v = json!({"colour": {"page": "#101114", "read": "#e8e8ea"}, "words": {"to_agent": "To {agent}"}, "type": {"body": 14.5}});
        for body in [crate::Body::Look(v.clone()), crate::Body::Snapshot(crate::Snapshot { look: Some(v.clone()), ..Default::default() })] {
            let e = crate::Event { seq: 3, body };
            let back: crate::Event = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
            match back.body {
                crate::Body::Look(x) => assert_eq!(x, v),
                crate::Body::Snapshot(s) => assert_eq!(s.look, Some(v.clone())),
                _ => panic!("another body came back"),
            }
        }
        let none = serde_json::to_value(crate::Snapshot::default()).unwrap();
        assert!(none.get("look").is_none(), "no key when there is no look");
        let old: crate::Snapshot = serde_json::from_value(none).unwrap();
        assert_eq!(old.look, None);
    }
}
