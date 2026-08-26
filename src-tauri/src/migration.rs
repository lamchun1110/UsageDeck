//! One-time migration from the pre-rename OpenQuota data locations.
//!
//! UsageDeck previously shipped as the OpenQuota fork. Existing users kept their
//! settings, usage history, pricing cache, and saved API keys under the legacy
//! bundle identifier (`io.github.deviffyy.openquota`). Because the application
//! data directory is derived from the bundle identifier, this module copies the
//! legacy data into the new location on first launch and re-keys saved API-key
//! credentials in the OS credential store. The legacy locations are never
//! modified beyond deleting migrated API-key entries after their replacement is
//! confirmed written.
//!
//! Reading a credential-store entry owned by another application makes macOS ask
//! the user to unlock it, so the API-key pass is guarded twice: it probes for the
//! legacy service with a prompt-free metadata search first, and it records a
//! marker file before touching the legacy entry so the consent prompt can appear
//! at most once per data directory — a denied or failed attempt is never retried.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::providers::credential_store::{
    delete_owned_password, generic_password_service_exists, read_owned_password,
    write_owned_password,
};

const LEGACY_IDENTIFIER: &str = "io.github.deviffyy.openquota";
const LEGACY_API_KEY_SERVICE: &str = "io.github.deviffyy.openquota.api-key";
const API_KEY_SERVICE: &str = "com.lamchun1110.usagedeck.api-key";
const LEGACY_KEY_MIGRATION_MARKER: &str = "legacy-api-key-migration.attempted";
const DATABASE_FILE: &str = "openquota.db";
const DATA_DIRECTORY_ITEMS: &[&str] = &["pricing", "antigravity"];
/// Provider ids that can hold a saved API key. Keep in sync with the
/// `ApiKeyStore::new_with_sources` call sites (kimi, minimax, openrouter, zai).
const API_KEY_ACCOUNTS: &[&str] = &["kimi", "minimax", "openrouter", "zai"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DataMigrationReport {
    pub copied: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ApiKeyMigrationOutcome {
    pub migrated: Vec<String>,
    pub failures: Vec<(String, String)>,
}

/// Best-effort resolution of the legacy application data directory.
pub fn legacy_app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(
            home_directory()?
                .join("Library")
                .join("Application Support")
                .join(LEGACY_IDENTIFIER),
        )
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|base| base.join(LEGACY_IDENTIFIER))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home_directory().map(|home| home.join(".config")))?;
        Some(config_home.join(LEGACY_IDENTIFIER))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

#[cfg(unix)]
fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Copies legacy data files into `destination` without overwriting anything.
pub fn migrate_app_data(destination: &Path) -> DataMigrationReport {
    migrate_app_data_from(legacy_app_data_dir().as_deref(), destination)
}

fn migrate_app_data_from(legacy: Option<&Path>, destination: &Path) -> DataMigrationReport {
    let mut report = DataMigrationReport::default();
    let Some(legacy) = legacy else {
        return report;
    };
    if !legacy.is_dir() || legacy == destination {
        return report;
    }

    if let Some(label) = copy_file_if_absent(legacy, destination, DATABASE_FILE) {
        report.copied.push(label);
    }
    for item in DATA_DIRECTORY_ITEMS {
        if let Some(label) = copy_tree_if_absent(&legacy.join(item), &destination.join(item)) {
            report.copied.push(label);
        }
    }
    report
}

fn copy_file_if_absent(source_root: &Path, destination_root: &Path, name: &str) -> Option<String> {
    let source = source_root.join(name);
    let destination = destination_root.join(name);
    if !source.is_file() || destination.exists() {
        return None;
    }
    if let Some(parent) = destination.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return Some(format!("{name}: {error}"));
        }
    }
    match fs::copy(&source, &destination) {
        Ok(_) => Some(name.to_owned()),
        Err(error) => Some(format!("{name}: {error}")),
    }
}

fn copy_tree_if_absent(source: &Path, destination: &Path) -> Option<String> {
    if !source.is_dir() {
        return None;
    }
    let label = source
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut copied_any = false;
    let mut first_error: Option<String> = None;
    let walker = walkdir::WalkDir::new(source).sort_by_file_name();
    for entry in walker.into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = match entry.path().strip_prefix(source) {
            Ok(relative) => relative,
            Err(error) => {
                first_error.get_or_insert_with(|| format!("{label}: {error}"));
                continue;
            }
        };
        let target = destination.join(relative);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                first_error.get_or_insert_with(|| format!("{label}: {error}"));
                continue;
            }
        }
        match fs::copy(entry.path(), &target) {
            Ok(_) => copied_any = true,
            Err(error) => {
                first_error.get_or_insert_with(|| format!("{label}: {error}"));
            }
        }
    }
    if let Some(error) = first_error {
        return Some(error);
    }
    copied_any.then_some(label)
}

