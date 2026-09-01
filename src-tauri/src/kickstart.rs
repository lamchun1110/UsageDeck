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

/// After this many consecutive kicks that failed to produce the scoped
/// windows, auto-kickstart suspends for that provider until a window is
/// observed alive again — a circuit breaker against prompts that run
/// cleanly but renew nothing: a custom command targeting the wrong
/// account, a plan that never reports the window, or a CLI flag that
/// stops opening one.
const INEFFECTIVE_KICK_LIMIT: u32 = 2;

/// Kick outcomes per provider: kicks charged as ineffective until a live
/// window clears them, plus whether the suspension has been announced so it
/// is logged once rather than on every refresh.
#[derive(Default)]
struct KickOutcomes {
    failures: HashMap<String, u32>,
    suspension_logged: std::collections::HashSet<String>,
}

impl KickOutcomes {
    /// A live scoped window clears the streak: whatever produced it, kicking
    /// is worth trying again.
    fn observe_alive(&mut self, provider_id: &str) {
        self.failures.remove(provider_id);
        self.suspension_logged.remove(provider_id);
    }

    /// Whether auto-kickstart should refuse to fire. The first refusal is
    /// announced so the operator can find the suspension in the log.
    fn refuses(&mut self, provider_id: &str) -> bool {
        let failures = self.failures.get(provider_id).copied().unwrap_or(0);
        if failures < INEFFECTIVE_KICK_LIMIT {
            return false;
        }
        if self.suspension_logged.insert(provider_id.to_owned()) {
            crate::app_warn!(
                "kickstart",
                "{provider_id} auto-kickstart suspended: the last {failures} kicks did not start the window"
            );
        }
        true
    }

    /// A kick is charged as ineffective until the next live window clears it.
    fn record_kick(&mut self, provider_id: &str) {
        *self.failures.entry(provider_id.to_owned()).or_insert(0) += 1;
    }

    /// Drops bookkeeping for providers that are no longer kickstart targets.
    fn retain(&mut self, targets: &[String]) {
        self.failures
            .retain(|provider_id, _| targets.contains(provider_id));
        self.suspension_logged
            .retain(|provider_id| targets.contains(provider_id));
    }
}

static KICK_OUTCOMES: LazyLock<Mutex<KickOutcomes>> =
    LazyLock::new(|| Mutex::new(KickOutcomes::default()));

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
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedCommand {
    Custom(String),
    BuiltIn {
        program: String,
        args: Vec<String>,
        envs: Vec<(String, String)>,
    },
}

/// How long the login-shell PATH probe may take before it is abandoned. A
/// profile that needs longer (heavy nvm setups) would also delay every
/// kickstart spawn, so the bound doubles as a health check.
#[cfg(not(target_os = "windows"))]
const PROGRAM_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Resolved built-in programs, remembered for the session so the probe and the
/// root scans run at most once per CLI per launch.
static RESOLVED_PROGRAMS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Built-in CLIs are resolved in tiers before falling back to the bare program
/// name, because a GUI app's login-shell PATH covers fewer locations than an
/// interactive terminal: (1) ask the same login shell that will run the
/// kickstart — whatever `command -v` finds there is exactly what the kick can
/// execute, covering every install method the user's own profile exposes;
/// (2) well-known user-local install roots, covering PATH edits that live in
/// interactive-only rc files (the native `claude` installer edits `.zshrc`,
/// which `zsh -l -c` never loads); (3) the bare name, resolved by the shell at
/// kick time. Custom commands are untouched — the user's shell performs its
/// own resolution.
fn resolve_builtin_program(program: &str) -> String {
    if program.contains('/') {
        return program.to_owned();
    }
    let mut resolved_cache = RESOLVED_PROGRAMS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(resolved) = resolved_cache.get(program) {
        return resolved.clone();
    }
    let resolved = resolve_builtin_program_with(
        program,
        Some(crate::providers::home_directory().as_path()),
        probe_login_path,
    );
    resolved_cache.insert(program.to_owned(), resolved.clone());
    resolved
}

fn resolve_builtin_program_with(
    program: &str,
    home: Option<&std::path::Path>,
    probe: impl Fn(&str) -> Option<String>,
) -> String {
    if let Some(found) = probe(program) {
        // A shell builtin or alias name is not a path; only an absolute hit is
        // usable as the kickstart program.
        if found.starts_with('/') {
            return found;
        }
    }
    resolve_builtin_program_in(program, home)
}

