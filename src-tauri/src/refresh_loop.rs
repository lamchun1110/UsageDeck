use std::sync::Arc;

use tauri::AppHandle;

use crate::{
    commands::usage::refresh_with_events, pacing::NotificationEvaluator, policy::REFRESH_INTERVAL,
    service::ProviderService, settings::SettingsService,
};

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
            tokio::time::sleep_until(next_refresh).await;
        }
    });
}
