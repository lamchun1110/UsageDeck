use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

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

static LAST_ATTEMPTS: LazyLock<Mutex<HashMap<String, Instant>>> =
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
    let now = chrono::Utc::now();
    snapshot.quotas.iter().any(|window| {
        session_ids.contains(&window.id) && window.resets_at.is_some_and(|reset| reset > now)
    })
}

/// Decides and runs session kickstarts after a refresh batch: for every
/// enabled provider opted into the feature whose session has rolled over and
/// is not in its cooldown, send the provider's tiny kickstart prompt, then
/// refresh so the UI shows the new window immediately.
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
        if session_window_active(snapshot, &session_ids) {
            continue;
        }
        if !record_attempt(&provider_id) {
            continue;
        }

        crate::app_info!("kickstart", "session rolled over; starting a fresh window");
        match send_prompt(&kickstart).await {
            Ok(()) => {
                crate::app_info!("kickstart", "{provider_id} session window restarted");
            }
            Err(error) => {
                crate::app_warn!("kickstart", "{provider_id} kickstart failed: {error}");
                continue;
            }
        }

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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{session_source_ids, session_window_active, LAST_ATTEMPTS};
    use crate::models::{
        MetricDefinition, MetricSection, MetricSource, ProviderDefinition, ProviderSnapshot,
        QuotaWindow,
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
            usage: crate::models::UsageHistory::default(),
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
        let ids = session_source_ids(&registry, "claude");
        assert!(ids.contains("session"));

        let future = Utc::now() + chrono::Duration::hours(2);
        assert!(session_window_active(
            &snapshot_with_session_reset(Some(future)),
            &ids
        ));

        let past = Utc::now() - chrono::Duration::hours(1);
        assert!(!session_window_active(
            &snapshot_with_session_reset(Some(past)),
            &ids
        ));
        assert!(!session_window_active(
            &snapshot_with_session_reset(None),
            &ids
        ));
        // A weekly window resetting in the future does not make the session active.
        let mut snapshot = snapshot_with_session_reset(None);
        snapshot.quotas[0].id = "weekly".into();
        snapshot.quotas[0].resets_at = Some(future);
        assert!(!session_window_active(&snapshot, &ids));
    }

    #[test]
    fn attempts_respect_the_cooldown() {
        let mut attempts = LAST_ATTEMPTS.lock().unwrap();
        attempts.clear();
        drop(attempts);

        assert!(super::record_attempt("claude"));
        assert!(!super::record_attempt("claude"));
        assert!(super::record_attempt("codex"));
    }
}
