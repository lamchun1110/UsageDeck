mod child_process;
mod commands;
mod desktop_integration;
mod hashing;
mod kickstart;
mod logging;
#[cfg(any(target_os = "macos", target_os = "linux", test))]
mod menu_bar;
mod migration;
mod models;
mod notifications;
mod pacing;
mod policy;
mod popup;
mod pricing;
mod provider_environment;
mod provider_options;
mod providers;
mod refresh_loop;
mod service;
mod settings;
mod storage;
mod svg_path;
#[cfg(any(all(not(target_os = "macos"), not(target_os = "linux")), test))]
mod tray_icon;
mod tray_presentation;
mod updates;
mod webview_memory;
mod window;
#[cfg(any(target_os = "linux", test))]
mod xdg_autostart;

use std::sync::{Arc, Mutex};

use popup::PopupDismissGuard;
use service::ProviderService;
use settings::{CredentialDetectionPlan, SettingsService};
#[cfg(not(target_os = "linux"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager,
};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::{
    desktop_integration::DesktopIntegration,
    pacing::NotificationEvaluator,
    pricing::PricingStore,
    providers::{
        antigravity::AntigravityProvider, claude, codex::reset_claim::CodexResetClaimService,
        codex::CodexProvider, commandcode::CommandCodeProvider, copilot::CopilotProvider,
        cursor::CursorProvider, detect_local_credentials, devin::DevinProvider, grok::GrokProvider,
        kimi::KimiProvider, minimax::MiniMaxProvider, opencode::OpenCodeProvider,
        openrouter::OpenRouterProvider, zai::ZaiProvider, ProviderRegistry, UsageProvider,
    },
    storage::Storage,
    window::{
        handle_window_event, open_screen, show_main_window, toggle_main_window, PanelResizeSession,
        MAIN_WINDOW,
    },
};

fn install_tray(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    let menu = {
        let refresh = MenuItem::with_id(app, "refresh", "Refresh Now", true, None::<&str>)?;
        let settings_item =
            MenuItem::with_id(app, "settings", "Settings", true, Some("CmdOrCtrl+,"))?;
        let separator = PredefinedMenuItem::separator(app)?;
        let quit = MenuItem::with_id(app, "quit", "Quit UsageDeck", true, Some("CmdOrCtrl+Q"))?;
        Menu::with_items(app, &[&refresh, &settings_item, &separator, &quit])?
    };
    #[cfg(not(target_os = "macos"))]
    let menu = {
        let open = MenuItem::with_id(app, "open", "Open UsageDeck", true, None::<&str>)?;
        let refresh = MenuItem::with_id(app, "refresh", "Refresh Now", true, None::<&str>)?;
        let customize = MenuItem::with_id(app, "customize", "Customize…", true, None::<&str>)?;
        let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
        let separator = PredefinedMenuItem::separator(app)?;
        let quit = MenuItem::with_id(app, "quit", "Quit UsageDeck", true, None::<&str>)?;
        Menu::with_items(
            app,
            &[
                &open,
                &refresh,
                &customize,
                &settings_item,
                &separator,
                &quit,
            ],
        )?
    };

    let icon = app
        .default_window_icon()
        .ok_or_else(|| std::io::Error::other("UsageDeck application icon is unavailable"))?
        .clone();
    let tray = TrayIconBuilder::with_id("usagedeck-tray")
        .icon(icon)
        .menu(&menu);
    #[cfg(not(target_os = "linux"))]
    let tray = tray.tooltip("UsageDeck").show_menu_on_left_click(false);
    let tray = tray.on_menu_event(|app, event| match event.id.as_ref() {
        "open" => {
            app.state::<PopupDismissGuard>().cancel_pending();
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                show_main_window(&window);
            }
        }
        "refresh" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let service = app.state::<Arc<ProviderService>>().inner().clone();
                let settings = app.state::<Arc<SettingsService>>().inner().clone();
                let notifications = app.state::<Arc<NotificationEvaluator>>().inner().clone();
                crate::commands::usage::run_forced_refresh(
                    &app,
                    &service,
                    &settings,
                    &notifications,
                )
                .await;
            });
        }
        "customize" => open_screen(app, "customize"),
        "settings" => open_screen(app, "settings"),
        "quit" => {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                window::finish_native_panel_resize(&window);
            }
            app.exit(0);
        }
        _ => {}
    });
    #[cfg(not(target_os = "linux"))]
    let tray = tray.on_tray_icon_event(|tray, event| {
        tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);

        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            toggle_main_window(tray.app_handle());
        }
    });
    tray.build(app)?;
    Ok(())
}

