//! What the server knows of a project: every declaration (kind, call, type, policy fact, act and decision) and every
//! use of one, with where it is written, and the loader's refusals. Read with docket-machine's parsers, the ones
//! coordd loads the same files with (`syntax` for kinds, calls and types, `policy_syntax` for the policy); the
//! positions come from their lexers (`syntax::lex_spans`, `policy_syntax::words`) and the parsed declarations' lines.

use docket_machine::policy_syntax as ps;
use docket_machine::syntax::{self, Kind, Named, Prog, Tok};
use docket_machine::{Defs, doc};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What a name names. A policy's facts, acts and decisions share one space, as the policy's own names do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Space {
    Kind,
    Call,
    Type,
    Policy,
    /// A template's `{{ include "f" }}`: the name is the file's path.
    File,
    /// A field of a declared record type (`after` in `p.after`), by its name: every record type with a field of that
    /// name answers, until the checker's scope says which type the value has.
    Field,
}

/// Where a name is written: its file, its line (0-based) and its columns in UTF-16 units, as LSP counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

#[derive(Clone, Debug)]
pub struct Decl {
    pub space: Space,
    pub name: String,
    /// `kind`, `call`, `decision`, `type`, `fact`, `act`, `policy decision`.
    pub what: &'static str,
    pub at: Place,
    /// The declaration's signature, one line (document symbols' detail).
    pub sig: String,
    /// Its signature in full and its comment (hover), markdown.
    pub hover: String,
    /// Whether this is the declaration that stands (a later folder's call or kind of the same name replaces it).
    pub stands: bool,
    /// Where its whole text ends (line, UTF-16 column, 0-based): from the name's line through its indented body
    /// (a record type's closing `}` too), trailing blank and comment lines left out. documentSymbol's `range`.
    pub end: (usize, usize),
}

/// The end of the declaration that starts on line L (0-based) of LINES; a field is its own line.
fn whole_end(lines: &[&str], l: usize, field: bool) -> (usize, usize) {
    let mut last = l;
    if !field {
        let mut k = l + 1;
        while k < lines.len() {
            let t = lines[k];
            if t.trim().is_empty() || t.starts_with(char::is_whitespace) {
                k += 1;
                continue;
            }
            if t.trim_start().starts_with('}') {
                last = k;
            }
            break;
        }
        // back over trailing blank and comment lines
        let mut e = k.min(lines.len()).saturating_sub(1).max(last);
        while e > l && (lines[e].trim().is_empty() || lines[e].trim_start().starts_with("//")) {
            e -= 1;
        }
        last = last.max(e);
    }
    let len = lines.get(last).map_or(0, |t| t.chars().map(char::len_utf16).sum());
    (last, len)
}

#[derive(Clone, Debug)]
pub struct Use {
    pub space: Space,
    pub name: String,
    pub at: Place,
}

#[derive(Default)]
pub struct Index {
    pub files: Vec<String>,
    pub decls: Vec<Decl>,
    pub uses: Vec<Use>,
    /// The loader's refusals by file: line (0-based) and its words. Those it could place nowhere under "".
    pub diags: BTreeMap<String, Vec<(usize, String)>>,
}

/// Where the server reads files: the editor's open texts first, then the disk, under ROOT (`/proc/PID/root` of the
/// process holding the agent's view, for a shared server; empty otherwise). Paths are as the agent sees them.
#[derive(Default, Clone)]
pub struct Fs {
    pub open: HashMap<String, String>,
    pub root: String,
}

impl Fs {
    pub fn read(&self, path: &str) -> Option<String> {
        if let Some(t) = self.open.get(path) {
            return Some(t.clone());
        }
        std::fs::read_to_string(format!("{}{path}", self.root)).ok()
    }

    fn is_dir(&self, path: &str) -> bool {
        std::path::Path::new(&format!("{}{path}", self.root)).is_dir()
    }

    /// The files of DIR ending `.EXT`, sorted (coordd's `files`), with any open one not yet on disk.
    fn list(&self, dir: &str, ext: &str) -> Vec<String> {
        let mut out: BTreeSet<String> = std::fs::read_dir(format!("{}{dir}", self.root))
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).filter_map(|p| Some(format!("{dir}/{}", p.file_name()?.to_string_lossy()))).collect())
            .unwrap_or_default();
        for p in self.open.keys() {
            if p.strip_prefix(dir).and_then(|r| r.strip_prefix('/')).is_some_and(|r| !r.contains('/') && r.ends_with(&format!(".{ext}"))) {
                out.insert(p.clone());
            }
        }
        out.into_iter().collect()
    }

    /// Where FOLDER keeps its SUB (`kinds`, `calls`): FOLDER/SUB, or FOLDER itself when it has neither (coordd
    /// project.rs `place`: plan/library is flat).
    fn place(&self, folder: &str, sub: &str) -> String {
        if self.is_dir(&format!("{folder}/kinds")) || self.is_dir(&format!("{folder}/calls")) { format!("{folder}/{sub}") } else { folder.to_string() }
    }
}