pub(crate) trait CredentialMigrator: Sync {
    fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, service: &str, account: &str, value: &[u8]) -> Result<(), String>;
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
    /// Prompt-free probe for whether any entry exists under `service`. `None`
    /// means the answer is unknown and the caller must not rely on it.
    fn service_exists(&self, _service: &str) -> Option<bool> {
        None
    }
}

struct SystemCredentialMigrator;

impl CredentialMigrator for SystemCredentialMigrator {
    fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        read_owned_password(service, account)
    }

    fn write(&self, service: &str, account: &str, value: &[u8]) -> Result<(), String> {
        write_owned_password(service, account, value)
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        delete_owned_password(service, account)
    }

    fn service_exists(&self, service: &str) -> Option<bool> {
        generic_password_service_exists(service, std::time::Duration::from_secs(2))
    }
}

/// Moves saved API keys from the legacy credential-store service to the new one.
pub fn migrate_api_keys(app_data_dir: &Path) -> ApiKeyMigrationOutcome {
    migrate_api_keys_in(app_data_dir, &SystemCredentialMigrator)
}

fn migrate_api_keys_in(
    app_data_dir: &Path,
    migrator: &dyn CredentialMigrator,
) -> ApiKeyMigrationOutcome {
    let marker = app_data_dir.join(LEGACY_KEY_MIGRATION_MARKER);
    if marker.is_file() {
        return ApiKeyMigrationOutcome::default();
    }
    // Record the attempt before touching the legacy store: the consent prompt must
    // be shown at most once, even if this pass is interrupted midway.
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, b"attempted");
    migrate_api_keys_with(migrator)
}

