use crate::models::ProviderDefinition;

use super::api_key::ApiKeyStore;

/// The API-key provider families that support named accounts, each account getting its own
/// dashboard card and its own credential-store entry.
pub const API_KEY_ACCOUNT_FAMILIES: &[&str] = &["openrouter", "zai", "kimi", "minimax"];

pub fn supports_api_key_accounts(family: &str) -> bool {
    API_KEY_ACCOUNT_FAMILIES.contains(&family)
}

pub fn is_api_key_account_provider_id(provider_id: &str) -> bool {
    let Some((family, suffix)) = provider_id.split_once('@') else {
        return false;
    };
    supports_api_key_accounts(family)
        && suffix.len() == 8
        && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Providers for every named account previously persisted in `provider_account_records`.
/// Account records whose payload has no saved name are skipped; duplicate provider ids are
/// deduplicated. Accounts whose constructor fails are logged and skipped.
///
/// The account id is bound to a family by constructing the runtime from that family; a mismatch
/// between the stored provider id family and the iteration family means the record belongs to a
/// different iteration pass and is simply skipped without error.
pub fn api_key_account_providers(
    storage: &crate::storage::Storage,
) -> Result<Vec<std::sync::Arc<dyn super::UsageProvider>>, crate::storage::StorageError> {
    let mut observed_account_ids = std::collections::HashSet::new();
    for provider_id in storage.load_observed_account_provider_ids()? {
        if is_api_key_account_provider_id(&provider_id) {
            observed_account_ids.insert(provider_id);
        }
    }
    if observed_account_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut providers: Vec<std::sync::Arc<dyn super::UsageProvider>> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for family in API_KEY_ACCOUNT_FAMILIES {
        let records = storage.load_provider_account_records(family)?;
        for (identity, provider_id, payload) in records {
            if !is_api_key_account_provider_id(&provider_id) {
                continue;
            }
            if crate::providers::provider_family(&provider_id) != *family {
                continue;
            }
            if !seen.insert(provider_id.clone()) {
                continue;
            }
            // Only introduce the card when it has a restorable name; artefacts without one are skipped.
            let Some(account_name) = account_name_from_payload(&payload) else {
                continue;
            };
            if !observed_account_ids.contains(&provider_id) {
                continue;
            }
            let _ = identity;
            match api_key_account_provider(family, &provider_id, &account_name) {
                Some(Ok(runtime)) => {
                    providers.push(runtime);
                }
                Some(Err(error)) => {
                    crate::app_warn!(
                        "lifecycle",
                        "API-key account {provider_id} could not be created: {error}"
                    );
                }
                None => {}
            }
        }
    }
    Ok(providers)
}

/// Builds the runtime for one named API-key account. `None` means the family
/// is unknown; `Err` carries the family constructor's failure.
pub fn api_key_account_provider(
    family: &str,
    provider_id: &str,
    account_name: &str,
) -> Option<Result<std::sync::Arc<dyn super::UsageProvider>, String>> {
    let runtime: Result<std::sync::Arc<dyn super::UsageProvider>, _> = match family {
        "openrouter" => crate::providers::openrouter::OpenRouterProvider::new_for_account(
            provider_id,
            account_name,
        )
        .map(|runtime| std::sync::Arc::new(runtime) as std::sync::Arc<dyn super::UsageProvider>),
        "zai" => crate::providers::zai::ZaiProvider::new_for_account(provider_id, account_name)
            .map(|runtime| {
                std::sync::Arc::new(runtime) as std::sync::Arc<dyn super::UsageProvider>
            }),
        "kimi" => crate::providers::kimi::KimiProvider::new_for_account(provider_id, account_name)
            .map(|runtime| {
                std::sync::Arc::new(runtime) as std::sync::Arc<dyn super::UsageProvider>
            }),
        "minimax" => {
            crate::providers::minimax::MiniMaxProvider::new_for_account(provider_id, account_name)
                .map(|runtime| {
                    std::sync::Arc::new(runtime) as std::sync::Arc<dyn super::UsageProvider>
                })
        }
        _ => {
            crate::app_warn!("lifecycle", "unknown API-key account family {family}");
            return None;
        }
    };
    Some(runtime.map_err(|error| error.to_string()))
}

/// Identity of one API-key provider instance: either the shared base provider (which keeps
/// its environment-variable and config-file fallbacks) or a named account under it whose key
/// lives only in the system credential store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyIdentity {
    pub provider_id: String,
    pub display_name: String,
    pub account: bool,
}

