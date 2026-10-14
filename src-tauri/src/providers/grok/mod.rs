mod auth;
mod bot;
mod bot_auth;
mod client;
mod local_usage;
mod mapper;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use thiserror::Error;

use crate::{
    models::{
        MetricDefinition, MetricSection, ProviderDefinition, ProviderErrorKind, ProviderLink,
        ProviderSnapshot, UsagePeriodSelection,
    },
    pricing::PricingStore,
    providers::log_usage::scan_or_cached_usage,
    storage::Storage,
};

use self::{
    auth::{GrokAuthState, GrokAuthStore},
    bot::{BotClient, BotUsage},
    bot_auth::BotAuthStore,
    client::GrokClient,
    local_usage::GrokLogUsageScanner,
    mapper::{map_credits, plan_name},
};

use super::{ProviderError, UsageProvider};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "grok".into(),
        display_name: "Grok".into(),
        short_name: "G".into(),
        fallback_enabled: false,
        local_usage_source_note: Some("From your Grok logs (estimated)".into()),
        links: vec![
            ProviderLink::new("Usage", "https://grok.com/?_s=usage"),
            ProviderLink::new("Bot Usage", "https://cursor.com/dashboard"),
        ],
        options: Vec::new(),
        metrics: vec![
            MetricDefinition::quota(
                "grok.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "W",
            ),
            MetricDefinition::quota(
                "grok.botWeekly",
                "Bot Weekly",
                "botWeekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "BW",
            ),
            MetricDefinition::status(
                "grok.payAsYouGo",
                "Extra Usage",
                "payAsYouGo",
                true,
                MetricSection::OnDemand,
                false,
                "E",
            ),
            MetricDefinition::status(
                "grok.botExtra",
                "Bot Extra Usage",
                "botExtra",
                true,
                MetricSection::OnDemand,
                false,
                "BE",
            ),
            MetricDefinition::trend("grok.trend"),
            MetricDefinition::usage(
                "grok.today",
                "Today",
                UsagePeriodSelection::Today,
                MetricSection::OnDemand,
                "T",
            ),
            MetricDefinition::usage(
                "grok.yesterday",
                "Yesterday",
                UsagePeriodSelection::Yesterday,
                MetricSection::OnDemand,
                "Y",
            ),
            MetricDefinition::usage(
                "grok.last30",
                "Last 30 Days",
                UsagePeriodSelection::Last30Days,
                MetricSection::OnDemand,
                "M",
            ),
        ],
    }
}

#[derive(Debug, Error)]
pub(crate) enum GrokError {
    #[error("Sign in to Grok Bot or run `grok login`.")]
    NotLoggedIn,
    #[error("Grok login data is invalid. Run `grok login` again.")]
    InvalidAuth,
    #[error("Grok login expired. Run `grok login` again.")]
    Expired,
    #[error("Refreshed Grok credentials could not be saved.")]
    AuthWrite,
    #[error("Could not reach Grok. Check your internet connection.")]
    ConnectionFailed,
    #[error("Grok returned an invalid billing response.")]
    InvalidResponse,
    #[error("Grok billing request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("Local Grok usage logs could not be processed.")]
    LocalUsage,
    #[error("UsageDeck cache is unavailable.")]
    Storage,
    #[error(transparent)]
    Bot(#[from] bot::BotError),
}

impl From<crate::storage::StorageError> for GrokError {
    fn from(_: crate::storage::StorageError) -> Self {
        Self::Storage
    }
}

pub struct GrokProvider {
    storage: Arc<Storage>,
    pricing: Arc<PricingStore>,
    auth: GrokAuthStore,
    client: GrokClient,
    bot_auth: BotAuthStore,
    bot_client: BotClient,
    log_usage: GrokLogUsageScanner,
    now: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

impl GrokProvider {
    pub fn new(storage: Arc<Storage>, pricing: Arc<PricingStore>) -> Result<Self, GrokError> {
        Ok(Self {
            storage,
            pricing,
            auth: GrokAuthStore::new(),
            client: GrokClient::new()?,
            bot_auth: BotAuthStore::new(),
            bot_client: BotClient::new()?,
            log_usage: GrokLogUsageScanner::new(),
            now: Arc::new(Utc::now),
        })
    }

    #[cfg(test)]
    fn with_dependencies(
        storage: Arc<Storage>,
        pricing: Arc<PricingStore>,
        auth: GrokAuthStore,
        client: GrokClient,
        log_usage: GrokLogUsageScanner,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            storage,
            pricing,
            auth,
            client,
            bot_auth: BotAuthStore::for_login(None),
            bot_client: BotClient::new().unwrap(),
            log_usage,
            now: Arc::new(move || now),
        }
    }

