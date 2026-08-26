//! The Kimi Code host the provider talks to.
//!
//! Kimi Code serves the same coding API from two domains, and an account's key is only accepted
//! by the domain it was issued for. The user picks the domain in Customize; this module owns the
//! choice ids, their URLs, and the option the provider publishes to the catalog.

use crate::models::{ProviderOption, ProviderOptionChoice};

pub(super) const OPTION_ID: &str = "endpoint";

const KIMI_COM: &str = "kimi.com";
const KIMI_AI: &str = "kimi.ai";

const KIMI_COM_USAGES_URL: &str = "https://api.kimi.com/coding/v1/usages";
const KIMI_AI_USAGES_URL: &str = "https://api.kimi.ai/coding/v1/usages";

/// The endpoint option offered in Customize. `kimi.com` stays the default so existing
/// installations keep the host they have been using.
pub(super) fn option() -> ProviderOption {
    ProviderOption {
        id: OPTION_ID.into(),
        label: "Endpoint".into(),
        description: Some(
            "Choose the Kimi Code domain your API key belongs to. Keys are not shared between \
             domains."
                .into(),
        ),
        default_choice: KIMI_COM.into(),
        choices: vec![
            ProviderOptionChoice::new(KIMI_COM, "kimi.com", Some("api.kimi.com")),
            ProviderOptionChoice::new(KIMI_AI, "kimi.ai", Some("api.kimi.ai")),
        ],
    }
}

/// The usages URL for a resolved choice id. Unknown ids fall back to the default host, so a
/// stale stored value can never produce a request to somewhere the option never offered.
pub(super) fn usages_url(choice: &str) -> &'static str {
    match choice {
        KIMI_AI => KIMI_AI_USAGES_URL,
        _ => KIMI_COM_USAGES_URL,
    }
}

#[cfg(test)]
mod tests {
    use super::{option, usages_url, KIMI_AI, KIMI_COM};

    #[test]
    fn option_defaults_to_a_declared_choice() {
        let option = option();
        assert!(option.is_coherent());
        assert_eq!(option.default_choice, KIMI_COM);
    }

    #[test]
    fn every_declared_choice_maps_to_its_own_host() {
        assert_eq!(
            usages_url(KIMI_COM),
            "https://api.kimi.com/coding/v1/usages"
        );
        assert_eq!(usages_url(KIMI_AI), "https://api.kimi.ai/coding/v1/usages");
    }

    #[test]
    fn unknown_and_blank_choices_fall_back_to_the_default_host() {
        assert_eq!(usages_url("kimi.example"), usages_url(KIMI_COM));
        assert_eq!(usages_url(""), usages_url(KIMI_COM));
    }

    #[test]
    fn a_stored_value_outside_the_choices_resolves_to_the_default() {
        let option = option();
        assert_eq!(option.resolve(Some(KIMI_AI)), KIMI_AI);
        assert_eq!(option.resolve(Some("  kimi.ai  ")), KIMI_AI);
        assert_eq!(option.resolve(Some("kimi.example")), KIMI_COM);
        assert_eq!(option.resolve(None), KIMI_COM);
    }
}
