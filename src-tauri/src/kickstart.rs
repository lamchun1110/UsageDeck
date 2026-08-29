use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::{
    child_process,
    commands::usage::refresh_with_events,
    pacing::NotificationEvaluator,
    providers::ProviderRegistry,
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

/// The shell command that kickstarts one provider: the user's custom command
/// when set, otherwise the provider's built-in CLI invocation. Returns `None`
/// for providers without session windows — the expiry the evaluator fires on —
/// or without any usable command.
fn resolve_command(
    provider_id: &str,
    kickstart_commands: &std::collections::BTreeMap<String, String>,
    registry: &ProviderRegistry,
) -> Option<String> {
    if !is_session_capable(registry, provider_id) {
        return None;
    }
    if let Some(custom) = kickstart_commands.get(provider_id) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    registry
        .runtime(provider_id)
        .and_then(|runtime| runtime.session_kickstart())
        .map(|kickstart| format!("{} {}", kickstart.program, kickstart.args.join(" ")))
}

/// Every provider with a first-message rolling session publishes it as the
/// quota window with source id "session" (claude, codex, zai, kimi, minimax,
/// opencode, commandcode). Matching by that convention keeps the kickstart
/// trigger independent of the `sessionWindow` UI flag, which only claude sets.
fn is_session_capable(registry: &ProviderRegistry, provider_id: &str) -> bool {
    registry.definition(provider_id).is_some_and(|definition| {
        definition.metrics.iter().any(|metric| {
            matches!(
                metric.source,
                crate::models::MetricSource::Quota {
                    source_id: _,
                    session_window: _,
                } | crate::models::MetricSource::QuotaOrValue {
                    source_id: _,
                    session_window: _,
                }
            ) && metric.source.source_id() == Some("session")
        })
    })
}

/// A session window is active while its reset time is still in the future;
/// once every session window is missing or past its reset, the session has
/// rolled over and a kickstart would begin a fresh one.
fn session_window_active(snapshot: &crate::models::ProviderSnapshot) -> bool {
    let now = Utc::now();
    snapshot
        .quotas
        .iter()
        .any(|window| window.id == "session" && window.resets_at.is_some_and(|reset| reset > now))
}

fn active_session_reset(snapshot: &crate::models::ProviderSnapshot) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    snapshot
        .quotas
        .iter()
        .filter(|window| window.id == "session")
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
/// Returns whether a kickstart ran a follow-up refresh, so the caller can
/// return the fresher service state instead of its stale pre-tail snapshot.
pub async fn evaluate(
    app: &AppHandle,
    state: &UsageViewState,
    settings: &Arc<SettingsService>,
    registry: &Arc<ProviderRegistry>,
    service: &Arc<ProviderService>,
    notifications: &Arc<NotificationEvaluator>,
) -> bool {
    let mut refreshed = false;
    let current_settings = settings.get();
    if current_settings.kickstart_provider_ids.is_empty() {
        return false;
    }
    let targets = current_settings
        .providers
        .iter()
        .filter(|layout| {
            layout.enabled && current_settings.kickstart_provider_ids.contains(&layout.id)
        })
        .map(|layout| layout.id.clone())
        .collect::<Vec<_>>();

    // A provider that left the target set (opted out, disabled, or lost
    // eligibility) must not keep an armed reset timer; the fired task would
    // no-op, but cancelling is cheaper and clearer.
    {
        let mut scheduled = SCHEDULED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let orphaned: Vec<String> = scheduled
            .keys()
            .filter(|id| !targets.contains(id))
            .cloned()
            .collect();
        for id in orphaned {
            if let Some((task, _)) = scheduled.remove(&id) {
                task.abort();
            }
        }
    }

    for provider_id in targets {
        let Some(kickstart_command) =
            resolve_command(&provider_id, &current_settings.kickstart_commands, registry)
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
        if let Some(reset_at) = active_session_reset(snapshot) {
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
        if let Err(error) = send_prompt(&kickstart_command).await {
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
        refreshed = true;
    }
    refreshed
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
    let Some(kickstart_command) = resolve_command(
        &provider_id,
        &current_settings.kickstart_commands,
        &registry,
    ) else {
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
    if session_window_active(snapshot) {
        // The user already sent a real message; nothing to restart.
        return;
    }
    if !record_attempt(&provider_id) {
        return;
    }

    crate::app_info!("kickstart", "reset time reached; starting a fresh window");
    if let Err(error) = send_prompt(&kickstart_command).await {
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
/// does not inherit the login PATH where `claude`/`codex` live. Built-in
/// scripts join compile-time constants; custom commands are the user's own
/// shell input, equivalent to running them in a terminal.
async fn send_prompt(script: &str) -> Result<(), String> {
    // Unix: the login shell resolves the CLI the way the user's terminal
    // would (the app process lacks that PATH). Windows has no SHELL in GUI
    // sessions; cmd.exe resolves npm-global CLIs from the user PATH.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = child_process::background_command("cmd.exe");
        command.args(["/C", script]);
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_owned()
            } else {
                "/bin/sh".to_owned()
            }
        });
        let mut command = child_process::background_command(&shell);
        command.args(["-l", "-c", script]);
        command
    };
    let output = tokio::task::spawn_blocking(move || {
        child_process::output_with_timeout(&mut command, KICKSTART_TIMEOUT)
    })
    .await
    .map_err(|error| format!("kickstart task failed: {error}"))?
    .map_err(|error| format!("could not run the kickstart command: {error}"))?;
    if !output.status.success() {
        // A custom command may embed secrets in its output; the redaction
        // layer cannot know arbitrary token shapes, so only the exit status
        // reaches the persistent log.
        crate::app_debug!(
            "kickstart",
            "kickstart command stderr: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or_default()
        );
        return Err(format!(
            "the kickstart command exited with status {}",
            output.status
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{is_session_capable, session_window_active, LAST_ATTEMPTS};
    use crate::models::{
        MetricDefinition, MetricSection, MetricSource, ProviderDefinition, ProviderSnapshot,
        QuotaWindow, UsageHistory,
    };

    fn definition_with_session_metric(id: &str) -> ProviderDefinition {
        ProviderDefinition {
            id: id.into(),
            display_name: "Provider".into(),
            short_name: "P".into(),
            fallback_enabled: false,
            local_usage_source_note: None,
            links: vec![],
            options: Vec::new(),
            metrics: vec![MetricDefinition::new(
                format!("{id}.session"),
                "Session",
                MetricSource::Quota {
                    source_id: "session".into(),
                    session_window: true,
                },
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                Some("S"),
                None,
            )],
        }
    }

    fn snapshot_with_session_reset(reset: Option<chrono::DateTime<Utc>>) -> ProviderSnapshot {
        ProviderSnapshot {
            provider_id: "claude".into(),
            plan: None,
            quotas: vec![QuotaWindow {
                id: "session".into(),
                label: "Session".into(),
                used_percent: 0.0,
                resets_at: reset,
                period_seconds: 5 * 3600,
                format: Default::default(),
                used_value: None,
                limit_value: None,
                unit: None,
                estimated: false,
                source_note: None,
            }],
            value_metrics: vec![],
            status_metrics: vec![],
            notices: vec![],
            usage: UsageHistory::default(),
            warnings: vec![],
            refreshed_at: Utc::now(),
        }
    }

    #[test]
    fn session_window_is_active_only_until_its_reset() {
        let mut definition = definition_with_session_metric("claude");
        definition.fallback_enabled = true;
        let registry =
            crate::providers::ProviderRegistry::from_definitions(vec![definition]).unwrap();
        assert!(is_session_capable(&registry, "claude"));

        let future = Utc::now() + chrono::Duration::hours(2);
        assert!(session_window_active(&snapshot_with_session_reset(Some(
            future
        ))));

        let past = Utc::now() - chrono::Duration::hours(1);
        assert!(!session_window_active(&snapshot_with_session_reset(Some(
            past
        ))));
        assert!(!session_window_active(&snapshot_with_session_reset(None)));
        // A weekly window resetting in the future does not make the session active.
        let mut snapshot = snapshot_with_session_reset(None);
        snapshot.quotas[0].id = "weekly".into();
        snapshot.quotas[0].resets_at = Some(future);
        assert!(!session_window_active(&snapshot));
        // A differently-named provider without a "session" metric is not capable.
        assert!(!is_session_capable(&registry, "codex"));
    }

    #[test]
    fn attempts_respect_the_cooldown() {
        {
            let mut attempts = LAST_ATTEMPTS.lock().unwrap();
            attempts.clear();
        }

        assert!(super::record_attempt("claude"));
        assert!(!super::record_attempt("claude"));
        assert!(super::record_attempt("codex"));
    }

    struct BuiltinKickstartProvider(ProviderDefinition);

    impl crate::providers::UsageProvider for BuiltinKickstartProvider {
        fn definition(&self) -> ProviderDefinition {
            self.0.clone()
        }

        fn has_local_credentials(&self) -> bool {
            false
        }

        fn session_kickstart(&self) -> Option<crate::providers::SessionKickstart> {
            Some(crate::providers::SessionKickstart::new(
                "claude",
                &["-p", "Hi"],
            ))
        }

        fn refresh(&self) -> Result<ProviderSnapshot, crate::providers::ProviderError> {
            unreachable!()
        }
    }

    #[test]
    fn resolve_command_prefers_custom_and_gates_on_session_windows() {
        let mut session = definition_with_session_metric("claude");
        session.fallback_enabled = true;
        let mut plain = definition_with_session_metric("cursor");
        plain.metrics[0].source = MetricSource::Value {
            source_id: "usage".into(),
        };
        plain.metrics[0].id = "cursor.usage".into();
        plain.fallback_enabled = true;
        struct PlainProvider(ProviderDefinition);

        impl crate::providers::UsageProvider for PlainProvider {
            fn definition(&self) -> ProviderDefinition {
                self.0.clone()
            }

            fn has_local_credentials(&self) -> bool {
                false
            }

            fn refresh(&self) -> Result<ProviderSnapshot, crate::providers::ProviderError> {
                unreachable!()
            }
        }

        let registry = crate::providers::ProviderRegistry::new(vec![
            std::sync::Arc::new(BuiltinKickstartProvider(session)),
            std::sync::Arc::new(PlainProvider(plain)),
        ])
        .unwrap();

        let commands = std::collections::BTreeMap::from([(
            "claude".to_owned(),
            "claude -p hi --model haiku".to_owned(),
        )]);

        // Custom command wins over the built-in.
        assert_eq!(
            super::resolve_command("claude", &commands, &registry).as_deref(),
            Some("claude -p hi --model haiku")
        );
        // Empty custom falls back to the built-in.
        assert_eq!(
            super::resolve_command(
                "claude",
                &std::collections::BTreeMap::from([("claude".to_owned(), "   ".to_owned())]),
                &registry,
            )
            .map(|script| script.starts_with("claude ")),
            Some(true)
        );
        // Built-in when no custom entry exists.
        assert_eq!(
            super::resolve_command("claude", &std::collections::BTreeMap::new(), &registry)
                .map(|script| script.starts_with("claude ")),
            Some(true)
        );
        // Providers without session windows are never kickstartable.
        assert_eq!(super::resolve_command("cursor", &commands, &registry), None);
    }
}
