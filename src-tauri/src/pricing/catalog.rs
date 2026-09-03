use std::collections::HashMap;

use super::ModelRates;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PricingCatalog {
    pub entries: HashMap<String, ModelRates>,
    pub retrieved_at: Option<String>,
}

impl PricingCatalog {
    pub fn find_exact(&self, model: &str) -> Option<(&str, ModelRates)> {
        self.entries
            .get_key_value(model)
            .map(|(key, rates)| (key.as_str(), *rates))
    }

    pub fn find_fuzzy(&self, model: &str) -> Option<(&str, ModelRates)> {
        let normalized_model = normalized_key(model);
        self.entries
            .iter()
            .filter(|(key, _)| key_matches(key, model, &normalized_model))
            .map(|(key, rates)| (key.as_str(), *rates))
            .min_by(|(left, _), (right, _)| {
                // A key that names this very model, once its vendor prefix is
                // dropped, beats one that names a variant of it. Length alone
                // preferred the variant, because a suffix only makes a key
                // longer: "GLM-4.7" matched "deepinfra/zai-org/GLM-4.7-Flash"
                // and billed the full model at a tenth of its rate.
                names_model_exactly(right, &normalized_model)
                    .cmp(&names_model_exactly(left, &normalized_model))
                    .then_with(|| right.len().cmp(&left.len()))
                    .then_with(|| left.cmp(right))
            })
    }

    /// Exact match, then an exact match on ids whose vendor prefix is stripped.
    ///
    /// OpenRouter publishes every model as `vendor/model` while provider logs
    /// record the bare name, so the gap filler is useless without this. It is
    /// deliberately narrower than [`Self::find_fuzzy`]: prefix stripping still
    /// requires the model name itself to match in full, where fuzzy matching
    /// would let `seed-1.6-flash` price plain `seed-1.6` - the same near miss
    /// the `-fast` handling in `ModelPricing::lookup` refuses to make. Ties
    /// between vendors publishing one bare name resolve to the lowest id, so
    /// the answer never depends on hash order.
    pub fn find_vendor_prefixed(&self, model: &str) -> Option<(&str, ModelRates)> {
        if let Some(hit) = self.find_exact(model) {
            return Some(hit);
        }
        let normalized_model = normalized_key(model);
        self.entries
            .iter()
            .filter(|(key, _)| {
                key.rsplit_once('/')
                    .is_some_and(|(_, bare)| normalized_key(bare) == normalized_model)
            })
            .map(|(key, rates)| (key.as_str(), *rates))
            .min_by(|(left, _), (right, _)| left.cmp(right))
    }

    pub fn merging(mut self, other: PricingCatalog) -> Self {
        self.entries.extend(other.entries);
        if other.retrieved_at.is_some() {
            self.retrieved_at = other.retrieved_at;
        }
        self
    }
}

/// Whether a catalog key names exactly this model once its vendor prefix is
/// dropped, rather than naming a variant such as a `-Flash` or `-FP8` build.
fn names_model_exactly(key: &str, normalized_model: &str) -> bool {
    let bare = key.rsplit('/').next().unwrap_or(key);
    normalized_key(bare) == normalized_model
}

pub fn normalized_key(value: &str) -> String {
    value.replace(['.', '@'], "-")
}

fn key_matches(candidate: &str, model: &str, normalized_model: &str) -> bool {
    if contains_key(model, candidate) || contains_key(candidate, model) {
        return true;
    }
    let normalized_candidate = normalized_key(candidate);
    contains_key(normalized_model, &normalized_candidate)
        || contains_key(&normalized_candidate, normalized_model)
}

