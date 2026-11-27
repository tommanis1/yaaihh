//! A physical disk's health (N-1860 p2): NVMe's Health Information log (page 0x02) read directly, on Windows through
//! IOCTL_STORAGE_QUERY_PROPERTY (no driver, no admin, nothing bundled); a SATA disk only through a smartctl whose
//! SHA-256 is pinned (the projects file's `[devices] smartctl`, `smartctl_sha256`), else not read. Reading only.

use serde::{Deserialize, Serialize};

/// Wear past this share of the drive's rated life needs him.
pub const WEAR_HIGH: f64 = 85.0;

/// One disk's health, each figure only when it was read (the reader's `health` items, or this machine's own).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct DiskHealth {
    #[serde(default, deserialize_with = "crate::devices::lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "crate::devices::lenient")]
    pub wear_pct: Option<f64>,
    #[serde(default, deserialize_with = "crate::devices::lenient")]
    pub temp_c: Option<f64>,
    /// NVMe's critical warning bits; a SATA disk's failed overall check is bit 0x04.
    #[serde(default, deserialize_with = "crate::devices::lenient")]
    pub critical: Option<u8>,
    #[serde(default, deserialize_with = "crate::devices::lenient")]
    pub media_errors: Option<u64>,
    #[serde(default, deserialize_with = "crate::devices::lenient")]
    pub power_on_hours: Option<u64>,
}

impl DiskHealth {
    /// Whether it needs him: worn past [`WEAR_HIGH`], a critical warning, or a media error.
    pub fn degraded(&self) -> bool {
        self.wear_pct.is_some_and(|w| w > WEAR_HIGH) || self.critical.is_some_and(|c| c != 0) || self.media_errors.is_some_and(|m| m > 0)
    }

    /// Its line: "wear 3% · 41 °C · no warning · no media errors · 1,204 h on".
    pub fn words(&self) -> String {
        let mut v = Vec::new();
        v.extend(self.wear_pct.map(|w| format!("wear {}%", w.round() as i64)));
        v.extend(self.temp_c.map(|t| format!("{} °C", t.round() as i64)));
        v.extend(self.critical.map(warning));
        v.extend(self.media_errors.map(|m| if m == 0 { "no media errors".to_string() } else { format!("{m} media errors") }));
        v.extend(self.power_on_hours.map(|h| format!("{} h on", thousands(h))));
        v.join(" · ")
    }
}

/// NVMe's critical warning bits in words.
fn warning(c: u8) -> String {
    const BITS: [(u8, &str); 6] = [(0x01, "spare low"), (0x02, "too hot"), (0x04, "reliability degraded"), (0x08, "read-only"), (0x10, "backup failed"), (0x20, "memory read-only")];
    if c == 0 {
        return "no warning".into();
    }
    format!("critical: {}", BITS.iter().filter(|(b, _)| c & b != 0).map(|(_, w)| *w).collect::<Vec<_>>().join(", "))
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// NVMe's Health Information log page (512 bytes) as its health, NAME the drive's model.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn from_log(name: Option<String>, log: &[u8]) -> Option<DiskHealth> {
    if log.len() < 176 {
        return None;
    }
    let u128_at = |o: usize| u128::from_le_bytes(log[o..o + 16].try_into().unwrap());
    let kelvin = u16::from_le_bytes([log[1], log[2]]);
    Some(DiskHealth {
        name,
        critical: Some(log[0]),
        temp_c: (kelvin > 0).then(|| kelvin as f64 - 273.15),
        wear_pct: Some(log[5] as f64),
        power_on_hours: Some(u128_at(128).min(u64::MAX as u128) as u64),
        media_errors: Some(u128_at(160).min(u64::MAX as u128) as u64),
    })
}

/// smartctl's `--json=c -H -A` answer for a SATA disk as its health: its overall check, temperature, hours on,
/// reallocated, pending and uncorrectable sectors as media errors, and wear from the SSD's life attribute.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn from_smartctl(text: &str) -> Option<DiskHealth> {
    let j: serde_json::Value = serde_json::from_str(text).ok()?;
    let attrs = j.pointer("/ata_smart_attributes/table").and_then(|t| t.as_array()).cloned().unwrap_or_default();
    let attr = |id: u64| attrs.iter().find(|a| a["id"].as_u64() == Some(id));
    let raw = |id: u64| attr(id).and_then(|a| a.pointer("/raw/value")?.as_u64());
    let errors: Vec<u64> = [5, 197, 198].into_iter().filter_map(raw).collect();
    let wear = [177, 231, 233].into_iter().find_map(|id| attr(id).and_then(|a| a["value"].as_f64())).map(|v| (100.0 - v).max(0.0));
    Some(DiskHealth {
        name: j["model_name"].as_str().map(String::from),
        critical: j.pointer("/smart_status/passed").and_then(|p| p.as_bool()).map(|ok| if ok { 0 } else { 0x04 }),
        temp_c: j.pointer("/temperature/current").and_then(|t| t.as_f64()),
        power_on_hours: j.pointer("/power_on_time/hours").and_then(|h| h.as_u64()),
        media_errors: (!errors.is_empty()).then(|| errors.iter().sum()),
        wear_pct: wear,
    })
}

