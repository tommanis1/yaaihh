//! The commit's file (paper-research2 N-101 p1589 parts c and d; the form agreed on N-387, posts 2 to 4): what the
//! publish's prepare wrote of each module at the line's commit, read in place of asking the server for a file the
//! view has not changed. `[[lsp]] records = { dir = "…", toolchain = "…" }` names the folder; under
//! `DIR/TOOLCHAIN/`:
//!
//! - `manifests/SET.json.zst`: SET as the published sets are named (`tovrin@aeefe9f4e04b`,
//!   `tovrin@aeefe9f4e04b-p4-tovrin-definition@c51d11563c0a`): each path (`REPO/PATH`) with its module and its
//!   record's key, or why it was not checked; each module's direct imports; every defined name, locals included,
//!   with its module, kind and defining range.
//! - `records/KEY.json.zst`: a module's outline (with each name's signature and its type as Agda printed it) and
//!   every name site with the qualified name it stands for.
//! - `uses/SET.json.zst`: each qualified name and the records that use it.
//!
//! Ranges are LSP's: 0-based lines and UTF-16 characters. A folder of another build (another TOOLCHAIN) is not
//! read: every file is then the server's, as before.

use crate::index::Sym;
use crate::lsp::Rng;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Where the records are: `[[lsp]] records`.
#[derive(Clone, Debug, PartialEq)]
pub struct Conf {
    pub dir: PathBuf,
    pub toolchain: String,
}

/// A path of the commit: its module, and its record's key or the reason it has none.
pub struct Entry {
    pub module: String,
    pub record: std::result::Result<String, String>,
}

/// A name the commit defines: its module and kind as numbers into the commit's `modules` and `kinds` (a table of
/// every name, locals included, is held by every symbols process: N-101 p1620).
pub struct Name {
    pub full: Box<str>,
    pub module: u32,
    pub kind: u16,
    pub range: Rng,
    /// Of an instance method (Haskell, N-387 p13): the instance it is in (`instance Show Foo`).
    pub instance: Option<Box<str>>,
}

/// One module's record, as far as symbols reads it.
pub struct Record {
    pub module: String,
    pub outline: Value,
    /// Each name site: where, and the qualified name it stands for.
    pub sites: Vec<(Rng, String, bool)>,
}

/// The manifests of a view's commits, merged.
pub struct Commit {
    /// The build's folder (`DIR/TOOLCHAIN`).
    pub dir: PathBuf,
    /// The sets read, by name.
    pub sets: Vec<String>,
    pub files: HashMap<PathBuf, Entry>,
    pub module_file: HashMap<String, PathBuf>,
    pub imports: HashMap<String, Vec<String>>,
    pub names: Vec<Name>,
    pub modules: Vec<String>,
    pub kinds: Vec<String>,
    /// Haskell's form (N-387 p13): a name and a module are `UNIT:Module.name`, the unit before the first `:`.
    pub haskell: bool,
    /// A name exported from a module not its own (`UNIT:M.x`) and the name it stands for there (`UNIT:Orig.x`).
    pub exports: HashMap<String, String>,
    /// Each qualified name and the keys of the records that use it, read at the first `references`.
    uses: Option<HashMap<String, Vec<String>>>,
}

