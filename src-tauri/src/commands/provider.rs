use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use crate::{
    commands::settings::settings_view_state,
    models::{ApiKeyMutationOutcome, ApiKeyStatus, ProviderApiKeyState, ProviderLink},
    notifications::finish_refresh,
    pacing::NotificationEvaluator,
    providers::{ProviderRegistry, UsageProvider},
    service::ProviderService,
    settings::SettingsService,
    tray_presentation,
};

fn resolve_provider_link<'a>(
    registry: &'a ProviderRegistry,
    provider_id: &str,
    link_index: usize,
) -> Result<&'a ProviderLink, String> {
    registry
        .definition(provider_id)
        .and_then(|provider| provider.links.get(link_index))
        .ok_or_else(|| "That provider link is unavailable.".to_owned())
}

#[tauri::command]
pub fn open_provider_link(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
    link_index: usize,
) -> Result<(), String> {
    let link = resolve_provider_link(&registry, &provider_id, link_index)?;
    crate::app_debug!(
        "http",
        "opening {provider_id} provider link {}",
        crate::logging::redact_url(&link.url)
    );
    app.opener()
        .open_url(&link.url, None::<&str>)
        .map_err(|_| "That provider link could not be opened.".to_owned())
}

async fn api_key_state(
    registry: Arc<ProviderRegistry>,
    provider_id: String,
) -> Result<Option<ProviderApiKeyState>, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(status) = runtime.api_key_status() else {
            return Ok(None);
        };
        let status = status.map_err(|error| error.to_string())?;
        Ok(Some(ProviderApiKeyState {
            provider_id,
            status,
        }))
    })
    .await
    .map_err(|_| "The API key status could not be read.".to_owned())?
}

enum ApiKeyMutation<'a> {
    Save(&'a str),
    Delete,
}

struct AppliedApiKeyMutation {
    state: ProviderApiKeyState,
    status_uncertain: bool,
}

fn mutate_api_key(
    runtime: &dyn UsageProvider,
    provider_id: String,
    mutation: ApiKeyMutation<'_>,
) -> Result<AppliedApiKeyMutation, String> {
    let initial_status = runtime
        .api_key_status()
        .ok_or_else(|| "That provider does not accept an API key.".to_owned())?
        .ok();
    let fallback_status = match &mutation {
        ApiKeyMutation::Save(_) => {
            if matches!(
                initial_status,
                Some(
                    ApiKeyStatus::FromEnvironment
                        | ApiKeyStatus::FromConfig
                        | ApiKeyStatus::OverrideActive
                )
            ) {
                ApiKeyStatus::OverrideActive
            } else {
                ApiKeyStatus::Saved
            }
        }
        ApiKeyMutation::Delete => ApiKeyStatus::NotSet,
    };

    match mutation {
        ApiKeyMutation::Save(value) => runtime.save_api_key(value),
        ApiKeyMutation::Delete => runtime.delete_api_key(),
    }
    .map_err(|error| error.to_string())?;

    let (status, status_uncertain) = match runtime.api_key_status() {
        Some(Ok(status)) => (status, false),
        Some(Err(_)) | None => (fallback_status, true),
    };
    Ok(AppliedApiKeyMutation {
        state: ProviderApiKeyState {
            provider_id,
            status,
        },
        status_uncertain,
    })
}

fn reconcile_provider_credential_state(
    app: &AppHandle,
    service: &ProviderService,
    settings: &SettingsService,
    provider_id: &str,
    detected: bool,
    enable: bool,
) -> Result<(), String> {
    let updated = settings.reconcile_provider_credential_state(provider_id, detected, enable)?;
    tray_presentation::update(app, &service.state(), &updated, settings.registry());
    let _ = app.emit("settings-state", settings_view_state(app, settings));
    Ok(())
}

fn incomplete_mutation_warning(action: &str) -> String {
    format!(
        "The API key was {action}, but UsageDeck could not finish updating provider status. Restart UsageDeck or try again."
    )
}

#[tauri::command]
pub async fn get_provider_api_key_state(
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
) -> Result<Option<ProviderApiKeyState>, String> {
    api_key_state(registry.inner().clone(), provider_id).await
}