/// The column of character CH of LINE in UTF-16 units.
fn u16col(line: &str, ch: usize) -> usize {
    line.chars().take(ch).map(char::len_utf16).sum()
}

fn place(file: &str, lines: &[&str], line1: usize, ch: usize, len: usize) -> Place {
    let l = lines.get(line1.wrapping_sub(1)).copied().unwrap_or("");
    let col = u16col(l, ch);
    let end = u16col(l, ch + len);
    Place { file: file.to_string(), line: line1.saturating_sub(1), col, len: end.saturating_sub(col) }
}

/// The column (in characters) of WORD in LINE after the first AFTER, as a whole word.
fn word_at(line: &str, after: &str, word: &str) -> Option<usize> {
    let from = line.find(after).map_or(0, |i| i + after.len());
    let mut i = from;
    while let Some(j) = line[i..].find(word).map(|j| j + i) {
        let before = line[..j].chars().last();
        let next = line[j + word.len()..].chars().next();
        let part = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
        if !part(before) && !part(next) {
            return Some(line[..j].chars().count());
        }
        i = j + word.len();
    }
    None
}

fn fence(s: &str) -> String {
    format!("```docket\n{s}\n```")
}

/// A declaration's doc (`///`, as the parser keeps it) for hover: its own text, then each part's (a field, parameter,
/// outcome or exception) under the part's signature, in the order NAMES gives them.
fn docs(d: Option<&syntax::Doc>, names: &[(String, String)]) -> Vec<String> {
    let Some(d) = d else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    if !d.text.trim().is_empty() {
        out.push(d.text.trim().to_string());
    }
    for (n, sig) in names {
        if let Some(t) = d.parts.get(n).filter(|t| !t.trim().is_empty()) {
            out.push(format!("`{sig}`: {}", t.trim()));
        }
    }
    out
}

fn fields_named(fs: &[(String, docket_machine::Type)]) -> Vec<(String, String)> {
    fs.iter().map(|(n, t)| (n.clone(), format!("{n}: {t}"))).collect()
}

fn with_comment(head: String, said: &[String], at: &str) -> String {
    let mut out = head;
    if !said.is_empty() {
        out.push_str("\n\n");
        out.push_str(&said.join("\n\n"));
    }
    out.push_str(&format!("\n\n{at}"));
    out
}

/// The steps, opens and decisions of P: what is named, in which space, and the line its node starts on.
fn walk(p: &Prog, out: &mut Vec<(Space, String, usize)>) {
    match p {
        Prog::Step { call, at, inline, .. } if inline.is_none() => out.push((Space::Call, call.clone(), *at)),
        Prog::Open { kind, at, .. } => out.push((Space::Kind, kind.clone(), *at)),
        Prog::Decide { name, at, .. } => out.push((Space::Call, format!("{}{name}", syntax::DECIDE), *at)),
        Prog::Bind { first, rest, .. } => {
            walk(first, out);
            walk(rest, out);
        }
        Prog::Match { arms, .. } => arms.iter().for_each(|(_, b)| walk(b, out)),
        Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().for_each(|b| walk(b, out)),
        Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => walk(body, out),
        Prog::Try { body, arms, .. } => {
            walk(body, out);
            arms.iter().for_each(|h| walk(&h.body, out));
        }
        Prog::During { body, whens, .. } => {
            walk(body, out);
            whens.iter().for_each(|w| walk(&w.body, out));
        }
        _ => {}
    }
}

/// The lines of a call file that are template text (indented deeper than their `template` line): no names there.
fn template_lines(lines: &[&str]) -> BTreeSet<usize> {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = BTreeSet::new();
    let mut deeper: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        if let Some(d) = deeper {
            if l.trim().is_empty() || indent(l) > d {
                out.insert(i);
                continue;
            }
            deeper = None;
        }
        let t = l.trim_start();
        if indent(l) > 0 && (t == "template" || t.starts_with("template ") || t.starts_with("template\t")) {
            deeper = Some(indent(l));
        }
    }
    out
}

/// The `{{ include "f" }}` of a template line: the file's name and its column (in characters).
fn includes(line: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (name, _, r) in syntax::placeholders(line) {
        let inner = line[r.clone()].trim_start_matches("{{").trim_end_matches("}}").trim();
        if name == "include" || inner.starts_with("include ") {
            if let Some(f) = inner.strip_prefix("include").map(str::trim).and_then(|s| s.strip_prefix('"')).and_then(|s| s.split('"').next()) {
                let q = line[r.clone()].find('"').map_or(r.start, |q| r.start + q + 1);
                out.push((f.to_string(), line[..q].chars().count()));
            }
        }
    }
    out
}

