//! Model prices: the bundled catalog (`resources/model_prices.json`, dollars per million tokens), how a model name
//! resolves to its price, and what a request's tokens cost (long-context and service-tier rules included). yaaihh
//! prices each agent's tokens with it; the account proxy (cpa) keeps a user's own prices over the catalog and prices
//! its usage records with the same functions, so the two agree.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub model: String,
    pub prompt: f64,
    pub completion: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
    pub prompt_configured: bool,
    pub completion_configured: bool,
    pub cache_read_configured: bool,
    pub cache_creation_configured: bool,
    pub source: String,
    pub source_model_id: String,
    pub updated_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceCatalog {
    pub schema_version: u8,
    #[serde(default)]
    pub updated_at: String,
    pub models: HashMap<String, CatalogModelPrice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModelPrice {
    pub input_per_1_m: f64,
    pub output_per_1_m: f64,
    #[serde(default)]
    pub cache_read_per_1_m: Option<f64>,
    #[serde(default)]
    pub cache_creation_per_1_m: Option<f64>,
}

/// Token counts as the usage page prices them: `input` is the whole prompt, its cached parts included; the `long_*`
/// fields are the part of each from long-context requests ([`long_context`]). `cache_creation_1h` is the part of
/// `cache_creation` written for an hour, priced at twice the input price (Anthropic's 1 h rate; the table's cache
/// write price is the 5 min one).
#[derive(Default)]
pub struct CostTokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub cache_creation_1h: u64,
    pub long_input: u64,
    pub long_output: u64,
    pub long_cache_read: u64,
    pub long_cache_creation: u64,
    pub long_cache_creation_1h: u64,
}

/// MODEL's long-context rule: (the prompt tokens above which a request is long, the multiplier of its input and cache
/// prices, the multiplier of its output price); None when the whole context is priced at the standard rates.
///
/// OpenAI's 272k rule (2× input, 1.5× output) stays the default. Anthropic (pricing page, read 17 Nov): "Claude 4.6
/// and later models (except Claude Haiku 5.5) … include the full 1M token context window at standard pricing", and
/// Haiku 5.5 pays 5× every rate over 100k. Grok 4.7: 2× both over 200k.
pub fn long_context(model: &str) -> Option<(u64, f64, f64)> {
    if is_model_family(model, "grok-4.7") {
        return Some((GROK_47_LONG_CONTEXT_INPUT_TOKEN_THRESHOLD, 2.0, 2.0));
    }
    if is_model_family(model, "claude-haiku-5-5") {
        return Some((HAIKU_55_LONG_CONTEXT_INPUT_TOKEN_THRESHOLD, 5.0, 5.0));
    }
    match claude_version(model) {
        Some(v) if v >= (4, 6) => None,
        _ => Some((LONG_CONTEXT_INPUT_TOKEN_THRESHOLD, 2.0, 1.5)),
    }
}

/// A Claude model's version from its name ("claude-opus-4-6" is (4, 6), "claude-fable-5-2" (5, 2), "claude-opus-5"
/// (5, 0), the older "claude-3-5-haiku" (3, 5); a date suffix is not a minor version). None for another model.
fn claude_version(model: &str) -> Option<(u32, u32)> {
    let tail = normalized_model_tail(model);
    let rest: Vec<&str> = tail.strip_prefix("claude-")?.split('-').collect();
    // the family name comes before the version, except in the older names
    let from = usize::from(rest.first()?.parse::<u32>().is_err());
    let mut parts = rest[from..].iter().map_while(|p| p.parse::<u32>().ok().filter(|_| p.len() < 8));
    Some((parts.next()?, parts.next().unwrap_or(0)))
}

