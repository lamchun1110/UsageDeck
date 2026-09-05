mod accounts;
pub mod auth;
mod client;
mod local_usage;
mod mapper;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use thiserror::Error;

use crate::{
    models::{
        MetricDefinition, MetricSection, ProviderDefinition, ProviderLink, ProviderNotice,
        ProviderNoticeTone, ProviderSnapshot, UsagePeriodSelection,
    },
    pricing::{ModelPricing, PricingStore},
    storage::Storage,
};

pub(crate) fn definition() -> ProviderDefinition {
    definition_for("claude", "Claude", true)
}

fn definition_for(id: &str, display_name: &str, fallback_enabled: bool) -> ProviderDefinition {
    let mut definition = ProviderDefinition {
        id: "claude".into(),
        display_name: display_name.into(),
        short_name: "Cl".into(),
        fallback_enabled,
        local_usage_source_note: Some("From your Claude usage history (estimated)".into()),
        links: vec![
            ProviderLink::new("Status", "https://status.anthropic.com/"),
            ProviderLink::new("Dashboard", "https://claude.ai/settings/usage"),
        ],
        options: Vec::new(),
        metrics: vec![
            MetricDefinition::quota(
                "claude.session",
                "Session",
                "session",
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                "S",
            ),
            MetricDefinition::quota(
                "claude.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "W",
            ),
            MetricDefinition::quota(
                "claude.sonnet",
                "Sonnet",
                "sonnet",
                false,
                false,
                MetricSection::OnDemand,
                false,
                "Sn",
            ),
            MetricDefinition::quota(
                "claude.fable",
                "Fable",
                "fable",
                false,
                false,
                MetricSection::OnDemand,
                false,
                "F",
            ),
            MetricDefinition::quota_or_value(
                "claude.extra",
                "Extra Usage",
                "extra",
                true,
                MetricSection::AlwaysVisible,
                false,
                "E",
            ),
            MetricDefinition::trend("claude.trend"),
            MetricDefinition::usage(
                "claude.today",
                "Today",
                UsagePeriodSelection::Today,
                MetricSection::OnDemand,
                "T",
            ),
            MetricDefinition::usage(
                "claude.yesterday",
                "Yesterday",
                UsagePeriodSelection::Yesterday,
                MetricSection::OnDemand,
                "Y",
            ),
            MetricDefinition::usage(
                "claude.last30",
                "Last 30 Days",
                UsagePeriodSelection::Last30Days,
                MetricSection::OnDemand,
                "M",
            ),
        ],
    };
    if id != "claude" {
        definition.id = id.into();
        for metric in &mut definition.metrics {
            if let Some(suffix) = metric.id.strip_prefix("claude.") {
                metric.id = format!("{id}.{suffix}");
            }
            metric.default_pinned = false;
        }
    }
    definition
}

use self::{
    auth::{
        load_candidates, oauth_config, ClaudeCredential, ClaudeCredentialGeneration,
        ClaudeCredentialScope,
    },
    client::ClaudeClient,
    local_usage::scan_local_usage,
    mapper::map_usage,
};
use crate::providers::log_usage::scan_or_cached_usage;

#[derive(Debug, Error)]
pub enum ClaudeError {
    #[error("Not logged in. Run `claude` to authenticate.")]
    NotLoggedIn,
    #[error(
        "Claude Desktop login found, but its macOS-only encrypted session cannot be reused safely. Run `claude` in a terminal and sign in once."
    )]
    DesktopAppOnly,
    #[error("Your Claude session expired. Run `claude` to sign in again.")]
    SessionExpired,
    #[error("Your Claude token expired. Run `claude` to sign in again.")]
    TokenExpired,
    #[error("Claude OAuth settings contain an invalid URL.")]
    InvalidOAuthUrl,
    #[error("Refreshed Claude credentials could not be saved.")]
    AuthWrite,
    #[error("Claude login changed during refresh. Refresh again.")]
    CredentialsChanged,
    #[error("The Claude account changed while UsageDeck was running. Restart UsageDeck to reconnect it safely.")]
    AccountChanged,
    #[error("Claude usage request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("Claude returned an invalid usage response.")]
    InvalidResponse,
    #[error("Could not connect to Claude. Check your internet connection.")]
    ConnectionFailed,
    #[error("Local Claude usage logs could not be processed.")]
    LocalUsage,
    #[error("Claude account settings could not be loaded.")]
    AccountStore(#[from] crate::storage::StorageError),
}

