use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    apply_shortcut_change, autostart_is_enabled, child_process,
    desktop_integration::DesktopIntegration,
    models::{AppSettings, SettingsViewState},
    notifications::permission as notification_permission,
    pacing::NotificationEvaluator,
    providers::{detect_local_credentials, ProviderRegistry},
    service::ProviderService,
    set_autostart,
    settings::{CredentialDetectionPlan, SettingsService},
    tray_presentation,
    window::PanelResizeSession,
};

#[derive(Clone, Copy)]
enum SettingsSaveMode {
    Normal,
    ResetAll,
}

#[tauri::command]
pub async fn get_app_settings(
    app: AppHandle,
    settings: State<'_, Arc<SettingsService>>,
) -> Result<SettingsViewState, ()> {
    // The view-state build reads storage (and the OS autostart state); it must
    // not run as a sync command on the main thread.
    let service = settings.inner().clone();
    let fallback = service.clone();
    let view_app = app.clone();
    Ok(
        tauri::async_runtime::spawn_blocking(move || settings_view_state(&view_app, &service))
            .await
            .unwrap_or_else(move |_| {
                fallback.view_state(
                    crate::models::NotificationPermission::Unavailable,
                    Some("Settings could not be read.".to_owned()),
                    false,
                    None,
                )
            }),
    )
}

#[tauri::command]
pub async fn save_app_settings(
    app: AppHandle,
    service: State<'_, Arc<ProviderService>>,
    settings_service: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    settings: AppSettings,
    expected_settings_revision: u64,
    expected_account_revision: u64,
) -> Result<SettingsViewState, String> {
    let (state, _, _) = save_app_settings_inner(
        app,
        service.inner().clone(),
        settings_service.inner().clone(),
        notifications.inner().clone(),
        settings,
        expected_settings_revision,
        expected_account_revision,
        SettingsSaveMode::Normal,
    )
    .await?;
    Ok(state)
}

/// Which OS-level side effects a settings save applied before it failed, so
/// the rollback undoes exactly what happened and logs what it could not.
#[derive(Default)]
struct AppliedSideEffects {
    shortcut: bool,
    autostart: bool,
    window_mode: bool,
}

