use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use tauri::{AppHandle, Emitter, State};

use crate::{
    commands::settings::emit_settings_if_account_changed,
    models::QuotaHistorySample,
    notifications::finish_refresh,
    pacing::NotificationEvaluator,
    providers::codex::reset_claim::{CodexResetClaimService, ResetClaimOutcome},
    service::{ProviderService, UsageViewState},
    settings::SettingsService,
    storage::Storage,
};

#[tauri::command]
pub async fn quota_history(
    storage: State<'_, Arc<Storage>>,
) -> Result<HashMap<String, Vec<QuotaHistorySample>>, String> {
    // A full history query against the shared connection must not run on the
    // main thread, where an in-flight snapshot write would freeze the UI.
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || storage.load_quota_history())
        .await
        .map_err(|_| "Quota history could not be loaded.".to_owned())?
        .map_err(|_| "Quota history could not be loaded.".to_owned())
}

/// The one refresh tail shared by every entry point — the UI command, the
/// tray's Refresh Now item, the background loop, startup detection, and
/// settings-triggered refreshes: progressive state events with account-
/// revision observation, a final full-state emit, and the notification pass.
/// Keeping it in one place is what makes the account-change emission uniform.
pub(crate) async fn refresh_with_events(
    app: &AppHandle,
    service: &Arc<ProviderService>,
    settings: &Arc<SettingsService>,
    notifications: &Arc<NotificationEvaluator>,
    provider_ids: &[String],
    force: bool,
    full_batch: bool,
) -> UsageViewState {
    let progress_app = app.clone();
    let progress_settings = settings.clone();
    let observed_account_revision = Arc::new(AtomicU64::new(settings.account_revision()));
    let progress_account_revision = observed_account_revision.clone();
    let progress = move |state: &UsageViewState| {
        emit_settings_if_account_changed(
            &progress_app,
            &progress_settings,
            &progress_account_revision,
        );
        let _ = progress_app.emit("usage-state", state);
    };
    let state = if full_batch {
        // Full batches also stamp `last_full_refresh_at`, which the panel
        // shows as the last complete refresh time.
        service
            .refresh_all_with_progress(provider_ids, force, progress)
            .await
    } else {
        service
            .refresh_enabled_with_progress(provider_ids, force, progress)
            .await
    };
    emit_settings_if_account_changed(app, settings, &observed_account_revision);
    let _ = app.emit("usage-state", &state);
    finish_refresh(app, &state, settings, notifications);
    state
}

/// One forced refresh across all enabled providers. Shared by the UI command
/// and the tray's Refresh Now item.
pub async fn run_forced_refresh(
    app: &AppHandle,
    service: &Arc<ProviderService>,
    settings: &Arc<SettingsService>,
    notifications: &Arc<NotificationEvaluator>,
) -> UsageViewState {
    refresh_with_events(
        app,
        service,
        settings,
        notifications,
        &settings.enabled_provider_ids(),
        true,
        true,
    )
    .await
}

#[tauri::command]
pub async fn claim_codex_reset_credit(
    app: AppHandle,
    claims: State<'_, Arc<CodexResetClaimService>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    expires_at: chrono::DateTime<chrono::Utc>,
    redeem_request_id: String,
) -> Result<ResetClaimOutcome, String> {
    if !settings
        .enabled_provider_ids()
        .iter()
        .any(|id| id == "codex")
    {
        return Err("Codex is not enabled.".to_owned());
    }
    let claims = claims.inner().clone();
    let outcome =
        tauri::async_runtime::spawn_blocking(move || claims.claim(expires_at, &redeem_request_id))
            .await
            .map_err(|_| "The reset claim could not be completed.".to_owned())?;

    if outcome != ResetClaimOutcome::Failed {
        refresh_with_events(
            &app,
            service.inner(),
            settings.inner(),
            notifications.inner(),
            &["codex".to_owned()],
            true,
            false,
        )
        .await;
    }
    Ok(outcome)
}

#[tauri::command]
pub async fn refresh_usage(
    app: AppHandle,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
) -> Result<UsageViewState, ()> {
    Ok(run_forced_refresh(
        &app,
        service.inner(),
        settings.inner(),
        notifications.inner(),
    )
    .await)
}

#[tauri::command]
pub async fn refresh_provider_usage(
    app: AppHandle,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
) -> Result<UsageViewState, String> {
    if !settings.enabled_provider_ids().contains(&provider_id) {
        return Err("Provider is not enabled.".to_owned());
    }

    Ok(refresh_with_events(
        &app,
        service.inner(),
        settings.inner(),
        notifications.inner(),
        &[provider_id],
        true,
        false,
    )
    .await)
}
