//! Every eFLINT text under docket/ reads, and every query in it succeeds: each query counted where a statement starts
//! with `?`, outside strings and comments.

use std::path::{Path, PathBuf};

fn walk(d: &Path, v: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(d).unwrap_or_else(|e| panic!("{}: {e}", d.display())) {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, v);
        } else if p.extension().is_some_and(|x| x == "eflint") {
            v.push(p);
        }
    }
}

/// How many statements of TEXT are queries: a `?` where a statement starts, after the text's start or a `.`, with
/// only space or comments between; a `?` in a string or a comment is none.
fn queries(text: &str) -> usize {
    let (b, mut i, mut n, mut start) = (text.as_bytes(), 0, 0, true);
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => i = text[i..].find('\n').map_or(b.len(), |k| i + k),
            b'/' if b.get(i + 1) == Some(&b'*') => i = text[i + 2..].find("*/").map_or(b.len(), |k| i + 2 + k + 2),
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                start = false;
            }
            c if c.is_ascii_whitespace() => i += 1,
            c => {
                n += usize::from(start && c == b'?');
                start = c == b'.';
                i += 1;
            }
        }
    }
    n
}

#[test]
fn a_query_is_counted_where_a_statement_starts_and_nowhere_else() {
    let text = "// ?not(\"a\").\n/* ?nor(\"b\").\n?this */ +f(\"a ?b\").\n?f(\"a\"). ?!f(\"b\").\nFact x Identified by String.\n  ?x(\"?\").";
    assert_eq!(queries(text), 3);
}

#[test]
fn every_eflint_text_reads_and_answers_its_queries() {
    let mut files = Vec::new();
    walk(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docket")), &mut files);
    files.sort();
    assert!(!files.is_empty());
    let (mut bad, mut all) = (Vec::new(), 0);
    for f in &files {
        let mut out = Vec::new();
        let read = eflint::session::Session::new(&mut out, false).run_file(f);
        let out = String::from_utf8_lossy(&out);
        let asked = queries(&std::fs::read_to_string(f).unwrap());
        let ok = out.matches("query successful").count();
        all += asked;
        if !read || out.contains("query failed") || ok != asked {
            bad.push(format!("{}: read {read}, {asked} asked, {ok} succeeded\n{out}", f.display()));
        }
    }
    eprintln!("{} eFLINT texts, {all} queries", files.len());
    assert!(bad.is_empty() && all > 0, "{}", bad.join("\n"));
}
