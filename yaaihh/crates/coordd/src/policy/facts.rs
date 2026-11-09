//! The facts the system supplies to the policy (draft §2, "the prelude"): never written by hand. For the slice, the
//! models: the ones the proxy serves (`GET /v1/models`), each with its family (from the provider that owns it), its
//! provider and its input price, in micro-dollars per million tokens, from the price catalog yaaihh's usage pages use
//! (cpa's `resources/model_prices.json`, compiled in: one catalog, not a second registry).

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

/// The proxy the agents use: its address and its key.
#[derive(Clone, Debug)]
pub struct Proxy {
    pub url: String,
    pub key: String,
}

impl Proxy {
    /// The proxy of the coord folder COORD: `ANTHROPIC_BASE_URL` (the box's `http://host.docker.internal:8317`), else
    /// the host's `http://127.0.0.1:8317`; the key the agents read (`$COORD/.proxy-key`).
    pub fn of(coord: &str) -> Proxy {
        let url = std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8317".into());
        let key = std::fs::read_to_string(format!("{coord}/.proxy-key")).unwrap_or_default().trim().to_string();
        Proxy { url: url.trim_end_matches('/').to_string(), key }
    }

    pub fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).http_status_as_error(false).build().into()
    }
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

const CATALOG: &str = include_str!("../../../prices/resources/model_prices.json");

/// The family of a provider's models, as the owner names them ("the Codex family first, Claude second").
fn family(provider: &str) -> &str {
    match provider {
        "anthropic" => "claude",
        "openai" => "codex",
        "moonshot" => "kimi",
        "google" => "gemini",
        other => other,
    }
}

/// The agent harness that runs a provider's models in a session:
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

/// The model facts, as (fact, arguments): `model`, `family`, `provider`, `harness`, `price` (a model without a catalog price
/// gets none, so `Requires price(option) >= 0` excludes it rather than taking it as free).
pub fn models(proxy: &Proxy) -> Result<Vec<(String, Vec<Value>)>, String> {
    let resp = proxy
        .agent()
        .get(&format!("{}/v1/models", proxy.url))
        .header("Authorization", &format!("Bearer {}", proxy.key))
        .call()
        .map_err(|e| format!("the proxy's models: {e}"))?;
    let status = resp.status();
    let body: Value = resp.into_body().read_json().map_err(|e| format!("the proxy's models: {e}"))?;
    if !status.is_success() {
        return Err(format!("the proxy's models: {status}: {body}"));
    }
    let served: Vec<(String, String)> = body["data"]
        .as_array()
        .ok_or("the proxy's models: no data")?
        .iter()
        .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["owned_by"].as_str().unwrap_or("").to_string())))
        .collect();
    Ok(model_facts(&served))
}

/// The facts of SERVED models (id, provider).
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
