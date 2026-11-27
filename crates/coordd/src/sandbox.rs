//! The light sandbox (plan/layers.md Part I §2; paper-research N-95): in a project whose file says `box = true`, an
//! agent made for a step in a layer runs as a Linux user of its own, against honest mistakes, not against malice.
//!
//! - Its user is a sub-uid of the owner's range (`/etc/subuid`, `/etc/subgid`), handed out by `newuidmap` and
//!   `newgidmap` through util-linux's `unshare --map-users/--map-groups`: no root at any point. A layer's holder maps
//!   the owner to 0 and the range to 1.. (`map_args`); an agent joins it and becomes its own uid with `setpriv`
//!   (agent-base's `box-enter`). Inside uid K is host uid START+K-1.
//! - Which inside uid an agent has: `.subids` beside the coords (`~/agent-coord/.subids`), one per machine so two
//!   coords never share a uid: `{"COORD/NAME": K}`. An agent keeps its uid, its home (`$COORD/box/NAME/home`) and its
//!   conversation after it ends, so it can be started again later in another layer.
//! - The box group, inside gid 1 (host gid START): a layer's files are its, group-writable, so two agents in one layer
//!   write each other's files; each agent's primary group is its own (inside gid K).
//! - Level one (written): its layer through the view, its build outputs, the conversations folder, its home and a tmp
//!   of its own. Level two: the rest of the machine as its permissions let others read; under the owner's home only
//!   what is bound in read-only (`box-enter`: `$COORD`, agent-base, Claude Code, `box_read`).
//! - Its home, built by coordd as it starts (`home`): settings, Claude Code's state, the route's key as a file
//!   only its uid reads. No request to coordd for anything afterwards.
//! - Limits on its cgroup `agents/NAME` from the project file (`box_memory`, `box_pids`, `box_cpu`, under its own
//!   `[limits.NAME]`), each only when given (`limit`).
//! - The project's `box_setup`, copied into the box as `setup` and run by `box-enter` as the agent before its session.
//! - What a sub-uid wrote the owner cannot read, change or remove from outside when its mode shuts others out; coordd
//!   does it as root of a namespace with the same map (`in_ns`).

use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// The owner's first sub-id and count in FILE (`/etc/subuid` or `/etc/subgid`), by name or uid.
fn range_in(file: &str) -> Result<(u32, u32), String> {
    let uid = unsafe { libc::getuid() };
    let name = unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() { String::new() } else { std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned() }
    };
    let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    text.lines()
        .filter_map(|l| {
            let mut f = l.trim().split(':');
            let (who, start, count) = (f.next()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?);
            (who == name || who == uid.to_string()).then_some((start, count))
        })
        .next()
        .ok_or_else(|| format!("{file} gives the owner no range of sub-ids"))
}

/// The `unshare` arguments of a user namespace mapping the owner to 0 and the owner's sub-uids and sub-gids to 1..
pub fn map_args() -> Result<Vec<String>, String> {
    let (us, uc) = range_in("/etc/subuid")?;
    let (gs, gc) = range_in("/etc/subgid")?;
    Ok(vec!["--user".into(), "--map-root-user".into(), format!("--map-users=1:{us}:{uc}"), format!("--map-groups=1:{gs}:{gc}")])
}

/// COORD's sub-uid registry: `.subids` in the folder of the coords.
fn registry(coord: &str) -> String {
    let parent = Path::new(coord).parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".into());
    format!("{parent}/.subids")
}

/// Agent NAME of COORD's inside uid (its own gid is the same number), handed out on first ask and kept: the smallest
/// not taken, from 2 (1 is the box group's gid, and no agent's uid).
pub fn uid_of(coord: &str, name: &str) -> Result<u32, String> {
    let (_, count) = range_in("/etc/subuid")?;
    let path = registry(coord);
    let mut f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).map_err(|e| format!("{path}: {e}"))?;
    // one writer at a time, across coords
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(format!("{path}: {}", std::io::Error::last_os_error()));
    }
    let mut text = String::new();
    f.read_to_string(&mut text).map_err(|e| format!("{path}: {e}"))?;
    let mut reg: BTreeMap<String, u32> = if text.trim().is_empty() { BTreeMap::new() } else { serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))? };
    let key = format!("{coord}/{name}");
    if let Some(k) = reg.get(&key) {
        return Ok(*k);
    }
    let taken: std::collections::BTreeSet<u32> = reg.values().copied().collect();
    let k = (2..=count).find(|k| !taken.contains(k)).ok_or_else(|| format!("{path}: every sub-uid is taken"))?;
    reg.insert(key, k);
    let out = serde_json::to_string_pretty(&reg).unwrap_or_default();
    f.set_len(0).and_then(|_| f.rewind()).and_then(|_| f.write_all(format!("{out}\n").as_bytes())).map_err(|e| format!("{path}: {e}"))?;
    Ok(k)
}

