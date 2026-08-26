use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{MetricValue, MetricValueKind, QuotaFormat, QuotaWindow, ValueMetric};

use super::{client::EndpointResponse, CommandCodeError};

const DEFAULT_MONTHLY_PERIOD_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, PartialEq)]
pub struct CommandCodeMappedUsage {
    pub plan: Option<String>,
    pub quotas: Vec<QuotaWindow>,
    pub value_metrics: Vec<ValueMetric>,
}

pub fn map_usage(
    credits: &EndpointResponse,
    subscription: &EndpointResponse,
) -> Result<CommandCodeMappedUsage, CommandCodeError> {
    require_success(credits)?;
    require_success(subscription)?;
    let windows = credits
        .body
        .get("windowLimits")
        .and_then(Value::as_object)
        .ok_or(CommandCodeError::InvalidResponse)?;
    let mut quotas = [
        quota(windows.get("fiveHour"), "session", "Session", 5 * 60 * 60),
        quota(windows.get("weekly"), "weekly", "Weekly", 7 * 24 * 60 * 60),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let credits_body = credits
        .body
        .get("credits")
        .and_then(Value::as_object)
        .ok_or(CommandCodeError::InvalidResponse)?;
    let subscription_data = subscription.body.get("data").and_then(Value::as_object);
    let plan_id = subscription_data
        .and_then(|data| data.get("planId"))
        .and_then(Value::as_str);
    let period_start = subscription_data
        .and_then(|data| data.get("currentPeriodStart"))
        .and_then(timestamp);
    let period_end = subscription_data
        .and_then(|data| data.get("currentPeriodEnd"))
        .and_then(timestamp);
    let period_seconds = period_start
        .zip(period_end)
        .and_then(|(start, end)| end.signed_duration_since(start).to_std().ok())
        .map_or(DEFAULT_MONTHLY_PERIOD_SECONDS, |duration| {
            duration.as_secs()
        });
    let mut values = Vec::new();
    if let Some(monthly_remaining) = number(credits_body.get("monthlyCredits")) {
        if let Some(monthly_limit) = plan_id.and_then(monthly_credit_limit) {
            quotas.push(monthly_quota(
                monthly_remaining,
                monthly_limit,
                period_end,
                period_seconds,
            ));
        } else {
            values.push(dollars_metric(
                "monthly",
                "Monthly",
                monthly_remaining,
                period_end.into_iter().collect(),
            ));
        }
    }
    if let Some(purchased) =
        number(credits_body.get("purchasedCredits")).filter(|value| *value > 0.0)
    {
        values.push(dollars_metric(
            "extraCredits",
            "Extra Credits",
            purchased,
            Vec::new(),
        ));
    }
    Ok(CommandCodeMappedUsage {
        plan: plan_id.map(display_plan),
        quotas,
        value_metrics: values,
    })
}

fn require_success(response: &EndpointResponse) -> Result<(), CommandCodeError> {
    if response.status.is_success() {
        Ok(())
    } else if response.status.as_u16() == 401 || response.status.as_u16() == 403 {
        Err(CommandCodeError::InvalidAuth)
    } else {
        Err(CommandCodeError::RequestFailed(response.status.as_u16()))
    }
}

fn quota(
    value: Option<&Value>,
    id: &str,
    label: &str,
    period_seconds: u64,
) -> Result<QuotaWindow, CommandCodeError> {
    let value = value
        .and_then(Value::as_object)
        .ok_or(CommandCodeError::InvalidResponse)?;
    let cap = number(value.get("cap"))
        .filter(|cap| *cap > 0.0)
        .ok_or(CommandCodeError::InvalidResponse)?;
    let used = number(value.get("used"))
        .filter(|used| *used >= 0.0)
        .ok_or(CommandCodeError::InvalidResponse)?;
    Ok(QuotaWindow {
        id: id.into(),
        label: label.into(),
        used_percent: (used / cap * 100.0).clamp(0.0, 100.0),
        resets_at: value.get("resetAt").and_then(timestamp),
        period_seconds,
        format: QuotaFormat::Dollars,
        used_value: Some(used.min(cap)),
        limit_value: Some(cap),
        unit: None,
        estimated: false,
        source_note: None,
    })
}

fn monthly_quota(
    remaining: f64,
    limit: f64,
    resets_at: Option<DateTime<Utc>>,
    period_seconds: u64,
) -> QuotaWindow {
    let used = (limit - remaining.max(0.0)).clamp(0.0, limit);
    QuotaWindow {
        id: "monthly".into(),
        label: "Monthly".into(),
        used_percent: (used / limit * 100.0).clamp(0.0, 100.0),
        resets_at,
        period_seconds,
        format: QuotaFormat::Dollars,
        used_value: Some(used),
        limit_value: Some(limit),
        unit: None,
        estimated: false,
        source_note: None,
    }
}

fn dollars_metric(
    id: &str,
    label: &str,
    amount: f64,
    expiries_at: Vec<DateTime<Utc>>,
) -> ValueMetric {
    ValueMetric {
        id: id.into(),
        label: label.into(),
        values: vec![MetricValue {
            number: amount.max(0.0),
            kind: MetricValueKind::Dollars,
            label: Some("remaining".into()),
            estimated: false,
        }],
        expiries_at,
    }
}

fn monthly_credit_limit(plan_id: &str) -> Option<f64> {
    // The API reports only the remaining balance, so pair it with Command Code's published
    // individual-plan allocations. Unknown plans deliberately keep the balance-only fallback.
    match plan_id.trim().to_ascii_lowercase().as_str() {
        "individual-go" => Some(10.0),
        "individual-goat" => Some(70.0),
        "individual-pro-v1" => Some(80.0),
        "individual-max" => Some(150.0),
        "individual-ultra" => Some(300.0),
        _ => None,
    }
}

fn display_plan(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    let known = match normalized.as_str() {
        "individual-go" => Some("Go"),
        "individual-goat" => Some("GOAT"),
        "individual-pro-v1" => Some("Pro"),
        "individual-max" => Some("Max 10x"),
        "individual-ultra" => Some("Max 20x"),
        _ => None,
    };
    if let Some(label) = known {
        return label.into();
    }
    normalized
        .strip_prefix("individual-")
        .unwrap_or(&normalized)
        .replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
        })
        .filter(|value| value.is_finite())
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(value) = value.as_str() {
        return DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|value| value.with_timezone(&Utc));
    }
    let value = value.as_i64()?;
    if value <= 0 {
        return None;
    }
    if value.unsigned_abs() >= 100_000_000_000 {
        DateTime::from_timestamp_millis(value)
    } else {
        DateTime::from_timestamp(value, 0)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use reqwest::StatusCode;
    use serde_json::json;

    use super::{map_usage, monthly_credit_limit, EndpointResponse};

    #[test]
    fn maps_live_window_limits_and_credit_balances() {
        let credits = EndpointResponse {
            status: StatusCode::OK,
            body: json!({
                "credits": {"monthlyCredits": 7.5, "purchasedCredits": 2.0},
                "windowLimits": {
                    "fiveHour": {"cap": 3, "used": 0.75, "resetAt": 1_786_363_200_000i64},
                    "weekly": {"cap": 6, "used": 3, "resetAt": 1_786_795_200}
                }
            }),
        };
        let subscription = EndpointResponse {
            status: StatusCode::OK,
            body: json!({
                "success": true,
                "data": {
                    "planId": "individual-goat",
                    "currentPeriodStart": "2026-08-01T12:00:00Z",
                    "currentPeriodEnd": "2026-09-01T12:00:00Z"
                }
            }),
        };

        let mapped = map_usage(&credits, &subscription).unwrap();
        assert_eq!(mapped.plan.as_deref(), Some("GOAT"));
        assert_eq!(mapped.quotas[0].used_percent, 25.0);
        assert_eq!(mapped.quotas[1].used_percent, 50.0);
        assert_eq!(
            mapped.quotas[0].resets_at,
            Some(
                DateTime::parse_from_rfc3339("2026-08-10T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        assert_eq!(
            mapped.quotas[1].resets_at,
            Some(
                DateTime::parse_from_rfc3339("2026-08-15T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        let monthly = &mapped.quotas[2];
        assert_eq!(monthly.id, "monthly");
        assert_eq!(monthly.used_value, Some(62.5));
        assert_eq!(monthly.limit_value, Some(70.0));
        assert_eq!(monthly.period_seconds, 31 * 24 * 60 * 60);
        assert_eq!(
            monthly.resets_at,
            Some(
                DateTime::parse_from_rfc3339("2026-09-01T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        assert_eq!(mapped.value_metrics.len(), 1);
        assert_eq!(mapped.value_metrics[0].id, "extraCredits");
    }

    #[test]
    fn unknown_plans_keep_the_monthly_balance_with_its_reset() {
        let credits = EndpointResponse {
            status: StatusCode::OK,
            body: json!({
                "credits": {"monthlyCredits": 7.5},
                "windowLimits": {
                    "fiveHour": {"cap": 3, "used": 0, "resetAt": 0},
                    "weekly": {"cap": 6, "used": 0, "resetAt": 0}
                }
            }),
        };
        let subscription = EndpointResponse {
            status: StatusCode::OK,
            body: json!({
                "data": {
                    "planId": "teams-custom",
                    "currentPeriodEnd": "2026-09-01T12:00:00Z"
                }
            }),
        };

        let mapped = map_usage(&credits, &subscription).unwrap();
        assert_eq!(mapped.plan.as_deref(), Some("Teams Custom"));
        assert_eq!(mapped.quotas.len(), 2);
        assert_eq!(mapped.quotas[0].resets_at, None);
        assert_eq!(mapped.value_metrics[0].id, "monthly");
        assert_eq!(mapped.value_metrics[0].values[0].number, 7.5);
        assert_eq!(
            mapped.value_metrics[0].expiries_at,
            vec![DateTime::parse_from_rfc3339("2026-09-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc)]
        );
    }

    #[test]
    fn parses_epoch_seconds_and_milliseconds_for_window_resets() {
        let credits = EndpointResponse {
            status: StatusCode::OK,
            body: json!({
                "credits": {"monthlyCredits": 7.5},
                "windowLimits": {
                    "fiveHour": {"cap": 3, "used": 0, "resetAt": 1_800_000_000},
                    "weekly": {"cap": 6, "used": 0, "resetAt": 1_800_000_000_000i64}
                }
            }),
        };
        let subscription = EndpointResponse {
            status: StatusCode::OK,
            body: json!({"data": {"planId": "individual-go"}}),
        };

        let mapped = map_usage(&credits, &subscription).unwrap();
        assert_eq!(
            mapped.quotas[0].resets_at,
            Some(
                DateTime::parse_from_rfc3339("2027-01-15T08:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        assert_eq!(mapped.quotas[1].resets_at, mapped.quotas[0].resets_at);
    }

    #[test]
    fn recognizes_current_individual_plan_allocations() {
        assert_eq!(monthly_credit_limit("individual-go"), Some(10.0));
        assert_eq!(monthly_credit_limit("individual-goat"), Some(70.0));
        assert_eq!(monthly_credit_limit("individual-pro-v1"), Some(80.0));
        assert_eq!(monthly_credit_limit("individual-max"), Some(150.0));
        assert_eq!(monthly_credit_limit("individual-ultra"), Some(300.0));
        assert_eq!(monthly_credit_limit("teams-custom"), None);
    }
}