fn resolve_builtin_program_in(program: &str, home: Option<&std::path::Path>) -> String {
    if program.contains('/') {
        return program.to_owned();
    }
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = home {
        candidates.push(home.join(".local").join("bin").join(program));
    }
    candidates.push(std::path::PathBuf::from("/opt/homebrew/bin").join(program));
    candidates.push(std::path::PathBuf::from("/usr/local/bin").join(program));
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.display().to_string())
        .unwrap_or_else(|| program.to_owned())
}

/// The shell every kickstart invocation and PATH probe runs through: the app
/// process does not inherit the login environment where the CLIs live.
#[cfg(not(target_os = "windows"))]
fn login_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_owned()
        } else {
            "/bin/sh".to_owned()
        }
    })
}

/// Asks the login shell to resolve `program` exactly as the kickstart spawn
/// will, with a deadline so a wedged profile cannot stall the refresh tail.
/// Only definitive answers count: a timeout or a non-absolute hit leaves
/// resolution to the static tiers, and an unanswered probe is not cached.
#[cfg(not(target_os = "windows"))]
fn probe_login_path(program: &str) -> Option<String> {
    let script = format!("command -v {program}");
    let mut command = child_process::background_command(&login_shell());
    command.args(["-l", "-c", &script]);
    let output = child_process::output_with_timeout(&mut command, PROGRAM_PROBE_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    let found = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_owned();
    (found.starts_with('/') && std::path::Path::new(&found).is_file()).then_some(found)
}

#[cfg(target_os = "windows")]
fn probe_login_path(_program: &str) -> Option<String> {
    // Windows kickstarts resolve through cmd.exe with the full user PATH that
    // GUI processes inherit, so a login-shell probe has nothing to add.
    None
}

fn resolve_command(
    provider_id: &str,
    kickstart_commands: &std::collections::BTreeMap<String, String>,
    registry: &ProviderRegistry,
) -> Option<ResolvedCommand> {
    if !is_kickstart_capable(registry, provider_id) {
        return None;
    }
    if let Some(custom) = kickstart_commands.get(provider_id) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(ResolvedCommand::Custom(trimmed.to_owned()));
        }
    }
    registry
        .runtime(provider_id)
        .and_then(|runtime| runtime.session_kickstart())
        .map(|kickstart| ResolvedCommand::BuiltIn {
            program: resolve_builtin_program(&kickstart.program),
            args: kickstart.args,
            envs: kickstart.envs,
        })
}

/// Every provider with a first-message rolling session publishes it as the
/// quota window with source id "session" (claude, codex, zai, kimi, minimax,
/// opencode, commandcode); Kimi additionally rolls its weekly window on the
/// first message. Matching by the runtime's declared rolling windows keeps
/// the kickstart trigger independent of the `sessionWindow` UI flag, which
/// only claude sets.
fn is_kickstart_capable(registry: &ProviderRegistry, provider_id: &str) -> bool {
    let Some(rolling) = registry
        .runtime(provider_id)
        .map(|runtime| runtime.rolling_windows())
    else {
        return false;
    };
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
            ) && metric
                .source
                .source_id()
                .is_some_and(|source_id| rolling.iter().any(|window| window == source_id))
        })
    })
}

/// The windows a kickstart renews for one provider under its stored scope:
/// the session by default, the non-session rolling windows for "weekly", and
/// everything for "both". Empty means the scope names nothing this provider
/// rolls and the provider is skipped.
fn scoped_windows(scope: Option<&str>, rolling: &[String]) -> Vec<String> {
    match scope {
        Some("weekly") => rolling
            .iter()
            .filter(|window| window.as_str() != "session")
            .cloned()
            .collect(),
        Some("both") => rolling.to_vec(),
        _ => rolling
            .iter()
            .filter(|window| window.as_str() == "session")
            .cloned()
            .collect(),
    }
}

/// A window is active while its reset time is still in the future; once a
/// scoped window is missing or past its reset, the window has rolled over
/// and a kickstart prompt would begin a fresh one.
#[cfg(test)]
fn session_window_active(snapshot: &crate::models::ProviderSnapshot) -> bool {
    let now = Utc::now();
    snapshot
        .quotas
        .iter()
        .any(|window| window.id == "session" && window.resets_at.is_some_and(|reset| reset > now))
}