/// Reverts the side effects a failed save already applied. Each failed
/// rollback step is logged — a silently stuck autostart entry or shortcut
/// registration is otherwise invisible until the next reboot.
fn rollback_settings_side_effects(
    app: &AppHandle,
    previous: &AppSettings,
    attempted_shortcut: Option<&str>,
    applied: AppliedSideEffects,
) {
    if applied.autostart {
        if let Err(error) = set_autostart(app, previous.launch_at_login) {
            crate::app_error!("config", "launch-at-login rollback failed: {error}");
        }
    }
    if applied.shortcut {
        if let Err(error) =
            apply_shortcut_change(app, attempted_shortcut, previous.global_shortcut.as_deref())
        {
            crate::app_error!("config", "global shortcut rollback failed: {error}");
        }
    }
    if applied.window_mode {
        if let Some(window) = app.get_webview_window(crate::window::MAIN_WINDOW) {
            if let Err(error) =
                crate::window::apply_window_mode(&window, previous.window_mode, false)
            {
                crate::app_error!("config", "window mode rollback failed: {error}");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn save_app_settings_inner(
    app: AppHandle,
    service: Arc<ProviderService>,
    settings_service: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
    settings: AppSettings,
    expected_settings_revision: u64,
    expected_account_revision: u64,
    mode: SettingsSaveMode,
) -> Result<(SettingsViewState, Vec<String>, CredentialDetectionPlan), String> {
    let command_guard = settings_service.lock_command_mutation().await;
    let previous = settings_service.get();
    let next_shortcut = settings.global_shortcut.clone();
    let autostart_changed = previous.launch_at_login != settings.launch_at_login;
    let window_mode_changed = previous.window_mode != settings.window_mode;
    let mut applied = AppliedSideEffects::default();
    apply_shortcut_change(
        &app,
        previous.global_shortcut.as_deref(),
        settings.global_shortcut.as_deref(),
    )?;
    applied.shortcut = true;
    if autostart_changed {
        if let Err(error) = set_autostart(&app, settings.launch_at_login) {
            rollback_settings_side_effects(
                &app,
                &previous,
                settings.global_shortcut.as_deref(),
                applied,
            );
            return Err(error);
        }
        applied.autostart = true;
    }
    if window_mode_changed {
        let Some(window) = app.get_webview_window(crate::window::MAIN_WINDOW) else {
            rollback_settings_side_effects(
                &app,
                &previous,
                settings.global_shortcut.as_deref(),
                applied,
            );
            return Err("UsageDeck window is unavailable.".to_owned());
        };
        if let Err(error) = crate::window::apply_window_mode(&window, settings.window_mode, true) {
            rollback_settings_side_effects(
                &app,
                &previous,
                settings.global_shortcut.as_deref(),
                applied,
            );
            return Err(error);
        }
        applied.window_mode = true;
    }
    let persist_service = settings_service.clone();
    let persisted = match mode {
        SettingsSaveMode::Normal => tauri::async_runtime::spawn_blocking(move || {
            persist_service.update_from_view(
                settings,
                expected_settings_revision,
                expected_account_revision,
            )
        })
        .await
        .map_err(|_| "UsageDeck settings could not be saved.".to_owned())?,
        SettingsSaveMode::ResetAll => tauri::async_runtime::spawn_blocking(move || {
            persist_service.reset_all_from_view(
                settings,
                expected_settings_revision,
                expected_account_revision,
            )
        })
        .await
        .map_err(|_| "UsageDeck settings could not be saved.".to_owned())?,
    };
    let updated = match persisted {
        Ok(settings) => settings,
        Err(error) => {
            crate::app_error!("config", "settings could not be persisted");
            rollback_settings_side_effects(&app, &previous, next_shortcut.as_deref(), applied);
            return Err(error);
        }
    };
    if previous.log_level != updated.log_level {
        crate::logging::set_level(updated.log_level);
        crate::app_info!(
            "config",
            "log level changed to {}",
            updated.log_level.log_label()
        );
    }
    if previous.theme != updated.theme {
        if let Some(window) = app.get_webview_window(crate::window::MAIN_WINDOW) {
            if crate::window::apply_panel_surface(&window, updated.theme).is_err() {
                crate::app_warn!("window", "panel surface theme could not be applied");
            }
        }
    }
    crate::app_debug!("config", "application settings persisted");
    tray_presentation::update(
        &app,
        &service.state(),
        &updated,
        settings_service.registry(),
    );
    let _ = app.emit(
        "settings-state",
        settings_view_state(&app, &settings_service),
    );

    let newly_enabled = newly_enabled_provider_ids(&previous, &updated);
    let credential_detection_plan = settings_service.reset_detection_plan();
    drop(command_guard);
    if matches!(mode, SettingsSaveMode::Normal) && !newly_enabled.is_empty() {
        crate::commands::usage::refresh_with_events(
            &app,
            &service,
            &settings_service,
            &notifications,
            &newly_enabled,
            true,
            false,
        )
        .await;
    }
    Ok((
        settings_view_state(&app, &settings_service),
        newly_enabled,
        credential_detection_plan,
    ))
}

/// Persists only the update-check timestamp; avoids the full save pipeline
/// (and its side-effect checks) when the periodic auto-check stamps its clock.
#[tauri::command]
pub async fn record_update_check(
    app: AppHandle,
    settings: State<'_, Arc<SettingsService>>,
    checked_at: chrono::DateTime<chrono::Utc>,
) -> Result<SettingsViewState, String> {
    let service = settings.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.record_update_check(checked_at))
        .await
        .map_err(|_| "The update check could not be recorded.".to_owned())?
        .map_err(|_| "The update check could not be recorded.".to_owned())?;
    let state = settings_view_state(&app, &settings);
    let _ = app.emit("settings-state", &state);
    Ok(state)
}

#[tauri::command]
pub async fn reset_customization(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    expected_settings_revision: u64,
    expected_account_revision: u64,
) -> Result<SettingsViewState, String> {
    crate::app_info!("config", "reset all customization requested");
    let command_guard = settings.lock_command_mutation().await;
    let previous = settings.get();
    let mut next = previous.as_ref().clone();
    let detected_before_reset = next
        .providers
        .iter()
        .filter(|provider| provider.detected)
        .map(|provider| provider.id.clone())
        .collect::<HashSet<_>>();
    next.providers = settings.default_settings(&detected_before_reset).providers;
    next.detection_notice_dismissed = false;
    let persist_service = settings.inner().clone();
    let next = tauri::async_runtime::spawn_blocking(move || {
        persist_service.update_from_view(
            next,
            expected_settings_revision,
            expected_account_revision,
        )
    })
    .await
    .map_err(|_| "UsageDeck settings could not be saved.".to_owned())??;
    let newly_enabled = newly_enabled_provider_ids(&previous, &next);
    let credential_detection_plan = settings.reset_detection_plan();
    tray_presentation::update(&app, &service.state(), &next, settings.registry());
    let state = settings_view_state(&app, &settings);
    let _ = app.emit("settings-state", &state);
    drop(command_guard);
    spawn_provider_reseed(
        app,
        registry.inner().clone(),
        service.inner().clone(),
        settings.inner().clone(),
        notifications.inner().clone(),
        credential_detection_plan,
        newly_enabled,
    );
    Ok(state)
}

#[tauri::command]
pub async fn reset_all_settings(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    expected_settings_revision: u64,
    expected_account_revision: u64,
) -> Result<SettingsViewState, String> {
    crate::app_info!("config", "reset all settings requested");
    let panel = app.state::<Arc<PanelResizeSession>>().inner().clone();
    let panel_reset = panel.begin_automatic_reset()?;
    let defaults = settings.reset_defaults();

    let result = save_app_settings_inner(
        app.clone(),
        service.inner().clone(),
        settings.inner().clone(),
        notifications.inner().clone(),
        defaults,
        expected_settings_revision,
        expected_account_revision,
        SettingsSaveMode::ResetAll,
    )
    .await;
    let (state, newly_enabled, credential_detection_plan) = match result {
        Ok(result) => result,
        Err(error) => {
            if let Err(rollback_error) = panel.rollback_automatic_reset(panel_reset) {
                crate::app_error!(
                    "config",
                    "panel state could not be restored after reset failure"
                );
                return Err(format!("{error} {rollback_error}"));
            }
            return Err(error);
        }
    };

    spawn_provider_reseed(
        app,
        registry.inner().clone(),
        service.inner().clone(),
        settings.inner().clone(),
        notifications.inner().clone(),
        credential_detection_plan,
        newly_enabled,
    );
    Ok(state)
}

fn spawn_provider_reseed(
    app: AppHandle,
    registry: Arc<ProviderRegistry>,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
    plan: CredentialDetectionPlan,
    mut refresh_provider_ids: Vec<String>,
) {
    tauri::async_runtime::spawn(async move {
        let detected = detect_local_credentials(registry, plan.provider_ids()).await;
        let command_guard = settings.lock_command_mutation().await;
        let apply_settings = settings.clone();
        let outcome = match tauri::async_runtime::spawn_blocking(move || {
            apply_settings.apply_credential_detection(&plan, &detected)
        })
        .await
        {
            Ok(Ok(outcome)) => Some(outcome),
            Ok(Err(_)) => {
                crate::app_warn!(
                    "config",
                    "provider detection after reset could not be saved"
                );
                None
            }
            Err(_) => {
                crate::app_warn!(
                    "config",
                    "provider detection after reset could not be applied"
                );
                None
            }
        };
        if let Some(outcome) = outcome {
            tray_presentation::update(
                &app,
                &service.state(),
                &outcome.settings,
                settings.registry(),
            );
            let _ = app.emit("settings-state", settings_view_state(&app, &settings));
            for provider_id in outcome.newly_enabled_provider_ids {
                if !refresh_provider_ids.contains(&provider_id) {
                    refresh_provider_ids.push(provider_id);
                }
            }
        }
        drop(command_guard);
        let enabled = settings
            .enabled_provider_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        refresh_provider_ids.retain(|provider_id| enabled.contains(provider_id));
        if refresh_provider_ids.is_empty() {
            return;
        }
        crate::commands::usage::refresh_with_events(
            &app,
            &service,
            &settings,
            &notifications,
            &refresh_provider_ids,
            true,
            false,
        )
        .await;
        let _ = app.emit("settings-state", settings_view_state(&app, &settings));
    });
}

fn newly_enabled_provider_ids(previous: &AppSettings, next: &AppSettings) -> Vec<String> {
    next.providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && !previous
                    .providers
                    .iter()
                    .any(|old| old.id == provider.id && old.enabled)
        })
        .map(|provider| provider.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::newly_enabled_provider_ids;
    use crate::models::{AppSettings, ProviderLayout};

    fn provider(id: &str, enabled: bool) -> ProviderLayout {
        ProviderLayout {
            id: id.to_owned(),
            enabled,
            detected: true,
            expanded: false,
            metrics: Vec::new(),
        }
    }

    #[test]
    fn newly_enabled_providers_exclude_unchanged_and_disabled_entries() {
        let previous = AppSettings {
            providers: vec![provider("codex", true), provider("cursor", false)],
            ..AppSettings::default()
        };
        let next = AppSettings {
            providers: vec![
                provider("codex", true),
                provider("cursor", true),
                provider("claude", true),
                provider("disabled", false),
            ],
            ..AppSettings::default()
        };

        assert_eq!(
            newly_enabled_provider_ids(&previous, &next),
            vec!["cursor".to_owned(), "claude".to_owned()]
        );
    }
}

#[tauri::command]
pub async fn reset_provider_customization(
    app: AppHandle,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    provider_id: String,
    expected_settings_revision: u64,
    expected_account_revision: u64,
) -> Result<SettingsViewState, String> {
    crate::app_info!("config", "provider customization reset for {provider_id}");
    let command_guard = settings.lock_command_mutation().await;
    let persist_settings = settings.inner().clone();
    let provider_for_reset = provider_id.clone();
    let updated = tauri::async_runtime::spawn_blocking(move || {
        persist_settings.reset_provider(
            &provider_for_reset,
            expected_settings_revision,
            expected_account_revision,
        )
    })
    .await
    .map_err(|_| "UsageDeck settings could not be saved.".to_owned())??;
    tray_presentation::update(&app, &service.state(), &updated, settings.registry());
    let state = settings_view_state(&app, &settings);
    let _ = app.emit("settings-state", &state);
    drop(command_guard);
    Ok(state)
}

#[tauri::command]
pub fn request_notification_permission(
    app: AppHandle,
    settings: State<'_, Arc<SettingsService>>,
) -> SettingsViewState {
    crate::app_info!("notifications", "notification permission requested");
    let error = app
        .notification()
        .request_permission()
        .err()
        .map(|_| "Notification permission could not be requested.".to_owned());
    if error.is_some() {
        crate::app_error!("notifications", "notification permission request failed");
    }
    settings.view_state(
        notification_permission(&app),
        error,
        app.state::<DesktopIntegration>().tray_available(),
        app.state::<DesktopIntegration>().platform_summary(),
    )
}

#[tauri::command]
pub fn open_notification_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = child_process::background_command("explorer.exe")
        .arg("ms-settings:notifications")
        .spawn();
    #[cfg(target_os = "macos")]
    let result = child_process::background_command("open")
        .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
        .spawn();
    #[cfg(target_os = "linux")]
    let result = [
        ("gnome-control-center", "notifications"),
        ("systemsettings", "kcm_notifications"),
        ("systemsettings5", "kcm_notifications"),
    ]
    .into_iter()
    .find_map(|(program, argument)| {
        child_process::background_command(program)
            .arg(argument)
            .spawn()
            .ok()
    })
    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "settings unavailable"));

    result
        .map(|_| ())
        .map_err(|_| "Notification settings could not be opened on this system.".to_owned())
}