/// The pinned smartctl, when both are set and its file's SHA-256 is the one named.
/// The projects file's `[devices]`: the smartctl a SATA disk is read through, and its pinned SHA-256.
#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub smartctl: Option<String>,
    #[serde(default)]
    pub smartctl_sha256: Option<String>,
}

/// TEXT's `[devices]`; none, or not TOML: nothing set.
#[cfg_attr(not(windows), allow(dead_code))]
fn config(text: &str) -> Config {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        devices: Config,
    }
    toml::from_str::<File>(text).map(|f| f.devices).unwrap_or_default()
}

/// C's smartctl, only when its file's SHA-256 is the pinned one.
#[cfg_attr(not(windows), allow(dead_code))]
fn pinned(c: &Config) -> Option<String> {
    use sha2::Digest;
    let (path, want) = (c.smartctl.clone()?, c.smartctl_sha256.as_deref()?.to_ascii_lowercase());
    let got = sha2::Sha256::digest(std::fs::read(&path).ok()?);
    (got.iter().map(|b| format!("{b:02x}")).collect::<String>() == want).then_some(path)
}

#[cfg(windows)]
fn smartctl() -> Option<String> {
    pinned(&config(&std::fs::read_to_string(crate::projects::config_path()).ok()?))
}

/// This machine's disks, NVMe by its log and the others through the pinned smartctl, at most once a minute.
#[cfg(windows)]
pub fn here() -> Vec<DiskHealth> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST: Mutex<Option<(Instant, Vec<DiskHealth>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, v)) = last.as_ref().filter(|(at, _)| at.elapsed() < Duration::from_secs(60)) {
        return v.clone();
    }
    let v = read_here();
    *last = Some((Instant::now(), v.clone()));
    v
}

#[cfg(windows)]
fn read_here() -> Vec<DiskHealth> {
    (0..8).filter_map(windows_disk::read).collect()
}

#[cfg(windows)]
mod windows_disk {
    use super::*;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::core::HSTRING;

    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
    const BUS_NVME: u32 = 17;

    /// DeviceIoControl's query IN, its answer in a buffer of LEN bytes.
    fn query(h: HANDLE, input: &[u8], len: usize) -> Option<Vec<u8>> {
        let mut out = vec![0u8; len];
        let mut got = 0u32;
        let r = unsafe { DeviceIoControl(h, IOCTL_STORAGE_QUERY_PROPERTY, Some(input.as_ptr().cast()), input.len() as u32, Some(out.as_mut_ptr().cast()), len as u32, Some(&mut got), None) };
        r.is_ok().then(|| {
            out.truncate(got as usize);
            out
        })
    }

    fn u32_at(b: &[u8], o: usize) -> Option<u32> {
        Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
    }

