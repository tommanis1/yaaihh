//! Who a connection acts as: the cgroup of the process at its other end, as a table vouches for it (spec 8.8).

// The kernel gives the process; the frame reads its cgroup; the table says which party it is and which driver
// serves its runs. Its rows come from the parties file at start and from each session the daemon starts. Nothing
// the connection sends can change it.

use docket_v2::run::value::Value;
use std::cell::RefCell;
use std::io;
use std::os::fd::RawFd;
use std::rc::Rc;

/// A connection's party, the driver (by its place among the linked drivers) that serves the runs it begins, and the
/// working folder its runs are bound to (empty: none), whose head each `effect` line reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Party {
    pub name: String,
    pub driver: Option<usize>,
    pub folder: String,
}

impl Party {
    /// Party `name`, its runs served by `driver`, bound to no folder.
    pub fn of(name: &str, driver: Option<usize>) -> Party {
        Party { name: name.to_string(), driver, folder: String::new() }
    }
}

/// The word for the processes of each cgroup: the party of `cgroup`, or none.
pub trait Vouch {
    fn party(&self, cgroup: &str) -> Option<Party>;
}

impl<V: Vouch> Vouch for Rc<V> {
    fn party(&self, cgroup: &str) -> Option<Party> {
        V::party(self, cgroup)
    }
}

/// A driver's name as the table writes it: the texts of its name's parts joined by `/`, as `claude-code/2.1`.
pub fn written(name: &[Value]) -> String {
    fn text(v: &Value) -> String {
        match v {
            Value::Text(s) => s.clone(),
            Value::Inst(i) => i.args.iter().map(text).collect::<Vec<_>>().join(","),
            _ => v.to_string(),
        }
    }
    name.iter().map(text).collect::<Vec<_>>().join("/")
}

/// A table of cgroups, one per line: `CGROUP PARTY [DRIVER]`, the driver by its written name. A row a session's
/// start added holds its cgroup's inode, and vouches only while that cgroup is there: one made again under the same
/// path is another.
pub struct Table(RefCell<Vec<(String, Party, Option<u64>)>>);

/// The inode of cgroup `path` while it is there.
fn inode(path: &str) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(format!("/sys/fs/cgroup{path}")).ok().map(|m| m.ino())
}

fn live((c, _, ino): &(String, Party, Option<u64>)) -> bool {
    ino.is_none_or(|i| inode(c) == Some(i))
}

impl Table {
    /// The table in `text`, `drivers` being the linked drivers' written names in order; a line not of that shape,
    /// or naming no linked driver, is an error naming it.
    pub fn read(text: &str, drivers: &[String]) -> Result<Table, String> {
        let row = |(k, l): (usize, &str)| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [c, p] => Ok((c.to_string(), Party::of(p, None), None)),
            [c, p, d] => match drivers.iter().position(|n| n == d) {
                Some(d) => Ok((c.to_string(), Party::of(p, Some(d)), None)),
                None => Err(format!("line {}: no linked driver is named {d}", k + 1)),
            },
            _ => Err(format!("line {}: not CGROUP PARTY [DRIVER]", k + 1)),
        };
        text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()).map(row).collect::<Result<_, _>>().map(|rows| Table(RefCell::new(rows)))
    }

    /// The party named `name`, as a live row has it.
    pub fn named(&self, name: &str) -> Option<Party> {
        self.0.borrow().iter().find(|r| r.1.name == name && live(r)).map(|(_, p, _)| p.clone())
    }

    /// The parties of the rows that vouch now, in the table's order.
    pub fn live(&self) -> Vec<String> {
        self.0.borrow().iter().filter(|r| live(r)).map(|(_, p, _)| p.name.clone()).collect()
    }

    /// Vouch for `cgroup`, as it is now, as `party`: refused if it is not there, or another party has it.
    pub fn add(&self, cgroup: String, party: Party) -> Result<(), String> {
        let ino = inode(&cgroup).ok_or(format!("there is no cgroup {cgroup}"))?;
        let mut rows = self.0.borrow_mut();
        rows.retain(live);
        if let Some((_, p, _)) = rows.iter().find(|(c, p, _)| *c == cgroup && p.name != party.name) {
            return Err(format!("{cgroup} is vouched for as {}, not {}", p.name, party.name));
        }
        rows.retain(|(c, _, _)| *c != cgroup);
        rows.push((cgroup, party, Some(ino)));
        Ok(())
    }
}

