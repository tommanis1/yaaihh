//! The eight operations. Read: definition, references, type_at, outline. Edit: replace, insert_after,
//! insert_before, rename, remove. Every edit is applied (never refused for breaking typechecking: that is a duty of
//! the policy, at the end of the turn) and answers with the errors and warnings the servers then report.

use crate::index::{Index, Sym, line_of};
use crate::lsp::{self, span};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

const QUIET: Duration = Duration::from_millis(1500);

fn read(p: &Path) -> Result<String> {
    std::fs::read_to_string(p).map_err(|e| anyhow!("reading {}: {e}", p.display()))
}

/// Where a definition's text runs, as byte offsets, and what is uncertain about it.
struct Extent {
    start: usize,
    end: usize,
    /// The definition's other pieces, in order (Agda: a signature first and the clauses later, as in a mutual
    /// group), each with the comment lines directly above it. Empty where the definition is one piece.
    rest: Vec<(usize, usize)>,
    /// Set when an edit of the whole definition cannot be done safely (the reason).
    unsafe_edit: Option<String>,
}

impl Extent {
    fn one(start: usize, end: usize, unsafe_edit: Option<String>) -> Extent {
        Extent { start, end, rest: Vec::new(), unsafe_edit }
    }

    /// Every piece, in order.
    fn pieces(&self) -> Vec<(usize, usize)> {
        let mut v = vec![(self.start, self.end)];
        v.extend(self.rest.iter().copied());
        v
    }

    fn holds(&self, off: usize) -> bool {
        self.pieces().iter().any(|&(a, b)| a <= off && off < b.max(a + 1))
    }
}

/// A symbol's whole definition: the server's range (in Haskell with the type signature above it: HLS's range of a
/// function holds its equations only), and in every language the comment lines directly above it, with no blank
/// line between, where the range leaves them out.
fn extent(ix: &Index, s: &Sym, text: &str) -> Extent {
    let mut e = server_extent(ix, s, text);
    // (a Haskell field starts within its line, after the record's `{` or `,`: comments above it stay where they are)
    if line_start(text, e.start) == e.start {
        e.start = comments_above(text, e.start, ix.lang(s));
    }
    for p in e.rest.iter_mut() {
        p.0 = comments_above(text, p.0, ix.lang(s));
    }
    e
}

/// Where the comments directly above a line start (START, a line's first byte) begin: line comments, and block
/// comments whose first line starts with the opening and whose last line ends with the closing, with no blank line
/// between them and the definition. A pragma (`{-# … #-}`) is not a comment.
fn comments_above(text: &str, start: usize, lang: &str) -> usize {
    let (line, open, close) = if lang == "rust" { ("//", "/*", "*/") } else { ("--", "{-", "-}") };
    let mut start = start;
    while start > 0 {
        let above = line_start(text, start - 1);
        let l = text[above..start - 1].trim();
        if l.starts_with(line) {
            start = above;
            continue;
        }
        if l.ends_with(close) && !l.ends_with("#-}") {
            // up to the line that opens it
            let mut k = above;
            let opened = loop {
                let t = text[k..line_end(text, k)].trim_start();
                if t.contains(open) {
                    break (t.starts_with(open) && !t.starts_with("{-#")).then_some(k);
                }
                if k == 0 {
                    break None;
                }
                k = line_start(text, k - 1);
            };
            if let Some(k) = opened {
                start = k;
                continue;
            }
        }
        break;
    }
    start
}

fn server_extent(ix: &Index, s: &Sym, text: &str) -> Extent {
    let utf8 = ix.enc(s);
    let (start, end) = span(text, &s.range, utf8);
    if ix.lang(s) == "agda" && !s.pieces.is_empty() {
        // agda-ls's pieces: each from its line's start (a definition in a `mutual` block is indented)
        let mut ps = s.pieces.iter().map(|r| {
            let (a, b) = span(text, r, utf8);
            (line_start(text, a), b)
        });
        let (start, end) = ps.next().unwrap();
        return Extent { start, end, rest: ps.collect(), unsafe_edit: None };
    }
    if ix.lang(s) == "agda" {
        // (an agda-ls without agda.ranges) its range runs from the signature to the last clause; where the signature is not in it (the
        // signatures of a mutual group first, the clauses later) an edit of the range would leave a piece behind
        let mut unsafe_edit = None;
        if let Some(sig) = &s.signature {
            let (a, b) = span(text, sig, utf8);
            if a < start || b > end {
                unsafe_edit = Some(format!(
                    "agda-ls's range for it (lines {}–{}) does not hold its signature (line {}): a definition in pieces; edit it as text",
                    line_of(&s.range) + 1,
                    crate::index::end_line_of(&s.range) + 1,
                    line_of(sig) + 1
                ));
            }
        }
        return Extent::one(line_start(text, start), end, unsafe_edit);
    }
    if ix.lang(s) != "haskell" {
        return Extent::one(line_start(text, start), end, None);
    }
    if s.kind == 8 {
        // HLS's range of a record field is its name only: the field runs from its name to the `,` or `}` that ends
        // it (`storeName :: String`), the record's `{` and `,` staying the record's
        return Extent::one(start, field_end(text, end), None);
    }
    let name = s.full.last().cloned().unwrap_or_default();
    let lines: Vec<&str> = text.split('\n').collect();
    let first = line_of(&s.range) as usize;
    let mut top = first;
    let mut unsafe_edit = None;
    // the signature: indented continuation lines, then its first line at column 0
    let mut i = first;
    while i > 0 && lines[i - 1].starts_with([' ', '\t']) && !lines[i - 1].trim().is_empty() {
        i -= 1;
    }
    if i > 0 {
        let l = lines[i - 1];
        let block: String = lines[i - 1..first].join("\n");
        let head = l.split("::").next().unwrap_or("");
        let names: Vec<&str> = head.split(',').map(str::trim).filter(|x| !x.is_empty()).collect();
        if block.contains("::") && names.iter().any(|n| n.trim_start_matches('(').trim_end_matches(')') == name) {
            top = i - 1;
            if names.len() > 1 {
                unsafe_edit = Some(format!("its type signature is shared ({}): edit that line as text", head.trim()));
            }
        }
    }
    let start = lines[..top].iter().map(|l| l.len() + 1).sum::<usize>().min(text.len());
    Extent::one(start, end, unsafe_edit)
}

/// Where a record field that starts before FROM ends: before the `,` or `}` outside brackets that follows it, without
/// the white space and comments before that (`-- ^` lines after a field are the next one's or the record's).
fn field_end(text: &str, from: usize) -> usize {
    let mut depth = 0i32;
    let mut end = text.len();
    let mut chars = text[from..].char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth -= 1,
            '}' if depth > 0 => depth -= 1,
            '-' if chars.peek().is_some_and(|&(_, d)| d == '-') => {
                // a line comment: to its line's end
                while chars.peek().is_some_and(|&(_, d)| d != '\n') {
                    chars.next();
                }
            }
            ',' | '}' if depth == 0 => {
                end = from + i;
                break;
            }
            _ => {}
        }
    }
    // back over white space and whole comment lines
    let mut e = end;
    loop {
        let t = text[from..e].trim_end();
        e = from + t.len();
        let ls = line_start(text, e);
        if ls > from && text[ls..e].trim_start().starts_with("--") {
            e = ls;
        } else {
            break;
        }
    }
    e
}

