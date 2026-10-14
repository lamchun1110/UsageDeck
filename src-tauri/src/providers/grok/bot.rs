use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;
use thiserror::Error;

use super::bot_auth::{BotAuthStore, BotLogin};
use crate::models::{ProviderErrorKind, QuotaFormat, QuotaWindow, StatusMetric, StatusTone};

const API_BASE: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService";

#[derive(Debug, Error)]
pub(crate) enum BotError {
    #[error("Grok Bot secure sign-in could not be read. Allow access to its secure storage, then refresh.")]
    Credentials,
    #[error("Grok Bot sign-in expired. Open Grok Bot or sign in to Cursor, then refresh.")]
    Expired,
    #[error("Could not reach Grok Bot's usage service.")]
    Connection,
    #[error("Grok Bot returned an invalid usage response.")]
    InvalidResponse,
    #[error("Grok Bot usage request failed (HTTP {0}).")]
    Request(u16),
}

impl BotError {
    pub fn kind(&self) -> ProviderErrorKind {
        match self {
            Self::Credentials => ProviderErrorKind::CredentialStorage,
            Self::Expired => ProviderErrorKind::Authentication,
            Self::Request(429) => ProviderErrorKind::RateLimited,
            Self::Connection | Self::Request(_) => ProviderErrorKind::Network,
            Self::InvalidResponse => ProviderErrorKind::InvalidResponse,
        }
    }
}

pub(super) struct BotUsage {
    pub quota: Option<QuotaWindow>,
    pub extra: Option<StatusMetric>,
    pub plan: Option<String>,
    pub warnings: Vec<String>,
}

pub(super) struct BotClient {
    client: Client,
    base: String,
}

impl BotClient {
    pub fn new() -> Result<Self, BotError> {
        Self::with_base(API_BASE.into())
    }