impl Vouch for Table {
    fn party(&self, cgroup: &str) -> Option<Party> {
        self.0.borrow().iter().find(|r| r.0 == cgroup && live(r)).map(|(_, p, _)| p.clone())
    }
}

/// The process at the other end of unix socket `fd`, as the kernel gives it.
pub fn peer_pid(fd: RawFd) -> io::Result<i32> {
    let mut c = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `c` and `len` are ours, sized for the answer SO_PEERCRED writes
    let r = unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut c).cast(), &mut len) };
    if r == 0 { Ok(c.pid) } else { Err(io::Error::last_os_error()) }
}

/// The party `v` vouches for as process `pid`, by its cgroup.
pub fn vouched(v: &dyn Vouch, pid: i32) -> Result<Party, String> {
    let cgroup = cgroup_of(pid).map_err(|e| e.to_string())?;
    v.party(&cgroup).ok_or_else(|| format!("nothing vouches for {cgroup}: this connection is no party"))
}

/// The cgroup of process `pid`: its line `0::PATH` in `/proc/PID/cgroup` (cgroup v2).
pub fn cgroup_of(pid: i32) -> io::Result<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    text.lines().find_map(|l| l.strip_prefix("0::")).map(str::to_string).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("process {pid} has no cgroup v2 line")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_has_a_cgroup_path() {
        let path = cgroup_of(std::process::id() as i32).unwrap();
        assert!(path.starts_with('/'), "{path}");
        assert!(cgroup_of(-1).is_err());
    }

    #[test]
    fn the_table_gives_each_cgroup_its_party_and_driver() {
        let names = [written(&[Value::Inst(docket_v2::run::value::Instance { ty: "std.hooks.Harness".into(), args: vec![Value::text("x")] }), Value::text("1")]), "y/2".into()];
        assert_eq!(names[0], "x/1");
        let t = Table::read("/a.scope w1 y/2\n\n/b.scope w2\n", &names).unwrap();
        assert_eq!(t.party("/a.scope"), Some(Party::of("w1", Some(1))));
        assert_eq!(t.party("/b.scope"), Some(Party::of("w2", None)));
        assert_eq!(t.party("/c.scope"), None);
        let party = |n: &str| Party::of(n, None);
        let mine = cgroup_of(std::process::id() as i32).unwrap();
        assert_eq!(t.add(mine.clone(), party("w3")), Ok(()));
        assert_eq!(t.party(&mine).map(|p| p.name), Some("w3".into()), "a session's start vouches");
        assert!(t.add(mine.clone(), party("w4")).unwrap_err().contains("vouched for as w3"), "another party's cgroup is not taken");
        assert_eq!(t.add(mine.clone(), party("w3")), Ok(()), "the same party's anew");
        assert!(t.add("/no/such.scope".into(), party("w3")).is_err(), "a cgroup that is not there");
        assert!(Table::read("/a.scope w1 z/9", &names).err().unwrap().starts_with("line 1"), "a driver not linked");
        assert!(Table::read("/a.scope", &names).is_err());
    }

    #[test]
    fn a_socket_s_other_end_is_the_party_its_process_is_vouched_for() {
        use std::os::fd::AsRawFd;
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        let pid = peer_pid(a.as_raw_fd()).unwrap();
        assert_eq!(pid, std::process::id() as i32);
        let t = Table::read("", &[]).unwrap();
        assert!(vouched(&t, pid).unwrap_err().starts_with("nothing vouches for"));
        t.add(cgroup_of(pid).unwrap(), Party::of("o1", None)).unwrap();
        assert_eq!(vouched(&t, pid).map(|p| p.name), Ok("o1".into()));
    }
}