fn line_start(text: &str, off: usize) -> usize {
    text[..off].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

fn line_end(text: &str, off: usize) -> usize {
    text[off..].find('\n').map(|i| off + i).unwrap_or(text.len())
}

/// The type the server's hover gives at a position: the block that gives OWN its type (`own ::`, `(own) ::`) when the
/// position is a symbol's own name, else the first code block, or the plain text. HLS's hover at a record field holds
/// its record's block first, and that block is sometimes the constructor with its type.
fn hover(ix: &Index, i: usize, file: &Path, pos: (u64, u64), own: Option<&str>) -> Result<Option<String>> {
    let r = ix.server(i).request("textDocument/hover", json!({"textDocument": {"uri": lsp::uri(file)}, "position": {"line": pos.0, "character": pos.1}}))?;
    let c = &r["contents"];
    let raw = match c {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("value").and_then(Value::as_str).unwrap_or("").to_string(),
        Value::Array(a) => a.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x["value"].as_str().unwrap_or("").to_string())).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    // agda-ls: `Store.insert : Nat → Store → Store`, the name first
    if ix.confs[i].lang == "agda" {
        let t = raw.trim();
        if let Some((n, ty)) = t.split_once(" : ") {
            if !n.contains(char::is_whitespace) {
                return Ok(Some(ty.trim().to_string()));
            }
        }
        return Ok(Some(t.to_string()));
    }
    // docket-ls: the declaration's block, then its docs (the declaration's, each parameter's or field's) and its
    // place; all of it, since the docs say what the code does not (a2h-speed, paper-research N-101 p767)
    if ix.confs[i].lang == "docket" {
        return Ok(Some(docket_hover(&raw)));
    }
    // the code blocks; a block that is only a path (rust-analyzer's first, the item's module) is not the type
    let mut blocks = Vec::new();
    let mut rest = raw.as_str();
    while let Some(a) = rest.find("```") {
        let r = &rest[a + 3..];
        let r = &r[r.find('\n').map(|i| i + 1).unwrap_or(0)..];
        let b = r.find("```").unwrap_or(r.len());
        blocks.push(r[..b].trim().to_string());
        rest = &r[(b + 3).min(r.len())..];
    }
    Ok(Some(pick_block(&blocks, own).unwrap_or_else(|| raw.trim().to_string())))
}

/// docket-ls's hover as an agent reads it: the first code block without its fences, then the text after it.
fn docket_hover(raw: &str) -> String {
    let t = raw.trim();
    let Some(a) = t.find("```") else { return t.to_string() };
    let r = &t[a + 3..];
    let r = &r[r.find('\n').map(|i| i + 1).unwrap_or(r.len())..];
    let b = r.find("```").unwrap_or(r.len());
    let after = r[(b + 3).min(r.len())..].trim();
    let block = r[..b].trim();
    if after.is_empty() { block.to_string() } else { format!("{block}\n\n{after}") }
}

/// A Haskell symbol's type when the hover does not give its name one: hs-ls's selection range is the name in the
/// signature, where ghcide's hover is the name alone; its detail is the type as written ("String -> Store").
fn typed_by_detail(s: &Sym, hover: Option<String>) -> Option<String> {
    let Some(own) = s.full.last() else { return hover };
    let gives = |t: &str| t.lines().any(|l| l.trim_start().strip_prefix(own.as_str()).is_some_and(|r| r.trim_start().starts_with("::")));
    if hover.as_deref().is_some_and(gives) {
        return hover;
    }
    match &s.detail {
        Some(d) if matches!(s.kind, 6 | 7 | 8 | 12 | 13 | 14 | 25) && !d.trim().is_empty() => Some(format!("{own} :: {}", d.trim())),
        _ => hover,
    }
}

/// Of a hover's code blocks: the one that gives OWN its type, else the first with more than one word, else the first.
fn pick_block(blocks: &[String], own: Option<&str>) -> Option<String> {
    if let Some(n) = own {
        let gives = |t: &&String| t.lines().any(|l| l.trim_start().strip_prefix(n).or_else(|| l.trim_start().strip_prefix(&format!("({n})"))).is_some_and(|r| r.trim_start().starts_with("::")));
        if let Some(t) = blocks.iter().find(gives) {
            return Some(t.clone());
        }
    }
    blocks.iter().find(|t| !t.is_empty() && t.contains(char::is_whitespace)).or(blocks.first()).cloned()
}

fn sel_pos(s: &Sym) -> (u64, u64) {
    crate::lsp::Ends::start(&s.sel)
}

/// A position at the symbol's name: HLS's selection range of a function is its whole first equation, which starts
/// with the name; elsewhere the selection range is the name itself.
fn name_pos(s: &Sym) -> (u64, u64) {
    sel_pos(s)
}

pub fn definition(ix: &mut Index, name: &str) -> Result<String> {
    let s = ix.resolve(name)?;
    let text = read(&s.file)?;
    let e = extent(ix, &s, &text);
    // an Agda definition's type is its signature as written, unless the signature uses a generalised `variable`:
    // then the server's, with the implicit arguments Agda adds. Asking the server for every type cost about 2 s a call
    // when the hover moved to another module (docket-core5, N-101 p1586), and the type was the written one in 13 of 15
    // read from the commit's record: its type as Agda printed it there, generalised implicits and all; no server
    if let Some(t) = s.typed.clone() {
        let printed = if ix.lang(&s) == "haskell" { "as GHC printed it at the commit" } else { "as Agda printed it at the commit" };
        let how = if ix.stale(&s) { "as of the commit; this module imports a file you changed and is not checked against it yet" } else { printed };
        let mut out = out_place(ix, &s, &text, &e, Some(format!("{t}\n({how})")))?;
        let inst = crate::index::instances_of(ix, &s);
        if !inst.is_empty() {
            out.push_str(&format!("instances ({}): {}\n", inst.len(), inst.join(", ")));
        }
        return Ok(out);
    }
    // A read never waits on the server's load (paper-research2, N-101 p1616): while it loads, a generalised signature
    // too is answered as written, with the variables it leaves to Agda named
    if let Some(w) = written(ix, &s, &text) {
        let free = ix.generalised(s.server, &w);
        if free.is_empty() {
            return out_place(ix, &s, &text, &e, Some(format!("{w}\n(as written: the signature uses no generalised variable)")));
        }
        if !ix.started(s.server) || ix.server(s.server).loading() {
            let note = format!("{w}\n(as written: Agda generalises it over {}, which it leaves unbound; the server has not finished loading this session's files, so it was not asked)", free.join(", "));
            return out_place(ix, &s, &text, &e, Some(note));
        }
    }
    if !ix.started(s.server) {
        // read from its record with no type printed there: the server, started for it
        ix.start(s.server)?;
    }
    let ty = hover(ix, s.server, &s.file, name_pos(&s), s.full.last().map(String::as_str))?;
    let ty = typed_by_detail(&s, ty);
    out_place(ix, &s, &text, &e, ty)
}

/// An Agda symbol's type as its signature (agda-ls's `agda.signatureRange`) writes it in TEXT; none in any other
/// language, or without the signature's range.
fn written(ix: &Index, s: &Sym, text: &str) -> Option<String> {
    // a data type's, a record's or a module's head is not `name : TYPE` alone: the server's, as before
    if ix.lang(s) != "agda" || matches!(s.kind_name.as_deref(), Some("data" | "record" | "module")) {
        return None;
    }
    let (a, b) = span(text, s.signature.as_ref()?, ix.enc(s));
    crate::index::written_type(text.get(a..b)?)
}