pub struct ClaudeProvider {
    definition: ProviderDefinition,
    credential_scope: ClaudeCredentialScope,
    account_identity: Option<String>,
    log_roots: Vec<PathBuf>,
    include_standard_logs: bool,
    include_pi: bool,
    storage: Arc<Storage>,
    pricing: Arc<PricingStore>,
    client: ClaudeClient,
    cached_credential_fingerprint: Mutex<Option<[u8; 32]>>,
    last_good: Mutex<Option<ProviderSnapshot>>,
    rate_limited_until: Mutex<Option<chrono::DateTime<Utc>>>,
}

struct ClaudeRuntimeConfig {
    definition: ProviderDefinition,
    credential_scope: ClaudeCredentialScope,
    account_identity: Option<String>,
    log_roots: Vec<PathBuf>,
    include_standard_logs: bool,
    include_pi: bool,
}

pub(crate) fn runtimes(
    storage: Arc<Storage>,
    pricing: Arc<PricingStore>,
) -> Result<Vec<Arc<dyn crate::providers::UsageProvider>>, ClaudeError> {
    let discovery = accounts::discover(&storage)?;
    let client = ClaudeClient::new()?;
    Ok(runtime_configs(discovery)
        .into_iter()
        .map(|config| {
            Arc::new(ClaudeProvider::new_scoped(
                config,
                storage.clone(),
                pricing.clone(),
                client.clone(),
            )) as Arc<dyn crate::providers::UsageProvider>
        })
        .collect())
}

fn runtime_configs(discovery: accounts::ClaudeAccountDiscovery) -> Vec<ClaudeRuntimeConfig> {
    let mut configs = Vec::new();
    // A bare-ID account that moved to a config dir is still the `claude` card, so it replaces the
    // empty default-login placeholder instead of colliding with a second runtime of the same ID.
    let has_bare_scoped_account = discovery
        .accounts
        .iter()
        .any(|account| account.id == "claude");
    if let Some(account) = discovery.default_account {
        configs.push(ClaudeRuntimeConfig {
            definition: definition_for(&account.id, &account.display_name, true),
            credential_scope: ClaudeCredentialScope::Standard,
            account_identity: Some(account.identity),
            log_roots: account.log_roots,
            include_standard_logs: true,
            include_pi: true,
        });
    } else if !has_bare_scoped_account {
        configs.push(ClaudeRuntimeConfig {
            definition: definition(),
            credential_scope: ClaudeCredentialScope::Standard,
            account_identity: None,
            log_roots: Vec::new(),
            include_standard_logs: true,
            include_pi: true,
        });
    }
    for account in discovery.accounts {
        configs.push(ClaudeRuntimeConfig {
            definition: definition_for(&account.id, &account.display_name, false),
            credential_scope: account.credential_scope,
            account_identity: Some(account.identity),
            log_roots: account.log_roots,
            include_standard_logs: false,
            include_pi: false,
        });
    }
    configs
}

impl ClaudeProvider {
    #[cfg(test)]
    pub fn new(storage: Arc<Storage>, pricing: Arc<PricingStore>) -> Result<Self, ClaudeError> {
        Ok(Self::new_scoped(
            ClaudeRuntimeConfig {
                definition: definition(),
                credential_scope: ClaudeCredentialScope::Standard,
                account_identity: None,
                log_roots: Vec::new(),
                include_standard_logs: true,
                include_pi: true,
            },
            storage,
            pricing,
            ClaudeClient::new()?,
        ))
    }

    fn new_scoped(
        config: ClaudeRuntimeConfig,
        storage: Arc<Storage>,
        pricing: Arc<PricingStore>,
        client: ClaudeClient,
    ) -> Self {
        Self {
            definition: config.definition,
            credential_scope: config.credential_scope,
            account_identity: config.account_identity,
            log_roots: config.log_roots,
            include_standard_logs: config.include_standard_logs,
            include_pi: config.include_pi,
            storage,
            pricing,
            client,
            cached_credential_fingerprint: Mutex::new(None),
            last_good: Mutex::new(None),
            rate_limited_until: Mutex::new(None),
        }
    }

    fn provider_id(&self) -> &str {
        &self.definition.id
    }

    fn refresh_inner(&self) -> Result<ProviderSnapshot, ClaudeError> {
        let config = oauth_config()?;
        self.refresh_inner_with_config(&config)
    }

