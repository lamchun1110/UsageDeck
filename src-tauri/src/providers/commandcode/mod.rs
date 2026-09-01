mod auth;
mod client;
mod mapper;

use chrono::Utc;
use thiserror::Error;

use crate::models::{
    MetricDefinition, MetricSection, ProviderDefinition, ProviderErrorKind, ProviderLink,
    ProviderSnapshot, UsageHistory,
};

use self::{auth::CommandCodeAuthStore, client::CommandCodeClient, mapper::map_usage};

use super::{ProviderError, UsageProvider};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "commandcode".into(),
        display_name: "Command Code".into(),
        short_name: "CC".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![ProviderLink::new("Usage", "https://commandcode.ai/usage")],
        options: Vec::new(),
        metrics: vec![
            MetricDefinition::quota(
                "commandcode.session",
                "Session",
                "session",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "S",
            ),
            MetricDefinition::quota(
                "commandcode.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "W",
            ),
            MetricDefinition::quota_or_value(
                "commandcode.monthly",
                "Monthly",
                "monthly",
                true,
                MetricSection::OnDemand,
                false,
                "M",
            ),
            MetricDefinition::value(
                "commandcode.extra",
                "Extra Credits",
                "extraCredits",
                true,
                MetricSection::OnDemand,
                false,
                "E",
                None,
            ),
        ],
    }
}

#[derive(Debug, Error)]
pub(crate) enum CommandCodeError {
    #[error("Command Code is not logged in. Run `command-code login`.")]
    NotLoggedIn,
    #[error("Command Code login data is invalid or expired. Run `command-code login` again.")]
    InvalidAuth,
    #[error("Could not reach Command Code. Check your internet connection.")]
    ConnectionFailed,
    #[error("Command Code returned an invalid usage response.")]
    InvalidResponse,
    #[error("Command Code usage request failed (HTTP {0}).")]
    RequestFailed(u16),
}

impl From<CommandCodeError> for ProviderError {
    fn from(error: CommandCodeError) -> Self {
        let kind = match error {
            CommandCodeError::NotLoggedIn | CommandCodeError::InvalidAuth => {
                ProviderErrorKind::Authentication
            }
            CommandCodeError::ConnectionFailed => ProviderErrorKind::Network,
            CommandCodeError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            CommandCodeError::RequestFailed(_) | CommandCodeError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
        };
        ProviderError::from_display(kind, error)
    }
}

pub struct CommandCodeProvider {
    auth: CommandCodeAuthStore,
    client: CommandCodeClient,
}

impl CommandCodeProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            auth: CommandCodeAuthStore::new(),
            client: CommandCodeClient::new().map_err(ProviderError::from)?,
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: CommandCodeAuthStore, client: CommandCodeClient) -> Self {
        Self { auth, client }
    }
}