#[tauri::command]
pub async fn save_provider_api_key(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
    api_key: String,
) -> Result<ApiKeyMutationOutcome, String> {
    let api_key = Zeroizing::new(api_key);
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let provider_for_save = provider_id.clone();
    let applied = tauri::async_runtime::spawn_blocking(move || {
        mutate_api_key(
            runtime.as_ref(),
            provider_for_save,
            ApiKeyMutation::Save(api_key.as_str()),
        )
    })
    .await
    .map_err(|_| "The API key could not be saved.".to_owned())??;

    let command_guard = settings.lock_command_mutation().await;
    let settings_reconciled = match reconcile_provider_credential_state(
        &app,
        &service,
        &settings,
        &provider_id,
        true,
        true,
    ) {
        Ok(()) => true,
        Err(error) => {
            crate::app_warn!(
                "auth",
                "provider state after API key save could not be reconciled for {provider_id}: {error}"
            );
            false
        }
    };
    if applied.status_uncertain {
        crate::app_warn!(
            "auth",
            "API key status could not be confirmed after saving for {provider_id}"
        );
    }
    drop(command_guard);
    drop(credential_guard);
    service.refresh(&provider_id, true).await;
    let usage = service.state();
    let _ = app.emit("usage-state", &usage);
    finish_refresh(&app, &usage, &settings, &notifications);
    crate::app_info!("auth", "API key saved for {provider_id}");
    Ok(ApiKeyMutationOutcome {
        state: applied.state,
        warning: (applied.status_uncertain || !settings_reconciled)
            .then(|| incomplete_mutation_warning("saved securely")),
    })
}

#[tauri::command]
pub async fn delete_provider_api_key(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
) -> Result<ApiKeyMutationOutcome, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let provider_for_delete = provider_id.clone();
    let applied = tauri::async_runtime::spawn_blocking(move || {
        mutate_api_key(
            runtime.as_ref(),
            provider_for_delete,
            ApiKeyMutation::Delete,
        )
    })
    .await
    .map_err(|_| "The API key could not be removed.".to_owned())??;

    let command_guard = settings.lock_command_mutation().await;
    let detected = applied.state.status != ApiKeyStatus::NotSet;
    let settings_reconciled = match reconcile_provider_credential_state(
        &app,
        &service,
        &settings,
        &provider_id,
        detected,
        false,
    ) {
        Ok(()) => true,
        Err(error) => {
            crate::app_warn!(
                "auth",
                "provider state after API key removal could not be reconciled for {provider_id}: {error}"
            );
            false
        }
    };
    if applied.status_uncertain {
        crate::app_warn!(
            "auth",
            "API key status could not be confirmed after removal for {provider_id}"
        );
    }
    let should_refresh = settings
        .get()
        .providers
        .iter()
        .any(|provider| provider.id == provider_id && provider.enabled);
    drop(command_guard);
    drop(credential_guard);
    if should_refresh {
        service.refresh(&provider_id, true).await;
        let usage = service.state();
        let _ = app.emit("usage-state", &usage);
        finish_refresh(&app, &usage, &settings, &notifications);
    }
    crate::app_info!("auth", "saved API key removed for {provider_id}");
    Ok(ApiKeyMutationOutcome {
        state: applied.state,
        warning: (applied.status_uncertain || !settings_reconciled)
            .then(|| incomplete_mutation_warning("removed")),
    })
}

/// Creates and persists a named API-key account under `family` (e.g. `kimi`) with its own
/// credential-store entry. The dashboard card appears after restart, when the runtime can
/// register the new `family@suffix` provider id.
#[tauri::command]
pub async fn add_api_key_account(
    storage: tauri::State<'_, std::sync::Arc<crate::storage::Storage>>,
    family: String,
    account_name: String,
) -> Result<String, String> {
    use crate::providers::api_key_account::API_KEY_ACCOUNT_FAMILIES;

    let family = family.trim().to_owned();
    let account_name = account_name.trim().to_owned();
    if account_name.is_empty() || account_name.chars().count() > 48 {
        return Err("Provider account name must be 1 to 48 characters.".into());
    }
    if !API_KEY_ACCOUNT_FAMILIES.contains(&family.as_str()) {
        return Err(format!(
            "Provider family '{family}' does not support multiple accounts."
        ));
    }
    storage
        .allocate_api_key_account(&family, &account_name)
        .map_err(|error| error.to_string())
}

