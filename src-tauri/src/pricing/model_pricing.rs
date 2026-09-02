use std::{collections::HashMap, sync::Mutex};

use super::{ModelRates, PricingCatalog, PricingSupplement, TokenBreakdown};

pub struct ModelPricing {
    pub supplement: PricingSupplement,
    pub primary: PricingCatalog,
    pub secondary: PricingCatalog,
    /// Gap filler, consulted only after every other source misses. OpenRouter
    /// quotes its own routing prices rather than each vendor's list price, so
    /// it must never outrank the catalogs that do.
    pub tertiary: PricingCatalog,
    memo: Mutex<HashMap<String, Option<ModelRates>>>,
}

impl ModelPricing {
    pub fn new(
        supplement: PricingSupplement,
        primary: PricingCatalog,
        secondary: PricingCatalog,
    ) -> Self {
        Self {
            supplement,
            primary,
            secondary,
            tertiary: PricingCatalog::default(),
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// Adds the lowest-precedence catalog. Consumes and returns the pricing so
    /// the memo cache can never outlive the sources it was computed from.
    pub fn with_tertiary(self, tertiary: PricingCatalog) -> Self {
        Self {
            tertiary,
            memo: Mutex::new(HashMap::new()),
            ..self
        }
    }

    pub fn resolve(&self, model: &str) -> Option<ModelRates> {
        if let Some(cached) = self
            .memo
            .lock()
            .ok()
            .and_then(|memo| memo.get(model).copied())
        {
            return cached;
        }
        let resolved = self.resolve_uncached(model);
        if let Ok(mut memo) = self.memo.lock() {
            memo.insert(model.to_owned(), resolved);
        }
        resolved
    }

    pub fn estimated_cost_dollars(
        &self,
        model: &str,
        tokens: TokenBreakdown,
        apply_long_context_rates: bool,
    ) -> Option<f64> {
        Some(
            self.resolve(model)?
                .cost_dollars(tokens, apply_long_context_rates),
        )
    }

    /// [`Self::estimated_cost_dollars`] for usage logs in Anthropic's format,
    /// whose one-hour cache writes carry Anthropic's 2x-input premium unless
    /// the pricing data states an explicit rate.
    pub fn estimated_cost_dollars_anthropic(
        &self,
        model: &str,
        tokens: TokenBreakdown,
        apply_long_context_rates: bool,
    ) -> Option<f64> {
        Some(
            self.resolve(model)?
                .with_anthropic_one_hour_cache()
                .cost_dollars(tokens, apply_long_context_rates),
        )
    }

    /// Returns the stable display family used for provider exports that contain one slug per model
    /// variant. Alias rules are the same source of truth used for pricing; fast variants fold into
    /// their base family without guessing at otherwise unknown names.
    pub fn display_family(&self, model: &str) -> String {
        let canonical = self.supplement.canonical_name(model).unwrap_or(model);
        canonical
            .strip_suffix("-fast")
            .filter(|base| !base.is_empty())
            .unwrap_or(canonical)
            .to_owned()
    }

    fn resolve_uncached(&self, model: &str) -> Option<ModelRates> {
        if let Some(canonical) = self.supplement.canonical_name(model) {
            if canonical != model {
                return self.lookup(canonical).or_else(|| self.lookup(model));
            }
        }
        self.lookup(model)
    }

    /// Walks the source ladder twice: once accepting only sources that state a
    /// real rate, then once accepting anything. Feeds carry zero-rated entries
    /// for free tiers, stubs, and models published before their price is set,
    /// and a zero ranked above a priced source would silently report no spend.
    /// The second pass keeps a genuine free model resolving to zero rather than
    /// to "unknown".
    fn lookup(&self, name: &str) -> Option<ModelRates> {
        if signals_free_tier(name) {
            // A free routing tier is priced at zero on purpose, and every feed
            // publishes the paid twin right beside it - near enough for fuzzy
            // matching to bill free usage at the full rate. An exact hit is the
            // only trustworthy answer, so it wins outright here; the ordinary
            // ladder still runs when no catalog names the slug at all.
            return self
                .exact_in_precedence_order(name)
                .or_else(|| self.lookup_accepting(name, |_| true));
        }
        self.lookup_accepting(name, ModelRates::is_priced)
            .or_else(|| self.lookup_accepting(name, |_| true))
    }

    fn lookup_accepting(
        &self,
        name: &str,
        accept: impl Fn(&ModelRates) -> bool,
    ) -> Option<ModelRates> {
        let exact = |catalog: &PricingCatalog| {
            catalog
                .find_exact(name)
                .map(|(_, rates)| rates)
                .filter(&accept)
        };
        let fuzzy = |catalog: &PricingCatalog| {
            catalog
                .find_fuzzy(name)
                .map(|(_, rates)| rates)
                .filter(&accept)
        };
        let vendor_prefixed = |catalog: &PricingCatalog| {
            catalog
                .find_vendor_prefixed(name)
                .map(|(_, rates)| rates)
                .filter(&accept)
        };
        if let Some(rates) = self.supplement.pricing.get(name).copied().filter(&accept) {
            return Some(rates);
        }
        if let Some(rates) = exact(&self.primary) {
            return Some(rates);
        }
        if let Some(rates) = self.fast_variant(name).filter(&accept) {
            return Some(rates);
        }
        if name.ends_with("-fast") {
            // Fuzzy matching is deliberately skipped here: a "-fast" name has no
            // slow twin's rates, so a near miss would price it too cheaply.
            return exact(&self.secondary).or_else(|| vendor_prefixed(&self.tertiary));
        }
        fuzzy(&self.primary)
            .or_else(|| exact(&self.secondary))
            // Only the primary catalog is trusted with fuzzy matching. The gap
            // filler gets vendor-prefix stripping instead: enough to match the
            // bare names logs record against OpenRouter's "vendor/model" ids,
            // without letting the least authoritative source make the loosest
            // matches - full fuzzy had it pricing "seed-1.6" from
            // "bytedance-seed/seed-1.6-flash" and "aion-3.0" from
            // "aion-labs/aion-3.0-mini".
            .or_else(|| vendor_prefixed(&self.tertiary))
    }

    /// Exact hits only, walked in source precedence order.
    fn exact_in_precedence_order(&self, name: &str) -> Option<ModelRates> {
        if let Some(rates) = self.supplement.pricing.get(name) {
            return Some(*rates);
        }
        self.primary
            .find_exact(name)
            .or_else(|| self.secondary.find_exact(name))
            .or_else(|| self.tertiary.find_exact(name))
            .map(|(_, rates)| rates)
    }

    fn fast_variant(&self, name: &str) -> Option<ModelRates> {
        let base = name.strip_suffix("-fast")?;
        if base.is_empty() {
            return None;
        }
        let (key, rates) = self.base_entry(base)?;
        let multiplier = if rates.fast_multiplier != 1.0 {
            rates.fast_multiplier
        } else {
            self.supplement
                .fast_multiplier(key)
                .or_else(|| self.supplement.fast_multiplier(base))?
        };
        Some(rates.scaled(multiplier))
    }

    fn base_entry<'a>(&'a self, base: &'a str) -> Option<(&'a str, ModelRates)> {
        self.base_entry_accepting(base, ModelRates::is_priced)
            .or_else(|| self.base_entry_accepting(base, |_| true))
    }

    fn base_entry_accepting<'a>(
        &'a self,
        base: &'a str,
        accept: impl Fn(&ModelRates) -> bool,
    ) -> Option<(&'a str, ModelRates)> {
        if let Some(rates) = self
            .supplement
            .pricing
            .get(base)
            .filter(|rates| accept(rates))
        {
            return Some((base, *rates));
        }
        let exact =
            |catalog: &'a PricingCatalog| catalog.find_exact(base).filter(|(_, r)| accept(r));
        let fuzzy =
            |catalog: &'a PricingCatalog| catalog.find_fuzzy(base).filter(|(_, r)| accept(r));
        exact(&self.primary)
            .or_else(|| fuzzy(&self.primary))
            .or_else(|| exact(&self.secondary))
            .or_else(|| {
                self.tertiary
                    .find_vendor_prefixed(base)
                    .filter(|(_, rates)| accept(rates))
            })
    }
}