/// A definition's head: its full name and kind, its place as file and begin and end line:column (the whole range,
/// as Rascal's `loc` carries it), its type; then its source.
fn out_place(ix: &Index, s: &Sym, text: &str, e: &Extent, ty: Option<String>) -> Result<String> {
    let at = |(a, b): (usize, usize)| {
        let (l0, c0) = lsp::position(text, a, false);
        let (l1, c1) = lsp::position(text, b, false);
        format!("{}:{}-{}:{}", l0 + 1, c0 + 1, l1 + 1, c1 + 1)
    };
    let places: Vec<String> = e.pieces().into_iter().map(at).collect();
    let mut out = format!("{}  ({}), {}:{}\n", s.name(ix.lang(s)), s.kind(), ix.rel(&s.file), places.join(", "));
    if !e.rest.is_empty() {
        out.push_str(&format!("in {} pieces, other definitions between them; below, the pieces one after the other. replace puts the new source's signature at the first piece and its clauses at the second (write the new source as one definition, signature first); remove takes all\n", places.len()));
    }
    if let Some(t) = ty {
        out.push_str(&format!("type: {}\n", t.replace('\n', "\n      ")));
    }
    if let Some(u) = &e.unsafe_edit {
        out.push_str(&format!("an edit of the whole of it would be refused: {u}\n"));
    }
    out.push('\n');
    let src: Vec<&str> = e.pieces().into_iter().map(|(a, b)| &text[a..b]).collect();
    out.push_str(&src.join("\n"));
    out.push('\n');
    Ok(out)
}

/// Every place the server says the symbol is used, each with the symbol it is in and its line.
fn refs(ix: &Index, s: &Sym, with_decl: bool) -> Result<Vec<(PathBuf, Value)>> {
    let pos = name_pos(s);
    ix.server(s.server).index_once(&s.file)?;
    let r = ix.server(s.server).request(
        "textDocument/references",
        json!({"textDocument": {"uri": lsp::uri(&s.file)}, "position": {"line": pos.0, "character": pos.1}, "context": {"includeDeclaration": with_decl}}),
    )?;
    Ok(r.as_array().cloned().unwrap_or_default().into_iter().map(|l| (lsp::path_of(l["uri"].as_str().unwrap_or("")), l["range"].clone())).collect())
}

/// The uses of S other than its own definition (its signature and equations).
fn uses(ix: &Index, s: &Sym) -> Result<Vec<(PathBuf, Value)>> {
    let text = read(&s.file)?;
    let e = extent(ix, s, &text);
    let utf8 = ix.enc(s);
    Ok(refs(ix, s, true)?
        .into_iter()
        .filter(|(f, r)| {
            if f != &s.file {
                return true;
            }
            let (a, _) = span(&text, r, utf8);
            !e.holds(a)
        })
        .collect())
}

/// Each reference: its place, the symbol it is in (its file outlined first, if it was not yet) and its line.
pub fn describe_refs(ix: &mut Index, list: &[(PathBuf, Value)]) -> Result<String> {
    let files: Vec<&Path> = list.iter().map(|(f, _)| f.as_path()).collect();
    ix.load_files(&files)?;
    ix.outline_files(&files)?;
    let mut texts: HashMap<PathBuf, String> = HashMap::new();
    let mut out = Vec::new();
    for (f, r) in list {
        let t = texts.entry(f.clone()).or_insert_with(|| read(f).unwrap_or_default());
        let line = line_of(r);
        let src = t.split('\n').nth(line as usize).unwrap_or("").trim();
        let within = ix.enclosing(f, line).map(|s| format!("in {}", s.name(ix.lang(s)))).unwrap_or_else(|| "at the top".into());
        let col = r.pointer("/start/character").and_then(Value::as_u64).unwrap_or(0) + 1;
        out.push(format!("{}:{}:{col}  {within}  | {src}", ix.rel(f), line + 1));
    }
    Ok(out.join("\n"))
}

pub fn references(ix: &mut Index, name: &str) -> Result<String> {
    let s = ix.resolve(name)?;
    if let Some(r) = record_references(ix, &s)? {
        return Ok(r);
    }
    if !ix.started(s.server) {
        ix.start(s.server)?;
    }
    let list = uses(ix, &s)?;
    if list.is_empty() {
        return Ok(format!("{}: no uses outside its definition", s.name(ix.lang(&s))));
    }
    let d = describe_refs(ix, &list)?;
    Ok(format!("{}: {} uses\n{d}{}", s.name(ix.lang(&s)), list.len(), holes_note(ix, &s)))
}

/// The uses of S from the commit's uses index and records, no server asked: each place in a file the view has not
/// changed, said "as of the commit" where that file imports a changed one; in a file the view changed, the uses
/// by name in its text now, said to be by name. None when the records do not know S's name.
fn record_references(ix: &mut Index, s: &Sym) -> Result<Option<String>> {
    let Some((list, changed)) = ix.record_uses(s)? else { return Ok(None) };
    let lang = ix.lang(s).to_string();
    let own = s.full.last().cloned().unwrap_or_default();
    // its own definition is not a use (the file's text now: a record's file is as the commit has it)
    let text = read(&s.file)?;
    let e = extent(ix, s, &text);
    let list: Vec<(PathBuf, Value)> = list.into_iter().filter(|(f, r)| f != &s.file || !e.holds(span(&text, r, false).0)).collect();
    let mut by_name = Vec::new();
    for f in &changed {
        let Ok(t) = read(f) else { continue };
        for (ln, l) in t.split('\n').enumerate() {
            for at in whole_words(l, &own, &lang) {
                let ch = l[..at].encode_utf16().count();
                let r = json!({"start": {"line": ln, "character": ch}, "end": {"line": ln, "character": ch + own.encode_utf16().count()}});
                // its own definition in the changed file it is now read from
                if f == &s.file && !s.of_record && e.holds(span(&t, &r, false).0) {
                    continue;
                }
                by_name.push((f.clone(), r));
            }
        }
    }
    let n = list.len() + by_name.len();
    if n == 0 {
        return Ok(Some(format!("{}: no uses outside its definition (the commit's uses index; no file you changed names it)", s.name(&lang))));
    }
    let mut out = format!("{}: {n} uses", s.name(&lang));
    if !list.is_empty() {
        out.push_str(&format!("\n{}", describe_refs(ix, &list)?));
    }
    let stale: Vec<String> = {
        let mut v: Vec<String> = list.iter().map(|(f, _)| f.clone()).filter(|f| ix.recs[s.server].as_ref().and_then(|st| st.commit.as_ref().and_then(|c| c.files.get(f)).map(|e| st.stale.contains(&e.module))).unwrap_or(false)).map(|f| ix.rel(&f)).collect();
        v.sort();
        v.dedup();
        v
    };
    if !stale.is_empty() {
        out.push_str(&format!("\n(as of the commit in {}: each imports a file you changed and is not checked against it yet)", stale.join(", ")));
    }
    if !by_name.is_empty() {
        let d = describe_refs(ix, &by_name)?;
        out.push_str(&format!("\nby name, in files you changed (not checked):\n{d}"));
    }
    Ok(Some(out))
}

/// agda-ls does not see the text inside holes: references and rename say so where a hole is open.
fn holes_note(ix: &Index, s: &Sym) -> String {
    if !ix.started(s.server) {
        return String::new();
    }
    if ix.lang(s) != "agda" {
        return String::new();
    }
    let mut at = Vec::new();
    for (u, ds) in ix.server(s.server).diagnostics() {
        for d in ds {
            if d["severity"].as_u64() == Some(3) {
                at.push(format!("{}:{}", ix.rel(&lsp::path_of(&u)), line_of(&d["range"]) + 1));
            }
        }
    }
    if at.is_empty() {
        return String::new();
    }
    at.sort();
    format!("\nnote: agda-ls does not see the text inside holes; uses there are not in this answer (holes open at {})", at.join(", "))
}