/// The JSON in zstd file PATH (`zstd -dc`: no library in symbols for it).
pub fn read_zst(path: &Path) -> Result<Value> {
    let out = std::process::Command::new("zstd").args(["-dcq", "--"]).arg(path).output().context("running zstd")?;
    if !out.status.success() {
        bail!("zstd -dc {}: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim());
    }
    let v: Value = serde_json::from_slice(&out.stdout).with_context(|| format!("{} is not JSON", path.display()))?;
    match v.get("format").and_then(Value::as_u64) {
        Some(1) => Ok(v),
        f => bail!("{}: format {f:?}, this symbols reads 1", path.display()),
    }
}

/// `[l0, c0, l1, c1]`.
pub fn rng4(v: &Value) -> Option<Rng> {
    let a = v.as_array()?;
    let n = |i: usize| a.get(i).and_then(Value::as_u64).and_then(|x| u32::try_from(x).ok());
    Some(Rng { start_line: n(0)?, start_char: n(1)?, end_line: n(2)?, end_char: n(3)? })
}

/// The sets a view's REPOS read: the main repository's (MAIN) alone, then each other repository with it.
pub fn sets(repos: &[crate::view::Repo], main: &str) -> Vec<String> {
    let short = |b: &str| b.chars().take(12).collect::<String>();
    let Some(m) = repos.iter().find(|r| r.name == main).or(repos.first()) else { return Vec::new() };
    let first = format!("{}@{}", m.name, short(&m.base));
    let mut out = vec![first.clone()];
    out.extend(repos.iter().filter(|r| r.name != m.name).map(|r| format!("{first}-{}@{}", r.name, short(&r.base))));
    out
}

/// The commit of REPOS as CONF's build wrote it; None when no manifest of this build is there for their bases.
pub fn open(conf: &Conf, repos: &[crate::view::Repo], main: &str, haskell: bool) -> Result<Option<Commit>> {
    let dir = conf.dir.join(&conf.toolchain);
    let mut c = Commit { dir: dir.clone(), sets: Vec::new(), files: HashMap::new(), module_file: HashMap::new(), imports: HashMap::new(), names: Vec::new(), modules: Vec::new(), kinds: Vec::new(), haskell, exports: HashMap::new(), uses: None };
    let mut seen_names = HashSet::new();
    let mut module_no: HashMap<String, u32> = HashMap::new();
    let mut kind_no: HashMap<String, u16> = HashMap::new();
    for set in sets(repos, main) {
        let f = dir.join("manifests").join(format!("{set}.json.zst"));
        if !f.exists() {
            continue;
        }
        let m = read_zst(&f)?;
        let place = |p: &str| -> Option<PathBuf> {
            let (repo, rel) = p.split_once('/')?;
            repos.iter().find(|r| r.name == repo).map(|r| r.root.join(rel))
        };
        for (p, e) in m.get("files").and_then(Value::as_object).into_iter().flatten() {
            let Some(abs) = place(p) else { continue };
            let module = e.get("module").and_then(Value::as_str).unwrap_or("").to_string();
            let record = match (e.get("record").and_then(Value::as_str), e.get("unchecked").and_then(Value::as_str)) {
                (Some(k), _) => Ok(k.to_string()),
                (None, why) => Err(why.unwrap_or("no record").to_string()),
            };
            c.module_file.insert(module.clone(), abs.clone());
            c.files.insert(abs, Entry { module, record });
        }
        for (m, is) in m.get("imports").and_then(Value::as_object).into_iter().flatten() {
            c.imports.insert(m.clone(), is.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect());
        }
        for n in m.get("names").and_then(Value::as_array).into_iter().flatten() {
            let s = |i: usize| n.get(i).and_then(Value::as_str);
            let (Some(full), Some(module), Some(kind), Some(range)) = (s(0), s(1), s(2), n.get(3).and_then(rng4)) else { continue };
            // a name two sets both list (the main repository's, in each pair set) once
            let h = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (full, module, range.start_line, range.start_char).hash(&mut h);
                h.finish()
            };
            if seen_names.insert(h) {
                let mn = *module_no.entry(module.to_string()).or_insert_with(|| {
                    c.modules.push(module.to_string());
                    (c.modules.len() - 1) as u32
                });
                let kn = *kind_no.entry(kind.to_string()).or_insert_with(|| {
                    c.kinds.push(kind.to_string());
                    (c.kinds.len() - 1) as u16
                });
                c.names.push(Name { full: full.into(), module: mn, kind: kn, range, instance: s(4).map(Into::into) });
            }
        }
        for (_, es) in m.get("exports").and_then(Value::as_object).into_iter().flatten() {
            for e in es.as_array().into_iter().flatten() {
                if let (Some(a), Some(o)) = (e.get(0).and_then(Value::as_str), e.get(1).and_then(Value::as_str)) {
                    c.exports.insert(a.to_string(), o.to_string());
                }
            }
        }
        c.sets.push(set);
        drop(m);
        // the decoded manifest's memory back to the system, not kept by the allocator for nothing
        unsafe {
            libc::malloc_trim(0);
        }
    }
    c.names.shrink_to_fit();
    Ok(if c.sets.is_empty() { None } else { Some(c) })
}

impl Commit {
    /// N's module.
    pub fn module(&self, n: &Name) -> &str {
        &self.modules[n.module as usize]
    }

    /// N's kind.
    pub fn kind(&self, n: &Name) -> &str {
        &self.kinds[n.kind as usize]
    }

