use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::{
    child_process,
    commands::usage::refresh_with_events,
    pacing::NotificationEvaluator,
    providers::{ProviderRegistry, SessionKickstart},
    service::{ProviderService, UsageViewState},
    settings::SettingsService,
    AppHandle,
};

/// One tiny prompt per provider per cooldown window: a success starts a fresh
/// multi-hour session, and a failure (CLI missing, not logged in) should not
/// turn into a spawn storm on every 5-minute refresh.
const ATTEMPT_COOLDOWN: Duration = Duration::from_secs(30 * 60);
/// Session CLIs can take a while to boot (auth, network, model latency).
const KICKSTART_TIMEOUT: Duration = Duration::from_secs(180);
/// The kick fires this long after the published reset time, so the provider
/// backend has actually rolled the window before the prompt lands.
const RESET_GRACE: Duration = Duration::from_secs(60);
/// An already-armed timer within this slack of the new target is reused
/// instead of torn down and re-armed on every refresh.
const RESCHEDULE_SLACK: chrono::Duration = chrono::Duration::minutes(1);

static LAST_ATTEMPTS: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// One armed reset timer per provider: the task handle (so a reschedule can
/// abort it) and the reset time it targets.
type ScheduledTask = (tauri::async_runtime::JoinHandle<()>, DateTime<Utc>);

static SCHEDULED: LazyLock<Mutex<HashMap<String, ScheduledTask>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn record_attempt(provider_id: &str) -> bool {
    let mut attempts = LAST_ATTEMPTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    if attempts
        .get(provider_id)
        .is_some_and(|last| now.duration_since(*last) < ATTEMPT_COOLDOWN)
    {
        return false;
    }
    attempts.insert(provider_id.to_owned(), now);
    true
}

fn cancel_scheduled(provider_id: &str) {
    if let Some((task, _)) = SCHEDULED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(provider_id)
    {
        task.abort();
    }
}

/// The provider's session-window source ids, derived from its metric
/// definitions in the catalog.
fn session_source_ids(registry: &ProviderRegistry, provider_id: &str) -> HashSet<String> {
    let Some(definition) = registry.definition(provider_id) else {
        return HashSet::new();
    };
    definition
        .metrics
        .iter()
        .filter(|metric| metric.source.session_window())
        .filter_map(|metric| metric.source.source_id())
        .map(str::to_owned)
        .collect()
}

/// A session window is active while its reset time is still in the future;
/// once every session window is missing or past its reset, the session has
/// rolled over and a kickstart would begin a fresh one.
fn session_window_active(
    snapshot: &crate::models::ProviderSnapshot,
    session_ids: &HashSet<String>,
) -> bool {
    let now = Utc::now();
    snapshot.quotas.iter().any(|window| {
        session_ids.contains(&window.id) && window.resets_at.is_some_and(|reset| reset > now)
    })
}

fn active_session_reset(
    snapshot: &crate::models::ProviderSnapshot,
    session_ids: &HashSet<String>,
) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    snapshot
        .quotas
        .iter()
        .filter(|window| session_ids.contains(&window.id))
        .filter_map(|window| window.resets_at)
        .filter(|reset| *reset > now)
        .min()
}

/// Decides and runs session kickstarts after a refresh batch. Two tiers:
///
/// - Session still active: arm (or reuse) a timer at its reset time plus a
///   short grace, so the next window starts right when the current one
///   expires instead of waiting for the next refresh tick.
/// - Session already rolled over: cancel any timer, kick now (cooldown
///   permitting), and refresh so the UI shows the new window immediately.
///
/// The refresh-tail path stays as the safety net either way: system sleep can
/// skew timers, and any later refresh re-evaluates from fresh data.
pub async fn evaluate(
    app: &AppHandle,
    state: &UsageViewState,
    settings: &Arc<SettingsService>,
    registry: &Arc<ProviderRegistry>,
    service: &Arc<ProviderService>,
    notifications: &Arc<NotificationEvaluator>,
) {
    let current_settings = settings.get();
    if current_settings.kickstart_provider_ids.is_empty() {
        return;
    }
    let targets = current_settings
        .providers
        .iter()
        .filter(|layout| {
            layout.enabled && current_settings.kickstart_provider_ids.contains(&layout.id)
        })
        .map(|layout| layout.id.clone())
        .collect::<Vec<_>>();

    for provider_id in targets {
        let Some(kickstart) = registry
            .runtime(&provider_id)
            .and_then(|runtime| runtime.session_kickstart())
        else {
            continue;
        };
        let Some(provider_state) = state.providers.get(&provider_id) else {
            continue;
        };
        // Without a successful snapshot we do not know the window state; also
        // never prompt a provider whose credentials are broken.
        let Some(snapshot) = provider_state.snapshot.as_ref() else {
            continue;
        };
        if matches!(
            provider_state.error_kind,
            Some(crate::models::ProviderErrorKind::Authentication)
        ) {
            continue;
        }
        let session_ids = session_source_ids(registry, &provider_id);

        if let Some(reset_at) = active_session_reset(snapshot, &session_ids) {
            // Still inside a live window: aim the timer at its reset.
            schedule_at_reset(
                provider_id.clone(),
                reset_at,
                app.clone(),
                service.clone(),
                settings.clone(),
                registry.clone(),
                notifications.clone(),
            );
            continue;
        }

        // The session has rolled over: a scheduled timer is obsolete — kick
        // now if the cooldown allows.
        cancel_scheduled(&provider_id);
        if !record_attempt(&provider_id) {
            continue;
        }

        crate::app_info!("kickstart", "session rolled over; starting a fresh window");
        if let Err(error) = send_prompt(&kickstart).await {
            crate::app_warn!("kickstart", "{provider_id} kickstart failed: {error}");
            continue;
        }
        crate::app_info!("kickstart", "{provider_id} session window restarted");

        // Show the brand-new window right away instead of waiting for the
        // next scheduled batch. The nested evaluation is a no-op: the window
        // is now active.
        // Boxed: kickstart -> refresh -> kickstart recursion is finite (the
        // fresh window suppresses the nested evaluation) but async recursion
        // still needs boxing for the compiler.
        Box::pin(refresh_with_events(
            app,
            service,
            settings,
            notifications,
            std::slice::from_ref(&provider_id),
            true,
            false,
        ))
        .await;
    }
}

