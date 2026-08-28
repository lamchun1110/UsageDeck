#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRates {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: f64,
    pub cache_read_per_million: f64,
    /// One-hour cache-write rate. `None` falls back to the regular cache-write
    /// rate: the 2x-input premium is an Anthropic convention, not a universal
    /// one, so it is applied only where the usage source is known to be
    /// Anthropic-style (see [`ModelRates::with_anthropic_one_hour_cache`]) or
    /// the pricing data states it explicitly.
    pub cache_write_1h_per_million: Option<f64>,
    pub input_above_200k_per_million: Option<f64>,
    pub output_above_200k_per_million: Option<f64>,
    pub cache_write_above_200k_per_million: Option<f64>,
    pub cache_read_above_200k_per_million: Option<f64>,
    pub cache_read_is_explicit: bool,
    pub long_context_threshold_tokens: u64,
    pub fast_multiplier: f64,
}

impl ModelRates {
    pub fn new(input_per_million: f64, output_per_million: f64) -> Self {
        Self {
            input_per_million,
            output_per_million,
            cache_write_per_million: input_per_million,
            cache_read_per_million: input_per_million * 0.1,
            cache_write_1h_per_million: None,
            input_above_200k_per_million: None,
            output_above_200k_per_million: None,
            cache_write_above_200k_per_million: None,
            cache_read_above_200k_per_million: None,
            cache_read_is_explicit: true,
            long_context_threshold_tokens: 200_000,
            fast_multiplier: 1.0,
        }
    }

    pub fn scaled(self, factor: f64) -> Self {
        Self {
            input_per_million: self.input_per_million * factor,
            output_per_million: self.output_per_million * factor,
            cache_write_per_million: self.cache_write_per_million * factor,
            cache_read_per_million: self.cache_read_per_million * factor,
            cache_write_1h_per_million: self.cache_write_1h_per_million.map(|rate| rate * factor),
            input_above_200k_per_million: self
                .input_above_200k_per_million
                .map(|rate| rate * factor),
            output_above_200k_per_million: self
                .output_above_200k_per_million
                .map(|rate| rate * factor),
            cache_write_above_200k_per_million: self
                .cache_write_above_200k_per_million
                .map(|rate| rate * factor),
            cache_read_above_200k_per_million: self
                .cache_read_above_200k_per_million
                .map(|rate| rate * factor),
            cache_read_is_explicit: self.cache_read_is_explicit,
            long_context_threshold_tokens: self.long_context_threshold_tokens,
            fast_multiplier: 1.0,
        }
    }

    /// Marks these rates as Anthropic-style, where one-hour cache writes cost
    /// twice the base input rate. Applied by the producers of Anthropic-style
    /// usage logs after resolving a model; explicit pricing data (supplement
    /// or catalog) always wins over this default.
    pub fn with_anthropic_one_hour_cache(mut self) -> Self {
        self.cache_write_1h_per_million = Some(
            self.cache_write_1h_per_million
                .unwrap_or(self.input_per_million * 2.0),
        );
        self
    }