    /// The record of KEY.
    pub fn record(&self, key: &str) -> Result<Record> {
        let v = read_zst(&self.dir.join("records").join(format!("{key}.json.zst")))?;
        let module = v.get("module").and_then(Value::as_str).unwrap_or("").to_string();
        let sites = v
            .get("sites")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| {
                let a = s.as_array()?;
                let r = rng4(&Value::Array(a.get(..4)?.to_vec()))?;
                // a bound variable's site (null) names no definition; Haskell's type namespace is marked "type"
                Some((r, a.get(4)?.as_str()?.to_string(), a.get(5).and_then(Value::as_str) == Some("type")))
            })
            .collect();
        Ok(Record { module, outline: v.get("outline").cloned().unwrap_or(Value::Null), sites })
    }

    /// Whether the record of KEY is in this build's folder.
    pub fn has_record(&self, key: &str) -> bool {
        self.dir.join("records").join(format!("{key}.json.zst")).exists()
    }

    /// The keys of the records that use NAME (every set's uses file, read once).
    pub fn users(&mut self, name: &str) -> Result<Vec<String>> {
        if self.uses.is_none() {
            let mut all: HashMap<String, Vec<String>> = HashMap::new();
            for set in &self.sets {
                let f = self.dir.join("uses").join(format!("{set}.json.zst"));
                if !f.exists() {
                    continue;
                }
                for (n, ks) in read_zst(&f)?.get("uses").and_then(Value::as_object).into_iter().flatten() {
                    let e = all.entry(n.clone()).or_default();
                    for k in ks.as_array().into_iter().flatten().filter_map(Value::as_str) {
                        if !e.iter().any(|x| x == k) {
                            e.push(k.to_string());
                        }
                    }
                }
            }
            self.uses = Some(all);
        }
        Ok(self.uses.as_ref().unwrap().get(name).cloned().unwrap_or_default())
    }

    /// NAME without its unit (Haskell's `UNIT:M.x` is `M.x`); as it is elsewhere.
    pub fn bare<'a>(&self, name: &'a str) -> &'a str {
        if self.haskell { name.split_once(':').map_or(name, |(_, r)| r) } else { name }
    }

    /// The path whose record is KEY.
    pub fn file_of_key(&self, key: &str) -> Option<&PathBuf> {
        self.files.iter().find(|(_, e)| e.record.as_deref() == Ok(key)).map(|(p, _)| p)
    }

    /// The modules that import one of CHANGED, directly or through others (CHANGED not among them).
    pub fn importers(&self, changed: &HashSet<String>) -> HashSet<String> {
        let mut out: HashSet<String> = HashSet::new();
        let mut grew = true;
        while grew {
            grew = false;
            for (m, is) in &self.imports {
                if !out.contains(m) && !changed.contains(m) && is.iter().any(|i| changed.contains(i) || out.contains(i)) {
                    out.insert(m.clone());
                    grew = true;
                }
            }
        }
        out
    }
}

/// The symbols of FILE from its record REC (server I's), as `outline_syms` makes them from a server's outline.
pub fn record_syms(server: usize, file: &Path, rec: &Record, c: &Commit) -> Vec<Sym> {
    fn walk(v: &Value, prefix: &[String], depth: usize, server: usize, file: &Path, out: &mut Vec<Sym>) {
        for s in v.as_array().into_iter().flatten() {
            let name = s.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let mut full = prefix.to_vec();
            full.push(name);
            out.push(Sym {
                server,
                file: file.to_path_buf(),
                full: full.clone(),
                kind: s.get("kind").and_then(Value::as_u64).unwrap_or(0),
                range: s.get("range").and_then(rng4).unwrap_or_default(),
                sel: s.get("sel").and_then(rng4).unwrap_or_default(),
                depth,
                kind_name: s.get("kind_name").and_then(Value::as_str).map(String::from),
                signature: s.pointer("/signature/range").and_then(rng4),
                pieces: s.get("pieces").and_then(Value::as_array).map(|a| a.iter().filter_map(rng4).collect()).unwrap_or_default(),
                detail: None,
                typed: s.get("type").and_then(Value::as_str).map(String::from),
                of_record: true,
            });
            if let Some(c) = s.get("children") {
                walk(c, &full, depth + 1, server, file, out);
            }
        }
    }
    let mut out = Vec::new();
    // agda-ls's outline has the module as its top entry, named in full (tool-scan N-387 p22: "at its outline's top
    // entry"): as from a live server (index.rs `outline_syms`), it is no symbol of its own, and what is in it stands
    // at the top under its name; an outline without it is the module's own names
    let prefix: Vec<String> = c.bare(&rec.module).split('.').filter(|x| !x.is_empty()).map(String::from).collect();
    let module = |s: &Value| matches!(s.get("kind").and_then(Value::as_u64), Some(1 | 2)) || s.get("kind_name").and_then(Value::as_str) == Some("module");
    for s in rec.outline.as_array().into_iter().flatten() {
        if module(s) {
            let segs: Vec<String> = c.bare(s.get("name").and_then(Value::as_str).unwrap_or("")).split('.').filter(|x| !x.is_empty()).map(String::from).collect();
            walk(s.get("children").unwrap_or(&Value::Null), &segs, 0, server, file, &mut out);
        } else {
            walk(&Value::Array(vec![s.clone()]), &prefix, 0, server, file, &mut out);
        }
    }
    out
}

