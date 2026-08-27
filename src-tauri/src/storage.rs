use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use crate::{
    models::{AppSettings, ProviderSnapshot},
    providers::CacheIdentity,
};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("UsageDeck data directory could not be created")]
    CreateDirectory(#[source] std::io::Error),
    #[error("UsageDeck database is unavailable")]
    Database(#[from] rusqlite::Error),
    #[error("Cached UsageDeck data is invalid")]
    InvalidCache(#[from] serde_json::Error),
    #[error("{0}")]
    InvalidInput(String),
    #[error("UsageDeck database lock is unavailable")]
    Poisoned,
}

/// The only non-automatic value stored in `panel_state.height_mode`; NULL and every other value
/// mean automatic, so absent or legacy rows default to the automatic height mode.
pub const MANUAL_HEIGHT_MODE: &str = "manual";

pub struct Storage {
    connection: Mutex<Connection>,
    /// Dedicated read-only path. WAL mode allows readers to proceed while a
    /// writer commits, but only through separate connections — routing reads
    /// here keeps UI-facing queries from queueing behind snapshot saves on
    /// the write connection's mutex.
    reader_connection: Mutex<Connection>,
}

pub struct ProviderAccountUpdate {
    pub provider_family: String,
    pub identity_key: String,
    pub provider_id: String,
    pub payload: String,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StorageError::CreateDirectory)?;
        }
        match Self::open_at(path) {
            Ok(storage) => Ok(storage),
            Err(error) => {
                // A malformed (or externally locked) database used to abort startup with no
                // recovery path. Snapshots and caches are all rebuildable from providers, so set
                // the file aside and start fresh rather than refusing to launch.
                if !Self::quarantine_corrupt_database(path) {
                    return Err(error);
                }
                crate::app_warn!(
                    "storage",
                    "UsageDeck database could not be opened ({error}); the corrupt file was moved aside and a fresh database was created"
                );
                Self::open_at(path)
            }
        }
    }

    /// Moves an unopenable database (and its WAL/SHM sidecars) aside so a fresh one can be created.
    /// Returns false when recovery cannot help, so the original error propagates.
    fn quarantine_corrupt_database(path: &Path) -> bool {
        if !path.exists() {
            return false;
        }
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        // Sidecars follow SQLite's `<db>-wal` / `<db>-shm` naming.
        let sidecar = |suffix: &str| {
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(suffix);
            path.with_file_name(name)
        };
        let mut moved_primary = false;
        for candidate in [path.to_path_buf(), sidecar("-wal"), sidecar("-shm")] {
            if !candidate.exists() {
                continue;
            }
            let base = candidate.file_name().unwrap_or_default().to_os_string();
            let quarantine_name = |suffix: &str| {
                let mut name = base.clone();
                name.push(suffix);
                candidate.with_file_name(name)
            };
            let mut target = quarantine_name(&format!(".corrupt-{stamp}"));
            let mut attempt = 0;
            while target.exists() && attempt < 10 {
                attempt += 1;
                target = quarantine_name(&format!(".corrupt-{stamp}-{attempt}"));
            }
            match fs::rename(&candidate, &target) {
                Ok(()) => {
                    if candidate == path {
                        moved_primary = true;
                    }
                }
                Err(_) => {
                    if candidate == path {
                        return false;
                    }
                    // The primary file is gone; a stale sidecar would only confuse the fresh
                    // database, so drop it rather than keep it.
                    let _ = fs::remove_file(&candidate);
                }
            }
        }
        moved_primary
    }

    fn open_at(path: &Path) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 2000;
             CREATE TABLE IF NOT EXISTS provider_snapshots (
               provider_id TEXT PRIMARY KEY,
               payload TEXT NOT NULL,
               refreshed_at TEXT NOT NULL,
               identity_key TEXT
             );
             CREATE TABLE IF NOT EXISTS log_file_cache (
               provider_id TEXT NOT NULL,
               path TEXT NOT NULL,
               size INTEGER NOT NULL,
               modified_nanos INTEGER NOT NULL,
               events_json TEXT NOT NULL,
               PRIMARY KEY(provider_id, path)
             );
             CREATE TABLE IF NOT EXISTS app_settings (
               id INTEGER PRIMARY KEY CHECK (id = 1),
               payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS panel_state (
               id INTEGER PRIMARY KEY CHECK (id = 1),
               height INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS provider_environment (
               id INTEGER PRIMARY KEY CHECK (id = 1),
               payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS provider_account_records (
               provider_family TEXT NOT NULL,
               identity_key TEXT NOT NULL,
               provider_id TEXT NOT NULL,
               payload TEXT NOT NULL,
               PRIMARY KEY(provider_family, identity_key),
               UNIQUE(provider_family, provider_id)
             );
             CREATE TABLE IF NOT EXISTS provider_account_graveyard (
               provider_family TEXT NOT NULL,
               identity_key TEXT NOT NULL,
               PRIMARY KEY(provider_family, identity_key)
             );
             CREATE TABLE IF NOT EXISTS quota_history (
               provider_id TEXT NOT NULL,
               quota_id TEXT NOT NULL,
               sampled_at TEXT NOT NULL,
               used_percent REAL NOT NULL,
               PRIMARY KEY(provider_id, quota_id, sampled_at)
             );",
        )?;
        if !Self::has_column(&connection, "log_file_cache", "modified_nanos")? {
            // Parsed log rows are disposable. Rebuilding the table is safer than converting the old
            // millisecond timestamp because the conversion would preserve the very collisions this
            // migration removes. The next refresh repopulates it from the source logs.
            connection.execute_batch(
                "DROP TABLE log_file_cache;
                 CREATE TABLE log_file_cache (
                   provider_id TEXT NOT NULL,
                   path TEXT NOT NULL,
                   size INTEGER NOT NULL,
                   modified_nanos INTEGER NOT NULL,
                   events_json TEXT NOT NULL,
                   PRIMARY KEY(provider_id, path)
                 );",
            )?;
        }
        if !Self::has_column(&connection, "provider_snapshots", "identity_key")? {
            connection.execute(
                "ALTER TABLE provider_snapshots ADD COLUMN identity_key TEXT",
                [],
            )?;
        }
        if !Self::has_column(&connection, "panel_state", "width")? {
            connection.execute("ALTER TABLE panel_state ADD COLUMN width INTEGER", [])?;
        }
        // Legacy rows keep their height as a dormant value but start in automatic mode: presence
        // of a stored height no longer implies a manual preference.
        if !Self::has_column(&connection, "panel_state", "height_mode")? {
            connection.execute("ALTER TABLE panel_state ADD COLUMN height_mode TEXT", [])?;
        }
        if !Self::has_column(&connection, "provider_account_graveyard", "provider_family")? {
            // New in the isolated comparison commit; creates are no-ops when absent.
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS provider_account_graveyard (
               provider_family TEXT NOT NULL,
               identity_key TEXT NOT NULL,
               PRIMARY KEY(provider_family, identity_key)
             );",
            )?;
        }
        // The daily_usage table was rewritten on every snapshot save but never read — the
        // snapshot JSON payload already carries the daily history. Drop it once to reclaim
        // the space; a failed drop is harmless and logged rather than fatal.
        if let Err(error) = connection.execute("DROP TABLE IF EXISTS daily_usage", []) {
            crate::app_warn!(
                "storage",
                "dead daily_usage table could not be dropped: {error}"
            );
        }
        let reader_connection = Connection::open(path)?;
        reader_connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 2000;",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
            reader_connection: Mutex::new(reader_connection),
        })
    }

    #[cfg(test)]
    pub fn load_snapshot(
        &self,
        provider_id: &str,
    ) -> Result<Option<ProviderSnapshot>, StorageError> {
        self.load_snapshot_for_identity(provider_id, CacheIdentity::Unscoped)
    }

    pub fn load_snapshot_for_identity(
        &self,
        provider_id: &str,
        identity: CacheIdentity<'_>,
    ) -> Result<Option<ProviderSnapshot>, StorageError> {
        let connection = self.reader()?;
        let cached: Option<(String, Option<String>)> = connection
            .query_row(
                "SELECT payload, identity_key FROM provider_snapshots WHERE provider_id = ?1",
                [provider_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        cached
            .filter(|(_, cached_identity)| match identity {
                CacheIdentity::Unscoped => cached_identity.is_none(),
                CacheIdentity::Resolved(expected) => cached_identity.as_deref() == Some(expected),
                CacheIdentity::Unresolved => true,
            })
            .map(|json| {
                let mut snapshot: ProviderSnapshot = serde_json::from_str(&json.0)?;
                // Count quotas predate the unit field. The only count quota persisted by older
                // releases was request-based, so normalize it once at the cache boundary instead of
                // teaching every presentation surface to infer provider semantics.
                for quota in &mut snapshot.quotas {
                    if quota.format == crate::models::QuotaFormat::Count && quota.unit.is_none() {
                        quota.unit = Some("requests".into());
                    }
                }
                Ok(snapshot)
            })
            .transpose()
    }

    #[cfg(test)]
    pub fn save_snapshot(&self, snapshot: &ProviderSnapshot) -> Result<(), StorageError> {
        self.save_snapshot_for_identity(snapshot, None)
    }

    pub fn save_snapshot_for_identity(
        &self,
        snapshot: &ProviderSnapshot,
        identity_key: Option<&str>,
    ) -> Result<(), StorageError> {
        let payload = serde_json::to_string(snapshot)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO provider_snapshots(provider_id, payload, refreshed_at, identity_key)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(provider_id) DO UPDATE SET
               payload = excluded.payload,
               refreshed_at = excluded.refreshed_at,
               identity_key = excluded.identity_key",
            params![
                snapshot.provider_id,
                payload,
                snapshot.refreshed_at.to_rfc3339(),
                identity_key,
            ],
        )?;
        Self::record_quota_history(&transaction, snapshot)?;
        transaction.commit()?;
        Ok(())
    }

    /// Persists one quota-level sample per hour bucket so the sparkline keeps a bounded trail of
    /// how each limit moved, even for providers with no local usage logs. Rows older than the
    /// trailing month are pruned on the same write.
    fn record_quota_history(
        transaction: &rusqlite::Transaction<'_>,
        snapshot: &ProviderSnapshot,
    ) -> Result<(), StorageError> {
        let bucket =
            |time: chrono::DateTime<chrono::Utc>| time.format("%Y-%m-%dT%H:00:00Z").to_string();
        let sampled_at = bucket(snapshot.refreshed_at);
        for quota in &snapshot.quotas {
            transaction.execute(
                "INSERT INTO quota_history(provider_id, quota_id, sampled_at, used_percent)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(provider_id, quota_id, sampled_at) DO UPDATE SET
                   used_percent = excluded.used_percent",
                params![
                    snapshot.provider_id,
                    quota.id,
                    sampled_at,
                    quota.used_percent
                ],
            )?;
        }
        let cutoff = bucket(snapshot.refreshed_at - chrono::Duration::days(31));
        transaction.execute("DELETE FROM quota_history WHERE sampled_at < ?1", [cutoff])?;
        Ok(())
    }

    /// Loads the quota history trail at one sample per day (the freshest of each day), grouped
    /// by provider, ordered oldest to newest.
    pub fn load_quota_history(
        &self,
    ) -> Result<HashMap<String, Vec<crate::models::QuotaHistorySample>>, StorageError> {
        let connection = self.reader()?;
        let mut statement = connection.prepare(
            "SELECT provider_id, quota_id, sampled_at, used_percent FROM (
               SELECT provider_id, quota_id, sampled_at, used_percent,
                      ROW_NUMBER() OVER (
                        PARTITION BY provider_id, quota_id, substr(sampled_at, 1, 10)
                        ORDER BY sampled_at DESC
                      ) AS day_rank
               FROM quota_history
             ) WHERE day_rank = 1
             ORDER BY provider_id, quota_id, sampled_at",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    crate::models::QuotaHistorySample {
                        quota_id: row.get(1)?,
                        sampled_at: row.get(2)?,
                        used_percent: row.get(3)?,
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut history: HashMap<String, Vec<crate::models::QuotaHistorySample>> = HashMap::new();
        for (provider_id, sample) in rows {
            history.entry(provider_id).or_default().push(sample);
        }
        Ok(history)
    }

    pub fn load_log_events(
        &self,
        provider_id: &str,
        path: &Path,
        size: u64,
        modified_nanos: i64,
    ) -> Result<Option<String>, StorageError> {
        let connection = self.reader()?;
        connection
            .query_row(
                "SELECT events_json FROM log_file_cache
                 WHERE provider_id = ?1 AND path = ?2 AND size = ?3 AND modified_nanos = ?4",
                params![
                    provider_id,
                    path.to_string_lossy(),
                    size as i64,
                    modified_nanos
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn save_log_events(
        &self,
        provider_id: &str,
        path: &Path,
        size: u64,
        modified_nanos: i64,
        events_json: &str,
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO log_file_cache(path, size, modified_nanos, events_json, provider_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider_id, path) DO UPDATE SET
               size = excluded.size,
               modified_nanos = excluded.modified_nanos,
               events_json = excluded.events_json",
            params![
                path.to_string_lossy(),
                size as i64,
                modified_nanos,
                events_json,
                provider_id
            ],
        )?;
        Ok(())
    }

    pub fn remove_log_events(&self, provider_id: &str, path: &Path) -> Result<(), StorageError> {
        self.connection()?.execute(
            "DELETE FROM log_file_cache WHERE provider_id = ?1 AND path = ?2",
            params![provider_id, path.to_string_lossy()],
        )?;
        Ok(())
    }

    pub fn prune_log_events(
        &self,
        provider_id: &str,
        seen_paths: &HashSet<PathBuf>,
    ) -> Result<(), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let cached_paths = {
            let mut statement =
                transaction.prepare("SELECT path FROM log_file_cache WHERE provider_id = ?1")?;
            let paths = statement
                .query_map([provider_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            paths
        };
        for cached_path in cached_paths {
            if !seen_paths.contains(Path::new(&cached_path)) {
                transaction.execute(
                    "DELETE FROM log_file_cache WHERE provider_id = ?1 AND path = ?2",
                    params![provider_id, cached_path],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<Option<AppSettings>, StorageError> {
        let connection = self.reader()?;
        let payload: Option<String> = connection
            .query_row("SELECT payload FROM app_settings WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        payload
            .map(|json| serde_json::from_str(&json).map_err(StorageError::from))
            .transpose()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), StorageError> {
        self.save_settings_with_account_updates(settings, &[])
    }

    pub fn save_settings_with_account_updates(
        &self,
        settings: &AppSettings,
        account_updates: &[ProviderAccountUpdate],
    ) -> Result<(), StorageError> {
        let payload = serde_json::to_string(settings)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for update in account_updates {
            transaction.execute(
                "INSERT INTO provider_account_records(
                   provider_family, identity_key, provider_id, payload
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(provider_family, identity_key) DO UPDATE SET
                   provider_id = excluded.provider_id,
                   payload = excluded.payload",
                params![
                    update.provider_family,
                    update.identity_key,
                    update.provider_id,
                    update.payload,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO app_settings(id, payload) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET payload = excluded.payload",
            [payload],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_provider_environment(
        &self,
    ) -> Result<Option<HashMap<String, String>>, StorageError> {
        let connection = self.reader()?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM provider_environment WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(payload.and_then(|json| match serde_json::from_str(&json) {
            Ok(environment) => Some(environment),
            Err(error) => {
                crate::app_warn!(
                    "storage",
                    "stored provider environment is unreadable and was reset: {error}"
                );
                None
            }
        }))
    }

    #[cfg(unix)]
    pub fn save_provider_environment(
        &self,
        environment: &HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let payload = serde_json::to_string(environment)?;
        self.connection()?.execute(
            "INSERT INTO provider_environment(id, payload) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET payload = excluded.payload",
            [payload],
        )?;
        Ok(())
    }

    pub fn load_provider_account_records(
        &self,
        provider_family: &str,
    ) -> Result<Vec<(String, String, String)>, StorageError> {
        let connection = self.reader()?;
        let mut statement = connection.prepare(
            "SELECT identity_key, provider_id, payload
             FROM provider_account_records
             WHERE provider_family = ?1
             ORDER BY provider_id",
        )?;
        let records = statement
            .query_map([provider_family], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        Ok(records)
    }

    pub fn load_all_provider_account_records(
        &self,
    ) -> Result<Vec<(String, String, String, String)>, StorageError> {
        let connection = self.reader()?;
        let mut statement = connection.prepare(
            "SELECT provider_family, identity_key, provider_id, payload
             FROM provider_account_records
             ORDER BY provider_family, provider_id, identity_key",
        )?;
        let records = statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        Ok(records)
    }

    pub fn load_observed_account_provider_ids(&self) -> Result<Vec<String>, StorageError> {
        let connection = self.reader()?;
        let mut statement = connection.prepare(
            "SELECT DISTINCT provider_id
             FROM provider_account_records
             ORDER BY provider_id",
        )?;
        let provider_ids = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        Ok(provider_ids)
    }

    pub fn save_provider_account_record(
        &self,
        provider_family: &str,
        identity_key: &str,
        provider_id: &str,
        payload: &str,
    ) -> Result<(), StorageError> {
        self.connection()?.execute(
            "INSERT INTO provider_account_records(
               provider_family, identity_key, provider_id, payload
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(provider_family, identity_key) DO UPDATE SET
               provider_id = excluded.provider_id,
               payload = excluded.payload",
            params![provider_family, identity_key, provider_id, payload],
        )?;
        Ok(())
    }

    pub fn delete_provider_account_by_identity(
        &self,
        provider_family: &str,
        identity_key: &str,
    ) -> Result<bool, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let deleted = transaction.execute(
            "DELETE FROM provider_account_records WHERE provider_family = ?1 AND identity_key = ?2",
            params![provider_family, identity_key],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO provider_account_graveyard(provider_family, identity_key) VALUES (?1, ?2)",
            params![provider_family, identity_key],
        )?;
        transaction.commit()?;
        Ok(deleted > 0)
    }

    /// Allocates an API-key account under `family` with a stable `@<8 hex>` suffix. The suffix
    /// is bounded to hash-miss attempts so it cannot grow without limit: absent `ApiKeyIdentity`
    /// names and occupied provider ids fallback preserve the shared helpers. Returns the already
    /// allocated provider id on re-entry for the same identity.
    pub fn allocate_api_key_account(
        &self,
        provider_family: &str,
        account_name: &str,
    ) -> Result<String, StorageError> {
        let account_name = account_name.trim();
        if account_name.is_empty() || account_name.chars().count() > 48 {
            return Err(StorageError::InvalidInput(
                "Provider account name must be 1 to 48 characters.".to_owned(),
            ));
        }
        let mut identity_suffix = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            account_name.to_ascii_lowercase().hash(&mut hasher);
            provider_family.hash(&mut hasher);
            format!("{:08x}", hasher.finish() & 0xffff_ffff)
        };
        let mut attempts: u32 = 0;
        let raw_identity = format!("{provider_family}:{account_name}");
        loop {
            let identity_key = format!("{raw_identity}#{identity_suffix}");
            let mut connection = self.connection()?;
            let transaction = connection.transaction()?;
            let exists: Option<(String, String)> = transaction
                .query_row(
                    "SELECT provider_id, payload FROM provider_account_records WHERE provider_family = ?1 AND identity_key = ?2",
                    params![provider_family, identity_key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((provider_id, _)) = exists {
                return Ok(provider_id);
            }
            if transaction
                .query_row(
                    "SELECT 1 FROM provider_account_graveyard WHERE provider_family = ?1 AND identity_key = ?2",
                    params![provider_family, identity_key],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some()
            {
                // The identity was previously removed; keep its id retired so it cannot be
                // reallocated to a different logical account.
                attempts = attempts.saturating_add(1);
                if attempts >= 32 {
                    return Err(StorageError::Poisoned);
                }
                identity_suffix = format!("{:08x}", attempts.wrapping_mul(0x9e37_79b9));
                continue;
            }
            let provider_id = format!("{provider_family}@{identity_suffix}");
            let occupied: Option<i64> = transaction
                .query_row(
                    "SELECT 1 FROM provider_account_records WHERE provider_family = ?1 AND provider_id = ?2",
                    params![provider_family, provider_id],
                    |row| row.get(0),
                )
                .optional()?;
            if occupied.is_some() {
                attempts = attempts.saturating_add(1);
                if attempts >= 32 {
                    return Err(StorageError::Poisoned);
                }
                identity_suffix = format!("{:08x}", attempts.wrapping_mul(0x9e37_79b9));
                continue;
            }
            let payload = serde_json::json!({"customName": account_name}).to_string();
            let inserted =
                transaction.execute(
                    "INSERT OR IGNORE INTO provider_account_records(provider_family, identity_key, provider_id, payload) VALUES (?1, ?2, ?3, ?4)",
                    params![provider_family, identity_key, provider_id, payload],
                )?;
            if inserted == 0 {
                // Another process won the race; retry from a fresh read of the table.
                transaction.commit()?;
                attempts = attempts.saturating_add(1);
                if attempts >= 32 {
                    return Err(StorageError::Poisoned);
                }
                identity_suffix = format!("{:08x}", attempts.wrapping_mul(0x9e37_79b9));
                continue;
            }
            transaction.commit()?;
            return Ok(provider_id);
        }
    }

    pub fn load_panel_height(&self) -> Result<Option<u32>, StorageError> {
        let connection = self.reader()?;
        let height = connection
            .query_row("SELECT height FROM panel_state WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?;
        // Width-only upserts write a zero height placeholder; treat it as "no height".
        Ok(height
            .and_then(|value| u32::try_from(value).ok())
            .filter(|height| *height > 0))
    }

    pub fn save_panel_height(&self, height: u32) -> Result<(), StorageError> {
        self.connection()?.execute(
            "INSERT INTO panel_state(id, height, height_mode) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
               height = excluded.height,
               height_mode = excluded.height_mode",
            params![i64::from(height), MANUAL_HEIGHT_MODE],
        )?;
        Ok(())
    }

    /// Switches the persisted height preference to automatic. The stored height is kept as the
    /// dormant value the next manual choice starts from; absence of a row is automatic too.
    pub fn mark_panel_height_automatic(&self) -> Result<(), StorageError> {
        self.connection()?.execute(
            "UPDATE panel_state SET height_mode = 'automatic' WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    pub fn load_panel_height_mode(&self) -> Result<Option<String>, StorageError> {
        let connection = self.reader()?;
        let mode = connection
            .query_row(
                "SELECT height_mode FROM panel_state WHERE id = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(mode.flatten())
    }

    pub fn load_panel_width(&self) -> Result<Option<u32>, StorageError> {
        let connection = self.reader()?;
        let width = connection
            .query_row("SELECT width FROM panel_state WHERE id = 1", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .optional()?
            .flatten();
        Ok(width.and_then(|value| u32::try_from(value).ok()))
    }

    pub fn save_panel_width(&self, width: u32) -> Result<(), StorageError> {
        // height is NOT NULL on panel_state, so preserve the stored height (or 0) when upserting the
        // width on a row that does not exist yet. The read and the upsert share one transaction so
        // a concurrent manual-height save cannot be clobbered with a stale placeholder.
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let height: Option<i64> = transaction
            .query_row("SELECT height FROM panel_state WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        transaction.execute(
            "INSERT INTO panel_state(id, height, width) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET width = excluded.width",
            [height.unwrap_or(0), i64::from(width)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, StorageError> {
        self.connection.lock().map_err(|_| StorageError::Poisoned)
    }

    /// Lock for read-only queries. Separate from the write connection so WAL
    /// readers never queue behind a snapshot save.
    fn reader(&self) -> Result<MutexGuard<'_, Connection>, StorageError> {
        self.reader_connection
            .lock()
            .map_err(|_| StorageError::Poisoned)
    }

    fn has_column(
        connection: &Connection,
        table: &str,
        column: &str,
    ) -> Result<bool, rusqlite::Error> {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(columns.iter().any(|name| name == column))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, path::PathBuf};

    use chrono::Utc;
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::{ProviderAccountUpdate, Storage};
    use crate::models::{
        AppSettings, DailyUsage, ModelUsageBreakdown, ModelUsageEntry, ProviderSnapshot,
        QuotaFormat, QuotaWindow, UsageHistory, UsagePeriod,
    };
    use crate::providers::CacheIdentity;

    #[test]
    fn quota_history_keeps_one_sample_per_hour_and_day() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let quota = |used_percent: f64| QuotaWindow {
            id: "session".into(),
            label: "Session".into(),
            used_percent,
            resets_at: None,
            period_seconds: 1,
            format: QuotaFormat::Percent,
            used_value: None,
            limit_value: None,
            unit: None,
            estimated: false,
            source_note: None,
        };
        let snapshot_at = |day: u32, hour: u32, used_percent: f64| ProviderSnapshot {
            provider_id: "codex".into(),
            plan: None,
            quotas: vec![quota(used_percent)],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc.with_ymd_and_hms(2026, 8, day, hour, 5, 0).unwrap(),
        };

        // Two saves inside the same hour bucket replace each other; the next day is kept.
        use chrono::TimeZone;
        storage.save_snapshot(&snapshot_at(20, 9, 40.0)).unwrap();
        storage.save_snapshot(&snapshot_at(20, 9, 45.0)).unwrap();
        storage.save_snapshot(&snapshot_at(21, 11, 60.0)).unwrap();

        let history = storage.load_quota_history().unwrap();
        let samples = history.get("codex").unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].quota_id, "session");
        assert_eq!(samples[0].used_percent, 45.0);
        assert_eq!(
            samples[0].sampled_at, "2026-08-20T09:00:00Z",
            "same-hour samples replace the earlier value"
        );
        assert_eq!(samples[1].used_percent, 60.0);

        // Samples past the trailing month are pruned by the newest save.
        let mut recent = snapshot_at(21, 11, 80.0);
        recent.refreshed_at = Utc.with_ymd_and_hms(2026, 9, 25, 1, 0, 0).unwrap();
        storage.save_snapshot(&recent).unwrap();
        let history = storage.load_quota_history().unwrap();
        let samples = history.get("codex").unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].used_percent, 80.0);
    }

    #[test]
    fn corrupted_database_is_moved_aside_and_recreated() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("usagedeck.db");
        std::fs::write(&path, b"this is definitely not a sqlite database").unwrap();
        std::fs::write(path.with_file_name("usagedeck.db-wal"), b"stale wal").unwrap();

        let storage = Storage::open(&path).unwrap();
        let snapshot = ProviderSnapshot {
            provider_id: "codex".into(),
            plan: Some("Plus".into()),
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        storage.save_snapshot(&snapshot).unwrap();
        assert_eq!(storage.load_snapshot("codex").unwrap(), Some(snapshot));

        let mut quarantined = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().into_string().ok()?;
                name.starts_with("usagedeck.db.corrupt-").then_some(name)
            })
            .collect::<Vec<_>>();
        quarantined.sort();
        assert_eq!(quarantined.len(), 1, "only the primary file remains to quarantine after the stale WAL is dropped or moved: {quarantined:?}");
        assert!(quarantined[0].starts_with("usagedeck.db.corrupt-"));
        assert_eq!(
            std::fs::read(directory.path().join(&quarantined[0])).unwrap(),
            b"this is definitely not a sqlite database"
        );
    }

    #[test]
    fn snapshot_round_trip_contains_no_credentials() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let snapshot = ProviderSnapshot {
            provider_id: "codex".into(),
            plan: Some("Plus".into()),
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory {
                today: Some(UsagePeriod {
                    tokens: 42,
                    estimated_cost_usd: Some(0.12),
                    cost_estimated: true,
                    estimate_complete: true,
                    model_breakdown: Some(ModelUsageBreakdown {
                        models: vec![ModelUsageEntry {
                            model: "gpt-5.4".into(),
                            total_tokens: 42,
                            cost_usd: Some(0.12),
                            variants: None,
                        }],
                        source_note: "From your Codex logs (estimated)".into(),
                    }),
                    unknown_models: Vec::new(),
                }),
                daily: vec![DailyUsage {
                    date: "2026-07-10".into(),
                    tokens: 42,
                    estimated_cost_usd: Some(0.12),
                    estimate_complete: true,
                }],
                ..UsageHistory::default()
            },
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };

        storage.save_snapshot(&snapshot).unwrap();

        assert_eq!(storage.load_snapshot("codex").unwrap(), Some(snapshot));
        let bytes = std::fs::read(directory.path().join("usagedeck.db")).unwrap();
        let database = String::from_utf8_lossy(&bytes);
        assert!(!database.contains("access_token"));
        assert!(!database.contains("refresh_token"));
    }

    #[test]
    fn account_scoped_snapshot_is_visible_only_to_the_same_identity() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let snapshot = ProviderSnapshot {
            provider_id: "claude".into(),
            plan: Some("Max".into()),
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };

        storage
            .save_snapshot_for_identity(&snapshot, Some("identity-a"))
            .unwrap();

        assert_eq!(
            storage
                .load_snapshot_for_identity("claude", CacheIdentity::Resolved("identity-a"))
                .unwrap(),
            Some(snapshot.clone())
        );
        assert!(storage
            .load_snapshot_for_identity("claude", CacheIdentity::Resolved("identity-b"))
            .unwrap()
            .is_none());
        assert!(storage.load_snapshot("claude").unwrap().is_none());
        assert_eq!(
            storage
                .load_snapshot_for_identity("claude", CacheIdentity::Unresolved)
                .unwrap(),
            Some(snapshot)
        );
    }

    #[test]
    fn provider_account_records_round_trip_by_family() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();

        storage
            .save_provider_account_record(
                "claude",
                "identity-a",
                "claude@12345678",
                r#"{"displayName":"Claude — Work"}"#,
            )
            .unwrap();

        assert_eq!(
            storage.load_provider_account_records("claude").unwrap(),
            [(
                "identity-a".to_owned(),
                "claude@12345678".to_owned(),
                r#"{"displayName":"Claude — Work"}"#.to_owned(),
            )]
        );
        assert!(storage
            .load_provider_account_records("codex")
            .unwrap()
            .is_empty());
        assert_eq!(
            storage.load_observed_account_provider_ids().unwrap(),
            ["claude@12345678"]
        );
    }

    #[test]
    fn account_records_and_settings_roll_back_together() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let original = AppSettings {
            theme: crate::models::ThemePreference::Light,
            ..AppSettings::default()
        };
        storage.save_settings(&original).unwrap();
        storage
            .save_provider_account_record("codex", "identity-a", "codex", r#"{"name":"old"}"#)
            .unwrap();
        let changed = AppSettings {
            theme: crate::models::ThemePreference::Dark,
            ..original.clone()
        };
        let updates = [
            ProviderAccountUpdate {
                provider_family: "codex".into(),
                identity_key: "identity-a".into(),
                provider_id: "codex".into(),
                payload: r#"{"name":"changed"}"#.into(),
            },
            ProviderAccountUpdate {
                provider_family: "codex".into(),
                identity_key: "identity-b".into(),
                provider_id: "codex".into(),
                payload: "{}".into(),
            },
        ];

        assert!(storage
            .save_settings_with_account_updates(&changed, &updates)
            .is_err());
        assert_eq!(storage.load_settings().unwrap(), Some(original));
        let records = storage.load_provider_account_records("codex").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].2, r#"{"name":"old"}"#);
    }

    #[test]
    fn legacy_snapshot_table_gains_the_account_identity_column() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("usagedeck.db");
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE provider_snapshots (
                   provider_id TEXT PRIMARY KEY,
                   payload TEXT NOT NULL,
                   refreshed_at TEXT NOT NULL
                 );",
            )
            .unwrap();
        drop(legacy);

        let storage = Storage::open(&path).unwrap();
        let connection = storage.connection().unwrap();

        assert!(Storage::has_column(&connection, "provider_snapshots", "identity_key").unwrap());
    }

    #[test]
    fn legacy_count_quota_cache_is_normalized_at_load_boundary() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let snapshot = ProviderSnapshot {
            provider_id: "cursor".into(),
            plan: None,
            quotas: vec![QuotaWindow {
                id: "requests".into(),
                label: "Requests".into(),
                used_percent: 25.0,
                resets_at: None,
                period_seconds: 2_592_000,
                format: QuotaFormat::Count,
                used_value: Some(25.0),
                limit_value: Some(100.0),
                unit: None,
                estimated: false,
                source_note: None,
            }],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };

        storage.save_snapshot(&snapshot).unwrap();

        assert_eq!(
            storage.load_snapshot("cursor").unwrap().unwrap().quotas[0].unit,
            Some("requests".into())
        );
    }

    #[test]
    fn settings_round_trip_uses_the_same_disk_database() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let settings = AppSettings {
            always_show_pacing: true,
            ..AppSettings::default()
        };
        storage.save_settings(&settings).unwrap();
        assert_eq!(storage.load_settings().unwrap(), Some(settings));
    }

    #[test]
    fn panel_height_round_trip_is_independent_from_app_settings() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let settings = AppSettings::default();
        storage.save_settings(&settings).unwrap();

        assert_eq!(storage.load_panel_height().unwrap(), None);
        assert_eq!(storage.load_panel_height_mode().unwrap(), None);
        storage.save_panel_height(734).unwrap();

        assert_eq!(storage.load_panel_height().unwrap(), Some(734));
        assert_eq!(
            storage.load_panel_height_mode().unwrap().as_deref(),
            Some(super::MANUAL_HEIGHT_MODE)
        );
        assert_eq!(storage.load_settings().unwrap(), Some(settings));

        // Automatic keeps the height as a dormant value while restoring the automatic mode.
        storage.mark_panel_height_automatic().unwrap();
        assert_eq!(storage.load_panel_height().unwrap(), Some(734));
        assert_eq!(
            storage.load_panel_height_mode().unwrap().as_deref(),
            Some("automatic")
        );
    }

    #[test]
    fn panel_width_round_trip_preserves_height_and_is_independent_from_app_settings() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let settings = AppSettings::default();
        storage.save_settings(&settings).unwrap();

        assert_eq!(storage.load_panel_width().unwrap(), None);
        storage.save_panel_height(734).unwrap();
        storage.save_panel_width(460).unwrap();

        assert_eq!(storage.load_panel_width().unwrap(), Some(460));
        assert_eq!(storage.load_panel_height().unwrap(), Some(734));
        assert_eq!(storage.load_settings().unwrap(), Some(settings));
    }

    #[test]
    fn log_cache_pruning_is_scoped_to_a_provider() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let codex_old = PathBuf::from("/logs/codex-old.jsonl");
        let codex_current = PathBuf::from("/logs/codex-current.jsonl");
        let claude_current = PathBuf::from("/logs/claude-current.jsonl");
        for (provider, path) in [
            ("codex", &codex_old),
            ("codex", &codex_current),
            ("claude", &claude_current),
        ] {
            storage
                .save_log_events(provider, path, 10, 20, "[]")
                .unwrap();
        }

        storage
            .prune_log_events("codex", &HashSet::from([codex_current.clone()]))
            .unwrap();

        assert_eq!(
            storage
                .load_log_events("codex", &codex_old, 10, 20)
                .unwrap(),
            None
        );
        assert_eq!(
            storage
                .load_log_events("codex", &codex_current, 10, 20)
                .unwrap(),
            Some("[]".to_owned())
        );
        assert_eq!(
            storage
                .load_log_events("claude", &claude_current, 10, 20)
                .unwrap(),
            Some("[]".to_owned())
        );
    }

    #[test]
    fn providers_can_cache_the_same_path_independently() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("usagedeck.db")).unwrap();
        let shared = PathBuf::from("/synced/session.jsonl");
        storage
            .save_log_events("claude", &shared, 10, 20, "claude")
            .unwrap();
        storage
            .save_log_events("codex", &shared, 10, 20, "codex")
            .unwrap();

        assert_eq!(
            storage.load_log_events("claude", &shared, 10, 20).unwrap(),
            Some("claude".to_owned())
        );
        assert_eq!(
            storage.load_log_events("codex", &shared, 10, 20).unwrap(),
            Some("codex".to_owned())
        );
    }

    #[test]
    fn legacy_millisecond_log_cache_is_safely_rebuilt() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("usagedeck.db");
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE log_file_cache (
                   path TEXT PRIMARY KEY,
                   size INTEGER NOT NULL,
                   modified_millis INTEGER NOT NULL,
                   events_json TEXT NOT NULL,
                   provider_id TEXT NOT NULL DEFAULT ''
                 );
                 INSERT INTO log_file_cache VALUES ('old.jsonl', 10, 20, '[]', 'codex');",
            )
            .unwrap();
        drop(legacy);

        let storage = Storage::open(&path).unwrap();
        assert_eq!(
            storage
                .load_log_events("codex", PathBuf::from("old.jsonl").as_path(), 10, 20)
                .unwrap(),
            None
        );
        storage
            .save_log_events("codex", PathBuf::from("new.jsonl").as_path(), 10, 20, "[]")
            .unwrap();
    }
}
