use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tauri::AppHandle;

use crate::{
    commands::usage::refresh_with_events, pacing::NotificationEvaluator, policy::REFRESH_INTERVAL,
    service::ProviderService, settings::SettingsService,
};

/// Slack between the wall clock and the monotonic wait before the gap is
/// attributed to a system suspend rather than ordinary timer jitter.
const SUSPEND_TOLERANCE: Duration = Duration::from_secs(5);

pub fn spawn(
    app: AppHandle,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
) {
    tauri::async_runtime::spawn(async move {
        // Anchor the schedule to a fixed deadline so a slow batch lengthens the gap by its own
        // duration instead of drifting the cadence by (interval + refresh time).
        let mut next_refresh = tokio::time::Instant::now();
        loop {
            next_refresh += REFRESH_INTERVAL;
            let provider_ids = settings.enabled_provider_ids();
            if !provider_ids.is_empty() {
                refresh_with_events(
                    &app,
                    &service,
                    &settings,
                    &notifications,
                    &provider_ids,
                    false,
                    true,
                )
                .await;
            }
            if next_refresh <= tokio::time::Instant::now() {
                next_refresh = tokio::time::Instant::now() + REFRESH_INTERVAL;
            }
            // The monotonic clock pauses while the system sleeps, so wall time running
            // well past the intended wait means the process was suspended: collapse the
            // remaining wait and refresh immediately on the next pass instead of
            // serving pre-sleep data for up to another full interval.
            let intended_wait = next_refresh.saturating_duration_since(tokio::time::Instant::now());
            let wall_before = SystemTime::now();
            tokio::time::sleep_until(next_refresh).await;
            if wall_before
                .elapsed()
                .is_ok_and(|elapsed| elapsed > intended_wait + SUSPEND_TOLERANCE)
            {
                next_refresh = tokio::time::Instant::now();
            }
        }
    });
}