impl Index {
    /// Read the project whose folders are FOLDERS, in coordd's order: agent-base, the library, the project.
    pub fn build(folders: &[String], fs: &Fs) -> Index {
        let mut ix = Index::default();
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        let mut errors: Vec<String> = Vec::new();

        // the calls (coordd policy/call.rs `Calls::load`): the system's types, one name one type, then the one parser
        let mut call_files: Vec<(String, String)> = Vec::new();
        let mut dir_of: BTreeMap<String, String> = BTreeMap::new();
        for f in folders {
            let dir = fs.place(f, "calls");
            for p in fs.list(&dir, "call") {
                if let Some(t) = fs.read(&p) {
                    dir_of.insert(p.clone(), dir.clone());
                    call_files.push((p, t));
                }
            }
        }
        let mut named = Named::new();
        let _ = syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named);
        let system: BTreeSet<String> = named.keys().cloned().collect();
        let mut declared: BTreeMap<String, String> = BTreeMap::new();
        let mut kept = Vec::new();
        'file: for (file, text) in &call_files {
            let mut mine = Vec::new();
            for l in text.lines() {
                let Some(rest) = l.strip_prefix("type ") else { continue };
                let t = rest.split(['=', '{', '/']).next().unwrap_or("").trim().to_string();
                let other = declared.get(&t).filter(|o| dir_of.get(*o) == dir_of.get(file));
                let why = if system.contains(&t) {
                    Some(format!("{file}: type {t} is the system's (the nets' built-ins use it); name yours otherwise"))
                } else {
                    other.map(|o| format!("{file}: type {t} is declared in {o} too; give one of them another name"))
                };
                if let Some(why) = why {
                    errors.push(format!("{why}; the calls of {file} are left out"));
                    continue 'file;
                }
                mine.push(t);
            }
            for t in mine {
                declared.insert(t, file.clone());
            }
            kept.push((file.clone(), text.clone()));
        }
        let (mut calls, errs) = syntax::parse_call_files(&kept, &mut named);
        errors.extend(errs);
        // each call's template parsed and checked as coordd's load does it (docket-machine `template`; coordd
        // policy/call.rs `Calls::one`), its refusal at the line it names, and a call refused left out as coordd leaves
        // it, so every kind with a step of it is refused too (paper-research N-1446 p1842: a misspelt `{{ made }}` here
        // took coordd's whole `cut` kind out). A nets' call coordd checks in the policy's types; here in docket's.
        calls.retain(|name, d| {
            let Some(t) = &d.template else { return true };
            let dir = dir_of.get(&d.file).cloned().unwrap_or_default();
            let include = |f: &str| fs.read(&format!("{dir}/{f}")).ok_or_else(|| format!("no file {dir}/{f}"));
            let scope: BTreeMap<String, docket_machine::Type> = d.sig.params.iter().cloned().collect();
            let why = match docket_machine::template::parse(t, d.template_line, &include) {
                Ok(nodes) => docket_machine::template::check_step(&nodes, &scope).err().map(|e| format!("the template: {e}")),
                Err(e) => Some(e),
            };
            if let Some(e) = &why {
                errors.push(format!("{}:{}: {e}; the call {name} is left out", d.file, d.line));
            }
            why.is_none()
        });
        // the types' docs as coordd reads them (a project's type replaces the library's), for what a step gives
        let mut type_docs = syntax::type_docs(&kept);
        // a doc written `kind NAME` shows that kind's first doc line, as coordd reads it (paper-research N-101 p906)
        let kind_lines = {
            let mut fs_kinds: Vec<(String, String)> = Vec::new();
            for f in folders {
                for p in fs.list(&fs.place(f, "kinds"), "kind") {
                    if let Some(t) = fs.read(&p) {
                        fs_kinds.push((p, t));
                    }
                }
            }
            syntax::kind_lines(&fs_kinds)
        };
        let of_kinds = |mut d: syntax::Doc| {
            syntax::kind_docs(&mut d, &kind_lines);
            d
        };
        type_docs = type_docs.into_iter().map(|(n, d)| (n, of_kinds(d))).collect();
        let sigs: BTreeMap<String, docket_machine::CallSig> = calls.iter().map(|(n, c)| (n.clone(), c.sig.clone())).collect();
        let type_names: BTreeSet<String> = declared.keys().cloned().collect();
        // the constructors (`Placed(pieces: ...)` in a call's answer): a name with `(` after it that is one is no use
        // of a type of the same name
        let mut ctors: BTreeSet<String> = BTreeSet::new();
        fn ctors_of(t: &docket_machine::Type, out: &mut BTreeSet<String>) {
            if let docket_machine::Type::Variant(cs) = t {
                out.extend(cs.iter().map(|(c, _)| c.clone()));
            }
        }
        named.values().for_each(|t| ctors_of(t, &mut ctors));
        calls.values().for_each(|c| ctors_of(&c.sig.head, &mut ctors));

        for (file, text) in &call_files {
            texts.insert(file.clone(), text.clone());
            let lines: Vec<&str> = text.lines().collect();
            let mut decl_at: BTreeSet<(usize, usize)> = BTreeSet::new();
            // types
            for (name, line) in syntax::type_places(file, text).unwrap_or_default() {
                let ch = word_at(lines[line - 1], "type ", &name).unwrap_or(5);
                let at = place(file, &lines, line, ch, name.chars().count());
                decl_at.insert((at.line, at.col));
                let ty = named.get(&name).map(|t| format!("type {name} = {t}")).unwrap_or(format!("type {name}"));
                // its fields' or alternatives' docs under it, as a step's `gives` shows them (N-101 p722)
                let own: BTreeMap<String, syntax::Doc> = syntax::type_docs(&[(file.clone(), text.clone())]).into_iter().map(|(n, d)| (n, of_kinds(d))).collect();
                let parts = named.get(&name).map(|t| doc::type_parts(&name, t, own.get(&name), &type_docs, &named)).filter(|p| !p.is_empty());
                let shown = parts.map_or(ty.clone(), |p| format!("{ty}\n{p}"));
                let stands = declared.get(&name) == Some(file);
                let tdoc = own.get(&name);
                // its own doc below; its fields' or alternatives' docs are in the block (`shown`)
                let said = docs(tdoc, &[]);
                // its fields, one a line up to `}` (the record form of `type_decls`)
                if let Some(docket_machine::Type::Record(fs)) = named.get(&name).filter(|_| lines[line - 1].trim_end().ends_with('{')) {
                    for (k, l) in lines.iter().enumerate().skip(line).take_while(|(_, l)| l.trim() != "}") {
                        let Some((f, _)) = l.split("//").next().unwrap_or("").split_once(':') else { continue };
                        let f = f.trim();
                        let Some((_, t)) = fs.iter().find(|(n, _)| n == f) else { continue };
                        let ch = word_at(l, "", f).unwrap_or(0);
                        let at = place(file, &lines, k + 1, ch, f.chars().count());
                        decl_at.insert((at.line, at.col));
                        let sig = format!("{name}.{f}: {t}");
                        let fdoc: Vec<String> = tdoc.and_then(|d| d.parts.get(f)).map(|t| t.trim().to_string()).into_iter().filter(|t| !t.is_empty()).collect();
                        ix.decls.push(Decl { space: Space::Field, name: f.to_string(), what: "field", hover: with_comment(fence(&sig), &fdoc, &format!("{file}:{}", k + 1)), sig, at, stands, end: (0, 0) });
                    }
                }
                ix.decls.push(Decl { space: Space::Type, what: "type", sig: ty.clone(), hover: with_comment(fence(&shown), &said, &format!("{file}:{line}")), name, at, stands, end: (0, 0) });
            }
            // calls and decisions: every one the file declares, the standing one marked
            for c in syntax::parse_call_defs_each(file, text, &named).0 {
                let line = c.line;
                let decision = c.sig.name.starts_with(syntax::DECIDE);
                let short = c.sig.name.trim_start_matches(syntax::DECIDE).to_string();
                let ch = word_at(lines[line - 1], if decision { "decision " } else { "call " }, &short).unwrap_or(0);
                let at = place(file, &lines, line, ch, short.chars().count());
                decl_at.insert((at.line, at.col));
                let stands = calls.get(&c.sig.name).is_some_and(|s| s.file == c.file && s.line == c.line);
                // its doc and its parameters'; the outcomes as the step shows them (`doc::gives`, one rendering)
                let cdoc = of_kinds(c.doc.clone());
                let mut said = docs(Some(&cdoc), &fields_named(&c.sig.params));
                let gives = doc::gives(&c.sig.head, Some(&cdoc), &type_docs, &named);
                if !gives.is_empty() {
                    said.push(format!("```\n{gives}\n```"));
                }
                // as written (the call's lines up to its template), without its comments: the docs are below
                let indent = |l: &str| l.len() - l.trim_start().len();
                let head: Vec<&str> = std::iter::once(lines[line - 1])
                    .chain(lines[line..].iter().copied().take_while(|l| indent(l) > 0 && !l.trim_start().starts_with("template")))
                    .map(|l| l.split("//").next().unwrap_or("").trim_end())
                    .filter(|l| !l.trim().is_empty())
                    .collect();
                let head = head.join("\n");
                let sig = format!("{} {}", if decision { "decision" } else { "call" }, doc::call_sig(&docket_machine::CallSig { name: short.clone(), ..c.sig.clone() }));
                ix.decls.push(Decl { space: Space::Call, name: c.sig.name.clone(), what: if decision { "decision" } else { "call" }, at, sig, hover: with_comment(fence(&head), &said, &format!("{file}:{line}")), stands, end: (0, 0) });
            }
            // uses: types in signatures and type declarations; includes in templates
            let tmpl = template_lines(&lines);
            let dir = std::path::Path::new(file).parent().map(|p| p.display().to_string()).unwrap_or_default();
            for (i, l) in lines.iter().enumerate() {
                if tmpl.contains(&i) {
                    for (f, ch) in includes(l) {
                        let path = format!("{dir}/{f}");
                        if fs.read(&path).is_none() {
                            ix.diag(file, i, format!("include {f}: no such file in {dir}"));
                        }
                        ix.uses.push(Use { space: Space::File, name: path, at: place(file, &lines, i + 1, ch, f.chars().count()) });
                    }
                    continue;
                }
                let Ok(toks) = syntax::lex_spans(l, i + 1) else { continue };
                for (j, t) in toks.iter().enumerate() {
                    if let Tok::Ident(w) = &t.tok {
                        let at = place(file, &lines, i + 1, t.col, t.len);
                        let ctor = toks.get(j + 1).is_some_and(|n| n.tok == Tok::Sym("(")) && ctors.contains(w);
                        if type_names.contains(w) && !ctor && !decl_at.contains(&(at.line, at.col)) {
                            ix.uses.push(Use { space: Space::Type, name: w.clone(), at });
                        }
                    }
                }
            }
        }

        // the kinds (coordd ticket/mod.rs `read_defs`, docket-machine `Defs::build_with`)
        let mut kind_files: Vec<(String, String)> = Vec::new();
        for f in folders {
            for p in fs.list(&fs.place(f, "kinds"), "kind") {
                if let Some(t) = fs.read(&p) {
                    kind_files.push((p, t));
                }
            }
        }
        let project = if folders.len() > 1 { folders.last().cloned() } else { None };
        let duties = project.as_ref().and_then(|p| fs.read(&format!("{p}/policy/duties.eflint"))).is_some_and(|t| t.contains("Violated when"));
        let (defs, errs) = Defs::build_with(&kind_files, sigs, named.clone(), 1, duties);
        errors.extend(errs);
        let mut standing: BTreeMap<String, (String, usize)> = BTreeMap::new();
        let mut all_kinds: Vec<Kind> = Vec::new();
        for (file, text) in &kind_files {
            for k in syntax::parse_kinds_each(file, text, &named).0 {
                standing.insert(k.name.clone(), (k.file.clone(), k.line));
                all_kinds.push(k);
            }
        }
        all_kinds.iter().for_each(|k| ctors_of(&k.result, &mut ctors));
        for (file, text) in &kind_files {
            texts.insert(file.clone(), text.clone());
            let lines: Vec<&str> = text.lines().collect();
            let mine: Vec<&Kind> = all_kinds.iter().filter(|k| &k.file == file).collect();
            let mut named_at: Vec<(Space, String, usize)> = Vec::new();
            let mut decl_at = BTreeSet::new();
            for k in &mine {
                walk(&k.pattern, &mut named_at);
                k.handlers.iter().for_each(|h| walk(&h.body, &mut named_at));
                let ch = word_at(lines[k.line - 1], "kind ", &k.name).unwrap_or(5);
                let at = place(file, &lines, k.line, ch, k.name.chars().count());
                decl_at.insert((at.line, at.col));
                if let Some(b) = &k.base {
                    if let Some(ch) = word_at(lines[k.line - 1], " from ", b) {
                        ix.uses.push(Use { space: Space::Kind, name: b.clone(), at: place(file, &lines, k.line, ch, b.chars().count()) });
                    }
                }
                let mut parts = fields_named(&k.form);
                parts.extend(k.raises.iter().map(|r| (r.clone(), format!("raises {r}"))));
                let said = docs(Some(&k.doc), &parts);
                let refused = if defs.kinds.get(&k.name).is_some_and(|d| d.file == k.file && d.line == k.line) { "" } else { "\n\n(refused: see its diagnostics)" };
                let results = if matches!(k.result, docket_machine::Type::Variant(_)) { format!("\n\nresults:\n{}", doc::results(k)) } else { String::new() };
                let hover = with_comment(format!("{}{results}{refused}", fence(&format!("kind {}", doc::kind_sig(k)))), &said, &format!("{file}:{}", k.line));
                let stands = standing.get(&k.name) == Some(&(k.file.clone(), k.line));
                ix.decls.push(Decl { space: Space::Kind, name: k.name.clone(), what: "kind", sig: format!("kind {}", doc::kind_sig(k)), hover, at, stands, end: (0, 0) });
            }
            // the bindings (`on ev ... open k(...)`, `when d is o  open k(...)`): the kind each opens, the decision asked
            let mut asked: Vec<(String, usize)> = Vec::new();
            for b in syntax::parse_bindings(file, text).unwrap_or_default().into_iter().flatten() {
                named_at.push((Space::Kind, b.kind.clone(), b.line));
                if let syntax::Every::When { decision, .. } = &b.every {
                    asked.push((decision.clone(), b.line));
                }
            }
            let toks = syntax::lex_spans(text, 1).unwrap_or_default();
            for (j, t) in toks.iter().enumerate() {
                let Tok::Ident(w) = &t.tok else { continue };
                let at = place(file, &lines, t.from, t.col, t.len);
                if decl_at.contains(&(at.line, at.col)) {
                    continue;
                }
                // `p.after`: a field read
                if j > 0 && toks[j - 1].tok == Tok::Sym(".") && ix.decls.iter().any(|d| d.space == Space::Field && d.name == *w) {
                    ix.uses.push(Use { space: Space::Field, name: w.clone(), at });
                    continue;
                }
                // `par … reach KIND`: the kind whose ticket closing done lets the parts after it start
                if j > 0 && toks[j - 1].tok == Tok::Ident("reach".into()) && standing.contains_key(w) {
                    ix.uses.push(Use { space: Space::Kind, name: w.clone(), at });
                    continue;
                }
                if asked.iter().any(|(d, l)| d == w && *l == t.from) {
                    ix.uses.push(Use { space: Space::Policy, name: w.clone(), at });
                    continue;
                }
                if toks.get(j + 1).is_some_and(|n| n.tok == Tok::Sym("(")) {
                    let decided = format!("{}{w}", syntax::DECIDE);
                    if let Some((sp, n, _)) = named_at.iter().find(|(_, n, a)| (n == w || *n == decided) && t.from >= *a && t.from <= a + 4) {
                        ix.uses.push(Use { space: *sp, name: n.clone(), at });
                        continue;
                    }
                }
                let ctor = toks.get(j + 1).is_some_and(|n| n.tok == Tok::Sym("(")) && ctors.contains(w);
                if type_names.contains(w) && !ctor {
                    ix.uses.push(Use { space: Space::Type, name: w.clone(), at });
                }
            }
        }

        // the policy (coordd policy/mod.rs `Places::sources`): agent-base's, a project's file of the same name in its
        // place; the project's; each agent's. duties.eflint is the duty engine's and is not read here.
        let read = |dir: &str| -> Vec<String> {
            let mut fs_: Vec<String> = fs.list(dir, "eflint").into_iter().filter(|p| !p.ends_with("/duties.eflint")).collect();
            fs_.sort_by_key(|p| (!p.ends_with("/prelude.eflint"), p.clone()));
            fs_
        };
        let name = |p: &str| p.rsplit('/').next().unwrap_or("").to_string();
        let mut policy: Vec<String> = Vec::new();
        let proj: Vec<String> = project.as_ref().map(|p| read(&format!("{p}/policy"))).unwrap_or_default();
        if let Some(base) = folders.first() {
            policy.extend(read(&format!("{base}/policy")).into_iter().filter(|f| !proj.iter().any(|g| name(g) == name(f))));
        }
        policy.extend(proj);
        if let Some(p) = &project {
            policy.extend(read(&format!("{p}/policy/agents")));
        }
        let mut pdecls: Vec<(String, ps::Decl)> = Vec::new();
        for file in &policy {
            let Some(text) = fs.read(file) else { continue };
            texts.insert(file.clone(), text.clone());
            match ps::parse(file, &text) {
                Ok(ds) => pdecls.extend(ds.into_iter().map(|d| (text.clone(), d))),
                Err(e) => errors.push(e),
            }
        }
        let mut pnames: BTreeSet<String> = BTreeSet::new();
        let mut pdecl_at = BTreeSet::new();
        for (text, d) in &pdecls {
            let (what, n, word) = match &d.kind {
                ps::DeclKind::Fact { name, .. } => ("fact", name, "Fact "),
                ps::DeclKind::Act { name, .. } => ("act", name, "Act "),
                ps::DeclKind::Decision { name, .. } => ("policy decision", name, "Decision "),
                ps::DeclKind::Extend { name, .. } => ("fact extended", name, "Extend Fact "),
                _ => continue,
            };
            let lines: Vec<&str> = text.lines().collect();
            let ch = word_at(lines[d.at.line - 1], word, n).unwrap_or(0);
            let at = place(&d.at.file, &lines, d.at.line, ch, n.chars().count());
            pdecl_at.insert((at.file.clone(), at.line, at.col));
            pnames.insert(n.clone());
            let said: Vec<String> = Some(d.doc.trim().to_string()).filter(|t| !t.is_empty()).into_iter().collect();
            let head = d.text.lines().next().unwrap_or("").to_string();
            // an `Extend Fact` adds ways a fact holds; the fact is declared by its `Fact`
            let stands = !matches!(d.kind, ps::DeclKind::Extend { .. });
            ix.decls.push(Decl { space: Space::Policy, name: n.clone(), what, sig: head, hover: with_comment(fence(&d.text), &said, &d.at.to_string()), at, stands, end: (0, 0) });
        }
        for file in &policy {
            let Some(text) = texts.get(file) else { continue };
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                let ws = ps::words(l);
                let first = ws.first().map(|w| w.0.as_str()).unwrap_or("");
                for (k, (w, ch, paren)) in ws.iter().enumerate() {
                    let at = place(file, &lines, i + 1, *ch, w.chars().count());
                    if pdecl_at.contains(&(at.file.clone(), at.line, at.col)) {
                        continue;
                    }
                    let prev = if k > 0 { ws[k - 1].0.as_str() } else { "" };
                    let names_one = *paren
                        || (prev == "Fact" && first == "Extend")
                        || (prev == "for" && (first == "Prefer" || first == "Only"))
                        || (k == 1 && (first == "Judge" || first == "Options" || first == "Actor" || first == "Recipient"))
                        || (first == "Related" && k >= 2)
                        || (first == "Fact" && k >= 2 && pnames.contains(w));
                    if names_one && (pnames.contains(w) || *paren) {
                        ix.uses.push(Use { space: Space::Policy, name: w.clone(), at });
                    }
                }
            }
        }

        for d in ix.decls.iter_mut() {
            if let Some(t) = texts.get(&d.at.file) {
                let lines: Vec<&str> = t.lines().collect();
                d.end = whole_end(&lines, d.at.line, d.space == Space::Field);
            }
        }
        ix.files = texts.keys().cloned().collect();
        for e in errors {
            ix.locate(&e);
        }
        ix
    }

    fn diag(&mut self, file: &str, line: usize, msg: String) {
        self.diags.entry(file.to_string()).or_default().push((line, msg));
    }

    /// Put refusal E at the file and line it names (`FILE:LINE: ...`, `FILE: line N: ...`, `FILE: ...`), else under "".
    fn locate(&mut self, e: &str) {
        let mut files = self.files.clone();
        files.sort_by_key(|f| std::cmp::Reverse(f.len()));
        let num = |s: &str| s.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<usize>().ok();
        for f in &files {
            if let Some(rest) = e.strip_prefix(f.as_str()) {
                // the innermost place said: `line N` within (a kind's statement, a policy declaration's body line), else
                // the `:N` after the file
                let inner = rest.match_indices("line ").find_map(|(i, _)| num(&rest[i + 5..]));
                let line = inner.or_else(|| rest.strip_prefix(':').and_then(num)).unwrap_or(1);
                self.diag(f, line.saturating_sub(1), e.to_string());
                return;
            }
        }
        for f in &files {
            if let Some(i) = e.find(&format!("{f}:")) {
                let line = num(&e[i + f.len() + 1..]).unwrap_or(1);
                self.diag(f, line.saturating_sub(1), e.to_string());
                return;
            }
        }
        self.diag("", 0, e.to_string());
    }

    /// The declaration or use at FILE, LINE, COL (0-based, UTF-16): its space and name.
    pub fn at(&self, file: &str, line: usize, col: usize) -> Option<(Space, String)> {
        let hit = |p: &Place| p.file == file && p.line == line && col >= p.col && col <= p.col + p.len;
        self.uses.iter().find(|u| hit(&u.at)).map(|u| (u.space, u.name.clone())).or_else(|| self.decls.iter().find(|d| hit(&d.at)).map(|d| (d.space, d.name.clone())))
    }

    /// The declarations of NAME in SPACE, the standing one first.
    pub fn decls_of(&self, space: Space, name: &str) -> Vec<&Decl> {
        let mut out: Vec<&Decl> = self.decls.iter().filter(|d| d.space == space && d.name == name).collect();
        out.sort_by_key(|d| !d.stands);
        out
    }

    pub fn uses_of(&self, space: Space, name: &str) -> Vec<&Use> {
        self.uses.iter().filter(|u| u.space == space && u.name == name).collect()
    }

    /// The other name of the same thing: a decision is declared twice, `decision X(...)` in a call file (the call
    /// `decide X`, what a kind's `decide X(...)` steps) and `Decision X` in the policy (what its rules are for).
    pub fn twin(&self, space: Space, name: &str) -> Option<(Space, String)> {
        match space {
            Space::Call => name.strip_prefix(syntax::DECIDE).map(|n| (Space::Policy, n.to_string())),
            Space::Policy => {
                let n = format!("{}{name}", syntax::DECIDE);
                self.decls.iter().any(|d| d.space == Space::Call && d.name == n).then_some((Space::Call, n))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project of three folders held open in the editor only (nothing on disk): agent-base's policy, a library, a project.
    fn project() -> (Vec<String>, Fs) {
        let root = "/nonexistent-docket-ls-test";
        let mut fs = Fs::default();
        let mut put = |p: &str, t: &str| {
            fs.open.insert(format!("{root}/{p}"), t.to_string());
        };
        put("base/policy/prelude.eflint", "Fact agent Identified by String\n// who holds which role\nFact role Identified by agent * String\n");
        put("lib/lib.call", "/// one pick\ntype Placed {\n  piece: Text  /// the piece's name\n  agent: Agent\n}\n\n/// who does it\ncall assign(words: Text) -> Assigned(picks: [Placed]) | Ask(question: Text)\n  asks Ask\n  template\n    {{ include \"a.txt\" }}\n");
        put("lib/a.txt", "Pick.\n");
        put("lib/lib.kind", "// history, not shown\n/// A change: the words, done.\nkind change\n  form words: Text\n  result Text\n  pattern\n    a <- step owner assign(words);\n    match a {\n      Assigned(ps) => return \"done\"\n    }\n");
        put("proj/p.kind", "kind direct\n  form words: Text\n  result Text\n  pattern\n    c <- open change(words);\n    return c\n");
        put("proj/policy/p.eflint", "Act pick\n  Actor agent\n  Recipient agent\n  Holds when role(actor, \"orchestrator\")\n");
        (["base", "lib", "proj"].iter().map(|f| format!("{root}/{f}")).collect(), fs)
    }

    #[test]
    fn names_go_to_their_declarations_and_back() {
        let (folders, fs) = project();
        let ix = Index::build(&folders, &fs);
        assert!(ix.diags.is_empty(), "{:?}", ix.diags);
        // `open change(` in the project's kind: the library's kind, on its `kind` line
        let f = |s: &str| format!("/nonexistent-docket-ls-test/{s}");
        assert_eq!(ix.at(&f("proj/p.kind"), 4, 16), Some((Space::Kind, "change".into())));
        let d = &ix.decls_of(Space::Kind, "change")[0];
        assert_eq!((d.at.file.as_str(), d.at.line, d.at.col, d.at.len), (f("lib/lib.kind").as_str(), 2, 5, 6));
        assert!(d.hover.contains("kind change(words: Text) -> Text") && d.hover.contains("A change: the words, done."), "{}", d.hover);
        // `step owner assign(`: the call, its hover the signature and its doc
        assert_eq!(ix.at(&f("lib/lib.kind"), 6, 22), Some((Space::Call, "assign".into())));
        let c = &ix.decls_of(Space::Call, "assign")[0];
        assert!(c.hover.contains("call assign(words: Text) -> Assigned(picks: [Placed]) | Ask(question: Text)\n  asks Ask") && c.hover.contains("who does it"), "{}", c.hover);
        // what the step gives, as `doc::gives` writes it for the step's text: the nested type's fields with their docs
        assert!(c.hover.contains("This step gives one of:\n  Assigned\n    picks: [Placed]\n      piece         the piece's name\n      agent: Agent\n  Ask\n    question"), "{}", c.hover);
        assert!(!c.hover.contains("read as"), "{}", c.hover);
        // a field's type, and the constructor `Assigned(` that is no type
        assert_eq!(ix.uses_of(Space::Type, "Placed").len(), 1);
        let p = &ix.decls_of(Space::Type, "Placed")[0];
        assert!(p.hover.contains("one pick") && p.hover.contains("type Placed = {piece: Text, agent: Agent}\n  piece         the piece's name\n  agent: Agent\n"), "{}", p.hover);
        assert!(ix.decls_of(Space::Field, "piece")[0].hover.contains("the piece's name"));
        assert!(!d.hover.contains("history"), "{}", d.hover);
        // the policy's `role(` in a rule: the prelude's fact; `Actor agent` the fact agent
        assert_eq!(ix.at(&f("proj/policy/p.eflint"), 3, 14), Some((Space::Policy, "role".into())));
        assert_eq!(ix.decls_of(Space::Policy, "role")[0].at.file, f("base/policy/prelude.eflint"));
        assert_eq!(ix.uses_of(Space::Policy, "agent").len(), 3);
        // the template's include: the file beside the call file
        assert_eq!(ix.uses_of(Space::File, &f("lib/a.txt")).len(), 1);
    }

    #[test]
    fn a_refusal_is_put_at_its_file_and_line() {
        let (folders, mut fs) = project();
        let k = "/nonexistent-docket-ls-test/lib/lib.kind".to_string();
        let t = fs.open[&k].replace("assign(words)", "assign(words, words)");
        fs.open.insert(k.clone(), t);
        let ix = Index::build(&folders, &fs);
        let ds = &ix.diags[&k];
        assert_eq!(ds[0].0, 6, "{ds:?}");
        assert!(ds[0].1.contains("call assign takes 1"), "{ds:?}");
    }

    /// A hole no parameter of its call names is refused at its line, as coordd's load refuses it (docket-machine
    /// `template::check_step`), and the call is left out, so the kind with a step of it is refused too
    /// (paper-research N-1446 p1842: stage 2's `{{ if made }}` misspelt `{{ if madex }}` took coordd's whole `cut`
    /// kind out, and docket-ls said nothing). A hole's line and an `if`'s; the include still read beside the call.
    #[test]
    fn a_hole_no_parameter_names_is_refused_at_its_line() {
        for (hole, says) in [("{{ madex }}", "{{ madex }}: no `madex` here (the call's parameters: words)"), ("{{ if madex }}x{{ end }}", "{{ madex }}: no `madex` here (the call's parameters: words)")] {
            let (folders, mut fs) = project();
            let c = "/nonexistent-docket-ls-test/lib/lib.call".to_string();
            let t = fs.open[&c].replace("    {{ include \"a.txt\" }}\n", &format!("    {{{{ include \"a.txt\" }}}}\n    Then {hole}.\n"));
            assert_ne!(t, fs.open[&c]);
            fs.open.insert(c.clone(), t);
            let ix = Index::build(&folders, &fs);
            let ds = &ix.diags[&c];
            // the file's line 12 (0-based 11): the hole's, not the call's (line 8)
            assert_eq!(ds.iter().map(|(l, _)| *l).collect::<Vec<_>>(), vec![11], "{ds:?}");
            assert!(ds[0].1.contains(says) && ds[0].1.contains("the call assign is left out"), "{ds:?}");
            let k = "/nonexistent-docket-ls-test/lib/lib.kind";
            assert!(ix.diags.get(k).is_some_and(|ds| ds.iter().any(|(_, m)| m.contains("assign"))), "{:?}", ix.diags);
        }
        // as it was, nothing refused there: the include is read, and `words` is the call's
        let (folders, fs) = project();
        let ix = Index::build(&folders, &fs);
        for f in ["lib/lib.call", "lib/lib.kind"] {
            assert!(ix.diags.get(&format!("/nonexistent-docket-ls-test/{f}")).is_none_or(|ds| ds.is_empty()), "{:?}", ix.diags);
        }
    }
}