/// The type at a place: a symbol, and a piece of text in its definition (the NTH, from 1); the server's hover at the
/// start of that text, and any diagnostic there (a typed hole's type comes as one).
pub fn type_at(ix: &mut Index, name: &str, at: &str, nth: usize) -> Result<String> {
    let s = ix.resolve(name)?;
    ix.start(s.server)?;
    let text = read(&s.file)?;
    // a place is in the code: the comment lines above are not searched
    let e = server_extent(ix, &s, &text);
    // the NTH occurrence, counted through the pieces in order
    let hits: Vec<usize> = e.pieces().into_iter().flat_map(|(a, b)| text[a..b].match_indices(at).map(move |(i, _)| a + i).collect::<Vec<_>>()).collect();
    let Some(&off) = hits.get(nth.max(1) - 1).filter(|_| !at.is_empty()) else { bail!("{at:?} is not in {} (occurrence {nth})", s.name(ix.lang(&s))) };
    let pos = lsp::position(&text, off, ix.server(s.server).utf8);
    // agda-ls: a name's type, and a hole's goal and context, by hover; an expression's by agda/inferType (at the
    // top level: an expression inside a hole is the hole's)
    let one_name = !at.contains(|c: char| c.is_whitespace() || "(){}".contains(c));
    let utf8 = ix.server(s.server).utf8;
    let in_hole = ix.server(s.server).diagnostics().get(&lsp::uri(&s.file)).into_iter().flatten().any(|d| {
        let (a, b) = span(&text, &d["range"], utf8);
        d["severity"].as_u64() == Some(3) && a <= off && off < b
    });
    let ty = if ix.lang(&s) == "agda" && !one_name && !in_hole {
        let r = ix.server(s.server).request("agda/inferType", json!({"textDocument": {"uri": lsp::uri(&s.file)}, "expression": at, "position": {"line": pos.0, "character": pos.1}}));
        Some(match r {
            Ok(v) => v["type"].as_str().unwrap_or("").to_string(),
            Err(e) => format!("(agda/inferType: {})", e.to_string().replace('\n', " ")),
        })
    } else {
        hover(ix, s.server, &s.file, pos, None)?
    };
    let line = pos.0;
    let mut out = format!("{at:?} in {} ({}:{})\n", s.name(ix.lang(&s)), ix.rel(&s.file), line + 1);
    out.push_str(&format!("type: {}\n", ty.unwrap_or_else(|| "(the server gives none here)".into())));
    let diags = ix.server(s.server).diagnostics();
    for d in diags.get(&lsp::uri(&s.file)).into_iter().flatten() {
        let (a, b) = span(&text, &d["range"], utf8);
        if a <= off + at.len() && off <= b {
            out.push_str(&format!("there: {}\n", d["message"].as_str().unwrap_or("").trim()));
        }
    }
    Ok(out)
}

pub fn outline(ix: &mut Index, file: &str) -> Result<String> {
    let path = ix.root.join(file);
    // a file the view has not changed: its record's outline and types, no server (paper-research2 N-387 p10)
    let rec = ix.from_record(&path)?;
    let i = match rec {
        Some(i) => i,
        None => {
            let i = ix.server_for(&path)?;
            ix.reindex(i, &path)?;
            i
        }
    };
    let text = read(&path)?;
    let mut syms: Vec<Sym> = ix.syms.iter().filter(|s| s.file == path).cloned().collect();
    syms.sort_by_key(|s| (line_of(&s.range), s.depth));
    let how = match (rec, syms.first()) {
        (Some(_), Some(s)) if ix.stale(s) => "; as of the commit: it imports a file you changed and is not checked against it yet",
        (Some(_), _) => "; as the commit's build checked it",
        _ => "",
    };
    let mut out = vec![format!("{} ({} symbols{how})", ix.rel(&path), syms.len())];
    let im = imports(&text, &ix.confs[i].lang);
    if !im.is_empty() {
        out.push(format!("imports ({}):", im.len()));
        out.extend(im.into_iter().map(|(l, t)| format!("  line {l}: {t}")));
        out.push("symbols:".into());
    }
    for s in &syms {
        // a type only where the hover gives a signature (a field's hover may start with its record's type)
        let ty = if s.of_record {
            s.typed.as_ref().map(|t| format!(": {}", t.split_whitespace().collect::<Vec<_>>().join(" ")))
        } else if ix.lang(s) == "agda" {
            if s.kind == 2 { None } else { hover(ix, i, &path, name_pos(s), None)?.map(|t| format!(": {}", t.split_whitespace().collect::<Vec<_>>().join(" "))) }
        } else if matches!(s.kind, 6 | 7 | 8 | 12 | 13 | 14 | 25) { typed_by_detail(s, hover(ix, i, &path, name_pos(s), s.full.last().map(String::as_str))?).and_then(|t| t.lines().find(|l| l.contains("::") || l.contains("fn ") || l.contains(": ")).map(|l| l.trim().to_string())) } else { None };
        let e = extent(ix, s, &text);
        let ln = lsp::position(&text, e.start, true).0 + 1;
        out.push(format!("{}{}  ({}, line {ln}){}", "  ".repeat(s.depth + 1), s.name(ix.lang(s)), s.kind(), ty.map(|t| format!("  {t}")).unwrap_or_default()));
    }
    Ok(out.join("\n"))
}

/// A file's import lines, each with its line (from 1) and its text, the lines that go on with it joined: Agda's
/// `import` and `open import`, Haskell's `import`, Rust's `use` (at the top of the file or of a module, `pub` too).
fn imports(text: &str, lang: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let starts = |l: &str| -> bool {
        let t = l.trim_start();
        match lang {
            "agda" => l == t && (t.starts_with("import ") || t.starts_with("open import ")),
            "haskell" => l == t && t.starts_with("import "),
            "rust" => {
                let t = t.strip_prefix("pub ").or_else(|| t.strip_prefix("pub(crate) ")).unwrap_or(t);
                t.starts_with("use ")
            }
            _ => false,
        }
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !starts(lines[i]) {
            i += 1;
            continue;
        }
        let first = i;
        let mut parts = vec![lines[i].trim().to_string()];
        i += 1;
        if lang == "rust" {
            // to its `;`
            while !parts.last().unwrap().ends_with(';') && i < lines.len() {
                parts.push(lines[i].trim().to_string());
                i += 1;
            }
        } else {
            // the lines indented deeper that go on with it
            while i < lines.len() && !lines[i].trim().is_empty() && lines[i].starts_with([' ', '\t']) {
                parts.push(lines[i].trim().to_string());
                i += 1;
            }
        }
        out.push((first + 1, parts.join(" ")));
    }
    out
}

/// The errors and warnings all servers report now, by file.
pub fn snapshot(ix: &Index) -> BTreeMap<PathBuf, Vec<String>> {
    let mut out = BTreeMap::new();
    for (i, s) in ix.servers.iter().enumerate() {
        let Some(s) = s else { continue };
        for (u, ds) in s.diagnostics() {
            let p = lsp::path_of(&u);
            let lines: Vec<String> = ds
                .iter()
                .filter(|d| d["severity"].as_u64().unwrap_or(1) <= 2)
                .map(|d| {
                    let sev = if d["severity"].as_u64().unwrap_or(1) == 1 { "error" } else { "warning" };
                    // agda-ls's messages begin with Agda's own `error: `
                    let m = d["message"].as_str().unwrap_or("").trim();
                    let m = m.strip_prefix(&format!("{sev}: ")).unwrap_or(m);
                    format!("{sev} {}:{}: {}", line_of(&d["range"]) + 1, d.pointer("/range/start/character").and_then(Value::as_u64).unwrap_or(0) + 1, m.replace('\n', "\n      "))
                })
                .collect();
            let _ = i;
            if !lines.is_empty() {
                out.insert(p, lines);
            }
        }
    }
    out
}

