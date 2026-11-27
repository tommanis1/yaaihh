//! Compute workers on a device's card: each NativeLink worker coordd found on that machine, its state and its sandbox.
//! They come in the Devices answer (`Request::Machines`, a device's `workers`); the compute cache is a Store row.

use crate::devices::{items, lenient};
use rune::{text, El};
use rune_kit::Kit;
use serde::Deserialize;

/// One worker: its name, `idle`, `busy` or `offline`, its sandbox (`bwrap`, `podman`) and how many actions it runs now.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Worker {
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub sandbox: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub running: Option<u64>,
}

/// A device's workers as the Devices answer gives them, a field of another type left out.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Workers(#[serde(deserialize_with = "items")] pub Vec<Worker>);

/// W in words: "compute w1 · busy · bwrap · 2 running".
pub fn words(w: &Worker) -> String {
    let mut parts = vec![format!("compute {}", w.name.as_deref().filter(|n| !n.is_empty()).unwrap_or("worker"))];
    parts.extend(w.state.clone().filter(|s| !s.is_empty()));
    parts.extend(w.sandbox.clone().filter(|s| !s.is_empty()));
    if let Some(n) = w.running.filter(|n| *n > 0 && w.state.as_deref() != Some("offline")) {
        parts.push(format!("{n} running"));
    }
    parts.join(" · ")
}

/// The card lines of DEVICE's workers (NAME is the card's): one each, quiet as the card's other lines.
pub fn lines(workers: &Workers, device: &str, k: &Kit) -> Vec<El> {
    workers
        .0
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let color = if w.state.as_deref() == Some("offline") { k.c.text_faint } else { k.c.text_muted };
            text(words(w)).font(k.ty.small).color(color).truncate().key(format!("device-worker-{device}-{i}"))
        })
        .collect()
}

#[cfg(test)]
#[path = "compute/tests.rs"]
mod tests;
