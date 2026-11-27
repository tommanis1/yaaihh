//! A session: a configuration, the files read so far, and where reports go.

use crate::ast::*;
use crate::compile::Static;
use crate::eval::{make_assignments, rebase_and_saturate, store_union, Ev, Store};
use crate::interp::{report, step, Config, Output};
use crate::parser::{parse_statements, parse_old_file};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Session<'w> {
    pub cfg: Config,
    pub test_mode: bool,
    include_paths: Vec<PathBuf>,
    included: Vec<PathBuf>,
    /// The number of the current configuration (1 at the start, one more per phrase that ran).
    pub state_id: usize,
    out: &'w mut dyn Write,
}

/// The directory part of a path the way file names are split: `.` for a file name alone.
fn dir_of(p: &Path) -> PathBuf {
    match p.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

impl<'w> Session<'w> {
    pub fn new(out: &'w mut dyn Write, test_mode: bool) -> Session<'w> {
        let mut s = Session {
            cfg: Config::default(),
            test_mode,
            include_paths: Vec::new(),
            included: Vec::new(),
            state_id: 1,
            out,
        };
        if let Ok(cwd) = std::env::current_dir() {
            s.add_include_path(&cwd);
        }
        s
    }

    /// Where `#include` and `#require` look for a file: the working folder, then the folders of the files read.
    fn add_include_path(&mut self, dir: &Path) {
        if dir.is_dir() {
            self.include_paths.push(dir.to_path_buf());
        }
    }

    fn print(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
    }

    /// Run one phrase and print its report.
    pub fn run_phrase(&mut self, p: &Phrase) -> Vec<Output> {
        let (outs, next) = step(&self.cfg, p);
        let text = match &next {
            Some(new) => report(&outs, &self.cfg, new, self.test_mode),
            None => report(&outs, &self.cfg, &self.cfg, self.test_mode),
        };
        self.print(&text);
        if let Some(new) = next {
            self.cfg = new;
            self.state_id += 1;
        }
        outs
    }

    pub fn run_statements(&mut self, statements: &[Statement]) {
        for statement in statements {
            match statement {
                Statement::Phrase(p) => {
                    self.run_phrase(p);
                }
                Statement::Include(fp) => self.import(fp),
                Statement::Require(fp) => {
                    if !self.has_been_included(fp) {
                        self.import(fp)
                    }
                }
            }
        }
    }

    /// Run a line or a text of phrases; a parse error is printed.
    pub fn run_text(&mut self, text: &str) {
        match parse_statements(text) {
            Ok(statements) => self.run_statements(&statements),
            Err(e) => self.print(&format!("input:{}\n", e)),
        }
    }

    fn find_file(&self, fp: &str) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for dir in &self.include_paths {
            let file = dir.join(fp);
            if file.is_file() {
                found.push(file);
            } else {
                let mut with_ext = file.into_os_string();
                with_ext.push(".eflint");
                let with_ext = PathBuf::from(with_ext);
                if with_ext.is_file() {
                    found.push(with_ext);
                }
            }
        }
        found
    }

    fn has_been_included(&self, fp: &str) -> bool {
        match self.find_file(fp).first() {
            None => false,
            Some(file) => match file.canonicalize() {
                Ok(c) => self.included.iter().any(|i| i.canonicalize().map_or(false, |ic| ic == c)),
                Err(_) => false,
            },
        }
    }

    fn import(&mut self, fp: &str) {
        let Some(file) = self.find_file(fp).into_iter().next() else {
            let dirs: Vec<String> = self.include_paths.iter().map(|d| show_string(&d.to_string_lossy())).collect();
            self.print(&format!("could not find {} in [{}]\n", fp, dirs.join(",")));
            return;
        };
        self.add_include_path(&dir_of(&file));
        self.included.push(file.clone());
        if file.extension().map_or(false, |e| e == "json") {
            self.print("including .json files is no longer supported\n");
            return;
        }
        match std::fs::read_to_string(&file) {
            Err(e) => self.print(&format!("{}: {}\n", fp, e)),
            Ok(text) => {
                let was = std::mem::replace(&mut self.cfg.spec.file, std::rc::Rc::from(fp));
                self.run_text(&text);
                self.cfg.spec.file = was;
            }
        }
    }

    /// Run a file given on the command line. Returns whether the file was read and parsed.
    pub fn run_file(&mut self, path: &Path) -> bool {
        self.add_include_path(&dir_of(path));
        if path.is_file() {
            self.included.push(path.to_path_buf());
        }
        self.cfg.spec.file = std::rc::Rc::from(path.display().to_string());
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.print(&format!("{}: {}\n", path.display(), e));
                return false;
            }
        };
        match parse_statements(&text) {
            Ok(statements) => {
                self.run_statements(&statements);
                true
            }
            Err(e1) => match parse_old_file(&text) {
                Ok(old) => self.run_old_file(&old),
                Err(e2) => {
                    // the four-part format is told by its `##` separators
                    let file = path.display();
                    if text.lines().any(|l| l.trim_start().starts_with("##")) {
                        self.print(&format!(
                            "{}:{}\n(read as a file of four parts: {}:{})\n",
                            file, e1, file, e2
                        ));
                    } else {
                        self.print(&format!("{}:{}\n", file, e1));
                    }
                    false
                }
            },
        }
    }

    /// A file in the four-part format: its declarations and its initial state are loaded.
    fn run_old_file(&mut self, old: &OldFile) -> bool {
        let mut spec = self.cfg.spec.clone();
        let initial = match (Static { spec: &mut spec }).old_file(old) {
            Ok((initial, _scenario)) => initial,
            Err(errs) => {
                self.print(&format!("compilation errors:\n{}\n\n", errs.join("\n")));
                return false;
            }
        };
        if self.test_mode {
            return true;
        }
        for (name, dom) in &old.refiner {
            if let Some(rc) = spec.decls.get_mut(name) {
                let fits = match (dom, &rc.domain) {
                    (Domain::Strings(_), Domain::AnyString) | (Domain::Ints(_), Domain::AnyInt) => true,
                    (Domain::Strings(a), Domain::Strings(b)) => a.iter().all(|s| b.contains(s)),
                    (Domain::Ints(a), Domain::Ints(b)) => a.iter().all(|i| b.contains(i)),
                    _ => false,
                };
                if fits {
                    std::rc::Rc::make_mut(rc).domain = dom.clone();
                }
            }
        }
        let empty = crate::eval::State::default();
        let state = (|| {
            let mut ev = Ev::new(&spec, &empty);
            let mut store = Store::new();
            for e in initial.iter().rev() {
                store = store_union(ev.eval_effect(e)?, store);
            }
            rebase_and_saturate(&spec, make_assignments(&spec, &store, &empty))
        })();
        match state {
            Ok(state) => {
                self.cfg = Config { spec, state, transitions: Vec::new(), duties: Vec::new() };
                true
            }
            Err(e) => {
                self.print(&format!("{}\n", e.message()));
                false
            }
        }
    }
}