    fn refresh_inner(&self) -> Result<ProviderSnapshot, GrokError> {
        let cli = self.refresh_cli_inner();
        let bot = if self.bot_auth.has_local_credentials() {
            self.bot_client
                .fetch(&self.bot_auth, (self.now)())
                .map_err(GrokError::from)
        } else {
            Ok(None)
        };
        match (cli, bot) {
            (Ok(mut snapshot), Ok(Some(bot))) => {
                append_bot_usage(&mut snapshot, bot);
                Ok(snapshot)
            }
            (Ok(mut snapshot), Err(error)) => {
                snapshot.warnings.push(error.to_string());
                Ok(snapshot)
            }
            (Ok(snapshot), Ok(None)) => Ok(snapshot),
            (Err(cli_error), Ok(Some(bot))) => {
                let now = (self.now)();
                let mut warnings = vec![];
                if !matches!(cli_error, GrokError::NotLoggedIn) {
                    warnings.push(cli_error.to_string());
                }
                let pricing = self.pricing.current();
                let usage = scan_or_cached_usage(
                    &self.storage,
                    "grok",
                    crate::providers::CacheIdentity::Unscoped,
                    "Grok",
                    || self.log_usage.scan(&self.storage, now, &pricing),
                    &mut warnings,
                );
                let mut snapshot = ProviderSnapshot {
                    provider_id: "grok".into(),
                    plan: None,
                    quotas: vec![],
                    value_metrics: vec![],
                    status_metrics: vec![],
                    notices: vec![],
                    usage,
                    warnings,
                    refreshed_at: now,
                };
                append_bot_usage(&mut snapshot, bot);
                Ok(snapshot)
            }
            (Err(_), Err(bot_error)) => Err(bot_error),
            (Err(error), Ok(None)) => Err(error),
        }
    }

    fn refresh_cli_inner(&self) -> Result<ProviderSnapshot, GrokError> {
        let now = (self.now)();
        let candidates = self.auth.load_candidates()?;
        crate::app_debug!(
            "auth:grok",
            "credential candidates loaded ({})",
            candidates.len()
        );
        let mut last_auth_error = None;
        for mut state in candidates {
            match self.refresh_candidate(&mut state, now) {
                Ok(snapshot) => return Ok(snapshot),
                Err(error @ (GrokError::Expired | GrokError::InvalidAuth)) => {
                    last_auth_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_auth_error.unwrap_or(GrokError::InvalidAuth))
    }

    fn refresh_candidate(
        &self,
        state: &mut GrokAuthState,
        now: DateTime<Utc>,
    ) -> Result<ProviderSnapshot, GrokError> {
        let mut warnings = Vec::new();
        if self.auth.needs_refresh(state, now) {
            if let Err(error) = self.refresh_access_token(state, now, &mut warnings) {
                if self.auth.is_expired(state, now) {
                    return Err(GrokError::Expired);
                }
                crate::app_warn!(
                    "auth:grok",
                    "proactive token refresh failed; trying the current access token ({error})"
                );
            }
        }

        let mut credits = self.client.fetch_credits(&state.token)?;
        if matches!(
            credits.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            self.refresh_access_token(state, now, &mut warnings)?;
            credits = self.client.fetch_credits(&state.token)?;
        }
        let mapped = map_credits(&credits)?;
        let plan = self
            .client
            .fetch_settings(&state.token)
            .ok()
            .as_ref()
            .and_then(plan_name);
        let pricing = self.pricing.current();
        let usage = scan_or_cached_usage(
            &self.storage,
            "grok",
            crate::providers::CacheIdentity::Unscoped,
            "Grok",
            || self.log_usage.scan(&self.storage, now, &pricing),
            &mut warnings,
        );
        Ok(ProviderSnapshot {
            provider_id: "grok".into(),
            plan,
            quotas: mapped.quotas,
            value_metrics: Vec::new(),
            status_metrics: mapped.status_metrics,
            notices: Vec::new(),
            usage,
            warnings,
            refreshed_at: now,
        })
    }

    fn refresh_access_token(
        &self,
        state: &mut GrokAuthState,
        now: DateTime<Utc>,
        warnings: &mut Vec<String>,
    ) -> Result<(), GrokError> {
        let refresh_token = self
            .auth
            .refresh_token(state)
            .ok_or(GrokError::Expired)?
            .to_owned();
        let client_id = self.auth.client_id(state);
        let refreshed = self.client.refresh_token(&refresh_token, &client_id)?;
        self.auth.update_from_refresh(
            state,
            refreshed.access_token,
            refreshed.refresh_token,
            refreshed.id_token,
            refreshed.expires_in,
            now,
        );
        if self.auth.save(state).is_err() {
            crate::app_error!(
                "auth:grok",
                "failed to persist rotated credentials; using them for this session only"
            );
            warnings.push(
                "The refreshed Grok login is active for this session but could not be saved."
                    .into(),
            );
        }
        Ok(())
    }
}

impl UsageProvider for GrokProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn has_local_credentials(&self) -> bool {
        self.auth.has_local_credentials() || self.bot_auth.has_local_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        self.refresh_inner().map_err(|error| {
            let kind = match error {
                GrokError::NotLoggedIn | GrokError::InvalidAuth | GrokError::Expired => {
                    ProviderErrorKind::Authentication
                }
                GrokError::AuthWrite => ProviderErrorKind::CredentialStorage,
                GrokError::RequestFailed(429) => ProviderErrorKind::RateLimited,
                GrokError::RequestFailed(_) | GrokError::ConnectionFailed => {
                    ProviderErrorKind::Network
                }
                GrokError::InvalidResponse => ProviderErrorKind::InvalidResponse,
                GrokError::LocalUsage => ProviderErrorKind::LocalData,
                GrokError::Storage => ProviderErrorKind::Storage,
                GrokError::Bot(ref error) => error.kind(),
            };
            ProviderError::from_display(kind, error)
        })
    }
}

fn append_bot_usage(snapshot: &mut ProviderSnapshot, bot: BotUsage) {
    snapshot.quotas.extend(bot.quota);
    snapshot.status_metrics.extend(bot.extra);
    snapshot.warnings.extend(bot.warnings);
    if snapshot.plan.is_none() {
        snapshot.plan = bot.plan;
    }
}

#[cfg(test)]
mod tests;
