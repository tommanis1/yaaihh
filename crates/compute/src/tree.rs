//! Directory trees as REAPI's `Directory` blobs: built from files, written out, and read back from a folder.

use crate::pb::{Enc, Msg};
use crate::reapi::{digest, read_digest};
use crate::Digest;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A file of a tree, by its path inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub data: Vec<u8>,
    pub executable: bool,
}

enum Node<'a> {
    File(&'a File),
    Dir(BTreeMap<&'a str, Node<'a>>),
}

/// The root directory's digest for `files`, and every blob of the tree: contents and directories.
pub fn tree(files: &[File]) -> Result<(Digest, Vec<Vec<u8>>), String> {
    let mut root = BTreeMap::new();
    for f in files {
        let parts: Vec<&str> = f.path.split('/').collect();
        if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
            return Err(format!("{}: not a path inside the tree", f.path));
        }
        let mut dir = &mut root;
        for p in &parts[..parts.len() - 1] {
            match dir.entry(*p).or_insert_with(|| Node::Dir(BTreeMap::new())) {
                Node::Dir(d) => dir = d,
                Node::File(_) => return Err(format!("{}: under a file", f.path)),
            }
        }
        if dir.insert(parts[parts.len() - 1], Node::File(f)).is_some() {
            return Err(format!("{}: twice", f.path));
        }
    }
    let mut blobs = Vec::new();
    let d = encode(&root, &mut blobs);
    Ok((d, blobs))
}

fn encode(dir: &BTreeMap<&str, Node>, blobs: &mut Vec<Vec<u8>>) -> Digest {
    let (mut files, mut dirs) = (Enc::new(), Enc::new());
    for (name, n) in dir {
        match n {
            Node::File(f) => {
                files = files.msg(1, Enc::new().str(1, name).msg(2, digest(&Digest::of(&f.data))).uint(4, f.executable.into()));
                blobs.push(f.data.clone());
            }
            Node::Dir(d) => dirs = dirs.msg(2, Enc::new().str(1, name).msg(2, digest(&encode(d, blobs)))),
        }
    }
    let mut b = files.done();
    b.extend(dirs.done());
    let d = Digest::of(&b);
    blobs.push(b);
    d
}

/// A root directory holding each of `dirs`, a tree already in the store, under its name; its digest and its one blob.
pub fn nest(dirs: &[(&str, Digest)]) -> Result<(Digest, Vec<u8>), String> {
    let mut sorted: BTreeMap<&str, &Digest> = BTreeMap::new();
    for (name, d) in dirs {
        if name.is_empty() || name.contains('/') || *name == "." || *name == ".." {
            return Err(format!("{name:?}: not a name in a directory"));
        }
        if sorted.insert(name, d).is_some() {
            return Err(format!("{name}: twice"));
        }
    }
    let b = sorted.into_iter().fold(Enc::new(), |e, (name, d)| e.msg(2, Enc::new().str(1, name).msg(2, digest(d)))).done();
    Ok((Digest::of(&b), b))
}

/// Writes the tree at `root` into folder `into`, each blob taken from `get`.
pub fn materialize(root: &Digest, get: &mut dyn FnMut(&Digest) -> Result<Vec<u8>, String>, into: &Path) -> Result<(), String> {
    let b = get(root)?;
    let m = Msg::read(&b)?;
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    for (field, is_dir) in [(1, false), (2, true)] {
        for raw in m.all(field) {
            let n = Msg::read(raw)?;
            let name = n.str(1);
            if name.is_empty() || name.contains('/') || name == ".." || name == "." {
                return Err(format!("{name:?}: not a name in a directory"));
            }
            let d = read_digest(&n.msg(2)?.ok_or("a node with no digest")?);
            let at = into.join(&name);
            if is_dir {
                materialize(&d, get, &at)?;
            } else {
                std::fs::write(&at, get(&d)?).map_err(|e| format!("{}: {e}", at.display()))?;
                let mode = if n.uint(4) != 0 { 0o755 } else { 0o644 };
                std::fs::set_permissions(&at, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// The files under folder `dir`, by their paths inside it; symlinks and the like are left out.
pub fn collect(dir: &Path) -> Result<Vec<File>, String> {
    fn walk(base: &Path, at: &Path, out: &mut Vec<File>) -> Result<(), String> {
        for e in std::fs::read_dir(at).map_err(|e| format!("{}: {e}", at.display()))? {
            let p = e.map_err(|e| e.to_string())?.path();
            let m = std::fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
            if m.is_dir() {
                walk(base, &p, out)?;
            } else if m.is_file() {
                let path = p.strip_prefix(base).map_err(|e| e.to_string())?.to_string_lossy().into_owned();
                let data = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                out.push(File { path, data, executable: m.permissions().mode() & 0o111 != 0 });
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_is_the_same_whatever_the_order_and_comes_back_out() {
        let f = |p: &str, d: &str, x| File { path: p.into(), data: d.as_bytes().to_vec(), executable: x };
        let a = [f("src/main.sh", "echo hi", true), f("README", "r", false), f("src/lib/x", "x", false)];
        let b = [a[2].clone(), a[0].clone(), a[1].clone()];
        let (ra, blobs) = tree(&a).unwrap();
        assert_eq!(ra, tree(&b).unwrap().0, "sorted by name, not by order given");
        assert!(tree(&[f("../x", "", false)]).is_err());
        let store: BTreeMap<Digest, Vec<u8>> = blobs.into_iter().map(|b| (Digest::of(&b), b)).collect();
        let dir = std::env::temp_dir().join(format!("compute-tree-{}", std::process::id()));
        materialize(&ra, &mut |d| store.get(d).cloned().ok_or("missing".into()), &dir).unwrap();
        let mut back = collect(&dir).unwrap();
        back.sort_by(|x, y| x.path.cmp(&y.path));
        assert_eq!(back, [a[1].clone(), a[2].clone(), a[0].clone()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nested_trees_come_out_each_under_its_name_and_are_the_tree_built_whole() {
        let f = |p: &str, d: &str| File { path: p.into(), data: d.as_bytes().to_vec(), executable: false };
        let (one, mut blobs) = tree(&[f("answer", "42")]).unwrap();
        let (two, more) = tree(&[f("check", "c"), f("lib/x", "x")]).unwrap();
        blobs.extend(more);
        let (root, b) = nest(&[("work", one.clone()), ("verifier", two.clone())]).unwrap();
        assert_eq!(root, nest(&[("verifier", two.clone()), ("work", one.clone())]).unwrap().0, "sorted by name");
        assert_eq!(root, tree(&[f("work/answer", "42"), f("verifier/check", "c"), f("verifier/lib/x", "x")]).unwrap().0);
        blobs.push(b);
        let store: BTreeMap<Digest, Vec<u8>> = blobs.into_iter().map(|b| (Digest::of(&b), b)).collect();
        let dir = std::env::temp_dir().join(format!("compute-nest-{}", std::process::id()));
        materialize(&root, &mut |d| store.get(d).cloned().ok_or("missing".into()), &dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("work/answer")).unwrap(), "42");
        assert!(dir.join("verifier/lib/x").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(nest(&[("a", one.clone()), ("a", two.clone())]).is_err() && nest(&[("../a", one)]).is_err());
    }
}
