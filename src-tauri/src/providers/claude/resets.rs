use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{MetricValue, MetricValueKind, ValueMetric};

/// A CLI-surface rejection is unknown, not a confirmed balance of zero.
pub(crate) fn map_offer(body: &Value, now: DateTime<Utc>) -> Option<ValueMetric> {
    let offer = body.get("cedar_ember")?;
    let eligible = offer.get("eligible")?.as_bool()?;
    if !eligible && !offer.get("ineligible_reason").is_some_and(Value::is_null) {
        return None;
    }
    let grants = offer.get("grants")?.as_array()?;
    let mut count = 0_u64;
    let mut expiries = Vec::new();
    for grant in grants {
        let left = grant.get("resets_left")?.as_u64()?;
        // Bound both a corrupt response and allocations for repeated credits.
        if left > 10_000 {
            return None;
        }
        let expiry = match grant.get("ends_at") {
            Some(Value::String(value)) => Some(DateTime::parse_from_rfc3339(value).ok()?.to_utc()),
            Some(Value::Null) | None => None,
            _ => return None,
        };
        if expiry.is_some_and(|date| date <= now) {
            continue;
        }
        count = count.checked_add(left)?;
        if count > 10_000 {
            return None;
        }
        // paused / usable_now / use_requires_limit describe redemption. They
        // don't remove owned credits that become usable at the next limit.
        if let Some(expiry) = expiry {
            expiries.extend(std::iter::repeat_n(expiry, left as usize));
        }
    }
    expiries.sort();
    Some(ValueMetric {
        id: "rateLimitResets".into(),
        label: "Rate Limit Resets".into(),
        values: vec![MetricValue {
            number: count as f64,
            kind: MetricValueKind::Count,
            label: None,
            estimated: false,
        }],
        expiries_at: expiries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use serde_json::json;

    #[test]
    fn counts_each_remaining_credit_and_sorts_earliest_expiry() {
        let now = Utc.with_ymd_and_hms(2026, 10, 5, 0, 0, 0).unwrap();
        let early = now + Duration::hours(3);
        let late = now + Duration::days(18);
        let body = json!({"cedar_ember":{"eligible":true,"grants":[
            {"resets_left":2,"ends_at":late.to_rfc3339(),"usable_now":false,"use_requires_limit":true},
            {"resets_left":1,"ends_at":early.to_rfc3339()},
            {"resets_left":3,"ends_at":now.to_rfc3339()},
            {"resets_left":0,"ends_at":late.to_rfc3339()},
            {"resets_left":1,"ends_at":null}
        ]}});
        let metric = map_offer(&body, now).unwrap();
        assert_eq!(metric.values[0].number, 4.0);
        assert_eq!(metric.expiries_at, vec![early, late, late]);
    }

    #[test]
    fn separates_confirmed_empty_from_missing_and_surface_filtered_data() {
        let now = Utc::now();
        assert_eq!(
            map_offer(&json!({"cedar_ember":{"eligible":true,"grants":[]}}), now)
                .unwrap()
                .values[0]
                .number,
            0.0
        );
        for body in [
            json!({}),
            json!({"cedar_ember":null}),
            json!({"cedar_ember":{"eligible":false,"ineligible_reason":"surface","grants":[]}}),
            json!({"cedar_ember":{"eligible":true,"grants":[{"resets_left":-1}]}}),
        ] {
            assert!(map_offer(&body, now).is_none());
        }
    }
}