/// The earliest upcoming reset among the scoped windows, or `None` when any
/// scoped window has rolled over — the state a kickstart renews.
fn next_scoped_reset(
    snapshot: &crate::models::ProviderSnapshot,
    scoped: &[String],
) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    scoped
        .iter()
        .map(|window_id| {
            snapshot
                .quotas
                .iter()
                .filter(|window| window.id == *window_id)
                .filter_map(|window| window.resets_at)
                .filter(|reset| *reset > now)
                .min()
        })
        .collect::<Option<Vec<_>>>()
        .map(|resets| {
            resets
                .into_iter()
                .min()
                .expect("scoped windows are non-empty")
        })
}

/// Decides and schedules session kickstarts after a refresh batch. Two tiers:
///
/// - Session still active: arm (or reuse) a timer at its reset time plus a
///   short grace, so the next window starts right when the current one
///   expires instead of waiting for the next refresh tick.
/// - Session already rolled over: cancel any timer, kick now (cooldown
///   permitting), and refresh so the UI shows the new window immediately.
///
/// The refresh-tail path stays as the safety net either way: system sleep can
/// skew timers, and any later refresh re-evaluates from fresh data.
/// Prompt execution is detached from the refresh tail: a slow CLI must not
/// hold a manual or background refresh open for up to `KICKSTART_TIMEOUT`.
pub fn evaluate(
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

    // Outcome bookkeeping only lives for current targets; entries for
    // removed accounts or vanished data directories would linger forever.
    KICK_OUTCOMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(&targets);

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
        // Without a successful live snapshot we do not know the window state.
        // Retained cache data after any refresh failure must never trigger an
        // automated prompt.
        let Some(snapshot) = fresh_snapshot(provider_state) else {
            continue;
        };
        let rolling = registry
            .runtime(&provider_id)
            .map(|runtime| runtime.rolling_windows())
            .unwrap_or_default();
        let scoped = scoped_windows(
            current_settings
                .kickstart_window_scopes
                .get(&provider_id)
                .map(String::as_str),
            &rolling,
        );
        if scoped.is_empty() {
            continue;
        }
        if let Some(reset_at) = next_scoped_reset(snapshot, &scoped) {
            // Every scoped window is still live: aim the timer at the earliest
            // reset among them, and clear any ineffective-kick streak.
            KICK_OUTCOMES
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .observe_alive(&provider_id);
            schedule_at_reset(
                provider_id.clone(),
                reset_at,
                app.clone(),
                service.clone(),
                settings.clone(),
                notifications.clone(),
            );
            continue;
        }

        // A scoped window has rolled over: a scheduled timer is obsolete —
        // kick now if the cooldown and the circuit breaker allow it.
        cancel_scheduled(&provider_id);
        {
            let mut outcomes = KICK_OUTCOMES
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if outcomes.refuses(&provider_id) {
                continue;
            }
        }
        if !record_attempt(&provider_id) {
            continue;
        }
        KICK_OUTCOMES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_kick(&provider_id);

        spawn_prompt(
            provider_id,
            kickstart_command,
            app.clone(),
            service.clone(),
            settings.clone(),
            notifications.clone(),
        );
    }
}

fn spawn_prompt(
    provider_id: String,
    command: ResolvedCommand,
    app: AppHandle,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
) {
    tauri::async_runtime::spawn(async move {
        crate::app_info!(
            "kickstart",
            "{provider_id} window rolled over; starting a fresh one"
        );
        if let Err(error) = send_prompt(&command).await {
            crate::app_warn!("kickstart", "{provider_id} kickstart failed: {error}");
            return;
        }
        crate::app_info!("kickstart", "{provider_id} window restarted");

        // Publish the brand-new window after the prompt completes. The nested
        // refresh tail sees an active live session and only rearms its timer.
        refresh_with_events(
            &app,
            &service,
            &settings,
            &notifications,
            std::slice::from_ref(&provider_id),
            true,
            false,
        )
        .await;
    });
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
        run_scheduled_kick(task_provider_id, app, service, settings, notifications).await;
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
    // Refresh first, then let the shared refresh tail evaluate the newly
    // authoritative session window. This catches a real user message sent
    // after the old reset and suppresses the synthetic prompt.
    refresh_with_events(
        &app,
        &service,
        &settings,
        &notifications,
        std::slice::from_ref(&provider_id),
        true,
        false,
    )
    .await;
}