impl ApiKeyIdentity {
    pub fn base(provider_id: &str, display_name: &str) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            display_name: display_name.to_owned(),
            account: false,
        }
    }

    pub fn account(provider_id: &str, account_name: &str, base_display_name: &str) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            display_name: format!("{base_display_name} — {account_name}"),
            account: true,
        }
    }

    /// The credential store for this instance. Accounts deliberately drop the environment and
    /// config-file fallbacks: those are machine-wide defaults for the base provider, not
    /// per-account secrets, so an account with no saved key reports missing instead of
    /// silently sharing the base key.
    pub fn credential_store(
        &self,
        environment_names: &[&str],
        config_paths: &[&str],
    ) -> ApiKeyStore {
        if self.account {
            ApiKeyStore::new_with_sources(&self.provider_id, &[], &[])
        } else {
            ApiKeyStore::new_with_sources(&self.provider_id, environment_names, config_paths)
        }
    }

    /// The account-adjusted definition: same metrics and links, but the account's provider id
    /// (metric ids carry the provider prefix), the account's display name, and never the
    /// fallback enablement.
    pub fn definition(&self, base: ProviderDefinition) -> ProviderDefinition {
        if !self.account {
            return base;
        }
        let base_prefix = format!("{}.", base.id);
        let account_prefix = format!("{}.", self.provider_id);
        let mut definition = base;
        definition.id = self.provider_id.clone();
        definition.display_name = self.display_name.clone();
        definition.fallback_enabled = false;
        for metric in &mut definition.metrics {
            metric.id = match metric.id.strip_prefix(&base_prefix) {
                Some(suffix) => format!("{account_prefix}{suffix}"),
                None => metric.id.clone(),
            };
        }
        definition
    }
}

/// The account's saved display name from its `provider_account_records` payload, if any.
pub fn account_name_from_payload(payload: &str) -> Option<String> {
    let name = serde_json::from_str::<serde_json::Value>(payload)
        .ok()?
        .get("customName")?
        .as_str()?
        .trim()
        .to_owned();
    (name.len() <= 48 && !name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::{
        account_name_from_payload, is_api_key_account_provider_id, supports_api_key_accounts,
        ApiKeyIdentity,
    };
    use crate::models::{MetricDefinition, MetricSection, MetricSource, ProviderDefinition};

    fn base_definition() -> ProviderDefinition {
        ProviderDefinition {
            id: "openrouter".into(),
            display_name: "OpenRouter".into(),
            short_name: "OR".into(),
            fallback_enabled: false,
            local_usage_source_note: None,
            links: vec![],
            options: Vec::new(),
            metrics: vec![MetricDefinition::new(
                "openrouter.credits",
                "Credits",
                MetricSource::Quota {
                    source_id: "credits".into(),
                    session_window: false,
                },
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                Some("C"),
                None,
            )],
        }
    }

    #[test]
    fn account_families_are_recognized_by_provider_id() {
        assert!(supports_api_key_accounts("openrouter"));
        assert!(supports_api_key_accounts("kimi"));
        assert!(!supports_api_key_accounts("claude"));
        assert!(is_api_key_account_provider_id("zai@1a2b3c4d"));
        assert!(!is_api_key_account_provider_id("zai@short"));
        assert!(!is_api_key_account_provider_id("claude@1a2b3c4d"));
        assert!(!is_api_key_account_provider_id("openrouter"));
    }

    #[test]
    fn account_definitions_rewrite_ids_and_names_but_keep_metrics() {
        let identity = ApiKeyIdentity::account("openrouter@1a2b3c4d", "Work", "OpenRouter");
        let definition = identity.definition(base_definition());
        assert_eq!(definition.id, "openrouter@1a2b3c4d");
        assert_eq!(definition.display_name, "OpenRouter — Work");
        assert!(!definition.fallback_enabled);
        assert_eq!(definition.metrics[0].id, "openrouter@1a2b3c4d.credits");
        assert_eq!(
            definition.metrics[0].label,
            base_definition().metrics[0].label
        );
    }

    #[test]
    fn payload_names_are_trimmed_and_bounded() {
        assert_eq!(
            account_name_from_payload(r#"{"customName":" Work "}"#).as_deref(),
            Some("Work")
        );
        assert_eq!(account_name_from_payload("{}"), None);
        assert_eq!(account_name_from_payload("not json"), None);
    }
}