pub fn cost_for_price(model: &str, service_tier: &str, tokens: &CostTokens, price: &ModelPrice) -> f64 {
    let price = enriched_model_price(model, price);
    // long tokens of a model without a long-context rule are priced as the rest
    let (_, long_input_multiplier, long_output_multiplier) = long_context(model).unwrap_or((0, 1.0, 1.0));
    let short_cost = cost_for_token_segment(
        tokens.input.saturating_sub(tokens.long_input),
        tokens.output.saturating_sub(tokens.long_output),
        tokens.cache_read.saturating_sub(tokens.long_cache_read),
        tokens
            .cache_creation
            .saturating_sub(tokens.long_cache_creation),
        tokens.cache_creation_1h.saturating_sub(tokens.long_cache_creation_1h),
        &price,
        1.0,
        1.0,
    );
    let long_cost = cost_for_token_segment(
        tokens.long_input,
        tokens.long_output,
        tokens.long_cache_read,
        tokens.long_cache_creation,
        tokens.long_cache_creation_1h,
        &price,
        long_input_multiplier,
        long_output_multiplier,
    );
    let tier = service_tier.trim().to_ascii_lowercase();
    let multiplier = match tier.as_str() {
        "flex" | "batch" => 0.5,
        "priority" | "fast" => service_tier_multiplier(model),
        _ => 1.0,
    };
    // A grouped total can contain both context lengths. Long-context overrides
    // must not suppress the service-tier multiplier on its short requests.
    let long_multiplier = if matches!(tier.as_str(), "priority" | "fast")
        && !is_model_family(model, "gpt-6")
    {
        1.0
    } else {
        multiplier
    };
    short_cost * multiplier + long_cost * long_multiplier
}

/// The cost of one segment's tokens: CACHE_CREATION_1H of CACHE_CREATION at twice the input price, the rest of the
/// writes at the table's (5 min) write price.
#[allow(clippy::too_many_arguments)]
pub fn cost_for_token_segment(
    input: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
    cache_creation_1h: u64,
    price: &ModelPrice,
    input_multiplier: f64,
    output_multiplier: f64,
) -> f64 {
    let prompt = input.saturating_sub(cache_read.saturating_add(cache_creation));
    let one_hour = cache_creation_1h.min(cache_creation);
    ((prompt as f64 * price.prompt
        + cache_read as f64 * price.cache_read
        + (cache_creation - one_hour) as f64 * price.cache_creation
        + one_hour as f64 * price.prompt * 2.0)
        * input_multiplier
        + output as f64 * price.completion * output_multiplier)
        / TOKENS_PER_PRICE_UNIT
}

pub fn official_model_price(model: &str) -> Option<ModelPrice> {
    find_model_price(cached_bundled_model_prices().ok()?, model).cloned()
}

pub fn bundled_model_prices() -> Result<HashMap<String, ModelPrice>, String> {
    cached_bundled_model_prices().cloned()
}

pub fn cached_bundled_model_prices() -> Result<&'static HashMap<String, ModelPrice>, String> {
    static PRICES: LazyLock<Result<HashMap<String, ModelPrice>, String>> =
        LazyLock::new(|| parse_model_price_catalog(BUNDLED_MODEL_PRICE_CATALOG, "builtin", 0));
    PRICES.as_ref().map_err(Clone::clone)
}

pub fn parse_model_price_catalog(
    content: &str,
    source: &str,
    updated_at_ms: i64,
) -> Result<HashMap<String, ModelPrice>, String> {
    let catalog = serde_json::from_str::<ModelPriceCatalog>(content)
        .map_err(|error| format!("Failed to parse model pricing file: {error}"))?;
    if catalog.schema_version != 1 {
        return Err(format!(
            "Unsupported model pricing file version {}",
            catalog.schema_version
        ));
    }
    if catalog.models.is_empty() {
        return Err("Model pricing file contains no models".to_string());
    }
    let _catalog_updated_at = catalog.updated_at;
    let mut prices = HashMap::with_capacity(catalog.models.len());
    for (model, entry) in catalog.models {
        let cache_read = entry.cache_read_per_1_m.unwrap_or(0.0);
        let cache_creation = entry.cache_creation_per_1_m.unwrap_or(0.0);
        let price = ModelPrice {
            model: model.trim().to_string(),
            prompt: entry.input_per_1_m,
            completion: entry.output_per_1_m,
            cache_read,
            cache_creation,
            prompt_configured: true,
            completion_configured: true,
            cache_read_configured: entry.cache_read_per_1_m.is_some(),
            cache_creation_configured: entry.cache_creation_per_1_m.is_some(),
            source: source.to_string(),
            source_model_id: String::new(),
            updated_at_ms,
        };
        validate_model_price(&price)?;
        prices.insert(price.model.clone(), price);
    }
    Ok(prices)
}