/// An edit written whose servers' checks are still to be waited on: the marks left, the errors before it, the files
/// it changed, what it did, the servers that ended meanwhile.
pub struct EditCheck {
    marks: Vec<(usize, lsp::Mark)>,
    before: BTreeMap<PathBuf, Vec<String>>,
    changed: Vec<PathBuf>,
    done: String,
    busy: Vec<String>,
}

impl EditCheck {
    /// Whether it waits on server I's check.
    pub fn waits_on(&self, i: usize) -> bool {
        self.marks.iter().any(|(s, _)| *s == i)
    }
}

/// Write the new texts, tell their servers, wait for the servers' checks, and report: the changed files first,
/// then every other file whose errors and warnings the edit changed, then those it left as they were.
fn apply(ix: &mut Index, changes: Vec<(PathBuf, String)>, done: String) -> Result<String> {
    let before = snapshot(ix);
    let mut marks = Vec::new();
    let mut views = Vec::new();
    for (p, t) in &changes {
        std::fs::write(p, t).map_err(|e| anyhow!("writing {}: {e}", p.display()))?;
        let i = ix.server_for(p)?;
        marks.push((i, ix.server(i).mark()));
        if ix.server(i).view {
            if !views.contains(&i) {
                views.push(i);
            }
        } else {
            ix.server(i).changed(p, t)?;
        }
    }
    // a server given the view as data: one update after the writes (design.md 4.1)
    for i in views {
        ix.view_sync(i, false)?;
    }
    let changed: Vec<PathBuf> = changes.into_iter().map(|(p, _)| p).collect();
    ix.written = changed.clone();
    let done2 = done.clone();
    match checked(ix, EditCheck { marks, before, changed, done, busy: Vec::new() }) {
        Err(e) if lsp::still_working(&e).is_some() => Ok(format!("{done2}\n\nIts errors are not in yet: {e}.\nCall any symbols tool to have them: it first waits on this same check (the check goes on meanwhile).")),
        r => r,
    }
}

/// Wait on an edit's checks, then its report. Cut at the call's deadline, the check is kept in ix.edit_check and the
/// cut is the error.
fn checked(ix: &mut Index, mut c: EditCheck) -> Result<String> {
    while let Some(&(i, since)) = c.marks.first() {
        match ix.server(i).settle(since, QUIET) {
            Ok(true) => {}
            Ok(false) => c.busy.push(ix.confs[i].lang.clone()),
            Err(e) if lsp::still_working(&e).is_some() => {
                ix.edit_check = Some(c);
                return Err(e);
            }
            Err(e) => return Err(e),
        }
        c.marks.remove(0);
    }
    for p in &c.changed {
        let i = ix.server_for(p)?;
        ix.reindex(i, p)?;
    }
    let after = snapshot(ix);
    Ok(report(ix, &c.before, &after, &c.changed, c.done, c.busy))
}

/// Files changed outside symbols (Claude Code's Edit, the shell, git) since symbols last sent them: every file a
/// running server serves whose text on disk is not what was last sent, a new one included, is sent again, a file gone
/// is closed, and their check is waited on before the call (paper-research, N-101 post 219). Without it the server
/// goes on checking symbols' own copy of the file. None when nothing changed; the check's report otherwise; cut at the
/// call's deadline, the check is kept as an edit's is.
pub fn outside_edits(ix: &mut Index) -> Result<Option<String>> {
    let before = snapshot(ix);
    let mut marks = Vec::new();
    // what was sent, as (server, path): changed files, and files gone
    let mut sent_changed: Vec<(usize, PathBuf)> = Vec::new();
    let mut sent_gone: Vec<(usize, PathBuf)> = Vec::new();
    for i in 0..ix.servers.len() {
        let Some(view) = ix.servers[i].as_ref().filter(|s| !s.closed()).map(|s| s.view) else { continue };
        let mark = ix.server(i).mark();
        if view {
            // given the view as data: what differs from the bases now and was not told (design.md 4.1)
            if let Some((c, g)) = ix.view_sync(i, false)? {
                marks.push((i, mark));
                sent_changed.extend(c.into_iter().map(|p| (i, p)));
                sent_gone.extend(g.into_iter().map(|p| (i, p)));
            }
            continue;
        }
        let s = ix.server(i);
        let sent: HashMap<PathBuf, u64> = s.sent().into_iter().collect();
        let c = &ix.confs[i];
        let now = crate::index::files_under(&ix.root, &c.files, &crate::index::skip_paths(&ix.root, &c.skip));
        let mut changes = Vec::new();
        for f in &now {
            if let Ok(t) = std::fs::read_to_string(f) {
                if sent.get(f) != Some(&lsp::hash(&t)) {
                    changes.push((f.clone(), t));
                }
            }
        }
        let gone: Vec<PathBuf> = sent.keys().filter(|f| !f.exists()).cloned().collect();
        if changes.is_empty() && gone.is_empty() {
            continue;
        }
        marks.push((i, mark));
        for (p, t) in changes {
            ix.server(i).changed(&p, &t)?;
            sent_changed.push((i, p));
        }
        for p in gone {
            ix.server(i).closed_file(&p)?;
            sent_gone.push((i, p));
        }
    }
    if marks.is_empty() {
        return Ok(None);
    }
    for (_, p) in &sent_gone {
        ix.syms.retain(|x| &x.file != p);
    }
    // the same path told to two servers (a view goes to each whole) is said once
    let mut names: Vec<String> = sent_changed.iter().map(|(_, p)| ix.rel(p)).collect();
    names.sort();
    names.dedup();
    let mut g: Vec<String> = sent_gone.iter().map(|(_, p)| ix.rel(p)).collect();
    g.sort();
    g.dedup();
    let mut what = Vec::new();
    if !names.is_empty() {
        what.push(format!("{} changed outside symbols since it last read them ({})", if names.len() == 1 { "1 file".to_string() } else { format!("{} files", names.len()) }, some_of(&names)));
    }
    if !g.is_empty() {
        what.push(format!("{} gone ({})", g.len(), some_of(&g)));
    }
    eprintln!("symbols: before the call: {}", what.join("; "));
    let done = format!("Before this call: {}; the servers were given them and checked them first.", what.join("; "));
    // outlined again: the files each server serves (a view's .cabal or libraries file has no outline)
    let mut changed: Vec<PathBuf> = Vec::new();
    for (i, p) in sent_changed {
        let c = &ix.confs[i];
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if c.files.iter().any(|pat| crate::index::matches(pat, &name)) && !crate::index::skipped(&p, &crate::index::skip_paths(&ix.root, &c.skip)) && !changed.contains(&p) {
            changed.push(p);
        }
    }
    // marks of one server once: the first, the earliest
    let mut seen = std::collections::HashSet::new();
    marks.retain(|(i, _)| seen.insert(*i));
    checked(ix, EditCheck { marks, before, changed, done, busy: Vec::new() }).map(Some)
}