    fn refresh_inner_with_config(
        &self,
        config: &auth::ClaudeOAuthConfig,
    ) -> Result<ProviderSnapshot, ClaudeError> {
        let mut credential_reloads_remaining = 1;
        loop {
            match self.refresh_inner_once(config) {
                Err(ClaudeError::CredentialsChanged) if credential_reloads_remaining > 0 => {
                    credential_reloads_remaining -= 1;
                    crate::app_info!(
                        "auth:claude",
                        "credential source changed during refresh; reloading current login"
                    );
                }
                Ok(snapshot) => {
                    self.ensure_account_identity_current()?;
                    return Ok(snapshot);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn refresh_inner_once(
        &self,
        config: &auth::ClaudeOAuthConfig,
    ) -> Result<ProviderSnapshot, ClaudeError> {
        self.ensure_account_identity_current()?;
        let candidates = load_candidates(&self.credential_scope);
        if candidates.is_empty() {
            crate::app_info!("auth:claude", "no reusable CLI credentials found");
            return Err(
                if matches!(self.credential_scope, ClaudeCredentialScope::Standard)
                    && auth::has_desktop_app_data()
                {
                    ClaudeError::DesktopAppOnly
                } else {
                    ClaudeError::NotLoggedIn
                },
            );
        }
        crate::app_debug!(
            "auth:claude",
            "credential candidates loaded ({})",
            candidates.len()
        );
        let now = Utc::now();
        let pricing = self.pricing.current();
        let mut credential_generation = ClaudeCredentialGeneration::from_candidates(&candidates);
        let mut last_auth_error = None;
        for mut credential in candidates {
            match self.refresh_candidate(
                &mut credential,
                config,
                now,
                &pricing,
                &mut credential_generation,
            ) {
                Ok(snapshot) => return Ok(snapshot),
                Err(error @ (ClaudeError::SessionExpired | ClaudeError::TokenExpired)) => {
                    last_auth_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_auth_error.unwrap_or(ClaudeError::NotLoggedIn))
    }

    fn ensure_account_identity_current(&self) -> Result<(), ClaudeError> {
        let Some(expected) = self.account_identity.as_deref() else {
            return Ok(());
        };
        (accounts::identity_for_scope(&self.credential_scope).as_deref() == Some(expected))
            .then_some(())
            .ok_or(ClaudeError::AccountChanged)
    }

    /// Snapshot returned when a Claude Code-owned login has gone stale.
    ///
    /// Rotating it ourselves would rewrite the Keychain item's access control
    /// list and lock the CLI out of its own credential, so the last good
    /// figures are shown with a notice until Claude Code refreshes the login on
    /// its next run. The new token is picked up on the following poll.
    fn deferred_refresh_snapshot(
        &self,
        credential: &ClaudeCredential,
        usage: crate::models::UsageHistory,
        mut warnings: Vec<String>,
        now: chrono::DateTime<Utc>,
    ) -> ProviderSnapshot {
        warnings.push(
            "Claude's saved login needs refreshing; showing the last successful limits until Claude Code signs in again."
                .into(),
        );
        if let Some(mut snapshot) = self.last_good.lock().ok().and_then(|value| value.clone()) {
            snapshot.usage = usage;
            snapshot.warnings = warnings;
            snapshot.notices = vec![deferred_refresh_notice()];
            snapshot.refreshed_at = now;
            return snapshot;
        }
        ProviderSnapshot {
            provider_id: self.provider_id().into(),
            plan: plan_name(credential),
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: vec![deferred_refresh_notice()],
            usage,
            warnings,
            refreshed_at: now,
        }
    }

    fn refresh_candidate(
        &self,
        credential: &mut ClaudeCredential,
        config: &auth::ClaudeOAuthConfig,
        now: chrono::DateTime<Utc>,
        pricing: &ModelPricing,
        credential_generation: &mut ClaudeCredentialGeneration,
    ) -> Result<ProviderSnapshot, ClaudeError> {
        let mut warnings = Vec::new();
        let usage = scan_or_cached_usage(
            &self.storage,
            self.provider_id(),
            crate::providers::UsageProvider::cache_identity(self),
            "Claude",
            || {
                scan_local_usage(
                    &self.storage,
                    now,
                    pricing,
                    self.provider_id(),
                    &self.log_roots,
                    self.include_standard_logs,
                    self.include_pi,
                )
            },
            &mut warnings,
        );

        if credential.inference_only {
            return Ok(ProviderSnapshot {
                provider_id: self.provider_id().into(),
                plan: plan_name(credential),
                quotas: Vec::new(),
                value_metrics: Vec::new(),
                status_metrics: Vec::new(),
                notices: Vec::new(),
                usage,
                warnings,
                refreshed_at: now,
            });
        }
        if !credential.has_profile_scope() {
            warnings.push(
                "Re-login for live usage. Run `claude` and sign in again to restore subscription limits."
                    .into(),
            );
            return Ok(ProviderSnapshot {
                provider_id: self.provider_id().into(),
                plan: plan_name(credential),
                quotas: Vec::new(),
                value_metrics: Vec::new(),
                status_metrics: Vec::new(),
                notices: Vec::new(),
                usage,
                warnings,
                refreshed_at: now,
            });
        }
        self.activate_live_usage_cache(credential.fingerprint());
        if credential.needs_refresh(now.timestamp_millis()) {
            if credential.is_foreign_keychain_item() {
                return Ok(self.deferred_refresh_snapshot(credential, usage, warnings, now));
            }
            let previous_fingerprint = credential.fingerprint();
            refresh_credential(
                &self.client,
                credential,
                config,
                now,
                &mut warnings,
                credential_generation,
                &self.credential_scope,
            )?;
            self.replace_live_usage_fingerprint(previous_fingerprint, credential.fingerprint());
        }

        let cooldown_until = self
            .rate_limited_until
            .lock()
            .ok()
            .and_then(|value| *value)
            .filter(|until| now < *until);
        if let Some(until) = cooldown_until {
            let retry = until.signed_duration_since(now).num_seconds().max(0) as u64;
            if let Some(mut snapshot) = self.last_good.lock().ok().and_then(|value| value.clone()) {
                snapshot.usage = usage;
                snapshot.warnings.push(
                    "Claude live usage is rate limited; showing the last successful limits.".into(),
                );
                snapshot.notices = vec![rate_limit_notice(retry, true)];
                snapshot.refreshed_at = now;
                return Ok(snapshot);
            }
            warnings.push(format!(
                "Claude live usage is rate limited; retrying in about {}.",
                retry_minutes(retry)
            ));
            return Ok(ProviderSnapshot {
                provider_id: self.provider_id().into(),
                plan: plan_name(credential),
                quotas: Vec::new(),
                value_metrics: Vec::new(),
                status_metrics: Vec::new(),
                notices: vec![rate_limit_notice(retry, false)],
                usage,
                warnings,
                refreshed_at: now,
            });
        }

        let token = credential.access_token().ok_or(ClaudeError::NotLoggedIn)?;
        let (mut status, mut body, mut retry_after) = self.client.fetch_usage(token, config)?;
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            if credential.is_foreign_keychain_item() {
                return Ok(self.deferred_refresh_snapshot(credential, usage, warnings, now));
            }
            let previous_fingerprint = credential.fingerprint();
            refresh_credential(
                &self.client,
                credential,
                config,
                now,
                &mut warnings,
                credential_generation,
                &self.credential_scope,
            )?;
            self.replace_live_usage_fingerprint(previous_fingerprint, credential.fingerprint());
            let token = credential.access_token().ok_or(ClaudeError::TokenExpired)?;
            (status, body, retry_after) = self.client.fetch_usage(token, config)?;
        }
        if auth::credential_generation(&self.credential_scope) != *credential_generation {
            return Err(ClaudeError::CredentialsChanged);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry = retry_after.unwrap_or(5 * 60);
            if let Ok(mut until) = self.rate_limited_until.lock() {
                *until = Some(now + Duration::seconds(retry as i64));
            }
            if let Some(mut snapshot) = self.last_good.lock().ok().and_then(|value| value.clone()) {
                snapshot.usage = usage;
                snapshot.warnings.push(format!(
                    "Claude live usage is rate limited; retrying in about {}.",
                    retry_minutes(retry)
                ));
                snapshot.notices = vec![rate_limit_notice(retry, true)];
                snapshot.refreshed_at = now;
                return Ok(snapshot);
            }
            warnings.push(format!(
                "Claude live usage is rate limited; retrying in about {}.",
                retry_minutes(retry)
            ));
            return Ok(ProviderSnapshot {
                provider_id: self.provider_id().into(),
                plan: plan_name(credential),
                quotas: Vec::new(),
                value_metrics: Vec::new(),
                status_metrics: Vec::new(),
                notices: vec![rate_limit_notice(retry, false)],
                usage,
                warnings,
                refreshed_at: now,
            });
        }
        self.build_snapshot(status, &body, credential, usage, warnings, now)
    }

    fn build_snapshot(
        &self,
        status: StatusCode,
        body: &serde_json::Value,
        credential: &ClaudeCredential,
        usage: crate::models::UsageHistory,
        warnings: Vec<String>,
        now: chrono::DateTime<Utc>,
    ) -> Result<ProviderSnapshot, ClaudeError> {
        let mapped = map_usage(status, body, &credential.oauth)?;
        let snapshot = ProviderSnapshot {
            provider_id: self.provider_id().into(),
            plan: mapped.plan,
            quotas: mapped.quotas,
            value_metrics: mapped.value_metrics,
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage,
            warnings,
            refreshed_at: now,
        };
        if let Ok(mut last) = self.last_good.lock() {
            *last = Some(snapshot.clone());
        }
        if let Ok(mut until) = self.rate_limited_until.lock() {
            *until = None;
        }
        Ok(snapshot)
    }

    fn activate_live_usage_cache(&self, fingerprint: [u8; 32]) {
        let changed = self
            .cached_credential_fingerprint
            .lock()
            .map(|mut active| {
                if active.as_ref() == Some(&fingerprint) {
                    false
                } else {
                    *active = Some(fingerprint);
                    true
                }
            })
            .unwrap_or(true);
        if !changed {
            return;
        }
        if let Ok(mut last) = self.last_good.lock() {
            *last = None;
        }
        if let Ok(mut until) = self.rate_limited_until.lock() {
            *until = None;
        }
    }

    fn replace_live_usage_fingerprint(&self, previous: [u8; 32], current: [u8; 32]) {
        if let Ok(mut active) = self.cached_credential_fingerprint.lock() {
            if active.as_ref() == Some(&previous) {
                *active = Some(current);
            }
        }
    }
}

/// Shown while a Claude Code-owned login is stale and UsageDeck is waiting for
/// the CLI to rotate it rather than rotating it itself.
fn deferred_refresh_notice() -> ProviderNotice {
    ProviderNotice {
        id: "loginRefreshDeferred".into(),
        title: "Waiting for Claude Code".into(),
        message: "Showing the last successful limits · Claude Code refreshes this login when you next use it".into(),
        tone: ProviderNoticeTone::Warning,
    }
}

fn rate_limit_notice(retry_seconds: u64, showing_stale_limits: bool) -> ProviderNotice {
    let retry = if retry_seconds == 0 {
        "Ready to retry".to_owned()
    } else {
        format!("Retrying in about {}", retry_minutes(retry_seconds))
    };
    ProviderNotice {
        id: "rateLimited".into(),
        title: "Live usage paused".into(),
        message: if showing_stale_limits {
            format!("Showing the last successful limits · {retry}")
        } else {
            retry
        },
        tone: ProviderNoticeTone::Warning,
    }
}

fn retry_minutes(retry_seconds: u64) -> String {
    let minutes = retry_seconds.div_ceil(60);
    format!(
        "{minutes} {}",
        if minutes == 1 { "minute" } else { "minutes" }
    )
}

fn refresh_credential(
    client: &ClaudeClient,
    credential: &mut ClaudeCredential,
    config: &auth::ClaudeOAuthConfig,
    now: chrono::DateTime<Utc>,
    warnings: &mut Vec<String>,
    credential_generation: &mut ClaudeCredentialGeneration,
    credential_scope: &ClaudeCredentialScope,
) -> Result<(), ClaudeError> {
    let refresh_token = credential
        .oauth
        .refresh_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(ClaudeError::TokenExpired)?;
    let refreshed = client.refresh_token(refresh_token, config)?;
    match credential.update_and_save(
        refreshed.access_token,
        refreshed.refresh_token,
        refreshed.expires_in,
        now.timestamp_millis(),
        credential_generation,
        credential_scope,
    ) {
        Ok(true) => {
            *credential_generation = credential_generation
                .replacing(credential)
                .ok_or(ClaudeError::CredentialsChanged)?;
        }
        Ok(false) => {}
        Err(ClaudeError::CredentialsChanged) => return Err(ClaudeError::CredentialsChanged),
        Err(_) => {
            crate::app_error!(
                "auth:claude",
                "failed to persist rotated credentials; using them for this session only"
            );
            warnings.push(
                "The refreshed Claude login is active for this session but could not be saved."
                    .into(),
            );
        }
    }
    Ok(())
}

fn plan_name(credential: &ClaudeCredential) -> Option<String> {
    credential.oauth.subscription_type.as_ref().map(|value| {
        let mut chars = value.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default()
    })
}

impl crate::providers::UsageProvider for ClaudeProvider {
    fn session_kickstart(&self) -> Option<crate::providers::SessionKickstart> {
        // Print mode sends exactly one prompt and exits, starting the 5-hour
        // session window without an interactive session. Sonnet is the
        // cheapest model that registers a session start — haiku runs on the
        // background track, which does not (user-verified).
        //
        // Scoped account runtimes point the CLI at their own config dir so
        // the prompt renews the account this card tracks — without the env
        // it would silently renew the default login instead.
        match &self.credential_scope {
            ClaudeCredentialScope::Standard => Some(crate::providers::SessionKickstart::new(
                "claude",
                &["-p", "Hi", "--model", "sonnet"],
            )),
            ClaudeCredentialScope::ConfigDir { path, .. } => {
                Some(crate::providers::SessionKickstart::with_envs(
                    "claude",
                    &["-p", "Hi", "--model", "sonnet"],
                    vec![("CLAUDE_CONFIG_DIR".to_owned(), path.display().to_string())],
                ))
            }
        }
    }

    fn definition(&self) -> ProviderDefinition {
        self.definition.clone()
    }

    fn has_local_credentials(&self) -> bool {
        auth::has_local_credentials(&self.credential_scope)
    }

    fn cache_identity(&self) -> crate::providers::CacheIdentity<'_> {
        self.account_identity
            .as_deref()
            .map(crate::providers::CacheIdentity::Resolved)
            .unwrap_or(crate::providers::CacheIdentity::Unresolved)
    }

    fn supports_account_names(&self) -> bool {
        true
    }

    fn account_identity(&self) -> Option<&str> {
        self.account_identity.as_deref()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, crate::providers::ProviderError> {
        self.refresh_inner().map_err(|error| {
            use crate::models::ProviderErrorKind as Kind;

            let kind = match error {
                ClaudeError::NotLoggedIn
                | ClaudeError::DesktopAppOnly
                | ClaudeError::SessionExpired
                | ClaudeError::TokenExpired
                | ClaudeError::CredentialsChanged
                | ClaudeError::AccountChanged => Kind::Authentication,
                ClaudeError::InvalidOAuthUrl | ClaudeError::InvalidResponse => {
                    Kind::InvalidResponse
                }
                ClaudeError::AuthWrite => Kind::CredentialStorage,
                ClaudeError::RequestFailed(429) => Kind::RateLimited,
                ClaudeError::RequestFailed(_) | ClaudeError::ConnectionFailed => Kind::Network,
                ClaudeError::LocalUsage => Kind::LocalData,
                ClaudeError::AccountStore(_) => Kind::Internal,
            };
            crate::providers::ProviderError::from_display(kind, error)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        sync::{mpsc, Arc},
        thread,
        time::Duration as StdDuration,
    };

    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    use crate::{
        models::{ProviderNoticeTone, ProviderSnapshot, UsageHistory},
        pricing::PricingStore,
        storage::Storage,
    };

    use super::{
        accounts::{self, ClaudeAccount, ClaudeAccountDiscovery},
        auth::{ClaudeCredentialScope, ClaudeOAuthConfig},
        client::ClaudeClient,
        definition, definition_for, rate_limit_notice, runtime_configs, ClaudeError,
        ClaudeProvider, ClaudeRuntimeConfig,
    };
    use crate::providers::UsageProvider;

    fn credential_json(access: &str, refresh: &str, plan: &str) -> String {
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{access}","refreshToken":"{refresh}","expiresAt":4102444800000,"subscriptionType":"{plan}","scopes":["user:profile"]}}}}"#
        )
    }

    fn write_http_response(stream: &mut impl Write, utilization: u8) {
        let body = format!(
            r#"{{"five_hour":{{"utilization":{utilization},"resets_at":"2099-01-01T00:00:00Z"}}}}"#
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    }

    #[test]
    fn account_definition_has_isolated_provider_and_metric_ids() {
        let definition = definition_for("claude@1234abcd", "Claude — Work", false);

        assert_eq!(definition.id, "claude@1234abcd");
        assert_eq!(definition.display_name, "Claude — Work");
        assert!(!definition.fallback_enabled);
        assert!(definition
            .metrics
            .iter()
            .all(|metric| metric.id.starts_with("claude@1234abcd.")));
        assert!(definition
            .metrics
            .iter()
            .all(|metric| !metric.default_pinned));
    }

    #[test]
    fn bare_account_in_a_config_dir_replaces_the_empty_default_placeholder() {
        let configs = runtime_configs(ClaudeAccountDiscovery {
            default_account: None,
            accounts: vec![ClaudeAccount {
                id: "claude".into(),
                display_name: "Claude".into(),
                label: Some("Personal".into()),
                identity: "identity-a".into(),
                credential_scope: ClaudeCredentialScope::ConfigDir {
                    path: "account-a".into(),
                    keychain_literal: "account-a".into(),
                },
                log_roots: vec!["account-a".into()],
            }],
        });

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].definition.id, "claude");
        assert!(matches!(
            &configs[0].credential_scope,
            ClaudeCredentialScope::ConfigDir { .. }
        ));
        assert!(!configs[0].include_standard_logs);
        assert!(!configs[0].include_pi);
    }

    #[test]
    fn scoped_account_runtimes_target_their_own_config_dir() {
        // Without CLAUDE_CONFIG_DIR a built-in prompt would renew the default
        // login while the evaluator watches the account card's windows —
        // kicking the wrong account on every cooldown.
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("usagedeck.db")).unwrap());
        let pricing = Arc::new(
            PricingStore::new_without_refresh_for_test(directory.path().join("pricing")).unwrap(),
        );
        let client = ClaudeClient::new().unwrap();
        let account_dir = directory.path().join("claude-work");
        let configs = runtime_configs(ClaudeAccountDiscovery {
            default_account: Some(ClaudeAccount {
                id: "claude".into(),
                display_name: "Claude".into(),
                label: None,
                identity: "identity-default".into(),
                credential_scope: ClaudeCredentialScope::Standard,
                log_roots: Vec::new(),
            }),
            accounts: vec![ClaudeAccount {
                id: "claude@1a2b3c4d".into(),
                display_name: "Claude — Work".into(),
                label: Some("Work".into()),
                identity: "identity-work".into(),
                credential_scope: ClaudeCredentialScope::ConfigDir {
                    path: account_dir.clone(),
                    keychain_literal: "claude-work".into(),
                },
                log_roots: Vec::new(),
            }],
        });
        assert_eq!(configs.len(), 2);

        for config in configs {
            let provider = ClaudeProvider::new_scoped(
                config,
                storage.clone(),
                pricing.clone(),
                client.clone(),
            );
            let scoped = matches!(
                provider.credential_scope,
                ClaudeCredentialScope::ConfigDir { .. }
            );
            let kickstart = provider
                .session_kickstart()
                .expect("every runtime offers a built-in");
            if scoped {
                assert_eq!(
                    kickstart.envs,
                    vec![(
                        "CLAUDE_CONFIG_DIR".to_owned(),
                        account_dir.display().to_string()
                    )],
                    "scoped kicks must target the account's config dir"
                );
            } else {
                assert!(
                    kickstart.envs.is_empty(),
                    "the standard login needs no environment"
                );
            }
        }
    }

    #[test]
    fn swapped_accounts_keep_their_ids_while_sources_change() {
        let configs = runtime_configs(ClaudeAccountDiscovery {
            default_account: Some(ClaudeAccount {
                id: "claude@1234abcd".into(),
                display_name: "Claude — Work".into(),
                label: Some("Work".into()),
                identity: "identity-b".into(),
                credential_scope: ClaudeCredentialScope::Standard,
                log_roots: Vec::new(),
            }),
            accounts: vec![ClaudeAccount {
                id: "claude".into(),
                display_name: "Claude".into(),
                label: Some("Personal".into()),
                identity: "identity-a".into(),
                credential_scope: ClaudeCredentialScope::ConfigDir {
                    path: "account-a".into(),
                    keychain_literal: "account-a".into(),
                },
                log_roots: vec!["account-a".into()],
            }],
        });

        assert_eq!(
            configs
                .iter()
                .map(|config| config.definition.id.as_str())
                .collect::<Vec<_>>(),
            ["claude@1234abcd", "claude"]
        );
        assert!(matches!(
            &configs[0].credential_scope,
            ClaudeCredentialScope::Standard
        ));
        assert!(matches!(
            &configs[1].credential_scope,
            ClaudeCredentialScope::ConfigDir { .. }
        ));
    }

    #[test]
    fn rate_limit_notice_distinguishes_empty_and_stale_live_usage() {
        let empty = rate_limit_notice(301, false);
        assert_eq!(empty.title, "Live usage paused");
        assert_eq!(empty.message, "Retrying in about 6 minutes");
        assert_eq!(empty.tone, ProviderNoticeTone::Warning);

        let stale = rate_limit_notice(60, true);
        assert_eq!(
            stale.message,
            "Showing the last successful limits · Retrying in about 1 minute"
        );
    }

    #[test]
    fn account_change_clears_live_usage_and_rate_limit_cache() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("usagedeck.db")).unwrap());
        let pricing = Arc::new(PricingStore::new(directory.path().join("pricing")).unwrap());
        let provider = ClaudeProvider::new(storage, pricing).unwrap();
        let snapshot = ProviderSnapshot {
            provider_id: "claude".into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };

        provider.activate_live_usage_cache([1; 32]);
        *provider.last_good.lock().unwrap() = Some(snapshot);
        *provider.rate_limited_until.lock().unwrap() = Some(Utc::now() + Duration::minutes(5));
        provider.activate_live_usage_cache([1; 32]);
        assert!(provider.last_good.lock().unwrap().is_some());
        assert!(provider.rate_limited_until.lock().unwrap().is_some());

        provider.activate_live_usage_cache([2; 32]);
        assert!(provider.last_good.lock().unwrap().is_none());
        assert!(provider.rate_limited_until.lock().unwrap().is_none());
    }