fn migrate_api_keys_with(migrator: &dyn CredentialMigrator) -> ApiKeyMigrationOutcome {
    let mut outcome = ApiKeyMigrationOutcome::default();
    // Reading an entry owned by another application can ask the user for consent,
    // so only go looking when the legacy service actually holds something.
    if migrator.service_exists(LEGACY_API_KEY_SERVICE) == Some(false) {
        return outcome;
    }
    for account in API_KEY_ACCOUNTS {
        match migrator.read(API_KEY_SERVICE, account) {
            // Already present under the new service: nothing to do.
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(error) => {
                outcome
                    .failures
                    .push(((*account).to_owned(), format!("read: {error}")));
                continue;
            }
        }
        let value = match migrator.read(LEGACY_API_KEY_SERVICE, account) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(error) => {
                outcome
                    .failures
                    .push(((*account).to_owned(), format!("legacy read: {error}")));
                continue;
            }
        };
        if let Err(error) = migrator.write(API_KEY_SERVICE, account, &value) {
            outcome
                .failures
                .push(((*account).to_owned(), format!("write: {error}")));
            continue;
        }
        // Only remove the legacy entry once its replacement is confirmed.
        if let Err(error) = migrator.delete(LEGACY_API_KEY_SERVICE, account) {
            outcome
                .failures
                .push(((*account).to_owned(), format!("cleanup: {error}")));
            continue;
        }
        outcome.migrated.push((*account).to_owned());
    }
    outcome
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use tempfile::tempdir;

    use super::{
        migrate_api_keys_in, migrate_api_keys_with, migrate_app_data_from, ApiKeyMigrationOutcome,
        CredentialMigrator, API_KEY_ACCOUNTS, API_KEY_SERVICE, LEGACY_API_KEY_SERVICE,
        LEGACY_KEY_MIGRATION_MARKER,
    };

    #[test]
    fn copies_the_database_when_the_destination_is_empty() {
        let legacy = tempdir().unwrap();
        let destination = tempdir().unwrap();
        std::fs::write(legacy.path().join("openquota.db"), b"database").unwrap();

        let report = migrate_app_data_from(Some(legacy.path()), destination.path());

        assert_eq!(report.copied, vec!["openquota.db".to_owned()]);
        assert!(report.errors.is_empty());
        assert_eq!(
            std::fs::read(destination.path().join("openquota.db")).unwrap(),
            b"database"
        );
    }

    #[test]
    fn never_overwrites_an_existing_database() {
        let legacy = tempdir().unwrap();
        let destination = tempdir().unwrap();
        std::fs::write(legacy.path().join("openquota.db"), b"legacy").unwrap();
        std::fs::write(destination.path().join("openquota.db"), b"current").unwrap();

        let report = migrate_app_data_from(Some(legacy.path()), destination.path());

        assert!(report.copied.is_empty());
        assert_eq!(
            std::fs::read(destination.path().join("openquota.db")).unwrap(),
            b"current"
        );
    }

    #[test]
    fn merges_missing_files_into_an_existing_pricing_cache() {
        let legacy = tempdir().unwrap();
        let destination = tempdir().unwrap();
        std::fs::create_dir_all(legacy.path().join("pricing")).unwrap();
        std::fs::write(legacy.path().join("pricing").join("a.json"), b"a").unwrap();
        std::fs::write(legacy.path().join("pricing").join("b.json"), b"b").unwrap();
        std::fs::create_dir_all(destination.path().join("pricing")).unwrap();
        std::fs::write(destination.path().join("pricing").join("b.json"), b"kept").unwrap();

        let report = migrate_app_data_from(Some(legacy.path()), destination.path());

        assert_eq!(report.copied, vec!["pricing".to_owned()]);
        assert_eq!(
            std::fs::read(destination.path().join("pricing").join("a.json")).unwrap(),
            b"a"
        );
        assert_eq!(
            std::fs::read(destination.path().join("pricing").join("b.json")).unwrap(),
            b"kept"
        );
    }

    #[test]
    fn reports_nothing_when_the_legacy_directory_is_missing() {
        let destination = tempdir().unwrap();
        let report = migrate_app_data_from(None, destination.path());
        assert!(report.copied.is_empty());
        assert!(report.errors.is_empty());
    }

    #[derive(Default)]
    struct MemoryMigrator {
        store: Mutex<HashMap<(String, String), Vec<u8>>>,
        fail_writes_for: Vec<String>,
        service_absent: bool,
    }

    impl MemoryMigrator {
        fn put(&self, service: &str, account: &str, value: &[u8]) {
            self.store
                .lock()
                .unwrap()
                .insert((service.to_owned(), account.to_owned()), value.to_vec());
        }
    }

    impl CredentialMigrator for MemoryMigrator {
        fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .get(&(service.to_owned(), account.to_owned()))
                .cloned())
        }

        fn write(&self, service: &str, account: &str, value: &[u8]) -> Result<(), String> {
            if self.fail_writes_for.iter().any(|name| name == account) {
                return Err("write refused".into());
            }
            self.put(service, account, value);
            Ok(())
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), String> {
            self.store
                .lock()
                .unwrap()
                .remove(&(service.to_owned(), account.to_owned()));
            Ok(())
        }

        fn service_exists(&self, _service: &str) -> Option<bool> {
            Some(!self.service_absent)
        }
    }

    #[test]
    fn moves_legacy_api_keys_into_the_new_service_and_cleans_up() {
        let migrator = MemoryMigrator::default();
        migrator.put(LEGACY_API_KEY_SERVICE, "openrouter", b"key-one");
        migrator.put(LEGACY_API_KEY_SERVICE, "zai", b"key-two");

        let outcome = migrate_api_keys_with(&migrator);

        assert_eq!(
            outcome.migrated,
            vec!["openrouter".to_owned(), "zai".to_owned()]
        );
        assert!(outcome.failures.is_empty());
        let store = migrator.store.lock().unwrap();
        assert_eq!(
            store
                .get(&(API_KEY_SERVICE.to_owned(), "openrouter".to_owned()))
                .map(Vec::as_slice),
            Some(b"key-one".as_slice())
        );
        assert_eq!(
            store
                .get(&(API_KEY_SERVICE.to_owned(), "zai".to_owned()))
                .map(Vec::as_slice),
            Some(b"key-two".as_slice())
        );
        assert!(!store.contains_key(&(LEGACY_API_KEY_SERVICE.to_owned(), "openrouter".to_owned())));
        assert!(!store.contains_key(&(LEGACY_API_KEY_SERVICE.to_owned(), "zai".to_owned())));
    }

    #[test]
    fn skips_accounts_that_already_exist_under_the_new_service() {
        let migrator = MemoryMigrator::default();
        migrator.put(LEGACY_API_KEY_SERVICE, "kimi", b"legacy");
        migrator.put(API_KEY_SERVICE, "kimi", b"current");

        let outcome = migrate_api_keys_with(&migrator);

        assert!(outcome.migrated.is_empty());
        let store = migrator.store.lock().unwrap();
        assert_eq!(
            store
                .get(&(API_KEY_SERVICE.to_owned(), "kimi".to_owned()))
                .map(Vec::as_slice),
            Some(b"current".as_slice())
        );
        assert_eq!(
            store
                .get(&(LEGACY_API_KEY_SERVICE.to_owned(), "kimi".to_owned()))
                .map(Vec::as_slice),
            Some(b"legacy".as_slice())
        );
    }

    #[test]
    fn keeps_the_legacy_entry_when_the_new_write_fails() {
        let mut migrator = MemoryMigrator::default();
        migrator.put(LEGACY_API_KEY_SERVICE, "minimax", b"precious");
        migrator.fail_writes_for.push("minimax".to_owned());

        let outcome = migrate_api_keys_with(&migrator);

        assert!(outcome.migrated.is_empty());
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].0, "minimax");
        let store = migrator.store.lock().unwrap();
        assert_eq!(
            store
                .get(&(LEGACY_API_KEY_SERVICE.to_owned(), "minimax".to_owned()))
                .map(Vec::as_slice),
            Some(b"precious".as_slice())
        );
        assert!(!store.contains_key(&(API_KEY_SERVICE.to_owned(), "minimax".to_owned())));
    }

    #[test]
    fn every_known_account_is_considered() {
        assert_eq!(API_KEY_ACCOUNTS.len(), 4);
    }

    #[test]
    fn a_prior_attempt_marker_prevents_any_legacy_access() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(LEGACY_KEY_MIGRATION_MARKER), b"attempted").unwrap();
        let migrator = MemoryMigrator::default();
        migrator.put(LEGACY_API_KEY_SERVICE, "kimi", b"legacy");

        let outcome = migrate_api_keys_in(dir.path(), &migrator);

        assert_eq!(outcome, ApiKeyMigrationOutcome::default());
        let store = migrator.store.lock().unwrap();
        assert!(store.contains_key(&(LEGACY_API_KEY_SERVICE.to_owned(), "kimi".to_owned())));
        assert!(!store.contains_key(&(API_KEY_SERVICE.to_owned(), "kimi".to_owned())));
    }

    #[test]
    fn a_failed_attempt_is_recorded_and_never_retried() {
        let dir = tempdir().unwrap();
        let mut migrator = MemoryMigrator::default();
        migrator.put(LEGACY_API_KEY_SERVICE, "minimax", b"precious");
        migrator.fail_writes_for.push("minimax".to_owned());

        let first = migrate_api_keys_in(dir.path(), &migrator);
        assert_eq!(first.failures.len(), 1);
        assert!(dir.path().join(LEGACY_KEY_MIGRATION_MARKER).is_file());

        migrator.fail_writes_for.clear();
        let second = migrate_api_keys_in(dir.path(), &migrator);
        assert_eq!(second, ApiKeyMigrationOutcome::default());
        let store = migrator.store.lock().unwrap();
        assert_eq!(
            store
                .get(&(LEGACY_API_KEY_SERVICE.to_owned(), "minimax".to_owned()))
                .map(Vec::as_slice),
            Some(b"precious".as_slice())
        );
    }

    #[test]
    fn does_not_read_accounts_when_the_legacy_service_is_missing() {
        let dir = tempdir().unwrap();
        let migrator = MemoryMigrator {
            service_absent: true,
            ..MemoryMigrator::default()
        };
        migrator.put(LEGACY_API_KEY_SERVICE, "kimi", b"legacy");

        let outcome = migrate_api_keys_in(dir.path(), &migrator);

        assert_eq!(outcome, ApiKeyMigrationOutcome::default());
        assert!(dir.path().join(LEGACY_KEY_MIGRATION_MARKER).is_file());
        let store = migrator.store.lock().unwrap();
        assert!(store.contains_key(&(LEGACY_API_KEY_SERVICE.to_owned(), "kimi".to_owned())));
        assert!(!store.contains_key(&(API_KEY_SERVICE.to_owned(), "kimi".to_owned())));
    }
}