/// A call made while an earlier edit's check went on: that check first. Its report, if it ended (the call's own
/// answer follows it); cut again, the error.
pub fn earlier_edit(ix: &mut Index) -> Result<Option<String>> {
    let Some(c) = ix.edit_check.take() else { return Ok(None) };
    let done = c.done.clone();
    match checked(ix, c) {
        Ok(r) => Ok(Some(format!("The check of the earlier edit has ended. {r}"))),
        Err(e) if lsp::still_working(&e).is_some() => Err(anyhow::Error::new(lsp::StillWorking(format!("the earlier edit ({done}) is still being checked: {e}")))),
        Err(e) => Err(e),
    }
}

/// An edit's answer: what was done, the changed files' errors and warnings, then every other file whose errors and
/// warnings the edit changed, then those it left as they were.
pub fn report(ix: &Index, before: &BTreeMap<PathBuf, Vec<String>>, after: &BTreeMap<PathBuf, Vec<String>>, changed: &[PathBuf], done: String, busy: Vec<String>) -> String {
    let mut out = vec![done, String::new(), "errors and warnings after the edit:".to_string()];
    let changed: Vec<&PathBuf> = changed.iter().collect();
    for p in &changed {
        match after.get(*p) {
            Some(l) => out.push(format!("{} (changed): \n  {}", ix.rel(p), l.join("\n  "))),
            None => out.push(format!("{} (changed): none", ix.rel(p))),
        }
    }
    let mut others = Vec::new();
    let mut same = Vec::new();
    // files whose one error is that a module they import does not check (agda-ls): one line for each such module
    let mut through: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (p, l) in after {
        if changed.contains(&p) {
            continue;
        }
        if before.get(p) == Some(l) {
            same.push(ix.rel(p));
        } else if let Some(m) = only_import_fails(l) {
            through.entry(ix.rel(Path::new(&m))).or_default().push(ix.rel(p));
        } else {
            others.push(format!("{} (its errors changed with this edit):\n  {}", ix.rel(p), l.join("\n  ")));
        }
    }
    for (m, fs) in &through {
        others.push(format!("{} other files do not check now only because they import {m}, which does not: {}", fs.len(), some_of(fs)));
    }
    let clean: Vec<String> = before.keys().filter(|p| !changed.contains(p) && !after.contains_key(*p)).map(|p| ix.rel(p)).collect();
    if clean.len() > LIST {
        others.push(format!("{} files have no errors now (they had some before this edit): {}", clean.len(), some_of(&clean)));
    } else {
        for p in &clean {
            others.push(format!("{p}: none now (it had some before this edit)"));
        }
    }
    out.extend(others);
    if !same.is_empty() {
        out.push(format!("unchanged by this edit, with errors or warnings from before: {}", if same.len() > LIST { format!("{} files: {}", same.len(), some_of(&same)) } else { same.join(", ") }));
    }
    if !busy.is_empty() {
        out.push(format!("note: {} ended during the check; its errors are not in this answer", busy.join(", ")));
    }
    out.join("\n")
}

fn edit_extent(ix: &mut Index, name: &str) -> Result<(Sym, String, Extent)> {
    let s = ix.resolve(name)?;
    let text = read(&s.file)?;
    let e = extent(ix, &s, &text);
    if let Some(why) = &e.unsafe_edit {
        bail!("{} was not changed: {why}", s.name(ix.lang(&s)));
    }
    Ok((s, text, e))
}

pub fn replace(ix: &mut Index, name: &str, new: &str) -> Result<String> {
    let (s, text, e) = edit_extent(ix, name)?;
    let new = new.trim_end_matches('\n');
    let mut t = text.clone();
    let what = if e.rest.is_empty() {
        t.replace_range(e.start..e.end, new);
        format!("replaced {} in {}", s.name(ix.lang(&s)), ix.rel(&s.file))
    } else {
        // in pieces: the new signature at the signature's piece, the new clauses at the first clause piece, the
        // other pieces taken away (from the last, so the offsets before stay)
        let Some((sig, clauses)) = split_signature(new) else {
            bail!("{} was not changed: it is in pieces, and the new source has no signature to put at the first (write it with its signature first, then its clauses)", s.name(ix.lang(&s)));
        };
        for (k, &(a, b)) in e.rest.iter().enumerate().rev() {
            if k == 0 && !clauses.is_empty() {
                t.replace_range(a..b, &clauses);
            } else {
                let (a, b) = with_its_line(&t, a, b);
                t.replace_range(a..b, "");
            }
        }
        t.replace_range(e.start..e.end, &sig);
        let at = |off: usize| lsp::position(&text, off, false).0 + 1;
        if clauses.is_empty() {
            format!("replaced {} in {}: its signature at line {}; the new source has no clauses, its clauses at line {} are gone", s.name(ix.lang(&s)), ix.rel(&s.file), at(e.start), at(e.rest[0].0))
        } else {
            format!("replaced {} in {}, in its pieces: the signature at line {}, the clauses at line {} (as they stood before the edit)", s.name(ix.lang(&s)), ix.rel(&s.file), at(e.start), at(e.rest[0].0))
        }
    };
    apply(ix, vec![(s.file.clone(), t)], what)
}

/// A new source as a definition in pieces needs it: its signature (the comment lines above it, its first line and
/// the lines indented deeper that go on with it) and its clauses (the rest, from the next line as deep as its
/// first). None if it holds no code.
fn split_signature(src: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = src.trim_matches('\n').split('\n').collect();
    let depth = |l: &str| l.len() - l.trim_start().len();
    let base = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| depth(l)).min()?;
    let mut i = 0;
    // the comment lines above it
    while i < lines.len() {
        let l = lines[i].trim();
        if l.starts_with("--") || l.is_empty() {
            i += 1;
        } else if l.starts_with("{-") && !l.starts_with("{-#") {
            while i < lines.len() && !lines[i].contains("-}") {
                i += 1;
            }
            i += 1;
        } else {
            break;
        }
    }
    if i >= lines.len() {
        return None;
    }
    let mut k = i + 1;
    while k < lines.len() && !lines[k].trim().is_empty() && depth(lines[k]) > base {
        k += 1;
    }
    let sig = lines[..k].join("\n");
    let rest: Vec<&str> = lines[k..].iter().copied().skip_while(|l| l.trim().is_empty()).collect();
    Some((sig, rest.join("\n")))
}

pub fn insert(ix: &mut Index, name: &str, source: &str, after: bool) -> Result<String> {
    let (s, text, e) = edit_extent(ix, name)?;
    let src = source.trim_matches('\n');
    let t = if after {
        let last = e.pieces().last().unwrap().1;
        let at = line_end(&text, last);
        format!("{}\n\n{src}{}", &text[..at], &text[at..])
    } else {
        format!("{}{src}\n\n{}", &text[..e.start], &text[e.start..])
    };
    let what = format!("inserted {} {} in {}", if after { "after" } else { "before" }, s.name(ix.lang(&s)), ix.rel(&s.file));
    apply(ix, vec![(s.file.clone(), t)], what)
}

pub fn remove(ix: &mut Index, name: &str) -> Result<String> {
    let (s, text, e) = edit_extent(ix, name)?;
    let list = uses(ix, &s)?;
    if !list.is_empty() {
        let d = describe_refs(ix, &list)?;
        bail!("{} was not removed: {} uses remain\n{d}", s.name(ix.lang(&s)), list.len());
    }
    // every piece, each with the blank line after it
    let mut t = text.clone();
    for (a, b) in e.pieces().into_iter().rev() {
        let (a, b) = with_its_line(&t, a, b);
        t.replace_range(a..b, "");
    }
    let what = format!("removed {} from {}", s.name(ix.lang(&s)), ix.rel(&s.file));
    apply(ix, vec![(s.file.clone(), t)], what)
}

