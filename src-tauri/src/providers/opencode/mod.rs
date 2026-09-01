mod accounts;
mod client;
mod database;
mod mapper;
mod paths;
mod record;
mod scanner;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::{
    models::{
        MetricDefinition, MetricSection, ProviderDefinition, ProviderErrorKind, ProviderLink,
        ProviderSnapshot, UsageHistory, UsagePeriodSelection,
    },
    pricing::PricingStore,
};

use self::{
    client::OpenCodeClient,
    mapper::map_go_usage,
    paths::OpenCodePaths,
    scanner::{OpenCodeUsageScanner, USAGE_SOURCE_NOTE},
};

use super::{ProviderError, UsageProvider};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "opencode".into(),
        display_name: "OpenCode".into(),
        short_name: "OC".into(),
        fallback_enabled: false,
        local_usage_source_note: Some(USAGE_SOURCE_NOTE.into()),
        links: vec![ProviderLink::new("Dashboard", "https://opencode.ai/auth")],
        options: Vec::new(),
        metrics: vec![
            MetricDefinition::quota(
                "opencode.session",
                "Session",
                "session",
                true,
                true,
                MetricSection::AlwaysVisible,
                false,
                "S",
            ),
            MetricDefinition::quota(
                "opencode.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "W",
            ),
            MetricDefinition::quota(
                "opencode.monthly",
                "Monthly",
                "monthly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "M",
            ),
            MetricDefinition::trend("opencode.trend"),
            MetricDefinition::usage(
                "opencode.today",
                "Today",
                UsagePeriodSelection::Today,
                MetricSection::OnDemand,
                "T",
            ),
            MetricDefinition::usage(
                "opencode.yesterday",
                "Yesterday",
                UsagePeriodSelection::Yesterday,
                MetricSection::OnDemand,
                "Y",
            ),
            MetricDefinition::usage(
                "opencode.last30",
                "Last 30 Days",
                UsagePeriodSelection::Last30Days,
                MetricSection::OnDemand,
                "30",
            ),
        ],
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenCodeError {
    #[error("OpenCode was not detected. Sign in to OpenCode Go or use OpenCode locally first.")]
    NotDetected,
    #[error("OpenCode login data could not be read. Sign in to OpenCode Go again.")]
    CredentialsUnreadable,
    #[error("The OpenCode data directory could not be read.")]
    DataDirectoryUnreadable,
    #[error("OpenCode local usage data is temporarily unavailable.")]
    DatabaseUnreadable,
    #[error("OpenCode Go login data is invalid or expired. Sign in to OpenCode Go again.")]
    InvalidAuth,
    #[error("OpenCode Go subscription required.")]
    GoSubscriptionRequired,
    #[error("Could not reach OpenCode Go. Check your internet connection.")]
    ConnectionFailed,
    #[error("OpenCode Go returned an invalid usage response.")]
    InvalidResponse,
    #[error("OpenCode Go usage request failed (HTTP {0}).")]
    RequestFailed(u16),
}

impl From<OpenCodeError> for ProviderError {
    fn from(error: OpenCodeError) -> Self {
        let kind = match error {
            OpenCodeError::NotDetected | OpenCodeError::InvalidAuth => {
                ProviderErrorKind::Authentication
            }
            OpenCodeError::GoSubscriptionRequired => ProviderErrorKind::Permission,
            OpenCodeError::CredentialsUnreadable => ProviderErrorKind::CredentialStorage,
            OpenCodeError::DataDirectoryUnreadable | OpenCodeError::DatabaseUnreadable => {
                ProviderErrorKind::LocalData
            }
            OpenCodeError::ConnectionFailed => ProviderErrorKind::Network,
            OpenCodeError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            OpenCodeError::RequestFailed(500..=599) => ProviderErrorKind::Network,
            OpenCodeError::InvalidResponse | OpenCodeError::RequestFailed(_) => {
                ProviderErrorKind::InvalidResponse
            }
        };
        ProviderError::new(kind, error.to_string())
    }
}