fn show_standalone_window_fallback(window: &tauri::WebviewWindow) {
    window
        .app_handle()
        .state::<DesktopIntegration>()
        .set_floating(true);
    let _ = window.set_resizable(false);
    let _ = window.set_skip_taskbar(false);
    let _ = window.set_always_on_top(false);
    let _ = window.center();
    show_main_window(window);
}

#[cfg(target_os = "linux")]
fn apply_linux_tray_fallback(app: &AppHandle) {
    let integration = app.state::<DesktopIntegration>();
    if !integration.disable_tray() {
        return;
    }
    app_warn!(
        "lifecycle",
        "system tray became unavailable; using standalone window"
    );
    // The tray icon is deliberately left in place. Every caller reaches this
    // fallback only after the StatusNotifierWatcher is confirmed gone, and
    // destroying the status-notifier item against a dead watcher is the one
    // native-teardown step in this window that can abort the process (the
    // v0.7.1 release smoke hit a silent exit here once). An orphaned icon is
    // harmless — it has no host to render it — and is freed with the process.
    app.state::<PopupDismissGuard>().cancel_pending();

    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let mode = app.state::<Arc<SettingsService>>().get().window_mode;
        match window::apply_window_mode(&window, mode, true) {
            Ok(()) => show_main_window(&window),
            Err(error) => {
                app_warn!(
                    "window",
                    "standalone fallback could not apply window mode: {error}"
                );
                show_standalone_window_fallback(&window);
            }
        }
    }

    let settings = app.state::<Arc<SettingsService>>();
    let state = commands::settings::settings_view_state(app, settings.inner().as_ref());
    let _ = app.emit("settings-state", state);
}

#[cfg(target_os = "linux")]
fn spawn_status_notifier_monitor(app: AppHandle) {
    if desktop_integration::status_notifier_monitor_forced_off() {
        return;
    }
    let monitor_app = app.clone();
    if std::thread::Builder::new()
        .name("usagedeck-tray-monitor".to_owned())
        .spawn(move || {
            loop {
                match desktop_integration::wait_for_status_notifier_loss() {
                    // A true loss: the watcher's owner went away. Keep the
                    // current behavior (log + standalone fallback).
                    Ok(()) => {
                        let fallback_app = monitor_app.clone();
                        if monitor_app
                            .run_on_main_thread(move || {
                                apply_linux_tray_fallback(&fallback_app)
                            })
                            .is_err()
                        {
                            app_warn!(
                                "lifecycle",
                                "standalone tray fallback could not be scheduled"
                            );
                        }
                        return;
                    }
                    // The signal stream ended without a loss event — a
                    // session-bus restart can do this while the watcher stays
                    // registered. Re-probe; if still registered, restart the
                    // monitor instead of permanently floating the session.
                    Err(error) if error.contains("watcher signal stream ended") => {
                        if desktop_integration::probe_status_notifier_watcher_available() {
                            crate::app_debug!(
                                "lifecycle",
                                "StatusNotifier monitor stream ended; watcher still registered, restarting monitor"
                            );
                            continue;
                        }
                        app_warn!("lifecycle", "system tray monitor stopped: {error}");
                        let fallback_app = monitor_app.clone();
                        if monitor_app
                            .run_on_main_thread(move || {
                                apply_linux_tray_fallback(&fallback_app)
                            })
                            .is_err()
                        {
                            app_warn!(
                                "lifecycle",
                                "standalone tray fallback could not be scheduled"
                            );
                        }
                        return;
                    }
                    Err(error) => {
                        app_warn!("lifecycle", "system tray monitor stopped: {error}");
                        let fallback_app = monitor_app.clone();
                        if monitor_app
                            .run_on_main_thread(move || {
                                apply_linux_tray_fallback(&fallback_app)
                            })
                            .is_err()
                        {
                            app_warn!(
                                "lifecycle",
                                "standalone tray fallback could not be scheduled"
                            );
                        }
                        return;
                    }
                }
            }
        })
        .is_err()
    {
        app_warn!("lifecycle", "system tray monitor could not be started");
        apply_linux_tray_fallback(&app);
    }
}