    pub fn cost_dollars(self, tokens: TokenBreakdown, apply_long_context_rates: bool) -> f64 {
        let use_long_context =
            apply_long_context_rates && tokens.prompt_tokens() > self.long_context_threshold_tokens;
        let select = |base: f64, long_context: Option<f64>| {
            if use_long_context {
                long_context.unwrap_or(base)
            } else {
                base
            }
        };
        let input_rate = select(self.input_per_million, self.input_above_200k_per_million);
        let output_rate = select(self.output_per_million, self.output_above_200k_per_million);
        let cache_write_rate = select(
            self.cache_write_per_million,
            self.cache_write_above_200k_per_million,
        );
        let cache_read_rate = select(
            self.cache_read_per_million,
            self.cache_read_above_200k_per_million,
        );
        let cache_write_1h_rate = self.cache_write_1h_per_million.unwrap_or(cache_write_rate);
        let cost = tokens.input as f64 * input_rate
            + tokens.output as f64 * output_rate
            + tokens.cache_write_5m as f64 * cache_write_rate
            + tokens.cache_write_1h as f64 * cache_write_1h_rate
            + tokens.cache_read as f64 * cache_read_rate;
        cost / 1_000_000.0
            * if tokens.is_fast {
                self.fast_multiplier
            } else {
                1.0
            }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenBreakdown {
    pub input: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub output: u64,
    pub is_fast: bool,
}

impl TokenBreakdown {
    pub fn prompt_tokens(self) -> u64 {
        self.input
            .saturating_add(self.cache_write_5m)
            .saturating_add(self.cache_write_1h)
            .saturating_add(self.cache_read)
    }

    pub fn total_tokens(self) -> u64 {
        self.prompt_tokens().saturating_add(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelRates, TokenBreakdown};

    #[test]
    fn one_hour_cache_writes_default_to_the_regular_cache_write_rate() {
        let mut rates = ModelRates::new(3.0, 15.0);
        rates.cache_write_per_million = 3.75;
        rates.cache_read_per_million = 0.3;
        let tokens = TokenBreakdown {
            input: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
            cache_read: 1_000_000,
            output: 1_000_000,
            is_fast: false,
        };
        // Without an explicit 1h rate (or the Anthropic convention), the 5m
        // cache-write rate prices the 1h bucket — never an invented 2x input.
        assert!((rates.cost_dollars(tokens, true) - 25.8).abs() < 0.000_001);
    }

    #[test]
    fn anthropic_style_rates_price_one_hour_writes_at_twice_input() {
        let mut rates = ModelRates::new(3.0, 15.0);
        rates.cache_write_per_million = 3.75;
        rates.cache_read_per_million = 0.3;
        let rates = rates.with_anthropic_one_hour_cache();
        assert_eq!(rates.cache_write_1h_per_million, Some(6.0));
        let tokens = TokenBreakdown {
            input: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
            cache_read: 1_000_000,
            output: 1_000_000,
            is_fast: false,
        };
        assert!((rates.cost_dollars(tokens, true) - 28.05).abs() < 0.000_001);
    }

    #[test]
    fn an_explicit_one_hour_rate_beats_the_anthropic_default() {
        let rates = ModelRates::new(3.0, 15.0).with_anthropic_one_hour_cache();
        let rates = ModelRates {
            cache_write_1h_per_million: Some(9.0),
            ..rates
        };
        assert_eq!(rates.cache_write_1h_per_million, Some(9.0));
    }

    #[test]
    fn combined_prompt_selects_request_wide_long_context_rates() {
        let mut rates = ModelRates::new(3.0, 15.0);
        rates.cache_write_per_million = 3.75;
        rates.cache_read_per_million = 0.3;
        rates.input_above_200k_per_million = Some(6.0);
        rates.output_above_200k_per_million = Some(22.5);
        rates.cache_write_above_200k_per_million = Some(7.5);
        rates.cache_read_above_200k_per_million = Some(0.6);
        let tokens = TokenBreakdown {
            input: 100_000,
            cache_write_5m: 60_000,
            cache_read: 50_000,
            output: 20_000,
            ..TokenBreakdown::default()
        };
        assert!((rates.cost_dollars(tokens, true) - 1.53).abs() < 0.000_001);
        assert!(rates.cost_dollars(tokens, false) < 1.53);
    }

    #[test]
    fn exactly_200k_keeps_base_rates_and_fast_scales_cost() {
        let mut rates = ModelRates::new(3.0, 15.0);
        rates.input_above_200k_per_million = Some(6.0);
        rates.fast_multiplier = 2.5;
        let tokens = TokenBreakdown {
            input: 200_000,
            output: 10_000,
            is_fast: true,
            ..TokenBreakdown::default()
        };
        assert!((rates.cost_dollars(tokens, true) - 1.875).abs() < 0.000_001);
    }

    #[test]
    fn large_output_alone_does_not_select_long_context_rates() {
        let mut rates = ModelRates::new(3.0, 15.0);
        rates.input_above_200k_per_million = Some(6.0);
        rates.output_above_200k_per_million = Some(22.5);
        let tokens = TokenBreakdown {
            input: 10_000,
            output: 300_000,
            ..TokenBreakdown::default()
        };
        assert!((rates.cost_dollars(tokens, true) - 4.53).abs() < 0.000_001);
    }
}