/// The account-adjusted definition: the account's provider id and display
/// name, with metric ids re-prefixed so every card owns its own metric ids.
/// Source ids stay in the family namespace — snapshots keep reporting
/// "session"/"weekly"/"monthly" regardless of the card.
pub(crate) fn definition_for(id: &str, display_name: &str) -> ProviderDefinition {
    let base_prefix = "opencode.";
    let account_prefix = format!("{id}.");
    let mut account = definition();
    account.id = id.to_owned();
    account.display_name = format!("OpenCode — {display_name}");
    for metric in &mut account.metrics {
        metric.id = match metric.id.strip_prefix(base_prefix) {
            Some(suffix) => format!("{account_prefix}{suffix}"),
            None => metric.id.clone(),
        };
    }
    account
}

pub struct OpenCodeProvider {
    definition: ProviderDefinition,
    paths: OpenCodePaths,
    scanner: OpenCodeUsageScanner,
    client: Result<OpenCodeClient, OpenCodeError>,
    pricing: Arc<PricingStore>,
    now: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

impl OpenCodeProvider {
    pub fn new(pricing: Arc<PricingStore>) -> Self {
        let paths = OpenCodePaths::new();
        Self {
            definition: definition(),
            scanner: OpenCodeUsageScanner::new(paths.clone()),
            paths,
            client: OpenCodeClient::new(),
            pricing,
            now: Arc::new(Utc::now),
        }
    }

    /// One runtime per discovered OpenCode login: the default data directory
    /// plus every sibling `opencode-<name>` directory holding a subscribed
    /// `opencode-go` login. Accounts keep their own usage databases and
    /// credentials inside their directories; nothing is copied or stored.
    pub fn runtimes(
        pricing: Arc<PricingStore>,
        storage: &crate::storage::Storage,
    ) -> Result<Vec<Arc<dyn UsageProvider>>, crate::storage::StorageError> {
        let mut runtimes: Vec<Arc<dyn UsageProvider>> = vec![Arc::new(Self::new(pricing.clone()))];
        for account in accounts::discover(storage)? {
            runtimes.push(Arc::new(Self::for_account(&account, pricing.clone())));
        }
        Ok(runtimes)
    }

    fn for_account(account: &accounts::OpenCodeAccount, pricing: Arc<PricingStore>) -> Self {
        let paths = OpenCodePaths::for_data_directory(account.data_directory.clone());
        Self {
            definition: definition_for(&account.id, &account.display_name),
            scanner: OpenCodeUsageScanner::new(paths.clone()),
            paths,
            client: OpenCodeClient::new(),
            pricing,
            now: Arc::new(Utc::now),
        }
    }

    #[cfg(test)]
    fn with_dependencies(
        paths: OpenCodePaths,
        client: OpenCodeClient,
        pricing: Arc<PricingStore>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            definition: definition(),
            scanner: OpenCodeUsageScanner::new(paths.clone()),
            paths,
            client: Ok(client),
            pricing,
            now: Arc::new(move || now),
        }
    }

    fn refresh_snapshot(&self) -> Result<ProviderSnapshot, OpenCodeError> {
        let now = (self.now)();
        let (go_api_key, go_key_error) = match self.paths.go_api_key() {
            Ok(key) => (key, None),
            Err(error) => (None, Some(error)),
        };
        let go_usage = go_api_key
            .as_deref()
            .map(|key| {
                self.client
                    .as_ref()
                    .map_err(|error| *error)
                    .and_then(|client| client.fetch_go_usage(key))
                    .and_then(map_go_usage)
            })
            .transpose();
        let pricing = self.pricing.current();
        let scan = self.scanner.scan(now, &pricing);

        let scan = match scan {
            Ok(scan) => scan,
            Err(error) => match go_usage {
                Ok(Some(quotas)) => {
                    return Ok(snapshot(
                        &self.definition.id,
                        Some("Go".into()),
                        quotas,
                        UsageHistory::default(),
                        vec!["OpenCode local usage data is temporarily unavailable.".into()],
                        now,
                    ));
                }
                _ => return Err(error),
            },
        };

        let Some(scan) = scan else {
            return match go_usage {
                Ok(Some(quotas)) => Ok(snapshot(
                    &self.definition.id,
                    Some("Go".into()),
                    quotas,
                    UsageHistory::default(),
                    Vec::new(),
                    now,
                )),
                Ok(None) => Err(go_key_error.unwrap_or(OpenCodeError::NotDetected)),
                Err(error) => Err(error),
            };
        };
        let mut warnings = scan.warnings;
        if go_key_error.is_some() {
            warnings.push(
                "OpenCode Go login data could not be read; local database usage is still shown."
                    .into(),
            );
        }
        let (plan, quotas) = match go_usage {
            Ok(Some(quotas)) => (Some("Go".into()), quotas),
            Ok(None) => (None, Vec::new()),
            Err(OpenCodeError::GoSubscriptionRequired) if scan.usage.last_30_days.is_some() => {
                warnings.push(
                    "OpenCode Go subscription required. Local usage is still shown while OpenCode Go quota data is unavailable."
                        .to_string(),
                );
                (None, Vec::new())
            }
            Err(error) => return Err(error),
        };
        Ok(snapshot(
            &self.definition.id,
            plan,
            quotas,
            scan.usage,
            warnings,
            now,
        ))
    }
}