/// One-time legacy OpenQuota → UsageDeck migrations: the in-place database
/// rename (ahead of the copy pass so a user's own newer file wins), the data
/// directory copy, and credential-store re-keying. Directory and credential
/// copies are best-effort; an incomplete in-place database rename aborts
/// startup so a new database cannot mask retryable legacy data.
fn migrate_legacy_data(app_data_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    match migration::rename_legacy_database(app_data_dir) {
        Ok(true) => {
            app_info!(
                "lifecycle",
                "renamed the legacy database file to usagedeck.db"
            );
        }
        Ok(false) => {}
        // Do not continue into Storage::open after an incomplete in-place
        // database migration: that would create a fresh current database and
        // permanently mask the still-retryable legacy database on next launch.
        Err(error) => return Err(std::io::Error::other(error).into()),
    }
    let data_migration = migration::migrate_app_data(app_data_dir);
    for copied in &data_migration.copied {
        app_info!("lifecycle", "migrated legacy OpenQuota data: {copied}");
    }
    for error in &data_migration.errors {
        app_warn!("lifecycle", "legacy data migration issue: {error}");
    }
    let key_migration = migration::migrate_api_keys(app_data_dir);
    if !key_migration.migrated.is_empty() {
        app_info!(
            "lifecycle",
            "migrated {} saved API key(s) from the OpenQuota credential entry",
            key_migration.migrated.len()
        );
    }
    for (account, error) in &key_migration.failures {
        app_warn!(
            "lifecycle",
            "API key migration failed for {account}: {error}"
        );
    }
    // Failures are never retried (the consent marker is written before the
    // pass), so surface them in Settings instead of leaving them log-only.
    let failed_providers = key_migration
        .failures
        .iter()
        .map(|(account, _)| account.clone())
        .collect::<Vec<_>>();
    let _ = crate::commands::settings::set_key_migration_failures(failed_providers);
    Ok(())
}

/// Opens the database, restores the persisted provider environment, and wires
/// the panel resize session.
fn open_application_storage(
    app: &App,
    app_data_dir: &std::path::Path,
) -> Result<Arc<Storage>, Box<dyn std::error::Error>> {
    let database_path = app_data_dir.join(migration::DATABASE_FILE);
    let storage = Arc::new(Storage::open(&database_path)?);
    provider_environment::initialize(storage.load_provider_environment()?);
    provider_environment::refresh_for_next_launch(storage.clone());
    app.manage(Arc::new(PanelResizeSession::new(storage.clone())));
    app_debug!("cache", "application database opened");
    Ok(storage)
}

/// Builds the full provider registry: every built-in provider plus the
/// per-account API-key providers materialized from the database.
fn build_provider_registry(
    app_data_dir: &std::path::Path,
    storage: &Arc<Storage>,
    pricing: &Arc<PricingStore>,
) -> Result<Arc<ProviderRegistry>, Box<dyn std::error::Error>> {
    let mut providers = claude::runtimes(storage.clone(), pricing.clone())?;
    providers.extend(vec![
        Arc::new(CodexProvider::new(storage.clone(), pricing.clone())?) as Arc<dyn UsageProvider>,
        Arc::new(CommandCodeProvider::new()?) as Arc<dyn UsageProvider>,
        Arc::new(CursorProvider::new(pricing.clone())?) as Arc<dyn UsageProvider>,
        Arc::new(AntigravityProvider::new(
            app_data_dir.join("antigravity").join("auth.json"),
        )?) as Arc<dyn UsageProvider>,
        Arc::new(CopilotProvider::new()?) as Arc<dyn UsageProvider>,
        Arc::new(DevinProvider::new()?) as Arc<dyn UsageProvider>,
        Arc::new(GrokProvider::new(storage.clone(), pricing.clone())?) as Arc<dyn UsageProvider>,
        Arc::new(OpenRouterProvider::new()?) as Arc<dyn UsageProvider>,
        Arc::new(ZaiProvider::new()?) as Arc<dyn UsageProvider>,
        Arc::new(KimiProvider::new()?) as Arc<dyn UsageProvider>,
        Arc::new(MiniMaxProvider::new()?) as Arc<dyn UsageProvider>,
    ]);
    providers.extend(OpenCodeProvider::runtimes(pricing.clone(), &storage)?);
    providers.extend(crate::providers::api_key_account::api_key_account_providers(storage)?);
    Ok(Arc::new(ProviderRegistry::new(providers)?))
}

/// Applies the persisted panel surface and (for standalone installs) the
/// window mode, and registers the saved global shortcut.
fn apply_initial_window_state(
    app: &App,
    desktop_integration: &DesktopIntegration,
    settings: &Arc<SettingsService>,
    floating_window: bool,
) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if window::apply_panel_surface(&window, settings.get().theme).is_err() {
            app_warn!("window", "initial panel surface theme could not be applied");
        }
        if !floating_window {
            webview_memory::set_inactive(&window, true);
        }
    }

    if let Some(shortcut) = settings.get().global_shortcut.clone() {
        let _ = register_shortcut(app.handle(), &shortcut);
    }

    if desktop_integration.is_floating() {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            if let Err(error) = window::apply_window_mode(&window, settings.get().window_mode, true)
            {
                app_warn!(
                    "window",
                    "standalone startup mode could not be applied: {error}"
                );
                show_standalone_window_fallback(&window);
            }
        }
    }
}