/// Agent NAME of COORD's inside uid if it has one (none: never boxed).
pub fn known_uid(coord: &str, name: &str) -> Option<u32> {
    let text = std::fs::read_to_string(registry(coord)).ok()?;
    let reg: BTreeMap<String, u32> = serde_json::from_str(&text).ok()?;
    reg.get(&format!("{coord}/{name}")).copied()
}

/// Inside uid K as a host uid.
pub fn host_uid(k: u32) -> Option<u32> {
    let (start, _) = range_in("/etc/subuid").ok()?;
    Some(start + k - 1)
}

/// Run sh SCRIPT (with ARGS as $1..) as root of a fresh user namespace with the box's map: what the owner cannot do
/// from outside to what a sub-uid wrote (read, freeze, remove).
pub fn in_ns(script: &str, args: &[&str]) -> Result<(), String> {
    let o = Command::new("unshare").args(map_args()?).args(["--", "sh", "-c", script, "box"]).args(args).stdin(Stdio::null()).output().map_err(|e| format!("unshare: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// `in_ns` with INPUT on the script's stdin: what must not be in a process's arguments (the route's key).
fn in_ns_input(script: &str, args: &[&str], input: &[u8]) -> Result<(), String> {
    let mut c = Command::new("unshare").args(map_args()?).args(["--", "sh", "-c", script, "box"]).args(args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("unshare: {e}"))?;
    if let Some(mut i) = c.stdin.take() {
        i.write_all(input).map_err(|e| format!("unshare: {e}"))?;
    }
    let o = c.wait_with_output().map_err(|e| format!("unshare: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// The route's key of COORD (`$COORD/.proxy-key`, host-coordd's route_write_key), or why there is none: a boxed agent
/// cannot use the owner's own login.
fn route_key(coord: &str) -> Result<Vec<u8>, String> {
    match std::fs::read(format!("{coord}/.proxy-key")) {
        Ok(k) if !k.iter().all(u8::is_ascii_whitespace) => Ok(k),
        _ => Err(String::from("this coord has no route key, and a boxed agent cannot use the owner's own login")),
    }
}

/// Write KEY as the key file of the box at D (`D/home/.proxy-key`): the agent's inside uid K's only (0400), written
/// as root of a namespace with the box's map, the key on stdin. Never logged.
fn write_key(d: &str, k: u32, key: &[u8]) -> Result<(), String> {
    let script = r#"set -e
umask 077
cat >"$1/home/.proxy-key.new"
chown "$2:$2" "$1/home/.proxy-key.new"
chmod 400 "$1/home/.proxy-key.new"
mv -f "$1/home/.proxy-key.new" "$1/home/.proxy-key""#;
    in_ns_input(script, &[d, &k.to_string()], key).map_err(|e| format!("its key file: {e}"))
}

/// Where a boxed agent's home is built from (`home`).
#[derive(Clone)]
pub struct Home<'a> {
    pub coord: &'a str,
    pub name: &'a str,
    /// Its inside uid (`uid_of`).
    pub k: u32,
    /// The folder it runs in (the view, at the project's path).
    pub dir: &'a str,
    /// agent-base: `claude/settings.json`, every agent's settings.
    pub base: &'a str,
    /// The project's own settings (`claude-settings.json` beside the project file), merged over agent-base's.
    pub project_settings: Option<String>,
    /// The owner's Claude Code state (`~/.claude.json`), where the flag cache is taken from.
    pub owner: String,
    /// The project's `box_setup` script, copied to `setup` in the box (box-enter runs it); none: no `setup` there.
    pub setup: Option<String>,
}

/// Build agent H.name's box (`$COORD/box/NAME`) as it starts (paper-research N-95 post 10): everything it needs,
/// handed over here, so the launcher patches nothing afterwards and the agent asks coordd for nothing.
/// - `claude-settings.json`: agent-base's, its key helper the same `cat` of a key file as any agent's, at its own home's;
///   the updater off; the project's settings merged over (objects merged, any other value replaced, as roster does);
///   then prompts bypassed (`box_mode`).
/// - `home/` (0711) and `tmp/` (0700), its own: `.claude.json` as yaaihh's recipe has it (N-102 post 4): Claude
///   Code's first start counted done, the updater off, DIR trusted, and the two keys of the owner's flag cache
///   (`cachedGrowthBookFeatures`, `cachedGrowthBookFeaturesAt`; without them its channels are off and every step is
///   dropped), nothing else of the owner's file; the route's key in `.proxy-key`, its uid's only; its Claude Code
///   project entry for DIR a link to the conversations folder.
/// - `home/.codex` (0711, as `app/` in it), a Codex agent's CODEX_HOME (roster): the hooks every Codex agent runs, the
///   coord's Codex settings (`codex-home/config.toml`, its provider; never its `auth.json`, a login), `sessions` a link
///   to the conversations folder; its app-server's socket in `app/`, reached by codex-rpc through the box's map.
/// - `home/.gitconfig`: `safe.directory = *` added (kept with anything else in it): a view's repositories are the
///   owner's (root inside), and git refuses them to the agent's uid (docket-core3, N-101 p536). roster's environment
///   says the same (box_env); this also holds in a shell entered without that environment.
/// - `$COORD/agents/NAME`, where its channel's socket is: the owner's and its own group's (2770).
/// What the agent may have written there before (a home kept after it ended) stays. No value is printed or logged.
/// A boxed agent bypasses permission prompts: the box is the boundary, not Claude Code's classifier (the owner,
/// N-216.p1: "the box should run in yolo mode eh not auto"). roster starts it with --dangerously-skip-permissions
/// (lib/route.sh `session_place`, ROSTER_BOX), and without the prompt's skip Claude Code stops at the bypass warning
/// before its first turn (box-run-test.sh run 5, N-101 post 185). Set last, so neither agent-base's file nor the
/// project's settings decide it.
fn box_mode(settings: &mut serde_json::Value) {
    let Some(o) = settings.as_object_mut() else { return };
    o.insert("skipDangerousModePermissionPrompt".into(), true.into());
    let perms = o.entry("permissions").or_insert_with(|| serde_json::json!({}));
    if let Some(p) = perms.as_object_mut() {
        p.insert("defaultMode".into(), "bypassPermissions".into());
    }
}

/// What a boxed agent needs of its box, the same at registration and at every start (N-101 post 406, the joint
/// proposal's A): its own uid (`uid_of`), its limits on `agents/NAME` (`limit`) and its home built for DIR, the
/// project's path, which is the same in every view (`home`). Each part keeps what is there, so doing it again is
/// harmless. Its inside uid, or why not.
pub fn ready(h: &Home, limits: &Limits) -> Result<u32, String> {
    let k = uid_of(h.coord, h.name)?;
    home(&Home { k, ..h.clone() })?;
    limit(h.name, limits).map_err(|e| format!("its limits: {e}"))?;
    Ok(k)
}

pub fn home(h: &Home) -> Result<(), String> {
    let d = format!("{}/box/{}", h.coord, h.name);
    std::fs::create_dir_all(&d).map_err(|e| format!("{d}: {e}"))?;
    let read = |p: &str| -> Result<serde_json::Value, String> { std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}")).and_then(|t| serde_json::from_str(&t).map_err(|e| format!("{p}: {e}"))) };
    let mut settings = read(&format!("{}/claude/settings.json", h.base))?;
    if let Some(o) = settings.as_object_mut() {
        o.insert("apiKeyHelper".into(), format!("cat \"{d}/home/.proxy-key\"").into());
        let env = o.entry("env").or_insert_with(|| serde_json::json!({}));
        if let Some(env) = env.as_object_mut() {
            env.insert("DISABLE_AUTOUPDATER".into(), "1".into());
        }
    }
    if let Some(p) = h.project_settings.as_deref().filter(|p| Path::new(p).is_file()) {
        merge(&mut settings, read(p)?);
    }
    box_mode(&mut settings);
    // the coord's compaction point is no setting of the session: roster gives it as `--autocompact`
    // (`Core::context_window`)
    if let Some(o) = settings.as_object_mut() {
        o.remove("autoCompactWindow");
    }
    let sf = format!("{d}/claude-settings.json");
    std::fs::write(format!("{sf}.new"), serde_json::to_string_pretty(&settings).unwrap_or_default()).and_then(|_| std::fs::rename(format!("{sf}.new"), &sf)).map_err(|e| format!("{sf}: {e}"))?;
    let key = route_key(h.coord)?;
    let entry: String = h.dir.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let script = r#"set -e
d=$1 k=$2 dir=$3 owner=$4 entry=$5 a=$6
mkdir -p "$d/home/.claude/projects" "$d/tmp" "$a"
chown "$k:$k" "$d/home" "$d/home/.claude" "$d/home/.claude/projects" "$d/tmp"
chmod 711 "$d/home" "$d/home/.claude" "$d/home/.claude/projects"; chmod 700 "$d/tmp"
chgrp "$k" "$a"; chmod 2770 "$a"
# with an ACL (a default one inherited from agents/: Tovrin's, the container's), chmod sets only its mask: the group's
# own entry is given rwx too, or the agent cannot make its channel's socket there (N-101 p1236)
if command -v setfacl >/dev/null; then setfacl -m g::rwx,d:g::rwx "$a"; fi
cj=$d/home/.claude.json
[ -s "$cj" ] || echo '{}' >"$cj"
jq --arg dir "$dir" '. + {hasCompletedOnboarding: true, autoUpdates: false} | .projects[$dir] = ((.projects[$dir] // {}) + {hasTrustDialogAccepted: true})' "$cj" >"$cj.new"
chown "$k:$k" "$cj.new"; chmod 600 "$cj.new"; mv -f "$cj.new" "$cj"
l=$d/home/.claude/projects/$entry
[ -e "$l" ] || [ -L "$l" ] || { ln -s "$dir/.conversations/claude" "$l"; chown -h "$k:$k" "$l"; }
c=$d/home/.codex
mkdir -p "$c/app"
chown "$k:$k" "$c" "$c/app"; chmod 711 "$c" "$c/app"
printf '%s\n' "$hooks" >"$c/hooks.json.new"; chown "$k:$k" "$c/hooks.json.new"; chmod 644 "$c/hooks.json.new"; mv -f "$c/hooks.json.new" "$c/hooks.json"
if [ -f "$cfg" ]; then cp "$cfg" "$c/config.toml.new"; chown "$k:$k" "$c/config.toml.new"; chmod 600 "$c/config.toml.new"; mv -f "$c/config.toml.new" "$c/config.toml"; fi
s=$c/sessions
[ -e "$s" ] || [ -L "$s" ] || { ln -s "$dir/.conversations/codex" "$s"; chown -h "$k:$k" "$s"; }
g=$d/home/.gitconfig
git config --file "$g" --get-all safe.directory 2>/dev/null | grep -qxF '*' || git config --file "$g" --add safe.directory '*'
chown "$k:$k" "$g""#;
    let a = format!("{}/agents/{}", h.coord, h.name);
    // a Codex agent's CODEX_HOME (roster: CODEX_HOME=home/.codex): the hooks roster writes for any Codex agent, the
    // coord's Codex settings (`codex-home/config.toml`: its provider, the route's proxy; never `auth.json`, which is a
    // login), its conversations in the project's folder as Claude Code's are (the same in every view, so a move keeps
    // its thread); its key is the same key file (bin/codex, CODEX_KEY_FILE)
    let hooks = codex_hooks(h.base);
    let cfg = format!("{}/codex-home/config.toml", h.coord);
    let script = format!("hooks=$7 cfg=$8\n{script}");
    in_ns(&script, &[&d, &h.k.to_string(), h.dir, &h.owner, &entry, &a, &hooks, &cfg]).map_err(|e| format!("its home: {e}"))?;
    in_ns(FLAGS, &[&d, &h.owner, &h.k.to_string()]).map_err(|e| format!("its flag cache: {e}"))?;
    // the project's box_setup, as it is now: the owner's, read and run by the agent (box-enter)
    let su = format!("{d}/setup");
    match &h.setup {
        Some(src) => {
            use std::os::unix::fs::PermissionsExt;
            let text = std::fs::read(src).map_err(|e| format!("box_setup {src}: {e}"))?;
            std::fs::write(format!("{su}.new"), text)
                .and_then(|_| std::fs::set_permissions(format!("{su}.new"), std::fs::Permissions::from_mode(0o755)))
                .and_then(|_| std::fs::rename(format!("{su}.new"), &su))
                .map_err(|e| format!("{su}: {e}"))?;
        }
        None => match std::fs::remove_file(&su) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{su}: {e}")),
            _ => {}
        },
    }
    write_key(&d, h.k, &key)
}

/// The hooks a Codex agent's app-server runs (roster's `app_server` writes the same for an agent not boxed): agent-base's
/// claude/hook on each of Codex's events.
fn codex_hooks(base: &str) -> String {
    let h = format!("{base}/claude/hook");
    let on = |a: &str| serde_json::json!([{"hooks": [{"type": "command", "command": format!("\"{h}\" {a}")}]}]);
    let tool = |a: &str| serde_json::json!([{"matcher": "*", "hooks": [{"type": "command", "command": format!("\"{h}\" {a}")}]}]);
    serde_json::json!({"hooks": {"SessionStart": on("start"), "UserPromptSubmit": on("prompt"), "PreToolUse": tool("pre"), "PostToolUse": tool("post"), "Stop": on("stop")}}).to_string()
}

/// jq's `*` on settings: objects merged key by key, anything else replaced by OVER's.
fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(bv) if bv.is_object() && v.is_object() => merge(bv, v),
                    _ => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (b, o) => *b = o,
    }
}

/// The owner's flag cache (OWNER, `~/.claude.json`: its two keys and nothing else) merged into the Claude Code state
/// of the box at D, as root of a namespace with the box's map: at every start, as the original box does
/// (agent-base/box/box.sh `claude_flags`), so a cache the owner's Claude Code has fetched again reaches the boxes.
const FLAGS: &str = r#"set -e
cj=$1/home/.claude.json owner=$2 k=$3
[ -s "$cj" ] || echo '{}' >"$cj"
flags='{}'; [ -f "$owner" ] && flags=$(jq -c '{cachedGrowthBookFeatures, cachedGrowthBookFeaturesAt} | with_entries(select(.value != null))' "$owner")
jq --argjson f "$flags" '. + $f' "$cj" >"$cj.new"
chown "$k:$k" "$cj.new"; chmod 600 "$cj.new"; mv -f "$cj.new" "$cj""#;

/// Every box of COORD (each agent the registry names for it whose home is there) brought up to date as coordd starts:
/// the route's key written again (host-coordd writes it just before) and the owner's flag cache (OWNER) merged in
/// again. What failed, said (no key or value in it).
pub fn refresh(coord: &str, owner: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(registry(coord)) else { return Vec::new() };
    let Ok(reg) = serde_json::from_str::<BTreeMap<String, u32>>(&text) else { return Vec::new() };
    let boxes: Vec<(String, u32)> = reg.into_iter().filter_map(|(key, k)| key.strip_prefix(&format!("{coord}/")).map(|n| (n.to_string(), k))).filter(|(n, _)| !n.contains('/') && Path::new(&format!("{coord}/box/{n}/home")).is_dir()).collect();
    if boxes.is_empty() {
        return Vec::new();
    }
    let key = match route_key(coord) {
        Ok(k) => k,
        Err(e) => return vec![e],
    };
    let mut failed = Vec::new();
    for (n, k) in &boxes {
        let d = format!("{coord}/box/{n}");
        if let Err(e) = write_key(&d, *k, &key) {
            failed.push(format!("{n}: {e}"));
        }
        if let Err(e) = in_ns(FLAGS, &[&d, owner, &k.to_string()]) {
            failed.push(format!("{n}: its flag cache: {e}"));
        }
    }
    failed
}

/// The limits the project file gives (`box_memory`, `box_pids`, `box_cpu`; `[limits.NAME]`; the header's `memory`,
/// `cpu`, `pids`), each only when given.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub memory: Option<String>,
    pub pids: Option<u64>,
    pub cpu: Option<f64>,
}

impl Limits {
    /// The cgroup files and values these limits write: memory (and no swap beside it), processes, CPU (in CPUs, as
    /// a quota per the kernel's default period of 100 ms).
    pub fn files(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(m) = &self.memory {
            out.push(("memory.max", m.clone()));
            out.push(("memory.swap.max", "0".into()));
        }
        if let Some(p) = self.pids {
            out.push(("pids.max", p.to_string()));
        }
        if let Some(c) = self.cpu {
            out.push(("cpu.max", format!("{} 100000", (c * 100000.0).round() as u64)));
        }
        out
    }
}

/// A size as `memory.max` takes it ("40G", "512M", "1073741824"; K, M, G, T are powers of 1024), in bytes: none for
/// "max" or what is not a size.
pub fn bytes(m: &str) -> Option<u64> {
    let m = m.trim();
    let (n, unit) = match m.char_indices().last()? {
        (i, c) if c.is_ascii_alphabetic() => (&m[..i], c.to_ascii_uppercase()),
        _ => (m, 'B'),
    };
    let shift = match unit {
        'B' => 0,
        'K' => 10,
        'M' => 20,
        'G' => 30,
        'T' => 40,
        _ => return None,
    };
    n.trim().parse::<u64>().ok()?.checked_mul(1 << shift)
}

/// Write LIMITS on agent NAME's own cgroup `agents/NAME` (made if missing), above its leaf `main` and the job cgroups
/// beside it, so they hold for all of it together: the runner's jobs share the runner's. What is not given stays as
/// it is.
pub fn limit(name: &str, limits: &Limits) -> Result<(), String> {
    // its subtree with the controllers enabled first: a limit's file is there only then
    crate::cgroup::delegate(name).map_err(|e| format!("agents/{name}: {e}"))?;
    let dir = crate::cgroup::root().join("agents").join(name);
    for (f, v) in limits.files() {
        std::fs::write(dir.join(f), &v).map_err(|e| format!("{}/{f} = {v}: {e}", dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_write_only_what_is_given() {
        assert!(Limits::default().files().is_empty());
        let l = Limits { memory: Some("4G".into()), pids: Some(512), cpu: Some(1.5) };
        assert_eq!(l.files(), vec![("memory.max", "4G".into()), ("memory.swap.max", "0".into()), ("pids.max", "512".into()), ("cpu.max", "150000 100000".into())]);
    }

    #[test]
    fn sizes_read_as_memory_max_takes_them() {
        assert_eq!(bytes("40G"), Some(40 << 30));
        assert_eq!(bytes("512m"), Some(512 << 20));
        assert_eq!(bytes("1073741824"), Some(1 << 30));
        assert_eq!(bytes("max"), None);
        assert_eq!(bytes("4X"), None);
    }

    #[test]
    fn settings_merge_as_jq_star_does() {
        let mut b = serde_json::json!({"env": {"A": "1"}, "hooks": {"Stop": [1]}, "theme": "dark"});
        merge(&mut b, serde_json::json!({"env": {"B": "2"}, "hooks": {"Stop": [2]}, "autoMemoryEnabled": false}));
        assert_eq!(b, serde_json::json!({"env": {"A": "1", "B": "2"}, "hooks": {"Stop": [2]}, "theme": "dark", "autoMemoryEnabled": false}));
    }

    /// The box coordd builds (post 10): settings with the key helper on the agent's own key file and the updater off,
    /// the project's settings over them; the home and key file its uid's only; Claude Code's state with the two flag
    /// keys and nothing else of the owner's; the key file and flag cache brought up to date by `refresh`. A stand-in key and a stand-in owner
    /// file; no key printed. Needs a sub-uid range and user namespaces (`--ignored`).
    #[test]
    #[ignore]
    fn coordd_builds_the_box_with_everything_in_it() {
        use std::os::unix::fs::MetadataExt;
        let t = std::env::temp_dir().join(format!("coordd-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let coord = t.join("coord").display().to_string();
        let base = t.join("base").display().to_string();
        std::fs::create_dir_all(format!("{base}/claude")).unwrap();
        std::fs::create_dir_all(format!("{coord}/agents")).unwrap();
        // agents/ with a default ACL whose group entry refuses writing, as Tovrin's from its container (N-101 p1236)
        let me = unsafe { libc::getuid() };
        assert!(Command::new("setfacl").args(["-m", &format!("d:u:{me}:rwx,d:g::r-x"), &format!("{coord}/agents")]).status().unwrap().success());
        std::fs::write(format!("{base}/claude/settings.json"), r#"{"apiKeyHelper": "cat \"$COORD/.proxy-key\"", "permissions": {"defaultMode": "bypassPermissions", "allow": []}, "skipDangerousModePermissionPrompt": true, "env": {"X": "1"}}"#).unwrap();
        std::fs::write(t.join("project-settings.json"), r#"{"autoMemoryEnabled": false}"#).unwrap();
        std::fs::write(t.join("owner.json"), r#"{"oauthAccount": {"x": 1}, "userID": "u", "cachedGrowthBookFeatures": {"tengu_harbor": true}, "cachedGrowthBookFeaturesAt": 5}"#).unwrap();
        std::fs::write(format!("{coord}/.proxy-key"), "stand-in-1\n").unwrap();
        // the coord's Codex settings and a login beside them, which never goes into a box
        std::fs::create_dir_all(format!("{coord}/codex-home")).unwrap();
        std::fs::write(format!("{coord}/codex-home/config.toml"), "model_provider = \"route\"\n").unwrap();
        std::fs::write(format!("{coord}/codex-home/auth.json"), "{}").unwrap();
        let k = uid_of(&coord, "f1-1").unwrap();
        let h = Home { coord: &coord, name: "f1-1", k, dir: "/w/repo", base: &base, project_settings: Some(t.join("project-settings.json").display().to_string()), owner: t.join("owner.json").display().to_string(), setup: None };
        home(&h).unwrap();
        let d = format!("{coord}/box/f1-1");
        let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{d}/claude-settings.json")).unwrap()).unwrap();
        assert_eq!(s["apiKeyHelper"], format!("cat \"{d}/home/.proxy-key\""));
        assert_eq!(s["env"], serde_json::json!({"X": "1", "DISABLE_AUTOUPDATER": "1"}));
        assert_eq!(s["autoMemoryEnabled"], false);
        assert!(s["permissions"]["defaultMode"] == "bypassPermissions" && s["skipDangerousModePermissionPrompt"] == true, "a boxed agent bypasses prompts");
        let host = host_uid(k).unwrap();
        let m = std::fs::metadata(format!("{d}/home")).unwrap();
        assert_eq!((m.uid(), m.mode() & 0o777), (host, 0o711));
        assert!(std::fs::read(format!("{d}/home/.proxy-key")).is_err(), "the owner's uid does not read the agent's key file");
        let mut got = String::new();
        let check = r#"set -e
[ "$(stat -c '%u %a' "$1/home/.proxy-key")" = "$2 400" ]
[ "$(cat "$1/home/.proxy-key")" = "$3" ]
[ "$(stat -c '%u %a' "$1/home/.claude.json")" = "$2 600" ]
jq -e '.hasCompletedOnboarding and .autoUpdates == false and .projects["/w/repo"].hasTrustDialogAccepted and .cachedGrowthBookFeatures.tengu_harbor and .cachedGrowthBookFeaturesAt == 5 and (has("oauthAccount") | not) and (has("userID") | not)' "$1/home/.claude.json" >/dev/null
[ "$(readlink "$1/home/.claude/projects/-w-repo")" = /w/repo/.conversations/claude ]
[ "$(stat -c '%g %a' "$4")" = "$2 2770" ]
getfacl -p "$4" 2>/dev/null | grep -qx 'group::rwx'
[ "$(stat -c '%u %a' "$1/home/.codex")" = "$2 711" ] && [ "$(stat -c '%u %a' "$1/home/.codex/app")" = "$2 711" ]
[ "$(stat -c '%u %a' "$1/home/.codex/config.toml")" = "$2 600" ] && grep -qx 'model_provider = "route"' "$1/home/.codex/config.toml"
[ ! -e "$1/home/.codex/auth.json" ]
jq -e '.hooks.Stop[0].hooks[0].command | endswith("/claude/hook\" stop")' "$1/home/.codex/hooks.json" >/dev/null
[ "$(readlink "$1/home/.codex/sessions")" = /w/repo/.conversations/codex ]
[ "$(stat -c %u "$1/home/.gitconfig")" = "$2" ] && [ "$(git config --file "$1/home/.gitconfig" --get-all safe.directory)" = '*' ]"#;
        let a = format!("{coord}/agents/f1-1");
        if let Err(e) = in_ns(check, &[&d, &k.to_string(), "stand-in-1", &a]) {
            got = e;
        }
        assert!(got.is_empty(), "the box as built: {got}");
        // the project's box_setup: copied in as `setup`, others may read and run it; gone again once the project has none
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(t.join("box-setup.sh"), "#!/bin/sh\necho set up\n").unwrap();
            home(&Home { setup: Some(t.join("box-setup.sh").display().to_string()), ..h.clone() }).unwrap();
            assert_eq!(std::fs::read_to_string(format!("{d}/setup")).unwrap(), "#!/bin/sh\necho set up\n");
            assert_eq!(std::fs::metadata(format!("{d}/setup")).unwrap().permissions().mode() & 0o777, 0o755);
            home(&Home { setup: None, ..h.clone() }).unwrap();
            assert!(!Path::new(&format!("{d}/setup")).exists(), "no box_setup, no setup in the box");
        }
        std::fs::write(format!("{coord}/.proxy-key"), "stand-in-2\n").unwrap();
        std::fs::write(t.join("owner.json"), r#"{"cachedGrowthBookFeatures": {"tengu_harbor": true, "x": 2}, "cachedGrowthBookFeaturesAt": 6}"#).unwrap();
        assert!(refresh(&coord, &h.owner).is_empty());
        in_ns("[ \"$(cat \"$1/home/.proxy-key\")\" = stand-in-2 ] && jq -e '.cachedGrowthBookFeaturesAt == 6 and .hasCompletedOnboarding' \"$1/home/.claude.json\" >/dev/null", &[&d]).expect("refresh rewrote the key file and the flag cache, and kept the rest");
        // built again (a second start): what was there stays, nothing doubles
        home(&h).unwrap();
        in_ns("rm -rf \"$1\"", &[&t.display().to_string()]).unwrap();
        // the registry is beside the coord, in t
        assert!(!t.exists());
    }

    #[test]
    fn a_boxed_agent_bypasses_prompts_whatever_the_settings_under_it_say() {
        let bypass = serde_json::json!({"permissions": {"defaultMode": "bypassPermissions", "allow": ["X"]}, "skipDangerousModePermissionPrompt": true});
        // agent-base's file as it is: kept
        let mut s = bypass.clone();
        box_mode(&mut s);
        assert_eq!(s, bypass);
        // a project's settings that asked for the owner's mode, or a file without the keys: bypass all the same
        let mut s = serde_json::json!({"permissions": {"defaultMode": "auto", "allow": ["X"]}, "skipDangerousModePermissionPrompt": false});
        box_mode(&mut s);
        assert_eq!(s, bypass);
        let mut s = serde_json::json!({});
        box_mode(&mut s);
        assert_eq!(s, serde_json::json!({"permissions": {"defaultMode": "bypassPermissions"}, "skipDangerousModePermissionPrompt": true}));
    }

    #[test]
    fn an_agent_keeps_its_uid_and_two_coords_never_share_one() {
        let dir = std::env::temp_dir().join(format!("coordd-subids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if range_in("/etc/subuid").is_err() {
            return eprintln!("no sub-uid range for this user: skipped");
        }
        let a = format!("{}/a", dir.display());
        let b = format!("{}/b", dir.display());
        let x = uid_of(&a, "dev").unwrap();
        assert_eq!(x, 2);
        assert_eq!(uid_of(&a, "dev").unwrap(), x);
        let y = uid_of(&b, "dev").unwrap();
        assert_ne!(x, y);
        assert_eq!(known_uid(&b, "dev"), Some(y));
        assert_eq!(known_uid(&b, "rev"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