    /// The descriptor's model (its product id) and bus type: StorageDeviceProperty.
    fn describe(h: HANDLE) -> Option<(Option<String>, u32)> {
        let d = query(h, &[0u8; 12], 1024)?;
        let product = u32_at(&d, 16).filter(|o| *o > 0 && (*o as usize) < d.len()).map(|o| {
            let s = &d[o as usize..];
            String::from_utf8_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())]).trim().to_string()
        });
        Some((product.filter(|p| !p.is_empty()), u32_at(&d, 28)?))
    }

    /// The NVMe log page 0x02: StorageDeviceProtocolSpecificProperty (50), NVMe (3), a log page (2), 512 bytes.
    fn nvme_log(h: HANDLE) -> Option<Vec<u8>> {
        let mut q = vec![0u8; 8 + 40 + 512];
        for (o, v) in [(0, 50u32), (8, 3), (12, 2), (16, 0x02), (24, 40), (28, 512)] {
            q[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        let out = query(h, &q, q.len())?;
        let (off, len) = (u32_at(&out, 8 + 16)? as usize, u32_at(&out, 8 + 20)? as usize);
        out.get(8 + off..8 + off + len).map(<[u8]>::to_vec)
    }

    /// Physical drive N's health; None when there is no such drive or nothing of it reads.
    pub fn read(n: u32) -> Option<DiskHealth> {
        let path = HSTRING::from(format!("\\\\.\\PhysicalDrive{n}"));
        let h = unsafe { CreateFileW(&path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) }.ok()?;
        let r = describe(h).and_then(|(name, bus)| match bus {
            BUS_NVME => nvme_log(h).and_then(|log| from_log(name, &log)),
            _ => {
                let exe = smartctl()?;
                let out = crate::places::command(&exe).args(["--json=c", "-H", "-A", &format!("/dev/pd{n}")]).output().ok()?;
                from_smartctl(&String::from_utf8_lossy(&out.stdout))
            }
        });
        unsafe {
            let _ = CloseHandle(h);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up NVMe log reads as its health; worn past 85%, a warning bit or a media error is degraded.
    #[test]
    fn an_nvme_log_reads_as_its_health_and_says_when_it_degrades() {
        let mut log = vec![0u8; 512];
        log[1..3].copy_from_slice(&314u16.to_le_bytes());
        log[5] = 3;
        log[128..136].copy_from_slice(&1204u64.to_le_bytes());
        let h = from_log(Some("Made-up NVMe 1TB".into()), &log).unwrap();
        assert_eq!(h.words(), "wear 3% · 41 °C · no warning · no media errors · 1,204 h on");
        assert!(!h.degraded());
        assert!(DiskHealth { wear_pct: Some(86.0), ..h.clone() }.degraded());
        log[0] = 0x03;
        log[160] = 2;
        let bad = from_log(None, &log).unwrap();
        assert!(bad.degraded() && bad.words().contains("critical: spare low, too hot · 2 media errors"), "{}", bad.words());
        assert!(from_log(None, &log[..100]).is_none());
    }

    /// On a Windows machine, as the window runs (no admin): which figures of each drive read; no figure printed.
    #[cfg(windows)]
    #[test]
    #[ignore = "reads this machine's drives"]
    fn this_machines_drives_read_without_admin() {
        let v = read_here();
        for h in &v {
            println!("drive: name {} wear {} temp {} critical {} media {} hours {}", h.name.is_some(), h.wear_pct.is_some(), h.temp_c.is_some(), h.critical.is_some(), h.media_errors.is_some(), h.power_on_hours.is_some());
        }
        assert!(!v.is_empty(), "no drive read");
    }

    /// The projects file's `[devices]` names the smartctl; it is run only when its file's SHA-256 is the pinned one.
    #[test]
    fn the_smartctl_is_the_projects_files_and_runs_only_pinned() {
        let exe = std::env::temp_dir().join(format!("yaaihh-smartctl-{}", std::process::id()));
        std::fs::write(&exe, b"made-up smartctl").unwrap();
        let sha = "bd85bc1fe0f8e0a2c0bd9e4f3d6d0b88b8b8e0b4f1e2bbd2a2c5f4b1d9a6e3c7";
        let text = |sha: &str| format!("[devices]\nsmartctl = {:?}\nsmartctl_sha256 = {sha:?}\n", exe.display().to_string());
        assert_eq!(config(&text(sha)).smartctl.as_deref(), Some(exe.to_str().unwrap()));
        assert_eq!(pinned(&config(&text(sha))), None, "another SHA-256: not run");
        use sha2::Digest;
        let real: String = sha2::Sha256::digest(b"made-up smartctl").iter().map(|b| format!("{b:02X}")).collect();
        assert_eq!(pinned(&config(&text(&real))).as_deref(), Some(exe.to_str().unwrap()), "its own, in either case");
        assert_eq!(config("[project]\nname = 1\n"), Config::default(), "no [devices]: nothing set");
        let _ = std::fs::remove_file(&exe);
    }

    /// smartctl's JSON for a SATA SSD: its check, heat, hours, bad sectors and life attribute.
    #[test]
    fn smartctls_answer_reads_as_a_sata_disks_health() {
        let j = serde_json::json!({"model_name": "Made-up SATA SSD", "smart_status": {"passed": true}, "temperature": {"current": 35}, "power_on_time": {"hours": 900},
            "ata_smart_attributes": {"table": [{"id": 5, "value": 100, "raw": {"value": 0}}, {"id": 177, "value": 92, "raw": {"value": 40}}, {"id": 197, "value": 100, "raw": {"value": 1}}]}});
        let h = from_smartctl(&j.to_string()).unwrap();
        assert_eq!((h.critical, h.wear_pct, h.media_errors, h.power_on_hours), (Some(0), Some(8.0), Some(1), Some(900)));
        assert!(h.degraded(), "a pending sector is a media error");
    }
}