/// Installs the tray when the desktop supports one, degrading to the
/// standalone window (and watching for StatusNotifier loss on Linux) when it
/// does not. Returns whether the tray is active.
fn install_tray_with_fallback(app: &mut App, desktop_integration: &DesktopIntegration) -> bool {
    let tray_installed = if desktop_integration.tray_available() {
        match install_tray(app) {
            Ok(()) => {
                app_info!("lifecycle", "system tray integration ready");
                true
            }
            Err(error) => {
                app_warn!(
                    "lifecycle",
                    "system tray integration failed; using standalone window: {error}"
                );
                desktop_integration.disable_tray();
                let _ = app.remove_tray_by_id("usagedeck-tray");
                false
            }
        }
    } else {
        false
    };

    #[cfg(target_os = "linux")]
    if tray_installed {
        spawn_status_notifier_monitor(app.handle().clone());
    }

    tray_installed
}

fn spawn_startup_credential_detection(
    app: AppHandle,
    registry: Arc<ProviderRegistry>,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
    plan: CredentialDetectionPlan,
) {
    tauri::async_runtime::spawn(async move {
        app_info!("config", "startup credential detection began");
        let detected = detect_local_credentials(registry, plan.provider_ids()).await;
        let command_guard = settings.lock_command_mutation().await;
        let Ok(outcome) = settings.apply_credential_detection(&plan, &detected) else {
            app_error!(
                "config",
                "startup credential detection could not be applied"
            );
            return;
        };
        app_info!(
            "config",
            "startup credential detection completed ({} detected, {} newly enabled)",
            detected
                .values()
                .filter(|status| { **status == providers::CredentialProbeStatus::Detected })
                .count(),
            outcome.newly_enabled_provider_ids.len()
        );

        tray_presentation::update(
            &app,
            &service.state(),
            &outcome.settings,
            settings.registry(),
        );
        let _ = app.emit(
            "settings-state",
            commands::settings::settings_view_state(&app, &settings),
        );
        drop(command_guard);
        if outcome.newly_enabled_provider_ids.is_empty() {
            return;
        }
        commands::usage::refresh_with_events(
            &app,
            &service,
            &settings,
            &notifications,
            &outcome.newly_enabled_provider_ids,
            true,
            false,
        )
        .await;
    });
}

fn register_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Released {
                toggle_main_window(app);
            }
        })
        .map_err(|_| {
            crate::app_warn!("config", "global shortcut registration failed");
            "That global shortcut is invalid or already in use.".to_owned()
        })
}

pub(crate) fn apply_shortcut_change(
    app: &AppHandle,
    previous: Option<&str>,
    next: Option<&str>,
) -> Result<(), String> {
    if previous == next {
        return Ok(());
    }
    if let Some(previous) = previous {
        let _ = app.global_shortcut().unregister(previous);
    }
    if let Some(next) = next.filter(|shortcut| !shortcut.trim().is_empty()) {
        if let Err(error) = register_shortcut(app, next) {
            if let Some(previous) = previous {
                let _ = register_shortcut(app, previous);
            }
            return Err(error);
        }
    }
    crate::app_debug!("config", "global shortcut configuration updated");
    Ok(())
}

pub(crate) fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let result = {
        let _ = app;
        xdg_autostart::set_enabled(enabled)
    };
    #[cfg(not(target_os = "linux"))]
    let result = {
        let manager = app.autolaunch();
        if enabled {
            manager.enable()
        } else {
            manager.disable()
        }
    };
    result
        .map(|_| {
            // Keep the probe cache in step so settings emissions stop re-reading
            // the OS registration (a registry / LaunchAgents / XDG file probe).
            *AUTOSTART_CACHE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(enabled);
        })
        .map_err(|_| "Launch at login could not be updated.".to_owned())
}

/// Cached autostart registration. `autostart_is_enabled` is called for every
/// settings emission and a handful of commands; the underlying OS probe hits
/// the registry, a LaunchAgents file, or an XDG directory each time, so the
/// last answer is cached and invalidated by `set_autostart`. External changes
/// (toggled from the OS itself) surface after the next in-app toggle or restart.
static AUTOSTART_CACHE: Mutex<Option<bool>> = Mutex::new(None);