/// `records = { dir, toolchain }` of an `[[lsp]]` entry, its `dir` from the project file's folder or `~/`.
pub fn conf_of(v: &toml::Value, lang: &str, project_dir: &Path) -> Result<Conf> {
    let t = v.as_table().ok_or_else(|| anyhow!("[[lsp]] {lang}: records must be a table, {{ dir, toolchain }}"))?;
    let s = |k: &str| t.get(k).and_then(toml::Value::as_str).map(str::trim).filter(|x| !x.is_empty()).ok_or_else(|| anyhow!("[[lsp]] {lang}: records needs {k}"));
    let dir = s("dir")?;
    let dir = match dir.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => match dir.strip_prefix("$COORD") {
            Some(rest) => PathBuf::from(format!("{}{rest}", std::env::var("COORD").unwrap_or_default())),
            None => project_dir.join(dir),
        },
    };
    Ok(Conf { dir, toolchain: s("toolchain")?.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rss_kb() -> u64 {
        std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|l| l.starts_with("VmRSS")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()).unwrap_or(0)
    }

    /// 6. Opening a manifest the size of Tovrin's (602 modules; names, locals included, at SYMBOLS_NAMES, 60000 by
    /// default): its size, the time and the resident size it takes. A measurement: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_manifest_of_tovrins_size_opens_in_well_under_a_second() {
        let names: usize = std::env::var("SYMBOLS_NAMES").ok().and_then(|n| n.parse().ok()).unwrap_or(60000);
        let top = std::env::temp_dir().join(format!("symbols-manifest-{}", std::process::id()));
        let dir = top.join("tc").join("manifests");
        std::fs::create_dir_all(&dir).unwrap();
        let files: serde_json::Map<String, Value> = (0..602).map(|i| (format!("r/agda/m/Module{i}/Part.agda"), json!({"sha256": format!("{:064x}", i), "module": format!("Interface.Engine.Module{i}.Part"), "record": format!("{:064x}", i * 7)}))).collect();
        let imports: serde_json::Map<String, Value> = (0..602).map(|i| (format!("Interface.Engine.Module{i}.Part"), json!((0..8).map(|k| format!("Interface.Engine.Module{}.Part", (i + k * 13) % 602)).collect::<Vec<_>>()))).collect();
        let ns: Vec<Value> = (0..names).map(|k| json!([format!("Interface.Engine.Module{}.Part.someDefinition{k}", k % 602), format!("Interface.Engine.Module{}.Part", k % 602), if k % 3 == 0 { "local" } else { "function" }, [k % 2000, 2, k % 2000, 20]])).collect();
        let base = "0123456789ab0123456789ab0123456789ab0123";
        let set = "r@0123456789ab";
        let f = dir.join(format!("{set}.json"));
        std::fs::write(&f, serde_json::to_string(&json!({"format": 1, "set": set, "files": files, "imports": imports, "names": ns})).unwrap()).unwrap();
        let json_size = std::fs::metadata(&f).unwrap().len();
        assert!(std::process::Command::new("zstd").args(["-qf", "--rm", "-19"]).arg(&f).status().unwrap().success());
        let zst_size = std::fs::metadata(dir.join(format!("{set}.json.zst"))).unwrap().len();
        let repos = vec![crate::view::Repo { name: "r".into(), root: "/v".into(), base: base.into() }];
        unsafe {
            libc::malloc_trim(0);
        }
        let before = rss_kb();
        let t = std::time::Instant::now();
        let c = open(&Conf { dir: top.clone(), toolchain: "tc".into() }, &repos, "r", false).unwrap().unwrap();
        let took = t.elapsed();
        let after = rss_kb();
        eprintln!("MANIFEST {} files, {} names: {} KB JSON, {} KB zstd; opened in {} ms, resident +{} MB", c.files.len(), c.names.len(), json_size / 1024, zst_size / 1024, took.as_millis(), after.saturating_sub(before) / 1024);
        assert_eq!(c.names.len(), names);
        let _ = std::fs::remove_dir_all(&top);
    }
}