/// Arms the reset-time timer for one provider, replacing any timer that aims
/// at a different time. Dedupes within a minute of slack so the five-minute
/// refresh cadence does not tear down and respawn the task on every batch.
fn schedule_at_reset(
    provider_id: String,
    target: DateTime<Utc>,
    app: AppHandle,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    registry: Arc<ProviderRegistry>,
    notifications: Arc<NotificationEvaluator>,
) {
    let mut scheduled = SCHEDULED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, existing_target)) = scheduled.get(&provider_id) {
        if (*existing_target - target).abs() < RESCHEDULE_SLACK {
            return;
        }
    }
    if let Some((task, _)) = scheduled.remove(&provider_id) {
        task.abort();
    }
    let delay = (target - Utc::now()).to_std().unwrap_or_default() + RESET_GRACE;
    let task_provider_id = provider_id.clone();
    let task = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        SCHEDULED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&task_provider_id);
        run_scheduled_kick(
            task_provider_id,
            app,
            service,
            settings,
            registry,
            notifications,
        )
        .await;
    });
    scheduled.insert(provider_id, (task, target));
}

/// The timer fired: re-derive everything from fresh state — the user may have
/// already started a new window themselves, opted the provider out, or logged
/// out while the timer slept.
async fn run_scheduled_kick(
    provider_id: String,
    app: AppHandle,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    registry: Arc<ProviderRegistry>,
    notifications: Arc<NotificationEvaluator>,
) {
    let current_settings = settings.get();
    let opted_in = current_settings
        .kickstart_provider_ids
        .contains(&provider_id)
        && current_settings
            .providers
            .iter()
            .any(|layout| layout.id == provider_id && layout.enabled);
    if !opted_in {
        return;
    }
    let Some(kickstart) = registry
        .runtime(&provider_id)
        .and_then(|runtime| runtime.session_kickstart())
    else {
        return;
    };
    let state = service.state();
    let Some(provider_state) = state.providers.get(&provider_id) else {
        return;
    };
    let Some(snapshot) = provider_state.snapshot.as_ref() else {
        return;
    };
    if matches!(
        provider_state.error_kind,
        Some(crate::models::ProviderErrorKind::Authentication)
    ) {
        return;
    }
    let session_ids = session_source_ids(&registry, &provider_id);
    if session_window_active(snapshot, &session_ids) {
        // The user already sent a real message; nothing to restart.
        return;
    }
    if !record_attempt(&provider_id) {
        return;
    }

    crate::app_info!("kickstart", "reset time reached; starting a fresh window");
    if let Err(error) = send_prompt(&kickstart).await {
        crate::app_warn!("kickstart", "{provider_id} kickstart failed: {error}");
        return;
    }
    crate::app_info!("kickstart", "{provider_id} session window restarted");
    Box::pin(refresh_with_events(
        &app,
        &service,
        &settings,
        &notifications,
        std::slice::from_ref(&provider_id),
        true,
        false,
    ))
    .await;
}

/// Runs the kickstart command through the user's login shell: the app process
/// does not inherit the login PATH where `claude`/`codex` live, and the
/// prompt/arguments are compile-time constants, so simple shell joining is
/// sufficient.
async fn send_prompt(kickstart: &SessionKickstart) -> Result<(), String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_owned()
        } else {
            "/bin/sh".to_owned()
        }
    });
    let script = format!("{} {}", kickstart.program, kickstart.args.join(" "));
    let mut command = child_process::background_command(&shell);
    command.args(["-l", "-c", &script]);
    let output = tokio::task::spawn_blocking(move || {
        child_process::output_with_timeout(&mut command, KICKSTART_TIMEOUT)
    })
    .await
    .map_err(|error| format!("kickstart task failed: {error}"))?
    .map_err(|error| format!("could not run {}: {error}", kickstart.program))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().next().unwrap_or_default();
        return Err(format!(
            "{} exited with status {}{}",
            kickstart.program,
            output.status,
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ));
    }
    Ok(())
}