/// Whether a model id advertises a free routing tier, which feeds publish as a
/// `-free` or `:free` slug beside the paid model. Their zero rate is the right
/// answer, so [`ModelPricing::lookup`] must not go looking for a better one.
fn signals_free_tier(model: &str) -> bool {
    model
        .rsplit(['-', ':', '_', '/'])
        .next()
        .is_some_and(|last| last.eq_ignore_ascii_case("free"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::ModelPricing;
    use crate::pricing::{ModelRates, PricingCatalog, PricingSupplement, TokenBreakdown};

    fn rates(input: f64, output: f64) -> ModelRates {
        ModelRates::new(input, output)
    }

    fn pricing(
        supplement: Option<&str>,
        primary: &[(&str, ModelRates)],
        secondary: &[(&str, ModelRates)],
    ) -> ModelPricing {
        ModelPricing::new(
            supplement
                .map(|json| PricingSupplement::decode(json.as_bytes()).unwrap())
                .unwrap_or_default(),
            PricingCatalog {
                entries: primary
                    .iter()
                    .map(|(name, rates)| ((*name).to_owned(), *rates))
                    .collect::<HashMap<_, _>>(),
                retrieved_at: None,
            },
            PricingCatalog {
                entries: secondary
                    .iter()
                    .map(|(name, rates)| ((*name).to_owned(), *rates))
                    .collect::<HashMap<_, _>>(),
                retrieved_at: None,
            },
        )
    }

    fn catalog(entries: &[(&str, ModelRates)]) -> PricingCatalog {
        PricingCatalog {
            entries: entries
                .iter()
                .map(|(name, rates)| ((*name).to_owned(), *rates))
                .collect::<HashMap<_, _>>(),
            retrieved_at: None,
        }
    }

    #[test]
    fn tertiary_fills_gaps_without_outranking_any_other_source() {
        let supplement =
            r#"{"pricing":{"supplemented":{"input_per_million":1,"output_per_million":2}}}"#;
        let pricing = pricing(
            Some(supplement),
            &[
                ("supplemented", rates(10.0, 20.0)),
                ("shared", rates(3.0, 6.0)),
            ],
            &[("secondary-only", rates(5.0, 10.0))],
        )
        .with_tertiary(catalog(&[
            ("supplemented", rates(90.0, 90.0)),
            ("shared", rates(90.0, 90.0)),
            ("secondary-only", rates(90.0, 90.0)),
            ("vendor/tertiary-only", rates(7.0, 14.0)),
        ]));

        // Every source that already answers keeps its answer.
        assert_eq!(
            pricing.resolve("supplemented").unwrap().input_per_million,
            1.0
        );
        assert_eq!(pricing.resolve("shared").unwrap().input_per_million, 3.0);
        assert_eq!(
            pricing.resolve("secondary-only").unwrap().input_per_million,
            5.0
        );
        // Only a model nothing else prices falls through to the tertiary, and a
        // vendor-prefixed slug still matches the bare name the logs record.
        assert_eq!(
            pricing.resolve("tertiary-only").unwrap().input_per_million,
            7.0
        );
        assert!(pricing.resolve("priced-nowhere").is_none());
    }

    #[test]
    fn tertiary_never_prices_a_fast_variant_by_fuzzy_match() {
        let pricing =
            pricing(None, &[], &[]).with_tertiary(catalog(&[("vendor/model", rates(2.0, 4.0))]));

        // The slow twin resolves; its "-fast" name must not borrow those rates
        // without an explicit multiplier, or fast usage is billed as slow.
        assert_eq!(pricing.resolve("model").unwrap().input_per_million, 2.0);
        assert!(pricing.resolve("model-fast").is_none());
    }

    #[test]
    fn a_zero_rated_entry_never_shadows_a_source_that_prices_the_model() {
        // Feeds publish a model before its price is set, or carry it as a stub.
        // The higher-ranked zero must not win and report no spend.
        let pricing = pricing(
            None,
            &[("stub-model", rates(0.0, 0.0))],
            &[("stub-model", rates(4.0, 8.0))],
        );
        assert_eq!(
            pricing.resolve("stub-model").unwrap().input_per_million,
            4.0
        );
    }

    #[test]
    fn a_model_no_source_prices_still_resolves_to_zero() {
        // Falling through to "unknown" would drop the model off the spend
        // breakdown entirely; zero is the honest answer when zero is all we have.
        let pricing = pricing(None, &[("only-zero", rates(0.0, 0.0))], &[]);
        assert_eq!(pricing.resolve("only-zero").unwrap().input_per_million, 0.0);
    }

    #[test]
    fn a_free_tier_slug_keeps_its_zero_instead_of_its_paid_twins_rate() {
        let pricing = pricing(
            None,
            &[
                ("vendor/model", rates(3.0, 6.0)),
                ("vendor/model-free", rates(0.0, 0.0)),
                ("vendor/other:free", rates(0.0, 0.0)),
                ("vendor/other", rates(9.0, 9.0)),
            ],
            &[],
        );
        // "vendor/model" fuzzy-matches "vendor/model-free", so without the
        // free-tier guard the free slug bills at the paid rate.
        assert_eq!(
            pricing
                .resolve("vendor/model-free")
                .unwrap()
                .input_per_million,
            0.0
        );
        assert_eq!(
            pricing
                .resolve("vendor/other:free")
                .unwrap()
                .input_per_million,
            0.0
        );
        assert_eq!(
            pricing.resolve("vendor/model").unwrap().input_per_million,
            3.0
        );
    }

    #[test]
    fn the_tertiary_strips_vendor_prefixes_but_refuses_near_misses() {
        let pricing = pricing(None, &[], &[]).with_tertiary(catalog(&[
            ("bytedance-seed/seed-1.6-flash", rates(0.075, 0.15)),
            ("aion-labs/aion-3.0", rates(3.0, 6.0)),
        ]));
        // The bare name provider logs record resolves against the prefixed id.
        assert_eq!(pricing.resolve("aion-3.0").unwrap().input_per_million, 3.0);
        assert_eq!(
            pricing.resolve("seed-1.6-flash").unwrap().input_per_million,
            0.075
        );
        // The gap filler is the least authoritative source, so it does not get
        // to make the loosest match: a flash variant cannot price the plain
        // model the way full fuzzy matching would have allowed.
        assert!(pricing.resolve("seed-1.6").is_none());
    }

    #[test]
    fn supplement_alias_and_precedence_follow_contract() {
        let supplement = r#"{
          "pricing":{"auto":{"input_per_million":1.25,"output_per_million":6}},
          "alias_rules":[{"pattern":"^claude-4\\.5-sonnet(?:-thinking)?$","canonical":"claude-sonnet-4-5"}]
        }"#;
        let model_pricing = pricing(
            Some(supplement),
            &[
                ("auto", rates(99.0, 99.0)),
                ("claude-sonnet-4-5", rates(3.0, 15.0)),
            ],
            &[],
        );
        assert_eq!(
            model_pricing.resolve("auto").unwrap().input_per_million,
            1.25
        );
        assert_eq!(
            model_pricing
                .resolve("claude-4.5-sonnet-thinking")
                .unwrap()
                .input_per_million,
            3.0
        );
    }

    #[test]
    fn alias_miss_falls_back_to_raw_name() {
        let supplement =
            r#"{"pricing":{},"alias_rules":[{"pattern":"^gpt-x$","canonical":"missing"}]}"#;
        let model_pricing = pricing(Some(supplement), &[("gpt-x", rates(1.0, 2.0))], &[]);
        assert_eq!(
            model_pricing.resolve("gpt-x").unwrap().input_per_million,
            1.0
        );
    }

    #[test]
    fn fast_variant_requires_a_multiplier_or_secondary_exact_rate() {
        let no_multiplier = pricing(None, &[("gpt-9", rates(1.0, 2.0))], &[]);
        assert!(no_multiplier.resolve("gpt-9-fast").is_none());

        let supplement = r#"{"pricing":{},"fast_multipliers":{"gpt-5.5":2.5},"alias_rules":[]}"#;
        let with_multiplier = pricing(
            Some(supplement),
            &[("gpt-5.5-20260423", rates(5.0, 30.0))],
            &[],
        );
        assert_eq!(
            with_multiplier
                .resolve("gpt-5.5-fast")
                .unwrap()
                .input_per_million,
            12.5
        );

        let secondary = pricing(
            None,
            &[("gpt-9", rates(1.0, 2.0))],
            &[("gpt-9-fast", rates(2.5, 5.0))],
        );
        assert_eq!(
            secondary.resolve("gpt-9-fast").unwrap().input_per_million,
            2.5
        );
    }

    #[test]
    fn unknown_model_cost_is_none() {
        let model_pricing = pricing(None, &[], &[]);
        assert!(model_pricing
            .estimated_cost_dollars(
                "mystery",
                TokenBreakdown {
                    input: 100,
                    ..TokenBreakdown::default()
                },
                true,
            )
            .is_none());
    }

    #[test]
    fn secondary_catalog_is_exact_only() {
        let model_pricing = pricing(
            None,
            &[],
            &[("provider/secondary-model-20260715", rates(1.0, 2.0))],
        );
        assert!(model_pricing
            .resolve("provider/secondary-model-20260715")
            .is_some());
        assert!(model_pricing.resolve("secondary-model").is_none());
    }
}
