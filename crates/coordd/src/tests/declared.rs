//! The one exception to a caller's cgroup naming it (paper-research N-1446 p2099). An agy agent's coord commands run in
//! its sign-in account's t3code server and declare its name (`coord_types::DECLARED`). serve takes the name from that
//! server's cgroup alone (`declared_caller`, `accounts::agy::declares`). There is a leg each way, as p2099 asks: the
//! declaration from the server's cgroup is taken, and the same declaration from any other cgroup is ignored, so the
//! cgroup's name stands.

use super::{declared_caller, Shared};
use crate::accounts::{agy, testing::{copy_program, Tmp}, Harness, Kind};
use crate::cgroup::{self, Caller};
use crate::core::Core;
use coord_types::Request;

/// A stand-in of an account's t3code server: a process whose program is named `t3`, a copy of `sleep` kept beside the
/// test's own program, where programs may run. The account's `server-runtime.json` names it, as t3code names its own
/// server.
struct Server(std::process::Child, std::path::PathBuf);

impl Server {
    fn start(dir: &std::path::Path, tag: &str) -> Server {
        let bin = std::env::current_exe().unwrap().parent().unwrap().join(format!("declared-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bin);
        std::fs::create_dir_all(&bin).unwrap();
        let t3 = bin.join("t3");
        copy_program(std::path::Path::new("/bin/sleep"), &t3);
        let child = std::process::Command::new(&t3).arg("600").spawn().unwrap();
        let rt = agy::home(dir).join(".t3/userdata");
        std::fs::create_dir_all(&rt).unwrap();
        std::fs::write(rt.join("server-runtime.json"), serde_json::json!({"pid": child.id()}).to_string()).unwrap();
        Server(child, bin)
    }

    /// The server's cgroup, read from its process.
    fn path(&self) -> String {
        cgroup::path_of(self.0.id() as i32)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

/// `coord ls`, as an agent's coordc sends it in its session, declaring NAME.
fn says(name: &str) -> String {
    Request::State.declared_line(name)
}

#[tokio::test]
async fn a_declared_name_is_taken_from_its_accounts_t3code_server_alone() {
    let t = Tmp::new("declared");
    t.store.create_agy("g", "G", 0, Kind::Signin, "x").unwrap();
    t.store.create_agy("k", "K", 0, Kind::Key, "x").unwrap();
    t.store.bind(Harness::Agy, "g", "research", "gemini-advisor", "x").unwrap();
    t.store.bind(Harness::Agy, "g", "cav-paper", "other", "x").unwrap();
    t.store.bind(Harness::Agy, "k", "research", "keyed", "x").unwrap();
    let server = Server::start(&t.store.dir(Harness::Agy, "g"), "g");
    // a key account has no t3code server; a `t3` its folder names is still not taken
    let keyed = Server::start(&t.store.dir(Harness::Agy, "k"), "k");
    let coord = t.dir.join("research");
    std::fs::create_dir_all(&coord).unwrap();
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.accounts = t.store.clone();
    let shared = Shared::new(core);
    let at = server.path();
    assert!(!at.is_empty(), "the server's cgroup is read");
    let advisor = || Caller::Agent("gemini-advisor".into());

    // taken: from the server's cgroup, as this coordd's own sign-in group, or as the owner by its cgroup (a server
    // started under another coordd's root)
    assert_eq!(declared_caller(&shared, Caller::SignIn, &at, &says("gemini-advisor")).await, advisor());
    assert_eq!(declared_caller(&shared, Caller::Owner, &at, &says("gemini-advisor")).await, advisor());

    // the same declaration from any other cgroup is ignored, and the cgroup's name stands
    assert_eq!(declared_caller(&shared, Caller::Owner, "/user.slice/elsewhere", &says("gemini-advisor")).await, Caller::Owner);
    assert_eq!(declared_caller(&shared, Caller::SignIn, &format!("{at}/below"), &says("gemini-advisor")).await, Caller::SignIn);
    assert_eq!(declared_caller(&shared, Caller::Owner, "", &says("gemini-advisor")).await, Caller::Owner, "a cgroup not read");
    assert_eq!(declared_caller(&shared, Caller::Agent("w2".into()), &at, &says("gemini-advisor")).await, Caller::Agent("w2".into()));
    assert_eq!(declared_caller(&shared, Caller::Project, &at, &says("gemini-advisor")).await, Caller::Project);
    assert_eq!(declared_caller(&shared, Caller::Unclaimed, &at, &says("gemini-advisor")).await, Caller::Unclaimed);

    // from the server's cgroup, only a name the account has agents for in this coord; nothing declared is a sign-in's
    assert_eq!(declared_caller(&shared, Caller::SignIn, &at, &says("someone")).await, Caller::SignIn);
    assert_eq!(declared_caller(&shared, Caller::SignIn, &at, &says("other")).await, Caller::SignIn, "another coord's agent");
    assert_eq!(declared_caller(&shared, Caller::SignIn, &at, &Request::State.line()).await, Caller::SignIn);
    assert_eq!(declared_caller(&shared, Caller::SignIn, &keyed.path(), &says("keyed")).await, Caller::SignIn, "a key account's agent");

    // the server gone: nothing is its cgroup
    drop(server);
    assert_eq!(declared_caller(&shared, Caller::SignIn, &at, &says("gemini-advisor")).await, Caller::SignIn);
    drop(keyed);
}
