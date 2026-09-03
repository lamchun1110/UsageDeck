use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::{ModelRates, PricingCatalog};

#[derive(Debug, Error)]
pub enum PricingCodecError {
    #[error("Pricing feed is not a JSON object.")]
    NotAnObject,
    #[error("Pricing feed contained no usable model entries.")]
    NoUsableEntries,
    #[error("Pricing feed is invalid JSON.")]
    InvalidJson(#[from] serde_json::Error),
}

pub fn catalog_from_litellm(data: &[u8]) -> Result<PricingCatalog, PricingCodecError> {
    let root = serde_json::from_slice::<Value>(data)?;
    let root = root.as_object().ok_or(PricingCodecError::NotAnObject)?;
    let mut entries = HashMap::new();
    for (key, value) in root {
        let Some(entry) = value.as_object() else {
            continue;
        };
        let (Some(input), Some(output)) = (
            number(entry.get("input_cost_per_token")),
            number(entry.get("output_cost_per_token")),
        ) else {
            continue;
        };
        let mut rates = ModelRates::new(input * 1_000_000.0, output * 1_000_000.0);
        rates.cache_write_per_million =
            number(entry.get("cache_creation_input_token_cost")).unwrap_or(input) * 1_000_000.0;
        let cache_read = number(entry.get("cache_read_input_token_cost"));
        rates.cache_read_per_million = cache_read.unwrap_or(input * 0.1) * 1_000_000.0;
        rates.cache_read_is_explicit = cache_read.is_some();
        rates.input_above_200k_per_million =
            number(entry.get("input_cost_per_token_above_200k_tokens"))
                .map(|rate| rate * 1_000_000.0);
        rates.output_above_200k_per_million =
            number(entry.get("output_cost_per_token_above_200k_tokens"))
                .map(|rate| rate * 1_000_000.0);
        rates.cache_write_above_200k_per_million =
            number(entry.get("cache_creation_input_token_cost_above_200k_tokens"))
                .map(|rate| rate * 1_000_000.0);
        rates.cache_read_above_200k_per_million =
            number(entry.get("cache_read_input_token_cost_above_200k_tokens"))
                .map(|rate| rate * 1_000_000.0);
        rates.fast_multiplier = entry
            .get("provider_specific_entry")
            .and_then(Value::as_object)
            .and_then(|specific| number(specific.get("fast")))
            .unwrap_or(1.0);
        if !rates.is_plausible() {
            continue;
        }
        entries.insert(key.clone(), rates);
    }
    if entries.is_empty() {
        return Err(PricingCodecError::NoUsableEntries);
    }
    Ok(PricingCatalog {
        entries,
        retrieved_at: None,
    })
}

pub fn catalog_from_models_dev(data: &[u8]) -> Result<PricingCatalog, PricingCodecError> {
    let root = serde_json::from_slice::<Value>(data)?;
    let root = root.as_object().ok_or(PricingCodecError::NotAnObject)?;
    let mut providers = root.keys().collect::<Vec<_>>();
    providers.sort();
    let mut entries = HashMap::new();
    for provider_name in providers {
        let Some(models) = root[provider_name].get("models").and_then(Value::as_object) else {
            continue;
        };
        for (model_id, value) in models {
            if entries.contains_key(model_id) {
                continue;
            }
            let Some(cost) = value.get("cost").and_then(Value::as_object) else {
                continue;
            };
            let (Some(input), Some(output)) =
                (number(cost.get("input")), number(cost.get("output")))
            else {
                continue;
            };
            let mut rates = ModelRates::new(input, output);
            rates.cache_write_per_million = number(cost.get("cache_write")).unwrap_or(input);
            let cache_read = number(cost.get("cache_read"));
            rates.cache_read_per_million = cache_read.unwrap_or(input * 0.1);
            rates.cache_read_is_explicit = cache_read.is_some();
            if !rates.is_plausible() {
                continue;
            }
            entries.insert(model_id.clone(), rates);
        }
    }
    if entries.is_empty() {
        return Err(PricingCodecError::NoUsableEntries);
    }
    Ok(PricingCatalog {
        entries,
        retrieved_at: None,
    })
}

/// OpenRouter's `/api/v1/models` feed. Rates are quoted per token as strings.
///
/// Only the plain model ids are kept: OpenRouter publishes variant slugs
/// (`:batch` at half price, `:free` at zero, `:nitro`, `:floor`) that describe
/// routing tiers rather than models, and letting them into the catalog would
/// hand fuzzy matching a cheaper twin of every model. `pricing.overrides` is
/// ignored for the same reason a historical event cannot be priced against it:
/// the discounts are keyed to time of day, so applying them would need the rate
/// table as it stood when the tokens were spent.
pub fn catalog_from_openrouter(data: &[u8]) -> Result<PricingCatalog, PricingCodecError> {
    let root = serde_json::from_slice::<Value>(data)?;
    let models = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or(PricingCodecError::NotAnObject)?;
    let mut entries = HashMap::new();
    for model in models {
        let Some(id) = model.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.contains(':') {
            continue;
        }
        let Some(pricing) = model.get("pricing").and_then(Value::as_object) else {
            continue;
        };
        let (Some(input), Some(output)) = (
            number(pricing.get("prompt")),
            number(pricing.get("completion")),
        ) else {
            continue;
        };
        // A zero-rated entry is a free routing tier, not a priced model; keeping
        // it would resolve real usage to no cost at all.
        if input <= 0.0 && output <= 0.0 {
            continue;
        }
        let mut rates = ModelRates::new(input * 1_000_000.0, output * 1_000_000.0);
        rates.cache_write_per_million =
            number(pricing.get("input_cache_write")).unwrap_or(input) * 1_000_000.0;
        let cache_read = number(pricing.get("input_cache_read"));
        rates.cache_read_per_million = cache_read.unwrap_or(input * 0.1) * 1_000_000.0;
        rates.cache_read_is_explicit = cache_read.is_some();
        // The only feed that states the one-hour cache-write rate outright,
        // rather than leaving it to the Anthropic-style 2x-input convention.
        rates.cache_write_1h_per_million =
            number(pricing.get("input_cache_write_1h")).map(|rate| rate * 1_000_000.0);
        if !rates.is_plausible() {
            continue;
        }
        entries.insert(id.to_owned(), rates);
    }
    if entries.is_empty() {
        return Err(PricingCodecError::NoUsableEntries);
    }
    Ok(PricingCatalog {
        entries,
        retrieved_at: None,
    })
}

fn number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value.as_f64().or_else(|| {
        // The LiteLLM feed quotes some costs as strings.
        value
            .as_str()
            .and_then(|text| text.trim().parse::<f64>().ok())
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactCatalog {
    #[serde(default)]
    retrieved_at: Option<String>,
    models: BTreeMap<String, CompactModel>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactModel {
    i: f64,
    o: f64,
    cw: f64,
    cr: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ia: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    oa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cra: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fast: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cre: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cw1: Option<f64>,
}

pub fn catalog_from_compact(data: &[u8]) -> Result<PricingCatalog, PricingCodecError> {
    let file = serde_json::from_slice::<CompactCatalog>(data)?;
    let entries = file
        .models
        .into_iter()
        .map(|(key, model)| {
            (
                key,
                ModelRates {
                    input_per_million: model.i,
                    output_per_million: model.o,
                    cache_write_per_million: model.cw,
                    cache_read_per_million: model.cr,
                    cache_write_1h_per_million: model.cw1,
                    // No feed states a long-context 1h rate; it is derived per
                    // usage source (see `with_anthropic_one_hour_cache`) after
                    // a catalog hand-off, so it never needs a compact key.
                    cache_write_1h_above_200k_per_million: None,
                    input_above_200k_per_million: model.ia,
                    output_above_200k_per_million: model.oa,
                    cache_write_above_200k_per_million: model.cwa,
                    cache_read_above_200k_per_million: model.cra,
                    cache_read_is_explicit: model.cre.unwrap_or(true),
                    long_context_threshold_tokens: 200_000,
                    fast_multiplier: model.fast.unwrap_or(1.0),
                },
            )
        })
        // A bundled snapshot or disk cache written before this guard existed
        // still carries the corrupt rows, so filter on the way back in too.
        .filter(|(_, rates)| rates.is_plausible())
        .collect::<HashMap<_, _>>();
    Ok(PricingCatalog {
        entries,
        retrieved_at: file.retrieved_at,
    })
}

pub fn compact_data(catalog: &PricingCatalog) -> Result<Vec<u8>, PricingCodecError> {
    let models = catalog
        .entries
        .iter()
        .map(|(key, rates)| {
            (
                key.clone(),
                CompactModel {
                    i: rates.input_per_million,
                    o: rates.output_per_million,
                    cw: rates.cache_write_per_million,
                    cr: rates.cache_read_per_million,
                    ia: rates.input_above_200k_per_million,
                    oa: rates.output_above_200k_per_million,
                    cwa: rates.cache_write_above_200k_per_million,
                    cra: rates.cache_read_above_200k_per_million,
                    fast: (rates.fast_multiplier != 1.0).then_some(rates.fast_multiplier),
                    cre: (!rates.cache_read_is_explicit).then_some(false),
                    cw1: rates.cache_write_1h_per_million,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(serde_json::to_vec(&CompactCatalog {
        retrieved_at: catalog.retrieved_at.clone(),
        models,
    })?)
}

#[cfg(test)]
mod tests {
    use super::{
        catalog_from_compact, catalog_from_litellm, catalog_from_models_dev,
        catalog_from_openrouter, compact_data,
    };

    #[test]
    fn every_decoder_drops_a_rate_no_price_list_could_carry() {
        // LiteLLM quoting input_cost_per_token in the wrong unit: 0.135 per
        // token is $135,000 per million, and it reached real model names
        // through fuzzy matching.
        let litellm = catalog_from_litellm(
            br#"{"good":{"input_cost_per_token":0.000003,"output_cost_per_token":0.000015},
                 "wandb/bad":{"input_cost_per_token":0.135,"output_cost_per_token":0.54}}"#,
        )
        .unwrap();
        assert_eq!(litellm.entries.keys().collect::<Vec<_>>(), vec!["good"]);

        let models_dev = catalog_from_models_dev(
            br#"{"p":{"models":{"good":{"cost":{"input":3,"output":15}},
                               "bad":{"cost":{"input":8000,"output":35000}}}}}"#,
        )
        .unwrap();
        assert_eq!(models_dev.entries.keys().collect::<Vec<_>>(), vec!["good"]);

        let openrouter = catalog_from_openrouter(
            br#"{"data":[{"id":"v/good","pricing":{"prompt":"0.000003","completion":"0.000015"}},
                         {"id":"v/bad","pricing":{"prompt":"0.135","completion":"0.54"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            openrouter.entries.keys().collect::<Vec<_>>(),
            vec!["v/good"]
        );

        // Snapshots and disk caches written before the guard still hold the
        // corrupt rows, so decoding filters them on the way back in.
        let compact = catalog_from_compact(
            br#"{"models":{"good":{"i":3,"o":15,"cw":3,"cr":0.3},
                           "bad":{"i":135000,"o":540000,"cw":135000,"cr":13500}}}"#,
        )
        .unwrap();
        assert_eq!(compact.entries.keys().collect::<Vec<_>>(), vec!["good"]);
    }

    #[test]
    fn parses_litellm_defaults_and_round_trips_compact_data() {
        let feed = br#"{"model":{"input_cost_per_token":0.000003,"output_cost_per_token":0.000015,"provider_specific_entry":{"fast":6}}}"#;
        let mut catalog = catalog_from_litellm(feed).unwrap();
        catalog
            .entries
            .get_mut("model")
            .unwrap()
            .cache_write_1h_per_million = Some(6.0);
        let rates = catalog.entries["model"];
        assert_eq!(rates.input_per_million, 3.0);
        assert_eq!(rates.cache_write_per_million, 3.0);
        assert!((rates.cache_read_per_million - 0.3).abs() < 0.000_001);
        assert!(!rates.cache_read_is_explicit);
        assert_eq!(rates.fast_multiplier, 6.0);
        let compact = compact_data(&catalog).unwrap();
        assert_eq!(super::catalog_from_compact(&compact).unwrap(), catalog);
    }

    #[test]
    fn models_dev_uses_first_provider_in_name_order() {
        let feed = br#"{"z":{"models":{"shared":{"cost":{"input":9,"output":9}}}},"a":{"models":{"shared":{"cost":{"input":1,"output":2}}}}}"#;
        let catalog = catalog_from_models_dev(feed).unwrap();
        assert_eq!(catalog.entries["shared"].input_per_million, 1.0);
        assert!(!catalog.entries["shared"].cache_read_is_explicit);
    }

    #[test]
    fn openrouter_converts_per_token_rates_and_keeps_the_one_hour_cache_rate() {
        let feed = br#"{"data":[{"id":"anthropic/claude-sonnet-5","pricing":{
            "prompt":"0.000002","completion":"0.00001","input_cache_read":"0.0000002",
            "input_cache_write":"0.0000025","input_cache_write_1h":"0.000004"}}]}"#;
        let catalog = catalog_from_openrouter(feed).unwrap();
        let rates = catalog.entries["anthropic/claude-sonnet-5"];

        assert_eq!(rates.input_per_million, 2.0);
        assert_eq!(rates.output_per_million, 10.0);
        assert!((rates.cache_read_per_million - 0.2).abs() < 0.000_001);
        assert_eq!(rates.cache_write_per_million, 2.5);
        assert_eq!(rates.cache_write_1h_per_million, Some(4.0));
        assert!(rates.cache_read_is_explicit);
    }