fn contains_key(value: &str, key: &str) -> bool {
    if key.is_empty() || key.len() > value.len() {
        return false;
    }
    let value = value.as_bytes();
    let key = key.as_bytes();
    for start in 0..=value.len() - key.len() {
        if &value[start..start + key.len()] != key {
            continue;
        }
        if start > 0 && value[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        if suffix_allows_match(key, &value[start + key.len()..]) {
            return true;
        }
    }
    false
}

fn suffix_allows_match(key: &[u8], suffix: &[u8]) -> bool {
    let Some(separator) = suffix.first() else {
        return true;
    };
    if separator.is_ascii_alphanumeric() {
        return false;
    }
    !suffix_starts_with_numeric_model_version(key, suffix)
}

fn suffix_starts_with_numeric_model_version(key: &[u8], suffix: &[u8]) -> bool {
    if !key.last().is_some_and(u8::is_ascii_digit) || !matches!(suffix.first(), Some(b'-' | b'.')) {
        return false;
    }
    let digits = suffix[1..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return false;
    }
    let after_digits = suffix.get(1 + digits);
    let is_date = digits == 8 && after_digits.is_none_or(|byte| !byte.is_ascii_alphanumeric());
    !is_date
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::PricingCatalog;
    use crate::pricing::ModelRates;

    fn catalog(entries: &[(&str, f64)]) -> PricingCatalog {
        PricingCatalog {
            entries: entries
                .iter()
                .map(|(name, input)| ((*name).to_owned(), ModelRates::new(*input, 2.0)))
                .collect::<HashMap<_, _>>(),
            retrieved_at: None,
        }
    }

    #[test]
    fn fuzzy_matching_handles_dates_prefixes_and_separators() {
        let pricing = catalog(&[("claude-sonnet-4-20250514", 3.0), ("xai/grok-4.3", 1.25)]);
        assert_eq!(
            pricing
                .find_fuzzy("claude-sonnet-4")
                .unwrap()
                .1
                .input_per_million,
            3.0
        );
        assert_eq!(
            pricing.find_fuzzy("grok-4-3").unwrap().1.input_per_million,
            1.25
        );
    }

    #[test]
    fn numeric_versions_do_not_conflate() {
        let newer = catalog(&[("claude-sonnet-4-5", 3.0)]);
        assert!(newer.find_fuzzy("claude-sonnet-4").is_none());
        let older = catalog(&[("claude-sonnet-4", 1.0)]);
        assert!(older.find_fuzzy("claude-sonnet-4-5").is_none());
    }

    #[test]
    fn vendor_prefix_matching_requires_the_whole_model_name() {
        let pricing = catalog(&[
            ("bytedance-seed/seed-1.6-flash", 0.075),
            ("aion-labs/aion-3.0", 3.0),
        ]);
        assert_eq!(
            pricing
                .find_vendor_prefixed("aion-3.0")
                .unwrap()
                .1
                .input_per_million,
            3.0
        );
        // Dots and dashes stay interchangeable, as everywhere else here.
        assert_eq!(
            pricing
                .find_vendor_prefixed("aion-3-0")
                .unwrap()
                .1
                .input_per_million,
            3.0
        );
        // find_fuzzy accepts this; prefix stripping must not, or the flash
        // variant prices the plain model.
        assert!(pricing.find_vendor_prefixed("seed-1.6").is_none());
        assert!(pricing.find_fuzzy("seed-1.6").is_some());
    }

    #[test]
    fn vendor_prefix_ties_resolve_to_the_lowest_id() {
        let pricing = catalog(&[("zeta/shared-model", 2.0), ("alpha/shared-model", 1.0)]);
        assert_eq!(
            pricing.find_vendor_prefixed("shared-model").unwrap().0,
            "alpha/shared-model"
        );
    }

    #[test]
    fn a_variant_build_never_shadows_the_model_it_is_a_variant_of() {
        // Every one of these is longer than the plain key, so ranking by length
        // alone handed the query to the variant: "GLM-4.7" resolved through
        // "deepinfra/zai-org/GLM-4.7-Flash" at a tenth of the real rate, and
        // "claude-sonnet-4.5" through a us-gov Bedrock key at $3.60 instead of
        // the $3.00 list price.
        let pricing = catalog(&[
            ("deepinfra/zai-org/GLM-4.7-Flash", 0.06),
            ("deepinfra/zai-org/GLM-4.7", 0.4),
            ("gmi/zai-org/GLM-4.7-FP8", 0.4),
        ]);
        assert_eq!(
            pricing.find_fuzzy("GLM-4.7").unwrap().0,
            "deepinfra/zai-org/GLM-4.7"
        );

        let regional = catalog(&[
            (
                "bedrock/us-gov-east-1/anthropic.claude-sonnet-4-5-20250929-v1:0",
                3.6,
            ),
            ("vercel_ai_gateway/anthropic/claude-sonnet-4.5", 3.0),
        ]);
        assert_eq!(
            regional
                .find_fuzzy("claude-sonnet-4.5")
                .unwrap()
                .1
                .input_per_million,
            3.0
        );

        // With no exact model-name match, the longest key still wins.
        let dated = catalog(&[("claude-sonnet-4-20250514", 3.0)]);
        assert_eq!(
            dated
                .find_fuzzy("claude-sonnet-4")
                .unwrap()
                .1
                .input_per_million,
            3.0
        );
    }

    #[test]
    fn longest_key_wins_deterministically() {
        let pricing = catalog(&[("gemini-3-pro", 1.0), ("gemini/gemini-3-pro-preview", 2.0)]);
        assert_eq!(
            pricing
                .find_fuzzy("gemini-3-pro-preview")
                .unwrap()
                .1
                .input_per_million,
            2.0
        );
    }
}