pub(crate) fn autostart_is_enabled(app: &AppHandle) -> Result<bool, ()> {
    let mut cache = AUTOSTART_CACHE.lock().map_err(|_| ())?;
    if let Some(enabled) = *cache {
        return Ok(enabled);
    }
    #[cfg(target_os = "linux")]
    let enabled = {
        // The XDG probe reads the autostart file directly; consume the app
        // handle so Linux builds stay clean under -D warnings.
        let _ = app;
        xdg_autostart::is_enabled()
    };
    #[cfg(not(target_os = "linux"))]
    let enabled = app.autolaunch().is_enabled();
    // Errors stay uncached: a transient probe failure deserves a retry on the
    // next emission rather than a permanently sticky value.
    enabled
        .inspect(|enabled| {
            *cache = Some(*enabled);
        })
        .map_err(|_| ())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        window::activate_existing_instance(app);
    }));

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(PopupDismissGuard::default())
        .manage(updates::UpdateCoordinator::default())
        .setup(|app| {
            #[cfg(target_os = "linux")]
            tray_presentation::init_main_thread_id();
            logging::init(logging::default_log_path(), models::LogLevel::Info);

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            app.handle().plugin(tauri_plugin_positioner::init())?;
            let desktop_integration = DesktopIntegration::detect();
            app_info!(
                "lifecycle",
                "desktop integration detected (tray={})",
                desktop_integration.tray_available()
            );
            app.manage(desktop_integration.clone());

            let app_data_dir = app.path().app_data_dir()?;
            migrate_legacy_data(&app_data_dir)?;
            let storage = open_application_storage(app, &app_data_dir)?;
            let pricing = Arc::new(PricingStore::new(app_data_dir.join("pricing"))?);
            let registry = build_provider_registry(&app_data_dir, &storage, &pricing)?;
            let (settings_service, credential_detection_plan) =
                SettingsService::new_deferred(storage.clone(), registry.clone())?;
            let settings = Arc::new(settings_service);
            let floating_window = desktop_integration.apply_window_mode(settings.get().window_mode);
            let service = Arc::new(ProviderService::new_with_settings(
                registry.clone(),
                storage.clone(),
                settings.clone(),
            ));
            logging::set_level(settings.get().log_level);
            app_info!(
                "config",
                "UsageDeck v{} starting (level={}, log=UsageDeck.log)",
                app.package_info().version,
                logging::current_level().log_label()
            );
            let notifications = Arc::new(NotificationEvaluator::default());
            app.manage(registry.clone());
            app.manage(service.clone());
            app.manage(settings.clone());
            app.manage(notifications.clone());
            app.manage(Arc::new(CodexResetClaimService::new()?));

            apply_initial_window_state(app, &desktop_integration, &settings, floating_window);
            install_tray_with_fallback(app, &desktop_integration);

            tray_presentation::update(
                app.handle(),
                &service.state(),
                &settings.get(),
                settings.registry(),
            );
            spawn_startup_credential_detection(
                app.handle().clone(),
                registry,
                service.clone(),
                settings.clone(),
                notifications.clone(),
                credential_detection_plan,
            );
            refresh_loop::spawn(app.handle().clone(), service, settings, notifications);
            app_info!("lifecycle", "UsageDeck startup completed");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap::get_bootstrap_state,
            commands::provider::open_provider_link,
            commands::provider::get_provider_api_key_state,
            commands::provider::save_provider_api_key,
            commands::provider::delete_provider_api_key,
            commands::provider::add_api_key_account,
            commands::provider::remove_api_key_account,
            commands::usage::refresh_usage,
            commands::usage::refresh_provider_usage,
            commands::usage::claim_codex_reset_credit,
            commands::usage::quota_history,
            commands::settings::get_app_settings,
            commands::settings::save_app_settings,
            commands::settings::record_update_check,
            commands::settings::reset_customization,
            commands::settings::reset_all_settings,
            commands::settings::reset_provider_customization,
            commands::settings::request_notification_permission,
            commands::settings::open_notification_settings,
            commands::settings::get_log_path,
            commands::settings::open_log_folder,
            commands::window::dismiss_main_window,
            commands::window::get_panel_resize_edge,
            commands::window::get_panel_height_mode,
            commands::window::fit_panel_to_content,
            commands::window::set_panel_height_automatic,
            commands::window::set_panel_height_manual,
            commands::window::begin_panel_resize,
            commands::window::lock_panel_resize_axis,
            commands::window::current_panel_width,
            commands::window::set_panel_width,
            commands::window::quit_app,
            updates::check_for_updates,
            updates::install_update,
            updates::open_update_page
        ])
        .on_window_event(handle_window_event)
        .run(tauri::generate_context!())
        .expect("error while running UsageDeck");
}