/// How many files a list in an edit's answer names before it gives the count of the others.
const LIST: usize = 8;

fn some_of(fs: &[String]) -> String {
    if fs.len() > LIST { format!("{}, and {} more", fs[..LIST].join(", "), fs.len() - LIST) } else { fs.join(", ") }
}

/// The module that LINES (a file's errors, as snapshot gives them) say only that it does not check, where that is
/// all they say (agda-ls: `error 12:1: an imported module does not check: PATH`).
fn only_import_fails(lines: &[String]) -> Option<String> {
    const SAYS: &str = "an imported module does not check: ";
    let mut ms = lines.iter().map(|l| l.split_once(SAYS).map(|(_, m)| m.trim().to_string()));
    let first = ms.next()??;
    ms.all(|m| m.as_deref() == Some(&first)).then_some(first)
}

/// A piece A..B to take away: to the end of its last line and its newline, and one blank line after it.
fn with_its_line(text: &str, a: usize, b: usize) -> (usize, usize) {
    let mut end = line_end(text, b);
    if text[end..].starts_with("\n\n") {
        end += 1;
    }
    (a, (end + 1).min(text.len()))
}

/// Edits (file, range, new text) applied to the files' texts, last first.
fn edited(ix: &Index, server: usize, edits: Vec<(PathBuf, Value, String)>) -> Result<Vec<(PathBuf, String)>> {
    let utf8 = ix.server(server).utf8;
    let mut by: BTreeMap<PathBuf, Vec<(Value, String)>> = BTreeMap::new();
    for (p, r, t) in edits {
        by.entry(p).or_default().push((r, t));
    }
    let mut out = Vec::new();
    for (p, mut es) in by {
        let mut text = read(&p)?;
        let mut spans: Vec<(usize, usize, String)> = es.drain(..).map(|(r, t)| {
            let (a, b) = span(&text, &r, utf8);
            (a, b, t)
        }).collect();
        spans.sort_by(|x, y| y.0.cmp(&x.0));
        spans.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        for (a, b, t) in spans {
            text.replace_range(a..b, &t);
        }
        out.push((p, text));
    }
    Ok(out)
}

/// The server's rename; where its edit leaves a use the server's own references know of (HLS 2.13 renames an
/// exported name only in its own module), the rename is done at each reference instead, and the answer says so.
pub fn rename(ix: &mut Index, name: &str, new_name: &str) -> Result<String> {
    let s = ix.resolve(name)?;
    ix.start(s.server)?;
    let old = s.full.last().cloned().unwrap_or_default();
    let pos = name_pos(&s);
    let all = refs(ix, &s, true)?;
    let answer = ix.server(s.server).request("textDocument/rename", json!({"textDocument": {"uri": lsp::uri(&s.file)}, "position": {"line": pos.0, "character": pos.1}, "newName": new_name}));
    let mut edits: Vec<(PathBuf, Value, String)> = Vec::new();
    let mut how = "by the language server";
    if let Ok(we) = &answer {
        for dc in we["documentChanges"].as_array().cloned().unwrap_or_default() {
            let p = lsp::path_of(dc.pointer("/textDocument/uri").and_then(Value::as_str).unwrap_or(""));
            for e in dc["edits"].as_array().cloned().unwrap_or_default() {
                edits.push((p.clone(), e["range"].clone(), e["newText"].as_str().unwrap_or("").to_string()));
            }
        }
        for (u, es) in we["changes"].as_object().cloned().unwrap_or_default() {
            for e in es.as_array().cloned().unwrap_or_default() {
                edits.push((lsp::path_of(&u), e["range"].clone(), e["newText"].as_str().unwrap_or("").to_string()));
            }
        }
    }
    if ix.lang(&s) == "agda" {
        // agda-ls's rename is whole (qualified and mixfix uses included) or refused with its reason
        if let Err(e) = &answer {
            bail!("{} was not renamed: agda-ls refused: {e}{}", s.name(ix.lang(&s)), holes_note(ix, &s));
        }
        let n = edits.len();
        let files = edited(ix, s.server, edits)?;
        let nf = files.len();
        let what = format!("renamed {} to {new_name} by agda-ls: {n} places in {nf} files{}\n{}", s.name(ix.lang(&s)), holes_note(ix, &s), still_as_text(ix, s.server, &old, &files));
        return apply(ix, files, what);
    }
    let files_edited: Vec<PathBuf> = edits.iter().map(|e| e.0.clone()).collect();
    let missed = all.iter().any(|(f, _)| !files_edited.contains(f));
    if answer.is_err() || missed {
        // at each reference: the old name's last occurrence in its range (a qualified use `S.open` keeps `S.`)
        edits.clear();
        for (f, r) in &all {
            let text = read(f)?;
            let utf8 = ix.server(s.server).utf8;
            let (a, b) = span(&text, r, utf8);
            let piece = &text[a..b];
            let Some(k) = piece.rfind(&old) else { bail!("{} was not renamed: the reference at {}:{} does not hold {old}", s.name(ix.lang(&s)), ix.rel(f), line_of(r) + 1) };
            let (l0, c0) = lsp::position(&text, a + k, utf8);
            let (l1, c1) = lsp::position(&text, a + k + old.len(), utf8);
            edits.push((f.clone(), json!({"start": {"line": l0, "character": c0}, "end": {"line": l1, "character": c1}}), new_name.to_string()));
        }
        how = match &answer {
            Err(_) => "at each of the server's references (the server's own rename refused)",
            Ok(_) => "at each of the server's references (the server's own rename left uses in other files)",
        };
    }
    let n = edits.len();
    let files = edited(ix, s.server, edits)?;
    let nf = files.len();
    let what = format!("renamed {} to {new_name} {how}: {n} places in {nf} files\n{}", s.name(ix.lang(&s)), still_as_text(ix, s.server, &old, &files));
    apply(ix, files, what)
}

/// The rename answer's line on what it leaves: the places in the server's files (as they will be once CHANGED is
/// written) where OLD still stands as a whole word. Those are comments, strings, and other symbols of the same name
/// alike: the count is of the text, not of what it means. An Agda mixfix name (`_+_`, `if_then_else_`) stands in
/// its uses by its parts: their first is counted.
fn still_as_text(ix: &Index, server: usize, old: &str, changed: &[(PathBuf, String)]) -> String {
    let lang = ix.confs[server].lang.as_str();
    let word = if lang == "agda" && old.contains('_') { old.split('_').find(|p| !p.is_empty()).unwrap_or(old) } else { old };
    let mut places = Vec::new();
    for f in crate::index::files_under(&ix.root, &ix.confs[server].files, &[]) {
        let text = match changed.iter().find(|(p, _)| p == &f) {
            Some((_, t)) => t.clone(),
            None => match std::fs::read_to_string(&f) {
                Ok(t) => t,
                Err(_) => continue,
            },
        };
        for (l, line) in text.lines().enumerate() {
            for _ in whole_words(line, word, lang) {
                places.push(format!("{}:{}", ix.rel(&f), l + 1));
            }
        }
    }
    let as_ = if word == old { old.to_string() } else { format!("{old} (its part {word})") };
    if places.is_empty() {
        return format!("comments and strings are not changed; {as_} no longer stands as text in the {lang} files");
    }
    let n = places.len();
    places.dedup();
    let some: Vec<&str> = places.iter().take(8).map(String::as_str).collect();
    let more = if places.len() > 8 { ", …" } else { "" };
    format!("comments and strings are not changed; {as_} still stands as text in {n} place{} of the {lang} files, in comments, strings or other names: {}{more}", if n == 1 { "" } else { "s" }, some.join(", "))
}