/// Removes an API-key account by its derived provider id (e.g. `kimi@1a2b3c4d`). The credential
/// entry is cleared, the account is graveyarded so its id cannot be reallocated to a different
/// logical account, and the dashboard card disappears after restart (`RestartRequired`). The
/// cached snapshot survives: `CacheIdentity::Unscoped` walls it off, so it is silently ignored
/// when no runtime exists.
#[tauri::command]
pub async fn remove_api_key_account(
    storage: tauri::State<'_, std::sync::Arc<crate::storage::Storage>>,
    provider_id: String,
) -> Result<(), String> {
    if !crate::providers::api_key_account::is_api_key_account_provider_id(&provider_id) {
        return Err(format!(
            "'{provider_id}' is not an API-key account provider. Expected '<family>@<8 hex>'."
        ));
    }
    let family = crate::providers::provider_family(&provider_id).to_owned();
    // Deleting unknown families would expose an error path with no valid CS value; truncate to a
    // non-sensitive prefix of the derived id instead.
    let prefix = if provider_id.len() > 16 {
        &provider_id[..16]
    } else {
        &provider_id
    };
    let identity_key = storage
        .load_provider_account_records(&family)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|(_, stored_provider_id, _)| stored_provider_id == &provider_id)
        .map(|(identity_key, _, _)| identity_key)
        .ok_or_else(|| {
            format!("API-key account '{prefix}...' could not be found. It may already have been removed.")
        })?;
    // The credential-store entry is tied to the derived provider id (e.g. `kimi@1a2b3c4d`);
    // deleting the raw ApiKeyStore entry is sufficient: every family ultimately delegates to it,
    // so calling it directly avoids depending on per-family ZaiAuthStore/KimiAuthStore privacy.
    // Missing entries are not errors: the database record is the source of truth and a keyring
    // miss simply means the secret was already absent.
    if !crate::providers::api_key_account::supports_api_key_accounts(&family) {
        return Err(format!(
            "Provider family '{family}' does not support multiple accounts."
        ));
    }
    let store = crate::providers::api_key::ApiKeyStore::new_with_sources(&provider_id, &[], &[]);
    let _ = store.delete();
    storage
        .delete_provider_account_by_identity(&family, &identity_key)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
    };

    use crate::{
        models::{
            ApiKeyStatus, MetricDefinition, MetricSection, MetricSource, ProviderDefinition,
            ProviderErrorKind, ProviderLink, ProviderSnapshot,
        },
        providers::{ProviderError, ProviderRegistry, UsageProvider},
    };

    use super::{mutate_api_key, resolve_provider_link, ApiKeyMutation};

    struct MutatingProvider {
        statuses: Mutex<VecDeque<Result<ApiKeyStatus, ProviderError>>>,
        saved_value: Mutex<Option<String>>,
        deleted: AtomicBool,
    }

    impl MutatingProvider {
        fn new(statuses: Vec<Result<ApiKeyStatus, ProviderError>>) -> Self {
            Self {
                statuses: Mutex::new(statuses.into()),
                saved_value: Mutex::new(None),
                deleted: AtomicBool::new(false),
            }
        }
    }

    impl UsageProvider for MutatingProvider {
        fn definition(&self) -> ProviderDefinition {
            registry().definition("provider").unwrap().clone()
        }

        fn has_local_credentials(&self) -> bool {
            false
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            unreachable!()
        }

        fn api_key_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
            Some(
                self.statuses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(ApiKeyStatus::NotSet)),
            )
        }

        fn save_api_key(&self, value: &str) -> Result<(), ProviderError> {
            *self.saved_value.lock().unwrap() = Some(value.to_owned());
            Ok(())
        }

        fn delete_api_key(&self) -> Result<(), ProviderError> {
            self.deleted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn registry() -> ProviderRegistry {
        ProviderRegistry::from_definitions(vec![ProviderDefinition {
            id: "provider".into(),
            display_name: "Provider".into(),
            short_name: "P".into(),
            fallback_enabled: true,
            local_usage_source_note: None,
            links: vec![ProviderLink::new("Status", "https://status.example.com/")],
            options: Vec::new(),
            metrics: vec![MetricDefinition::new(
                "provider.session",
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
        }])
        .unwrap()
    }

    #[test]
    fn resolves_only_links_declared_by_the_provider_registry() {
        let registry = registry();

        assert_eq!(
            resolve_provider_link(&registry, "provider", 0).unwrap().url,
            "https://status.example.com/"
        );
        assert!(resolve_provider_link(&registry, "provider", 1).is_err());
        assert!(resolve_provider_link(&registry, "unknown", 0).is_err());
    }

    #[test]
    fn applied_api_key_save_is_not_reported_as_failed_when_status_refresh_fails() {
        let provider = MutatingProvider::new(vec![
            Ok(ApiKeyStatus::FromEnvironment),
            Err(ProviderError::new(
                ProviderErrorKind::CredentialStorage,
                "status unavailable",
            )),
        ]);

        let applied =
            mutate_api_key(&provider, "provider".into(), ApiKeyMutation::Save("secret")).unwrap();

        assert_eq!(applied.state.status, ApiKeyStatus::OverrideActive);
        assert!(applied.status_uncertain);
        assert_eq!(
            provider.saved_value.lock().unwrap().as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn applied_api_key_delete_is_not_reported_as_failed_when_status_refresh_fails() {
        let provider = MutatingProvider::new(vec![
            Ok(ApiKeyStatus::Saved),
            Err(ProviderError::new(
                ProviderErrorKind::CredentialStorage,
                "status unavailable",
            )),
        ]);

        let applied = mutate_api_key(&provider, "provider".into(), ApiKeyMutation::Delete).unwrap();

        assert_eq!(applied.state.status, ApiKeyStatus::NotSet);
        assert!(applied.status_uncertain);
        assert!(provider.deleted.load(Ordering::SeqCst));
    }
}