    #[test]
    fn login_changed_during_usage_request_is_not_published_under_the_old_card() {
        let directory = tempdir().unwrap();
        let account_root = directory.path().join("account");
        fs::create_dir_all(&account_root).unwrap();
        let credential_path = account_root.join(".credentials.json");
        let identity_path = account_root.join(".claude.json");
        fs::write(
            &credential_path,
            credential_json("account-a", "refresh-a", "pro"),
        )
        .unwrap();
        fs::write(
            &identity_path,
            r#"{"oauthAccount":{"accountUuid":"account-a"}}"#,
        )
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (first_request_tx, first_request_rx) = mpsc::sync_channel(0);
        let (first_response_tx, first_response_rx) = mpsc::sync_channel(0);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 4096];
            let length = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..length]);
            let authorization = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned())
                .unwrap();
            first_request_tx.send(()).unwrap();
            first_response_rx.recv().unwrap();
            write_http_response(&mut stream, 25);
            authorization
        });

        let storage = Arc::new(Storage::open(&directory.path().join("usagedeck.db")).unwrap());
        let pricing = Arc::new(PricingStore::new(directory.path().join("pricing")).unwrap());
        let credential_scope = ClaudeCredentialScope::ConfigDir {
            path: account_root.clone(),
            keychain_literal: account_root.to_string_lossy().into_owned(),
        };
        let provider = Arc::new(ClaudeProvider::new_scoped(
            ClaudeRuntimeConfig {
                definition: definition(),
                account_identity: accounts::identity_for_scope(&credential_scope),
                credential_scope,
                log_roots: vec![account_root],
                include_standard_logs: false,
                include_pi: false,
            },
            storage,
            pricing,
            ClaudeClient::new().unwrap(),
        ));
        let config = ClaudeOAuthConfig {
            usage_url: format!("{base}/usage"),
            refresh_url: format!("{base}/token"),
            client_id: "test-client".into(),
        };
        let refresh = thread::spawn(move || provider.refresh_inner_with_config(&config));

        first_request_rx
            .recv_timeout(StdDuration::from_secs(2))
            .unwrap();
        fs::write(
            &credential_path,
            credential_json("account-b", "refresh-b", "max"),
        )
        .unwrap();
        fs::write(
            &identity_path,
            r#"{"oauthAccount":{"accountUuid":"account-b"}}"#,
        )
        .unwrap();
        first_response_tx.send(()).unwrap();

        let error = refresh.join().unwrap().unwrap_err();
        let authorization = server.join().unwrap();
        assert!(matches!(error, ClaudeError::AccountChanged));
        assert_eq!(authorization, "Bearer account-a");
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_claude_code_owned_login_defers_its_refresh_instead_of_rotating_it() {
        // Scoped to this test: on a platform that keeps rotating these logins
        // the test is not compiled, and an unused import is a hard error.
        use super::auth::{ClaudeCredential, ClaudeCredentialGeneration};

        let directory = tempdir().unwrap();
        let account_root = directory.path().join("account");
        fs::create_dir_all(&account_root).unwrap();

        // Nothing may reach this listener. Rotating the login would POST to the
        // token endpoint, and that write is what rewrites the Keychain item's
        // access control list and makes the CLI prompt for a password.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (contacted, was_contacted) = mpsc::channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stream.is_ok() {
                    let _ = contacted.send(());
                }
            }
        });

        let storage = Arc::new(Storage::open(&directory.path().join("usagedeck.db")).unwrap());
        let pricing = Arc::new(
            PricingStore::new_without_refresh_for_test(directory.path().join("pricing")).unwrap(),
        );
        let credential_scope = ClaudeCredentialScope::ConfigDir {
            path: account_root.clone(),
            keychain_literal: account_root.to_string_lossy().into_owned(),
        };
        let provider = ClaudeProvider::new_scoped(
            ClaudeRuntimeConfig {
                definition: definition(),
                account_identity: accounts::identity_for_scope(&credential_scope),
                credential_scope,
                log_roots: vec![account_root],
                include_standard_logs: false,
                include_pi: false,
            },
            storage,
            Arc::clone(&pricing),
            ClaudeClient::new().unwrap(),
        );
        let config = ClaudeOAuthConfig {
            usage_url: format!("{base}/usage"),
            refresh_url: format!("{base}/token"),
            client_id: "test-client".into(),
        };

        let now = Utc::now();
        // Expired an hour ago, so the pre-emptive refresh path is the one taken.
        let expired = (now - Duration::hours(1)).timestamp_millis() as f64;
        let mut credential = ClaudeCredential::keychain_backed_for_test(expired);
        let mut generation =
            ClaudeCredentialGeneration::from_candidates(std::slice::from_ref(&credential));

        let snapshot = provider
            .refresh_candidate(
                &mut credential,
                &config,
                now,
                &pricing.current(),
                &mut generation,
            )
            .unwrap();

        assert_eq!(
            snapshot
                .notices
                .iter()
                .map(|notice| notice.id.as_str())
                .collect::<Vec<_>>(),
            ["loginRefreshDeferred"],
            "a deferred login must say so rather than looking like fresh figures"
        );
        assert!(
            was_contacted
                .recv_timeout(StdDuration::from_millis(250))
                .is_err(),
            "a Claude Code-owned login must not be rotated, so nothing may reach the token endpoint"
        );
    }
}