/// Where WORD stands in LINE as a whole word: in Agda a name runs to whitespace or one of `(){};.@"`; elsewhere
/// to anything but a letter, digit, `_` or `'`.
fn whole_words<'a>(line: &'a str, word: &'a str, lang: &'a str) -> impl Iterator<Item = usize> + 'a {
    let part = move |c: char| if lang == "agda" { !(c.is_whitespace() || "(){};.@\"".contains(c)) } else { c.is_alphanumeric() || c == '_' || c == '\'' };
    line.match_indices(word).map(|(i, _)| i).filter(move |&i| {
        !word.is_empty() && !line[..i].chars().next_back().is_some_and(part) && !line[i + word.len()..].chars().next().is_some_and(part)
    })
}

#[cfg(test)]
mod tests {
    use super::comments_above;

    /// hs-ls: the hover at a signature's name is the name alone; the detail gives the type. HLS: the hover's type stands.
    #[test]
    fn a_haskell_type_from_the_detail_when_the_hover_has_none() {
        use super::typed_by_detail;
        let s = crate::index::Sym { server: 0, file: "src/Store.hs".into(), full: vec!["Store".into(), "open".into()], kind: 12, range: crate::lsp::Rng::default(), sel: crate::lsp::Rng::default(), depth: 0, kind_name: None, signature: None, pieces: vec![], detail: Some("String -> Store".into()), typed: None, of_record: false };
        assert_eq!(typed_by_detail(&s, Some("open".into())).as_deref(), Some("open :: String -> Store"));
        assert_eq!(typed_by_detail(&s, None).as_deref(), Some("open :: String -> Store"));
        assert_eq!(typed_by_detail(&s, Some("open :: String -> Store\n".into())).as_deref(), Some("open :: String -> Store\n"));
        let mut d = s.clone();
        d.detail = None;
        assert_eq!(typed_by_detail(&d, Some("open".into())).as_deref(), Some("open"));
    }

    #[test]
    fn a_fields_type_is_its_own_not_its_constructors() {
        let b = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // HLS's hover at storeName as the box gave it: the record's block, with the constructor's type, first
        let blocks = b(&["Store :: String %1 -> [(String, Int)] %1 -> Store", "storeName :: Store -> String", "_ :: String"]);
        assert_eq!(super::pick_block(&blocks, Some("storeName")).unwrap(), "storeName :: Store -> String");
        // docket-ls: the block and the docs under it (a2h-speed's hover of assign, N-101 p767)
        let h = "```docket\ncall assign(words: Text, what: Text) -> Assigned | Ask\n```\n\nwho writes the design\n\n`words: Text`: the owner's words for this thread\n\nlibrary/lib.call:60";
        assert_eq!(super::docket_hover(h), "call assign(words: Text, what: Text) -> Assigned | Ask\n\nwho writes the design\n\n`words: Text`: the owner's words for this thread\n\nlibrary/lib.call:60");
        assert_eq!(super::docket_hover("```docket\nkind k\n```"), "kind k");
        assert_eq!(super::pick_block(&b(&["Store", "(<+>) :: Store -> Store -> Store"]), Some("<+>")).unwrap(), "(<+>) :: Store -> Store -> Store");
        assert_eq!(super::pick_block(&b(&["store", "pub fn size(&self) -> usize"]), Some("size")).unwrap(), "pub fn size(&self) -> usize");
    }

    #[test]
    fn a_field_runs_to_its_comma() {
        let t = "data Store = Store\n  { storeName :: String\n  , entries   :: [(String, Int)] -- the pairs\n  -- ^ newest first\n  }\n";
        let a = t.find("storeName").unwrap();
        assert_eq!(&t[a..super::field_end(t, a + 9)], "storeName :: String");
        let b = t.find("entries").unwrap();
        assert_eq!(&t[b..super::field_end(t, b + 7)], "entries   :: [(String, Int)] -- the pairs");
        let t = "data P = P { x, y :: Int, f :: Map String (Int, Int) }";
        let f = t.find("f ::").unwrap();
        assert_eq!(&t[f..super::field_end(t, f + 1)], "f :: Map String (Int, Int)");
    }

    #[test]
    fn importers_that_fail_only_through_an_import() {
        let l = vec!["error 12:1: an imported module does not check: /w/src/Data/Maybe/Base.agda".to_string()];
        assert_eq!(super::only_import_fails(&l).as_deref(), Some("/w/src/Data/Maybe/Base.agda"));
        let l2 = vec![l[0].clone(), "error 3:1: [NotInScope] x".to_string()];
        assert_eq!(super::only_import_fails(&l2), None);
    }

    #[test]
    fn a_new_source_splits_into_signature_and_clauses() {
        let (s, c) = super::split_signature("-- Even.\nisEven : Nat\n       → Nat\nisEven zero = 1\nisEven (suc n) = isOdd n\n").unwrap();
        assert_eq!(s, "-- Even.\nisEven : Nat\n       → Nat");
        assert_eq!(c, "isEven zero = 1\nisEven (suc n) = isOdd n");
        let (s, c) = super::split_signature("data Tree : Set\n\ndata Tree where\n  leaf : Tree").unwrap();
        assert_eq!((s.as_str(), c.as_str()), ("data Tree : Set", "data Tree where\n  leaf : Tree"));
        assert_eq!(super::split_signature("isOdd : Nat → Nat").unwrap().1, "");
        assert!(super::split_signature("-- only a comment").is_none());
    }

    #[test]
    fn a_files_imports() {
        let t = "module M where\n\nopen import Data.Bool.Base\n  using (Bool; true)\nimport Data.Nat as N\n\nx : Bool\n";
        assert_eq!(super::imports(t, "agda"), vec![(3, "open import Data.Bool.Base using (Bool; true)".to_string()), (5, "import Data.Nat as N".to_string())]);
        let t = "use std::io;\npub use crate::a::{\n    b,\n    c,\n};\nfn f() {}\n";
        assert_eq!(super::imports(t, "rust"), vec![(1, "use std::io;".to_string()), (2, "pub use crate::a::{ b, c, };".to_string())]);
        assert_eq!(super::imports("module A where\nimport Store\n", "haskell"), vec![(2, "import Store".to_string())]);
    }

    #[test]
    fn whole_words_only() {
        let w: Vec<usize> = super::whole_words("open opener (open) S.open open'", "open", "haskell").collect();
        assert_eq!(w, vec![0, 13, 21]);
        let w: Vec<usize> = super::whole_words("x + y +1 (+) a+b", "+", "agda").collect();
        assert_eq!(w, vec![2, 10]);
    }

    #[test]
    fn comments_above_a_definition_belong_to_it() {
        let t = "x = 1\n\n{- A block\n   comment. -}\n-- and a line\nf : A\n";
        let def = t.find("f :").unwrap();
        assert_eq!(&t[comments_above(t, def, "agda")..def], "{- A block\n   comment. -}\n-- and a line\n");
        let t = "{-# TERMINATING #-}\nf : A\n";
        assert_eq!(comments_above(t, t.find("f :").unwrap(), "agda"), t.find("f :").unwrap());
        let t = "a\n/* one */\n// two\nfn f() {}\n";
        assert_eq!(comments_above(t, t.find("fn").unwrap(), "rust"), 2);
        let t = "g = 1 {- trailing\n -}\nf : A\n";
        assert_eq!(comments_above(t, t.find("f :").unwrap(), "haskell"), t.find("f :").unwrap());
    }
}
