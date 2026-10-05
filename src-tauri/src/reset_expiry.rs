use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Duration, Utc};

use crate::{hashing::sha256_hex, models::ValueMetric, storage::Storage};

#[derive(Default)]
struct DeliveryState {
    sent: HashMap<String, DateTime<Utc>>,
    failed: HashMap<String, (u32, DateTime<Utc>, DateTime<Utc>)>,
}

/// Receipts are scoped to the account and deadline, so restarting the app or
/// changing the warning lead time does not repeat a delivered reminder.
pub struct ResetExpiryNotifier {
    storage: Arc<Storage>,
    state: Mutex<DeliveryState>,
}

impl ResetExpiryNotifier {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            state: Mutex::new(DeliveryState::default()),
        }
    }

    pub fn notify(
        &self,
        metric: &ValueMetric,
        account_scope: &str,
        hours: u16,
        now: DateTime<Utc>,
        mut deliver: impl FnMut(usize, DateTime<Utc>) -> bool,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.sent.retain(|_, expiry| *expiry > now);
        let groups = expiring_groups(metric, hours, now);
        state.failed.retain(|_, (_, _, expiry)| *expiry > now);
        if let Err(error) = self
            .storage
            .prune_reset_expiry_notifications(now.timestamp())
        {
            crate::app_warn!(
                "notifications",
                "could not prune reset reminder receipts: {error}"
            );
        }
        for (expiry, count) in groups {
            let key = receipt_key(account_scope, &metric.id, expiry);
            if state.sent.contains_key(&key)
                || state
                    .failed
                    .get(&key)
                    .is_some_and(|(_, retry, _)| *retry > now)
            {
                continue;
            }
            match self.storage.reset_expiry_notified(&key) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    crate::app_warn!(
                        "notifications",
                        "could not read reset reminder receipt: {error}"
                    );
                    continue;
                }
            }
            if !deliver(count, expiry) {
                let attempts = state
                    .failed
                    .get(&key)
                    .map_or(1, |(attempts, _, _)| attempts.saturating_add(1));
                state.failed.insert(
                    key,
                    (
                        attempts,
                        now + Duration::minutes(if attempts >= 3 { 60 } else { 5 }),
                        expiry,
                    ),
                );
                continue;
            }
            state.failed.remove(&key);
            state.sent.insert(key.clone(), expiry);
            if let Err(error) = self
                .storage
                .save_reset_expiry_notification(&key, expiry.timestamp().saturating_add(1))
            {
                crate::app_warn!(
                    "notifications",
                    "could not save reset reminder receipt: {error}"
                );
            }
        }
    }
}

fn receipt_key(scope: &str, metric_id: &str, expiry: DateTime<Utc>) -> String {
    sha256_hex(
        format!(
            "{}:{scope}{}:{metric_id}:{}",
            scope.len(),
            metric_id.len(),
            expiry.to_rfc3339()
        )
        .as_bytes(),
    )
}

fn expiring_groups(
    metric: &ValueMetric,
    hours: u16,
    now: DateTime<Utc>,
) -> BTreeMap<DateTime<Utc>, usize> {
    let count = metric.values.first().map_or(0.0, |value| value.number);
    if !count.is_finite() || count < 1.0 {
        return BTreeMap::new();
    }
    let known_expired = metric
        .expiries_at
        .iter()
        .filter(|expiry| **expiry <= now)
        .count();
    let available = (count.floor() as usize).saturating_sub(known_expired);
    let mut expiries = metric
        .expiries_at
        .iter()
        .copied()
        .filter(|expiry| *expiry > now)
        .collect::<Vec<_>>();
    expiries.sort();
    let deadline = now + Duration::hours(i64::from(hours.clamp(1, 168)));
    let mut groups = BTreeMap::new();
    for expiry in expiries
        .into_iter()
        .take(available)
        .take_while(|expiry| *expiry <= deadline)
    {
        *groups.entry(expiry).or_default() += 1;
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MetricValue, MetricValueKind};

    fn metric(now: DateTime<Utc>) -> ValueMetric {
        ValueMetric {
            id: "rateLimitResets".into(),
            label: "Rate Limit Resets".into(),
            values: vec![MetricValue {
                number: 4.0,
                kind: MetricValueKind::Count,
                label: Some("available".into()),
                estimated: false,
            }],
            expiries_at: vec![
                now + Duration::hours(25),
                now + Duration::hours(1),
                now - Duration::hours(1),
                now + Duration::hours(1),
            ],
        }
    }

    #[test]
    fn groups_live_credits_and_respects_count_and_lead_time() {
        let now = Utc::now();
        let mut value = metric(now);
        assert_eq!(
            expiring_groups(&value, 24, now),
            BTreeMap::from([(now + Duration::hours(1), 2)])
        );
        assert_eq!(
            expiring_groups(&value, 1, now - Duration::seconds(1)).len(),
            0
        );
        value.values[0].number = 0.0;
        assert!(expiring_groups(&value, 24, now).is_empty());
        value.values[0].number = f64::NAN;
        assert!(expiring_groups(&value, 24, now).is_empty());
        value.values[0].number = 4.0;
        value.expiries_at.clear();
        assert!(expiring_groups(&value, 24, now).is_empty());
    }

    #[test]
    fn delivered_reminders_survive_restart_and_remain_account_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("usagedeck.db")).unwrap());
        let now = Utc::now();
        let value = metric(now);
        let notifier = ResetExpiryNotifier::new(storage.clone());
        let mut calls = 0;
        notifier.notify(&value, "codex:account-a", 24, now, |count, _| {
            assert_eq!(count, 2);
            calls += 1;
            true
        });
        let restarted = ResetExpiryNotifier::new(storage);
        restarted.notify(&value, "codex:account-a", 48, now, |_, _| {
            calls += 1;
            true
        });
        // The 25-hour credit is newly within the wider warning window.
        assert_eq!(calls, 2);
        restarted.notify(&value, "codex:account-a", 48, now, |_, _| {
            calls += 1;
            true
        });
        assert_eq!(calls, 2);
        restarted.notify(&value, "codex:account-b", 24, now, |_, _| {
            calls += 1;
            true
        });
        assert_eq!(calls, 3);
    }

    #[test]
    fn failed_delivery_retries_without_marking_the_credit_delivered() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("usagedeck.db")).unwrap());
        let notifier = ResetExpiryNotifier::new(storage);
        let now = Utc::now();
        let value = metric(now);
        notifier.notify(&value, "account", 24, now, |_, _| false);
        let mut calls = 0;
        notifier.notify(&value, "account", 24, now + Duration::minutes(1), |_, _| {
            calls += 1;
            true
        });
        assert_eq!(calls, 0);
        notifier.notify(&value, "account", 24, now + Duration::minutes(5), |_, _| {
            calls += 1;
            true
        });
        assert_eq!(calls, 1);
    }
}