fn fresh_snapshot(
    state: &crate::models::ProviderViewState,
) -> Option<&crate::models::ProviderSnapshot> {
    // A provider mid-refresh keeps its previous Live snapshot with the error
    // cleared, so a rolled-over-but-not-yet-refreshed session could otherwise
    // look eligible for a synthetic prompt.
    if state.refreshing
        || state.error.is_some()
        || state.source != crate::models::SnapshotSource::Live
    {
        return None;
    }
    state.snapshot.as_ref()
}

/// The login-shell script for a built-in: exports the account-scoped
/// environment inside the script (so profiles cannot override it) and then
/// `exec`s the CLI with its arguments intact.
#[cfg(not(target_os = "windows"))]
fn login_script_with_envs(envs: &[(String, String)]) -> String {
    if envs.is_empty() {
        return "exec \"$@\"".to_owned();
    }
    let exports = envs
        .iter()
        .map(|(key, value)| format!("export {}={}", key, shell_word(value)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{exports}; exec \"$@\"")
}

/// Single-quotes a shell word; the standard `'\''` dance escapes embedded
/// quotes.
#[cfg(not(target_os = "windows"))]
fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Runs the kickstart command through the user's login shell: the app process
/// does not inherit the login PATH where `claude`/`codex` live. Built-ins keep
/// program, args, and environment separate; custom commands remain
/// user-authored shell input.
async fn send_prompt(resolved: &ResolvedCommand) -> Result<(), String> {
    // Unix: the login shell resolves the CLI the way the user's terminal
    // would (the app process lacks that PATH). Windows has no SHELL in GUI
    // sessions; cmd.exe resolves npm-global CLIs from the user PATH.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = child_process::background_command("cmd.exe");
        match resolved {
            ResolvedCommand::Custom(script) => {
                command.args(["/C", script]);
            }
            ResolvedCommand::BuiltIn {
                program,
                args,
                envs,
            } => {
                command.arg("/C").arg(program).args(args);
                command.envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            }
        }
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut command = child_process::background_command(&login_shell());
        match resolved {
            ResolvedCommand::Custom(script) => {
                command.args(["-l", "-c", script]);
            }
            ResolvedCommand::BuiltIn {
                program,
                args,
                envs,
            } => {
                // `$@` preserves argument boundaries while the login shell
                // still supplies the user's CLI PATH. The environment is
                // exported inside the `-c` script: setting it on the spawned
                // process would let login profiles override account-scoped
                // variables such as CLAUDE_CONFIG_DIR before `exec` runs.
                command.args([
                    "-l",
                    "-c",
                    &login_script_with_envs(envs),
                    "usagedeck-kickstart",
                ]);
                command.arg(program).args(args);
            }
        }
        command
    };
    let status = tokio::task::spawn_blocking(move || {
        child_process::status_with_timeout(&mut command, KICKSTART_TIMEOUT)
    })
    .await
    .map_err(|error| format!("kickstart task failed: {error}"))?
    .map_err(|error| format!("could not run the kickstart command: {error}"))?;
    if !status.success() {
        // A custom command may embed arbitrary secrets in stdout or stderr;
        // neither stream may reach the persistent log.
        return Err(format!(
            "the kickstart command exited with status {}",
            status
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{
        fresh_snapshot, is_kickstart_capable, next_scoped_reset, scoped_windows,
        session_window_active, LAST_ATTEMPTS,
    };
    use crate::models::{
        MetricDefinition, MetricSection, MetricSource, ProviderDefinition, ProviderSnapshot,
        ProviderViewState, QuotaWindow, SnapshotSource, UsageHistory,
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
        assert!(is_kickstart_capable(&registry, "claude"));

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
        assert!(!is_kickstart_capable(&registry, "codex"));
    }

    #[test]
    fn retained_snapshot_after_refresh_error_is_not_eligible_for_kickstart() {
        let mut state = ProviderViewState {
            snapshot: Some(snapshot_with_session_reset(None)),
            source: SnapshotSource::Live,
            ..ProviderViewState::default()
        };
        assert!(fresh_snapshot(&state).is_some());

        state.error = Some("offline".into());
        assert!(fresh_snapshot(&state).is_none());

        state.error = None;
        state.refreshing = true;
        assert!(fresh_snapshot(&state).is_none());

        state.refreshing = false;
        state.source = SnapshotSource::Cache;
        assert!(fresh_snapshot(&state).is_none());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn login_script_exports_envs_inside_the_c_script() {
        use super::login_script_with_envs;
        assert_eq!(login_script_with_envs(&[]), "exec \"$@\"");
        assert_eq!(
            login_script_with_envs(&[(
                "CLAUDE_CONFIG_DIR".to_owned(),
                "/Users/x/My Dir/.claude-work".to_owned()
            )]),
            "export CLAUDE_CONFIG_DIR='/Users/x/My Dir/.claude-work'; exec \"$@\""
        );
        // Embedded quotes escape rather than break out of the word.
        assert!(
            login_script_with_envs(&[("K".to_owned(), "a'b".to_owned())])
                .contains("export K='a'\\''b'")
        );
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

    #[test]
    fn ineffective_kicks_suspend_until_a_live_window_clears_them() {
        let mut outcomes = super::KickOutcomes::default();

        // Under the limit, kicks proceed.
        outcomes.record_kick("kimi");
        assert!(!outcomes.refuses("kimi"));
        outcomes.record_kick("kimi");
        // The second ineffective kick exhausts the budget: the next
        // evaluation refuses instead of paying for another prompt.
        assert!(outcomes.refuses("kimi"));
        assert!(outcomes.refuses("kimi"));

        // A live window (the kick worked, or the user messaged) clears the
        // streak and the suspension.
        outcomes.observe_alive("kimi");
        assert!(!outcomes.refuses("kimi"));
        assert!(!outcomes.refuses("other"));
    }

    #[test]
    fn a_limited_provider_suspends_after_two_ineffective_kicks() {
        // End-to-end over the shared state: two recorded kicks without an
        // alive observation in between refuse the third.
        {
            let mut outcomes = super::KICK_OUTCOMES
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            outcomes.observe_alive("breaker-provider");
        }
        assert_kick_allowed_and_record("breaker-provider");
        assert_kick_allowed_and_record("breaker-provider");
        {
            let mut outcomes = super::KICK_OUTCOMES
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            assert!(outcomes.refuses("breaker-provider"));
        }
    }

    fn assert_kick_allowed_and_record(provider_id: &str) {
        let mut outcomes = super::KICK_OUTCOMES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(!outcomes.refuses(provider_id));
        outcomes.record_kick(provider_id);
    }

    #[test]
    fn scoped_windows_follow_the_stored_scope() {
        let rolling = vec!["session".to_owned(), "weekly".to_owned()];
        assert_eq!(scoped_windows(None, &rolling), ["session"]);
        assert_eq!(scoped_windows(Some("session"), &rolling), ["session"]);
        assert_eq!(scoped_windows(Some("weekly"), &rolling), ["weekly"]);
        assert_eq!(
            scoped_windows(Some("both"), &rolling),
            ["session", "weekly"]
        );
        // A weekly-only provider stays eligible under the weekly scope and is
        // skipped entirely under the session default.
        assert_eq!(
            scoped_windows(Some("weekly"), &["weekly".to_owned()]),
            ["weekly"]
        );
        assert!(scoped_windows(None, &["weekly".to_owned()]).is_empty());
    }

    fn weekly_window(reset: Option<chrono::DateTime<Utc>>) -> QuotaWindow {
        QuotaWindow {
            id: "weekly".into(),
            label: "Weekly".into(),
            used_percent: 0.0,
            resets_at: reset,
            period_seconds: 7 * 24 * 3600,
            format: Default::default(),
            used_value: None,
            limit_value: None,
            unit: None,
            estimated: false,
            source_note: None,
        }
    }

    #[test]
    fn next_reset_requires_every_scoped_window_to_be_live() {
        let soon = Utc::now() + chrono::Duration::hours(1);
        let later = Utc::now() + chrono::Duration::hours(3);
        let past = Utc::now() - chrono::Duration::hours(1);

        // Both live: the earliest upcoming reset among them wins.
        let mut snapshot = snapshot_with_session_reset(Some(later));
        snapshot.quotas.push(weekly_window(Some(soon)));
        assert_eq!(
            next_scoped_reset(&snapshot, &["session".to_owned(), "weekly".to_owned()]),
            Some(soon)
        );

        // A missing scoped window has rolled over and demands a kick.
        let session_only = snapshot_with_session_reset(Some(later));
        assert_eq!(
            next_scoped_reset(&session_only, &["session".to_owned(), "weekly".to_owned()]),
            None
        );

        // A past reset on a scoped window demands a kick even though the
        // session itself is live.
        let mut expired_weekly = snapshot_with_session_reset(Some(later));
        expired_weekly.quotas.push(weekly_window(Some(past)));
        assert_eq!(
            next_scoped_reset(
                &expired_weekly,
                &["session".to_owned(), "weekly".to_owned()]
            ),
            None
        );

        // A weekly-only scope ignores the dead session entirely.
        assert_eq!(
            next_scoped_reset(&expired_weekly, &["weekly".to_owned()]),
            None
        );
        let mut live_weekly = snapshot_with_session_reset(None);
        live_weekly.quotas.push(weekly_window(Some(soon)));
        assert_eq!(
            next_scoped_reset(&live_weekly, &["weekly".to_owned()]),
            Some(soon)
        );
    }

    #[test]
    fn builtin_programs_resolve_from_user_local_install_roots() {
        let directory = tempfile::tempdir().unwrap();
        let local_bin = directory.path().join(".local").join("bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        let installed = local_bin.join("usagedeck-test-cli");
        std::fs::write(&installed, b"#!/bin/sh\n").unwrap();

        // A CLI under ~/.local/bin resolves to its absolute path even though a
        // GUI app's login shell PATH never includes that directory.
        assert_eq!(
            super::resolve_builtin_program_in("usagedeck-test-cli", Some(directory.path())),
            installed.display().to_string()
        );
        // Nothing installed anywhere: the bare name falls through to the shell.
        assert_eq!(
            super::resolve_builtin_program_in("usagedeck-test-cli", None),
            "usagedeck-test-cli"
        );
        // Already-qualified programs are never rewritten.
        assert_eq!(
            super::resolve_builtin_program_in("/opt/other/cli", Some(directory.path())),
            "/opt/other/cli"
        );
    }

    #[test]
    fn login_shell_hits_win_over_static_roots_and_only_when_absolute() {
        let directory = tempfile::tempdir().unwrap();
        let local_bin = directory.path().join(".local").join("bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        let installed = local_bin.join("usagedeck-test-cli");
        std::fs::write(&installed, b"#!/bin/sh\n").unwrap();

        // The login shell's answer is exactly what the kickstart will execute,
        // so it wins even when a static root also holds a copy.
        assert_eq!(
            super::resolve_builtin_program_with(
                "usagedeck-test-cli",
                Some(directory.path()),
                |_| Some("/usr/custom/bin/usagedeck-test-cli".to_owned())
            ),
            "/usr/custom/bin/usagedeck-test-cli"
        );
        // A non-absolute probe result (shell built-in, alias) is not a path and
        // must not shadow the static roots.
        assert_eq!(
            super::resolve_builtin_program_with(
                "usagedeck-test-cli",
                Some(directory.path()),
                |_| Some("usagedeck-test-cli".to_owned())
            ),
            installed.display().to_string()
        );
        // Probe miss falls through to the roots, then the bare name.
        assert_eq!(
            super::resolve_builtin_program_with(
                "usagedeck-test-cli",
                Some(directory.path()),
                |_| None
            ),
            installed.display().to_string()
        );
        assert_eq!(
            super::resolve_builtin_program_with("usagedeck-test-cli", None, |_| None),
            "usagedeck-test-cli"
        );
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
            super::resolve_command("claude", &commands, &registry),
            Some(super::ResolvedCommand::Custom(
                "claude -p hi --model haiku".into()
            ))
        );
        // Empty custom falls back to the built-in, resolved against the
        // well-known user-local install roots on this machine.
        assert_eq!(
            super::resolve_command(
                "claude",
                &std::collections::BTreeMap::from([("claude".to_owned(), "   ".to_owned())]),
                &registry,
            ),
            Some(super::ResolvedCommand::BuiltIn {
                program: super::resolve_builtin_program("claude"),
                args: vec!["-p".into(), "Hi".into()],
                envs: Vec::new(),
            })
        );
        // Built-in when no custom entry exists.
        assert_eq!(
            super::resolve_command("claude", &std::collections::BTreeMap::new(), &registry),
            Some(super::ResolvedCommand::BuiltIn {
                program: super::resolve_builtin_program("claude"),
                args: vec!["-p".into(), "Hi".into()],
                envs: Vec::new(),
            })
        );
        // Providers without session windows are never kickstartable.
        assert_eq!(super::resolve_command("cursor", &commands, &registry), None);
    }
}