pub fn find_model_price<'a>(
    prices: &'a HashMap<String, ModelPrice>,
    model: &str,
) -> Option<&'a ModelPrice> {
    if let Some(price) = find_exact_model_price(prices, model, |_| true) {
        return Some(price);
    }
    let normalized_tail = normalized_model_tail(model);
    prices
        .iter()
        .filter_map(|(key, price)| {
            let key_tail = normalized_model_tail(key);
            normalized_tail
                .starts_with(&format!("{key_tail}-"))
                .then_some((key_tail.len(), price))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, price)| price)
}

pub fn find_exact_model_price<'a>(
    prices: &'a HashMap<String, ModelPrice>,
    model: &str,
    accepts: impl Fn(&ModelPrice) -> bool,
) -> Option<&'a ModelPrice> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    if let Some(price) = prices.get(model).filter(|price| accepts(price)) {
        return Some(price);
    }
    let case_insensitive = prices
        .iter()
        .filter(|(key, price)| key.eq_ignore_ascii_case(model) && accepts(price))
        .collect::<Vec<_>>();
    if case_insensitive.len() == 1 {
        return Some(case_insensitive[0].1);
    }
    let tail = canonical_model_tail(model);
    let exact_tail = prices
        .iter()
        .filter(|(key, price)| canonical_model_tail(key) == tail && accepts(price))
        .collect::<Vec<_>>();
    if exact_tail.len() == 1 {
        return Some(exact_tail[0].1);
    }
    // a dated snapshot ("claude-haiku-4-5-20251001") is priced as its model, unless the table has the date itself
    let undated = model.rsplit_once('-').filter(|(_, date)| date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit())).map(|(m, _)| m);
    undated.and_then(|m| find_exact_model_price(prices, m, accepts))
}

pub fn resolve_model_price<'a>(
    model: &'a str,
    alias: &'a str,
    prices: &HashMap<String, ModelPrice>,
) -> Option<(&'a str, ModelPrice)> {
    // Price the routed model first. Manual entries remain editable in the merged
    // catalog, but must not hide an automatic price from the bundled catalog.
    let automatic = find_exact_model_price(prices, model, |price| price.source != "manual")
        .or_else(|| {
            find_exact_model_price(cached_bundled_model_prices().ok()?, model, |_| true)
        });
    if let Some(price) = automatic {
        return Some((model, price.clone()));
    }
    for candidate in [model, alias] {
        if let Some(price) = find_exact_model_price(prices, candidate, |price| price.source == "manual") {
            return Some((model, price.clone()));
        }
    }
    None
}

/// Why there is no price table yet when the Windows proxy is still reading the saved one at the window's start
/// (yaaihh-proxy `Prices::Asking`, N-763.p1): costs show their tokens alone meanwhile, with no warning.
pub const BEING_READ: &str = "being read";

/// The model price table as the usage page edits it, for tokens counted outside the proxy: yaaihh prices each agent's
/// transcript tokens with it, so its estimate and the usage page agree.
pub struct PriceTable(HashMap<String, ModelPrice>, Option<String>);

impl PriceTable {
    /// The table of PRICES by model (the bundled catalog with a user's own prices over it, as cpa keeps them).
    pub fn new(prices: HashMap<String, ModelPrice>) -> PriceTable {
        PriceTable(prices, None)
    }

    /// The bundled catalog alone (`resources/model_prices.json`).
    pub fn bundled() -> Result<PriceTable, String> {
        bundled_model_prices().map(PriceTable::new)
    }

    /// The table with NOTE, one line the Agents page shows faint beside its costs: which table this is when it is not
    /// the one he would take it for (consult-4, N-585.p34: the bundled prices after his saved table could not be read).
    /// What follows its first `: ` is the why, which the page puts on the line's hover (N-585.p44).
    pub fn noted(mut self, note: String) -> PriceTable {
        self.1 = Some(note);
        self
    }

    /// The table's note (`noted`), if it has one.
    pub fn note(&self) -> Option<&str> {
        self.1.as_deref()
    }

    /// The cost in dollars of TOKENS of MODEL at service TIER, split by token type (uncached input, cache read, cache
    /// write, output), priced as the usage page prices a group of requests (the model resolved as a request's routed
    /// model is; each type alone by `cost_for_price`, as `usage_record_cost` splits a record, so the parts sum to the
    /// total); None when MODEL resolves to no price.
    pub fn cost_parts(&self, model: &str, tier: &str, tokens: &CostTokens) -> Option<[f64; 4]> {
        let (model, price) = resolve_model_price(model, "", &self.0)?;
        Some(split_cost(model, tier, tokens, &price))
    }
}

