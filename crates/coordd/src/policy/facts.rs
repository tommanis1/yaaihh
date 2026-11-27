//! The facts the system supplies to the policy (draft §2, "the prelude"): never written by hand. For the slice, the
//! models: the ones the places serve (`accounts::router::Router::served`), each with its family (from its provider), its
//! provider and its input price, in micro-dollars per million tokens, from the price catalog yaaihh's usage pages use
//! (cpa's `resources/model_prices.json`, compiled in: one catalog, not a second registry).

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

/// The gateway: its address and its key, which the router reads and sends (`accounts::router`).
#[derive(Clone, Debug)]
pub struct Proxy {
    pub url: String,
    pub key: String,
}

/// How long one LLM call coordd makes may take, wherever the router runs it: the gateway's whole request
/// ([`Proxy::agent`]), and Claude Code on the owner's accounts (`accounts::router`), after which it is ended and the
/// call fails, so no decision waits on it for ever (proto-reader, I-1469 p82).
pub const CALL_BOUND: std::time::Duration = std::time::Duration::from_secs(120);

impl Proxy {
    /// The proxy of the coord folder COORD: `ANTHROPIC_BASE_URL` (the box's `http://host.docker.internal:8317`), else
    /// the host's `http://127.0.0.1:8317`; the key the agents read (`$COORD/.proxy-key`).
    pub fn of(coord: &str) -> Proxy {
        #[cfg(test)]
        if let Some(url) = TEST_URL.with(|u| u.borrow().clone()) {
            return Proxy { url, key: String::new() };
        }
        let url = std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8317".into());
        let key = std::fs::read_to_string(format!("{coord}/.proxy-key")).unwrap_or_default().trim().to_string();
        Proxy { url: url.trim_end_matches('/').to_string(), key }
    }

    pub fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder().timeout_global(Some(CALL_BOUND)).http_status_as_error(false).build().into()
    }
}

#[cfg(test)]
thread_local! {
    /// A test's stand-in proxy, for the engines started on its thread: never the real one (its usage is recorded).
    pub static TEST_URL: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

#[derive(Deserialize)]
struct Catalog {
    models: HashMap<String, CatalogPrice>,
}

#[derive(Deserialize)]
struct CatalogPrice {
    #[serde(rename = "inputPer1M")]
    input_per_1m: f64,
}

const CATALOG: &str = include_str!("../../../usage/prices/resources/model_prices.json");

/// The family of a provider's models, as the owner names them ("the Codex family first, Claude second").
pub(crate) fn family(provider: &str) -> &str {
    match provider {
        "anthropic" => "claude",
        "openai" => "codex",
        "moonshot" => "kimi",
        "google" => "gemini",
        other => other,
    }
}

/// The agent harness that runs a provider's models in a session (owner m116: a net's session is a harness session):
/// Codex for OpenAI's, Claude Code for Anthropic's; none for the others.
fn harness(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("codex"),
        "anthropic" => Some("claude"),
        _ => None,
    }
}

/// The catalog's input price of MODEL in micro-dollars per million tokens: its own entry, else the entry it is a dated
/// version of (`claude-haiku-4-5-20251001` is priced as `claude-haiku-4-5`; `gpt-5.7` is not priced as `gpt-5`).
fn price(prices: &HashMap<String, CatalogPrice>, model: &str) -> Option<i64> {
    let dated = model.rsplit_once('-').filter(|(_, d)| d.len() >= 6 && d.chars().all(|c| c.is_ascii_digit())).map(|(base, _)| base);
    let p = prices.get(model).or_else(|| prices.get(dated?))?;
    Some((p.input_per_1m * 1_000_000.0).round() as i64)
}

/// The model facts of SERVED models (id, provider), as (fact, arguments): `model`, `family`, `provider`, `harness`,
/// `price` (a model without a catalog price gets none, so `Requires price(option) >= 0` excludes it rather than taking
/// it as free).
pub fn model_facts(served: &[(String, String)]) -> Vec<(String, Vec<Value>)> {
    let catalog: Catalog = serde_json::from_str(CATALOG).expect("the compiled-in price catalog parses");
    let mut out = Vec::new();
    for (id, provider) in served {
        out.push(("model".into(), vec![json!(id)]));
        out.push(("provider".into(), vec![json!(id), json!(provider)]));
        out.push(("family".into(), vec![json!(id), json!(family(provider))]));
        if let Some(h) = harness(provider) {
            out.push(("harness".into(), vec![json!(id), json!(h)]));
        }
        if let Some(p) = price(&catalog.models, id) {
            out.push(("price".into(), vec![json!(id), json!(p)]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dated_ids_are_priced_as_their_catalog_entry_and_unknown_ones_not_at_all() {
        let f = model_facts(&[("claude-haiku-4-5-20251001".into(), "anthropic".into()), ("gpt-image-2".into(), "openai".into())]);
        let catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
        let want = (catalog.models["claude-haiku-4-5"].input_per_1m * 1e6).round() as i64;
        assert!(f.contains(&("price".into(), vec![json!("claude-haiku-4-5-20251001"), json!(want)])), "{f:?}");
        assert!(!f.iter().any(|(n, a)| n == "price" && a[0] == json!("gpt-image-2")));
        assert!(f.contains(&("family".into(), vec![json!("gpt-image-2"), json!("codex")])));
    }
}