    #[test]
    fn openrouter_skips_variant_slugs_and_free_tiers() {
        let feed = br#"{"data":[
            {"id":"vendor/model","pricing":{"prompt":"0.000001","completion":"0.000002"}},
            {"id":"vendor/model:batch","pricing":{"prompt":"0.0000005","completion":"0.000001"}},
            {"id":"vendor/free","pricing":{"prompt":"0","completion":"0"}}]}"#;
        let catalog = catalog_from_openrouter(feed).unwrap();

        assert_eq!(catalog.entries.len(), 1);
        assert!(catalog.entries.contains_key("vendor/model"));
    }

    #[test]
    fn openrouter_defaults_cache_rates_and_ignores_time_of_day_overrides() {
        let feed = br#"{"data":[{"id":"vendor/plain","pricing":{
            "prompt":"0.000001","completion":"0.000002",
            "overrides":[{"utc_days":["saturday"],"prompt":"0.0000005"}]}}]}"#;
        let catalog = catalog_from_openrouter(feed).unwrap();
        let rates = catalog.entries["vendor/plain"];

        assert_eq!(rates.input_per_million, 1.0);
        assert_eq!(rates.cache_write_per_million, 1.0);
        assert!((rates.cache_read_per_million - 0.1).abs() < 0.000_001);
        assert!(!rates.cache_read_is_explicit);
        assert_eq!(rates.cache_write_1h_per_million, None);
    }

    #[test]
    fn openrouter_rejects_a_feed_with_no_priced_models() {
        assert!(catalog_from_openrouter(br#"{"data":[]}"#).is_err());
        assert!(catalog_from_openrouter(br#"{"models":[]}"#).is_err());
    }

    #[test]
    fn legacy_compact_catalog_treats_unmarked_cache_read_as_explicit() {
        let legacy = br#"{"models":{"gpt-test":{"i":5,"o":30,"cw":5,"cr":0.5}}}"#;
        let catalog = super::catalog_from_compact(legacy).unwrap();

        assert!(catalog.entries["gpt-test"].cache_read_is_explicit);
    }
}