    fn with_base(base: String) -> Result<Self, BotError> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("UsageDeck/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| BotError::Connection)?,
            base,
        })
    }

    #[cfg(test)]
    pub fn for_test(base: String) -> Self {
        Self::with_base(base).unwrap()
    }

    pub fn fetch(
        &self,
        auth: &BotAuthStore,
        now: DateTime<Utc>,
    ) -> Result<Option<BotUsage>, BotError> {
        let Some(login) = auth.load(now)? else {
            return Ok(None);
        };
        let status = self.request("GetSandUsageStatus", &login)?;
        let mut mapped = map_status(&status)?;
        // Account spending is optional and must never suppress a valid weekly meter.
        if mapped
            .extra
            .as_ref()
            .is_some_and(|metric| metric.text != "Disabled")
        {
            match self.request("GetCurrentPeriodUsage", &login) {
                Ok(period) => apply_spending(&mut mapped, &period),
                Err(error) => mapped.warnings.push(error.to_string()),
            }
        }
        Ok(Some(mapped))
    }

    fn request(&self, method: &str, login: &BotLogin) -> Result<Value, BotError> {
        let mut request = self
            .client
            .post(format!("{}/{method}", self.base))
            .bearer_auth(&login.token)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("X-Cursor-Client-Type", "sand")
            .header("X-Cursor-Client-Source", "sand-desktop")
            .header("X-Sand-Box-Namespace", "prod")
            .body("{}");
        if let Some(team_id) = login.team_id {
            request = request.header("X-Cursor-Team-Id", team_id.to_string());
        }
        let response = self
            .send_request(request)
            .map_err(|_| BotError::Connection)?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(BotError::Expired),
            status if !status.is_success() => return Err(BotError::Request(status.as_u16())),
            _ => {}
        }
        response.json().map_err(|_| BotError::InvalidResponse)
    }

    #[cfg(not(test))]
    fn send_request(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response, reqwest::Error> {
        request.send()
    }

    #[cfg(test)]
    fn send_request(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response, reqwest::Error> {
        // Match the regular Grok client's tolerance for transient test-server
        // loopback failures on loaded platform runners. Production sends once.
        for attempt in 0..3 {
            match request.try_clone().expect("buffered usage request").send() {
                Ok(response) => return Ok(response),
                Err(error) if attempt == 2 => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        unreachable!()
    }
}

fn map_status(body: &Value) -> Result<BotUsage, BotError> {
    if !body.is_object() {
        return Err(BotError::InvalidResponse);
    }
    let pooled = body
        .get("usesPooledEnterpriseAllowance")
        .and_then(Value::as_bool)
        == Some(true);
    let unified = body.get("isUnified").and_then(Value::as_bool) == Some(true);
    let quota = if pooled {
        None
    } else {
        body.get("usagePercent")
            .map(|value| {
                let percent = number(value).ok_or(BotError::InvalidResponse)?;
                let start = date(body.get("currentPeriodStart"));
                let reset = date(body.get("nextResetTimestampUtc"));
                let period_seconds = match (start, reset) {
                    (Some(start), Some(reset)) => u64::try_from((reset - start).num_seconds())
                        .ok()
                        .filter(|seconds| *seconds > 0)
                        .ok_or(BotError::InvalidResponse)?,
                    _ => 7 * 24 * 60 * 60,
                };
                Ok(QuotaWindow {
                    id: "botWeekly".into(),
                    label: if unified {
                        "Shared Weekly"
                    } else {
                        "Bot Weekly"
                    }
                    .into(),
                    used_percent: percent.clamp(0.0, 100.0),
                    resets_at: reset,
                    period_seconds,
                    format: QuotaFormat::Percent,
                    used_value: None,
                    limit_value: None,
                    unit: None,
                    estimated: false,
                    source_note: Some(
                        if unified {
                            "Shared allowance for Grok, Grok Bot, and Cursor."
                        } else {
                            "Grok Bot's included weekly allowance."
                        }
                        .into(),
                    ),
                })
            })
            .transpose()?
    };
    let extra = body
        .get("onDemandSettings")
        .filter(|settings| settings.get("visible").and_then(Value::as_bool) == Some(true))
        .map(|settings| {
            let enabled = settings.get("enabled").and_then(Value::as_bool) == Some(true);
            StatusMetric {
                id: "botExtra".into(),
                label: "Bot Extra Usage".into(),
                text: if enabled { "Enabled" } else { "Disabled" }.into(),
                tone: if enabled {
                    StatusTone::Positive
                } else {
                    StatusTone::Neutral
                },
                subtitle: Some("Monthly spending is billed through your Cursor account.".into()),
            }
        });
    let plan = body
        .get(if unified {
            "cursorPlanName"
        } else {
            "grokPlanLabel"
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(|plan| format!("{plan} · Grok Bot"));
    Ok(BotUsage {
        quota,
        extra,
        plan,
        warnings: if pooled {
            vec!["Grok Bot uses a pooled Enterprise allowance; a personal weekly percentage is unavailable.".into()]
        } else {
            vec![]
        },
    })
}

fn apply_spending(mapped: &mut BotUsage, body: &Value) {
    let Some(metric) = mapped.extra.as_mut() else {
        return;
    };
    let Some(spend) = body.get("spendLimitUsage") else {
        return;
    };
    // Protobuf omits default-valued counters; absent spending is zero only
    // within a returned spendLimitUsage record, never a fabricated quota.
    let used = spend.get("individualUsed").and_then(number).unwrap_or(0.0) / 100.0;
    metric.text = match spend.get("individualLimit").and_then(number) {
        Some(limit) if limit >= 0.0 => format!("${used:.2} / ${:.2} cap", limit / 100.0),
        _ => format!("${used:.2} spent"),
    };
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .filter(|value| value.is_finite())
}

fn date(value: Option<&Value>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value?.as_str()?)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_the_verified_meter_and_proto_default_disabled_state() {
        let mapped = map_status(&json!({"usagePercent":22.372069,"currentPeriodStart":"2026-10-09T13:37:45.949Z","nextResetTimestampUtc":"2026-10-16T13:37:45.949Z","grokPlanLabel":"SuperGrok","onDemandSettings":{"visible":true,"eligible":true}})).unwrap();
        let quota = mapped.quota.unwrap();
        assert_eq!(quota.label, "Bot Weekly");
        assert_eq!(quota.used_percent, 22.372069);
        assert_eq!(quota.period_seconds, 604800);
        assert!(quota.resets_at.is_some());
        assert_eq!(mapped.extra.unwrap().text, "Disabled");
    }

    #[test]
    fn shared_and_pooled_allowances_are_not_presented_as_separate_personal_quota() {
        let mapped = map_status(&json!({"isUnified":true,"usagePercent":0})).unwrap();
        assert_eq!(mapped.quota.unwrap().label, "Shared Weekly");
        assert!(map_status(&json!({})).unwrap().quota.is_none());
        assert!(map_status(&json!({"usagePercent":null})).is_err());
        assert!(
            map_status(&json!({"usagePercent":40,"usesPooledEnterpriseAllowance":true}))
                .unwrap()
                .quota
                .is_none()
        );
        assert!(map_status(&json!({"usagePercent":4,"currentPeriodStart":"2026-10-16T00:00:00Z","nextResetTimestampUtc":"2026-10-09T00:00:00Z"})).is_err());
    }

    #[test]
    fn on_demand_spending_uses_cents_and_reports_a_shared_account_cap() {
        let mut mapped =
            map_status(&json!({"onDemandSettings":{"visible":true,"enabled":true}})).unwrap();
        apply_spending(
            &mut mapped,
            &json!({"spendLimitUsage":{"individualUsed":1250,"individualLimit":2000}}),
        );
        assert_eq!(mapped.extra.unwrap().text, "$12.50 / $20.00 cap");
    }
}