/// TOKENS' cost at PRICE by token type (uncached input, cache read, cache write, output). The cost is linear in each
/// type, so pricing one type at a time with [`cost_for_price`] (with its long-context and 1 h shares) splits the total
/// exactly.
pub fn split_cost(model: &str, tier: &str, tokens: &CostTokens, price: &ModelPrice) -> [f64; 4] {
    let t = tokens;
    let (cr, cc, lcr, lcc) = (t.cache_read, t.cache_creation, t.long_cache_read, t.long_cache_creation);
    let only = |input: u64, output: u64, cache_read: u64, cache_creation: u64, long: (u64, u64, u64, u64), one_hour: (u64, u64)| {
        let part = CostTokens {
            input,
            output,
            cache_read,
            cache_creation,
            cache_creation_1h: one_hour.0,
            long_input: long.0,
            long_output: long.1,
            long_cache_read: long.2,
            long_cache_creation: long.3,
            long_cache_creation_1h: one_hour.1,
        };
        cost_for_price(model, tier, &part, price)
    };
    let uncached = t.input.saturating_sub(cr + cc);
    let long_uncached = t.long_input.saturating_sub(lcr + lcc);
    [
        only(uncached, 0, 0, 0, (long_uncached, 0, 0, 0), (0, 0)),
        only(cr, 0, cr, 0, (lcr, 0, lcr, 0), (0, 0)),
        only(cc, 0, 0, cc, (lcc, 0, 0, lcc), (t.cache_creation_1h, t.long_cache_creation_1h)),
        only(0, t.output, 0, 0, (0, t.long_output, 0, 0), (0, 0)),
    ]
}

pub fn enriched_model_price(model: &str, price: &ModelPrice) -> ModelPrice {
    let mut price = price.clone();
    if let Some(official) = official_model_price(model) {
        if !price.prompt_configured && price.prompt <= 0.0 {
            price.prompt = official.prompt;
        }
        if !price.completion_configured && price.completion <= 0.0 {
            price.completion = official.completion;
        }
    }
    price
}

pub fn is_model_family(model: &str, family: &str) -> bool {
    let normalized = model
        .trim()
        .to_ascii_lowercase()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    normalized == family || normalized.starts_with(&format!("{family}-"))
}

pub fn service_tier_multiplier(model: &str) -> f64 {
    if is_model_family(model, "gpt-5.5") {
        2.5
    } else if is_model_family(model, "gpt-6")
        || is_model_family(model, "gpt-5.6")
        || is_model_family(model, "gpt-5.4")
        || is_model_family(model, "gpt-5.4-mini")
        || is_model_family(model, "gpt-5.3-codex")
        // fast mode on Claude Fable 5.2 and Mythos 5.2: $16 / $80 per MTok over $8 / $40 (pricing page, 9 Nov)
        || is_model_family(model, "claude-fable-5-2")
        || is_model_family(model, "claude-mythos-5-2")
    {
        2.0
    } else {
        1.0
    }
}

pub fn canonical_model_tail(value: &str) -> String {
    normalized_model_tail(value)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

pub const TOKENS_PER_PRICE_UNIT: f64 = 1_000_000.0;

pub const LONG_CONTEXT_INPUT_TOKEN_THRESHOLD: u64 = 272_000;

pub const GROK_47_LONG_CONTEXT_INPUT_TOKEN_THRESHOLD: u64 = 200_000;

pub const HAIKU_55_LONG_CONTEXT_INPUT_TOKEN_THRESHOLD: u64 = 100_000;

pub const BUNDLED_MODEL_PRICE_CATALOG: &str = include_str!("../resources/model_prices.json");

pub fn validate_model_price(price: &ModelPrice) -> Result<(), String> {
    if price.model.trim().is_empty() {
        return Err("Model name cannot be empty".to_string());
    }
    for value in [
        price.prompt,
        price.completion,
        price.cache_read,
        price.cache_creation,
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(format!("Model {} contains invalid pricing", price.model));
        }
    }
    Ok(())
}

pub fn normalized_model_tail(value: &str) -> String {
    value
        .trim()
        .rsplit('/')
        .find(|part| !part.trim().is_empty() && !part.eq_ignore_ascii_case("models"))
        .unwrap_or(value)
        .to_ascii_lowercase()
}