impl UsageProvider for OpenCodeProvider {
    fn definition(&self) -> ProviderDefinition {
        self.definition.clone()
    }

    fn cache_identity(&self) -> super::CacheIdentity<'_> {
        if self.definition.id == "opencode" {
            super::CacheIdentity::Unscoped
        } else {
            super::CacheIdentity::Resolved(&self.definition.id)
        }
    }

    fn has_local_credentials(&self) -> bool {
        match self.paths.go_api_key() {
            Ok(Some(_)) | Err(OpenCodeError::CredentialsUnreadable) => true,
            Ok(None) | Err(_) => self.scanner.has_hosted_usage(),
        }
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        self.refresh_snapshot().map_err(ProviderError::from)
    }
}

fn snapshot(
    provider_id: &str,
    plan: Option<String>,
    quotas: Vec<crate::models::QuotaWindow>,
    usage: UsageHistory,
    warnings: Vec<String>,
    refreshed_at: DateTime<Utc>,
) -> ProviderSnapshot {
    ProviderSnapshot {
        provider_id: provider_id.into(),
        plan,
        quotas,
        value_metrics: Vec::new(),
        status_metrics: Vec::new(),
        notices: Vec::new(),
        usage,
        warnings,
        refreshed_at,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod account_tests {
    use std::sync::Arc;

    use super::{definition_for, OpenCodeProvider};
    use crate::providers::UsageProvider;

    #[test]
    fn account_definitions_reprefix_metrics_and_keep_sources() {
        let definition = definition_for("opencode@1a2b3c4d", "work");

        assert_eq!(definition.id, "opencode@1a2b3c4d");
        assert_eq!(definition.display_name, "OpenCode — work");
        assert!(definition
            .metrics
            .iter()
            .any(|metric| metric.id == "opencode@1a2b3c4d.session"));
        // Source ids stay in the family namespace; snapshots report them
        // regardless of the card.
        assert!(definition
            .metrics
            .iter()
            .any(|metric| metric.source.source_id() == Some("session")));
    }

    #[test]
    fn base_card_stays_unscoped_and_accounts_resolve_their_cache_identity() {
        let pricing = Arc::new(
            crate::pricing::PricingStore::new_without_refresh_for_test(
                std::env::temp_dir().join("usagedeck-opencode-account-test"),
            )
            .unwrap(),
        );
        let base = OpenCodeProvider::new(pricing.clone());
        assert!(matches!(
            base.cache_identity(),
            crate::providers::CacheIdentity::Unscoped
        ));
        assert_eq!(base.definition().id, "opencode");

        let account = super::accounts::OpenCodeAccount {
            id: "opencode@1a2b3c4d".into(),
            display_name: "work".into(),
            data_directory: std::env::temp_dir().join("usagedeck-opencode-account-missing"),
        };
        let provider = OpenCodeProvider::for_account(&account, pricing);
        assert!(matches!(
            provider.cache_identity(),
            crate::providers::CacheIdentity::Resolved(id) if id == "opencode@1a2b3c4d"
        ));
        assert_eq!(provider.definition().id, "opencode@1a2b3c4d");
    }
}