#[tauri::command]
pub fn get_log_path() -> String {
    crate::logging::log_path().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    let path = crate::logging::log_path();
    let result = if path.is_file() {
        app.opener().reveal_item_in_dir(&path)
    } else if let Some(parent) = path.parent() {
        app.opener()
            .open_path(parent.to_string_lossy(), None::<&str>)
    } else {
        return Err("The UsageDeck log folder is unavailable.".to_owned());
    };
    result
        .inspect(|_| crate::app_debug!("config", "log folder opened"))
        .map_err(|_| {
            crate::app_warn!("config", "log folder could not be opened");
            "The UsageDeck log folder could not be opened.".to_owned()
        })
}

pub(crate) fn settings_view_state(app: &AppHandle, service: &SettingsService) -> SettingsViewState {
    let (autostart, mut integration_error) = match autostart_is_enabled(app) {
        Ok(enabled) => (Some(enabled), None),
        Err(_) => (
            None,
            Some("Launch at login status could not be read.".to_owned()),
        ),
    };
    if let Some(shortcut) = service.get().global_shortcut.clone() {
        if !app.global_shortcut().is_registered(shortcut.as_str()) {
            integration_error =
                Some("The saved global shortcut is currently unavailable.".to_owned());
        }
    }
    let mut state = service.view_state(
        notification_permission(app),
        integration_error,
        app.state::<DesktopIntegration>().tray_available(),
        app.state::<DesktopIntegration>().platform_summary(),
    );
    if let Some(enabled) = autostart {
        state.settings.launch_at_login = enabled;
    }
    state
}

pub(crate) fn emit_settings_if_account_changed(
    app: &AppHandle,
    service: &SettingsService,
    observed_revision: &AtomicU64,
) {
    let revision = service.account_revision();
    if observed_revision.swap(revision, Ordering::SeqCst) != revision {
        let _ = app.emit("settings-state", settings_view_state(app, service));
    }
}