impl UsageProvider for CommandCodeProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    /// Both windows are first-message rolling — observed live: a single
    /// `cmd -p` prompt started the session and the weekly anchored to the
    /// same millisecond.
    fn rolling_windows(&self) -> Vec<String> {
        vec!["session".to_owned(), "weekly".to_owned()]
    }

    /// Print mode sends one prompt and exits; `--no-session` keeps the
    /// throwaway kick out of the user's conversation history. Not offered on
    /// Windows: the CLI's `cmd` binary name collides with cmd.exe, which the
    /// kick shell would resolve to itself.
    fn session_kickstart(&self) -> Option<crate::providers::SessionKickstart> {
        if cfg!(target_os = "windows") {
            return None;
        }
        Some(crate::providers::SessionKickstart::new(
            "cmd",
            &["-p", "Hi", "--no-session"],
        ))
    }

    fn has_local_credentials(&self) -> bool {
        self.auth.has_local_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        let api_key = self.auth.load().map_err(ProviderError::from)?;
        let (credits, subscription) = self
            .client
            .fetch(api_key.as_str())
            .map_err(ProviderError::from)?;
        let mapped = map_usage(&credits, &subscription).map_err(ProviderError::from)?;
        Ok(ProviderSnapshot {
            provider_id: "commandcode".into(),
            plan: mapped.plan,
            quotas: mapped.quotas,
            value_metrics: mapped.value_metrics,
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod transport_tests {
    use std::{fs, time::Duration};

    use tempfile::tempdir;

    use crate::{
        models::{ProviderErrorKind, QuotaFormat},
        providers::{test_http, UsageProvider},
    };

    use super::{auth::CommandCodeAuthStore, client::CommandCodeClient, CommandCodeProvider};

    const CREDITS_BODY: &str = r#"{
        "credits": {"monthlyCredits": 7.5, "purchasedCredits": 2.0},
        "windowLimits": {
            "fiveHour": {"cap": 3, "used": 0.75, "resetAt": 1786363200000},
            "weekly": {"cap": 6, "used": 3, "resetAt": 1786795200}
        }
    }"#;
    const SUBSCRIPTION_BODY: &str = r#"{
        "success": true,
        "data": {
            "planId": "individual-goat",
            "currentPeriodStart": "2026-08-01T12:00:00Z",
            "currentPeriodEnd": "2026-09-01T12:00:00Z"
        }
    }"#;

    fn auth(key: Option<&str>) -> CommandCodeAuthStore {
        crate::providers::http::clear_rate_limits();
        let directory = tempdir().unwrap();
        let path = directory.path().join("auth.json");
        if let Some(key) = key {
            fs::write(&path, format!(r#"{{"apiKey":"{key}"}}"#)).unwrap();
        }
        // The store only keeps the path; hold the directory open for the rest
        // of the test process rather than racing its cleanup.
        std::mem::forget(directory);
        CommandCodeAuthStore::with_path(path)
    }

    fn provider(
        key: Option<&str>,
        credits_status: u16,
        credits_body: &str,
        subscription_status: u16,
        subscription_body: &str,
    ) -> CommandCodeProvider {
        let credits_url = test_http::serve_once(credits_status, &[], credits_body);
        let subscription_url = test_http::serve_once(subscription_status, &[], subscription_body);
        let auth = auth(key);
        CommandCodeProvider::with_dependencies(
            auth,
            CommandCodeClient::for_test(&credits_url, &subscription_url, Duration::from_secs(1)),
        )
    }

    #[test]
    fn refresh_maps_window_limits_and_plan_through_the_transport() {
        let snapshot = provider(
            Some("secret-key"),
            200,
            CREDITS_BODY,
            200,
            SUBSCRIPTION_BODY,
        )
        .refresh()
        .unwrap();

        assert_eq!(snapshot.plan.as_deref(), Some("GOAT"));
        assert_eq!(
            snapshot
                .quotas
                .iter()
                .map(|quota| quota.id.as_str())
                .collect::<Vec<_>>(),
            ["session", "weekly", "monthly"]
        );
        assert_eq!(snapshot.quotas[0].format, QuotaFormat::Dollars);
        assert_eq!(snapshot.quotas[0].used_percent, 25.0);
        assert_eq!(snapshot.value_metrics[0].id, "extraCredits");
    }

    #[test]
    fn missing_credentials_are_authentication_errors() {
        let error = provider(None, 200, CREDITS_BODY, 200, SUBSCRIPTION_BODY)
            .refresh()
            .unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Authentication);
        assert!(error.to_string().contains("command-code login"));
    }

    #[test]
    fn unauthorized_forbidden_and_rate_limited_statuses_are_classified() {
        for status in [401, 403] {
            let error = provider(Some("secret-key"), status, "{}", 200, SUBSCRIPTION_BODY)
                .refresh()
                .unwrap_err();
            assert_eq!(error.kind(), ProviderErrorKind::Authentication, "{status}");
        }

        let rate_limited = provider(
            Some("limited-secret-key"),
            429,
            "{}",
            200,
            SUBSCRIPTION_BODY,
        )
        .refresh()
        .unwrap_err();
        assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);

        let server_error = provider(Some("secret-key"), 500, "{}", 200, SUBSCRIPTION_BODY)
            .refresh()
            .unwrap_err();
        assert_eq!(server_error.kind(), ProviderErrorKind::InvalidResponse);
    }

    #[test]
    fn malformed_success_bodies_are_invalid_responses() {
        let error = provider(Some("secret-key"), 200, "not json", 200, SUBSCRIPTION_BODY)
            .refresh()
            .unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::InvalidResponse);
    }

    #[test]
    fn timeouts_are_network_errors_and_never_expose_the_key() {
        let credits_url = test_http::serve_once_after(
            test_http::TIMEOUT_TEST_RESPONSE_DELAY,
            200,
            &[],
            CREDITS_BODY,
        );
        let subscription_url = test_http::serve_once(200, &[], SUBSCRIPTION_BODY);
        let auth = auth(Some("secret-key"));
        let provider = CommandCodeProvider::with_dependencies(
            auth,
            CommandCodeClient::for_test(
                &credits_url,
                &subscription_url,
                test_http::TIMEOUT_TEST_CLIENT_LIMIT,
            ),
        );

        let error = provider.refresh().unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Network);
        assert!(!error.to_string().contains("secret-key"));
    }
}

#[cfg(test)]
mod rolling_window_tests {
    use super::CommandCodeProvider;
    use crate::providers::UsageProvider;

    #[test]
    fn both_windows_roll_and_the_built_in_prompt_is_noninteractive() {
        let provider = CommandCodeProvider::new().unwrap();

        assert_eq!(
            provider.rolling_windows(),
            vec!["session".to_owned(), "weekly".to_owned()]
        );
        // On Windows the `cmd` binary name resolves to cmd.exe inside the
        // kick shell, so no built-in is offered there.
        if cfg!(target_os = "windows") {
            assert_eq!(provider.session_kickstart(), None);
        } else {
            assert_eq!(
                provider
                    .session_kickstart()
                    .map(|kickstart| (kickstart.program, kickstart.args)),
                Some((
                    "cmd".to_owned(),
                    vec!["-p".to_owned(), "Hi".to_owned(), "--no-session".to_owned()]
                ))
            );
        }
    }
}
