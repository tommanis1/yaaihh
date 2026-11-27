//! Home's Store: the build-runner's service asked on its socket, one line of JSON each way. The walk is the runner's.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// The runner's service socket, beside its job files.
pub fn socket() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("agent-coord/build-runner/sock")
}

/// Q asked of the service on SOCK: its answer's line as it came. It waits while the runner keeps the connection.
pub fn line(sock: &Path, q: &Value) -> std::io::Result<String> {
    let mut s = UnixStream::connect(sock)?;
    s.write_all(format!("{q}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line)?;
    Ok(line)
}

/// The store's rows, or a page of ROW's entries from FROM, as the runner's one line; its refusal or silence in words.
pub fn rows(sock: &Path, row: Option<&str>, from: u64) -> Result<String, String> {
    let line = line(sock, &json!({"store": {"row": row, "from": from}})).map_err(|e| format!("the runner's service at {}: {e}", sock.display()))?;
    if line.is_empty() {
        return Err("the runner ended the connection without an answer".into());
    }
    match serde_json::from_str::<Value>(line.trim()) {
        Ok(v) if v.get("error").is_some() => Err(v["error"].as_str().unwrap_or("the runner refused it").to_string()),
        Ok(v) if v.is_object() => Ok(line.trim().to_string()),
        _ => Err("the runner answered no store (a runner from before Home's Store?)".into()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread::JoinHandle;

    /// A stand-in runner on SOCK: each of ANSWERS for one connection ("" ends it unanswered); what each asked.
    pub(crate) fn runner(sock: &Path, answers: &[&str]) -> JoinHandle<Vec<Value>> {
        let (l, answers) = (UnixListener::bind(sock).unwrap(), answers.iter().map(|a| a.to_string()).collect::<Vec<_>>());
        std::thread::spawn(move || {
            let mut asked = Vec::new();
            for a in answers {
                let (s, _) = l.accept().unwrap();
                let mut q = String::new();
                BufReader::new(s.try_clone().unwrap()).read_line(&mut q).unwrap();
                asked.push(serde_json::from_str::<Value>(&q).unwrap());
                if !a.is_empty() {
                    (&s).write_all(format!("{a}\n").as_bytes()).unwrap();
                }
            }
            asked
        })
    }

    /// The runner's line goes back as it is; its `error`, an answer that is no store, a runner that ends without an
    /// answer and a socket with no service are said in words.
    #[test]
    fn the_stores_rows_come_from_the_runners_service_as_it_answers() {
        let d = std::env::temp_dir().join(format!("machine-state-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let sock = d.join("sock");
        let rows_line = r#"{"rows": [{"id": "crates", "device": "build-1", "count": 4945}]}"#;
        let serve = runner(&sock, &[rows_line, r#"{"error": "no row 'states' here: crates, binaries"}"#, "build-runner: refused: an old runner", ""]);
        assert_eq!(rows(&sock, None, 0), Ok(rows_line.to_string()));
        assert_eq!(rows(&sock, Some("states"), 200), Err("no row 'states' here: crates, binaries".into()));
        assert!(rows(&sock, None, 0).is_err_and(|e| e.contains("the runner answered no store")));
        assert_eq!(rows(&sock, None, 0), Err("the runner ended the connection without an answer".into()));
        let asked = serve.join().unwrap();
        assert_eq!(asked[..2], [json!({"store": {"row": null, "from": 0}}), json!({"store": {"row": "states", "from": 200}})]);
        let none = d.join("none");
        assert!(rows(&none, None, 0).is_err_and(|e| e.starts_with(&format!("the runner's service at {}", none.display()))));
        assert_eq!(socket(), PathBuf::from(std::env::var("HOME").unwrap()).join("agent-coord/build-runner/sock"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
