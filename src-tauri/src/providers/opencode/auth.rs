//! V2's active SQLite credential is authoritative, including after logout.
//! V1 auth.json is used only when no V2 credential store exists. OpenCode owns
//! OAuth renewal; UsageDeck only reads the current access token.

use std::{fs, path::Path};

use chrono::{DateTime, Utc};
use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};

use super::{
    database::{open_read_only, table_columns},
    paths::OpenCodePaths,
    OpenCodeError,
};

const MAX_CREDENTIAL_BYTES: i64 = 1024 * 1024;

pub(super) struct GoCredentials {
    pub(super) token: Zeroizing<String>,
    pub(super) organization: Option<String>,
}

enum DatabaseLogin {
    Legacy,
    Current(Option<GoCredentials>),
}

pub(super) fn load(
    paths: &OpenCodePaths,
    now: DateTime<Utc>,
) -> Result<Option<GoCredentials>, OpenCodeError> {
    let mut databases = paths
        .database_files()
        .map_err(|_| OpenCodeError::CredentialsUnreadable)?;
    // Prefer the release database to a prerelease channel's saved login.
    databases.sort_by_key(|path| path.file_name().is_none_or(|name| name != "opencode.db"));
    if let Some(database) = databases.first() {
        match read_database(database, now)? {
            DatabaseLogin::Current(credentials) => return Ok(credentials),
            DatabaseLogin::Legacy => {}
        }
    }
    paths.go_api_key().map(|key| {
        key.map(|token| GoCredentials {
            token,
            organization: None,
        })
    })
}

fn read_database(path: &Path, now: DateTime<Utc>) -> Result<DatabaseLogin, OpenCodeError> {
    match fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DatabaseLogin::Legacy)
        }
        Ok(metadata) if metadata.is_file() => {}
        _ => return Err(OpenCodeError::CredentialsUnreadable),
    }
    let connection = open_read_only(path).map_err(|_| OpenCodeError::CredentialsUnreadable)?;
    let Some(columns) = table_columns(&connection, "credential")
        .map_err(|_| OpenCodeError::CredentialsUnreadable)?
    else {
        return Ok(DatabaseLogin::Legacy);
    };
    if !["id", "integration_id", "active", "value", "time_updated"]
        .iter()
        .all(|column| columns.contains(*column))
    {
        return Err(OpenCodeError::CredentialsUnreadable);
    }
    let mut statement = connection
        .prepare(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?1 THEN value END \
         FROM credential WHERE active = 1 AND integration_id IN ('opencode','opencode-go') \
         ORDER BY CASE integration_id WHEN 'opencode' THEN 0 ELSE 1 END, time_updated DESC, id \
         LIMIT 1",
        )
        .map_err(|_| OpenCodeError::CredentialsUnreadable)?;
    let mut rows = statement
        .query([MAX_CREDENTIAL_BYTES])
        .map_err(|_| OpenCodeError::CredentialsUnreadable)?;
    let Some(row) = rows
        .next()
        .map_err(|_| OpenCodeError::CredentialsUnreadable)?
    else {
        return Ok(DatabaseLogin::Current(None));
    };
    let content = Zeroizing::new(
        row.get::<_, String>(0)
            .map_err(|_| OpenCodeError::CredentialsUnreadable)?,
    );
    let mut value: Value =
        serde_json::from_str(&content).map_err(|_| OpenCodeError::CredentialsUnreadable)?;
    let result = parse(&mut value, now);
    erase_strings(&mut value);
    result.map(DatabaseLogin::Current)
}

fn parse(value: &mut Value, now: DateTime<Utc>) -> Result<Option<GoCredentials>, OpenCodeError> {
    if !value.is_object() {
        return Err(OpenCodeError::CredentialsUnreadable);
    }
    let server = value.pointer("/metadata/server");
    // A custom Console's access token must never be sent to the public service.
    if server.is_some_and(|server| {
        server
            .as_str()
            .is_none_or(|server| server.trim_end_matches('/') != "https://opencode.ai/console")
    }) {
        return Ok(None);
    }
    let field = match value.get("type").and_then(Value::as_str) {
        Some("key" | "api") => "key",
        Some("oauth") => {
            if value
                .get("expires")
                .and_then(Value::as_i64)
                .is_none_or(|expiry| expiry <= now.timestamp_millis())
            {
                return Err(OpenCodeError::InvalidAuth);
            }
            "access"
        }
        _ => return Ok(None),
    };
    let organization = value
        .pointer("/metadata/orgID")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned);
    if organization
        .as_ref()
        .is_some_and(|id| id.parse::<reqwest::header::HeaderValue>().is_err())
    {
        return Err(OpenCodeError::InvalidAuth);
    }
    let Some(Value::String(token)) = value.get_mut(field) else {
        return Err(OpenCodeError::CredentialsUnreadable);
    };
    let token = Zeroizing::new(std::mem::take(token));
    if token.trim().is_empty() {
        return Err(OpenCodeError::InvalidAuth);
    }
    Ok(Some(GoCredentials {
        token,
        organization,
    }))
}

fn erase_strings(value: &mut Value) {
    match value {
        Value::String(value) => value.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(erase_strings),
        Value::Object(values) => values.values_mut().for_each(erase_strings),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rusqlite::{params, Connection};
    use serde_json::json;
    use tempfile::tempdir;

    use super::{GoCredentials, OpenCodeError};
    use crate::providers::opencode::paths::OpenCodePaths;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 18, 0, 0, 0).unwrap()
    }

    pub(crate) fn create_store(connection: &Connection) {
        connection.execute_batch("CREATE TABLE credential (
            id TEXT PRIMARY KEY, integration_id TEXT, label TEXT, value TEXT NOT NULL,
            connector_id TEXT, method_id TEXT, active INTEGER, time_created INTEGER, time_updated INTEGER
        )").unwrap();
    }

    fn save(connection: &Connection, id: &str, integration: &str, active: bool, value: &str) {
        connection.execute("INSERT OR REPLACE INTO credential (id,integration_id,active,value,time_updated) VALUES (?1,?2,?3,?4,1)",params![id,integration,active,value]).unwrap();
    }

    pub(crate) fn oauth() -> serde_json::Value {
        json!({"type":"oauth","methodID":"device","access":"current-access","refresh":"do-not-use-refresh", "expires":now().timestamp_millis()+3_600_000,
            "metadata":{"server":"https://opencode.ai/console","orgID":"org-test"}})
    }

    fn load(paths: &OpenCodePaths) -> Option<GoCredentials> {
        paths.go_credentials(now()).unwrap()
    }

    #[test]
    fn active_console_login_wins_over_legacy_and_inactive_keys_and_tracks_rotation_and_logout() {
        let directory = tempdir().unwrap();
        std::fs::write(
            directory.path().join("auth.json"),
            r#"{"opencode-go":{"type":"api","key":"stale-key"}}"#,
        )
        .unwrap();
        let db = Connection::open(directory.path().join("opencode.db")).unwrap();
        create_store(&db);
        save(
            &db,
            "inactive",
            "opencode",
            false,
            &json!({"type":"key","key":"inactive-key"}).to_string(),
        );
        save(
            &db,
            "go",
            "opencode-go",
            true,
            &json!({"type":"key","key":"go-key"}).to_string(),
        );
        save(&db, "console", "opencode", true, &oauth().to_string());
        let paths = OpenCodePaths::for_data_directory(directory.path().into());
        let credential = load(&paths).unwrap();
        assert_eq!(credential.token.as_str(), "current-access");
        assert_eq!(credential.organization.as_deref(), Some("org-test"));
        let mut value = oauth();
        value["access"] = json!("rotated-access");
        save(&db, "console", "opencode", true, &value.to_string());
        assert_eq!(load(&paths).unwrap().token.as_str(), "rotated-access");
        db.execute("DELETE FROM credential", []).unwrap();
        assert!(
            load(&paths).is_none(),
            "logout must not resurrect auth.json"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM credential", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn v2_key_logins_work_and_unrelated_or_inactive_accounts_are_ignored() {
        let directory = tempdir().unwrap();
        let db = Connection::open(directory.path().join("opencode.db")).unwrap();
        create_store(&db);
        let paths = OpenCodePaths::for_data_directory(directory.path().into());
        save(
            &db,
            "unrelated",
            "anthropic",
            true,
            &json!({"type":"key","key":"other"}).to_string(),
        );
        save(
            &db,
            "inactive",
            "opencode-go",
            false,
            &json!({"type":"key","key":"inactive"}).to_string(),
        );
        assert!(load(&paths).is_none());
        save(
            &db,
            "go",
            "opencode-go",
            true,
            &json!({"type":"key","key":"go-key"}).to_string(),
        );
        assert_eq!(load(&paths).unwrap().token.as_str(), "go-key");
    }

    #[test]
    fn expired_malformed_and_oversized_credentials_fail_without_falling_back() {
        let directory = tempdir().unwrap();
        std::fs::write(
            directory.path().join("auth.json"),
            r#"{"opencode-go":{"key":"stale"}}"#,
        )
        .unwrap();
        let db = Connection::open(directory.path().join("opencode.db")).unwrap();
        create_store(&db);
        let paths = OpenCodePaths::for_data_directory(directory.path().into());
        let mut expired = oauth();
        expired["expires"] = json!(now().timestamp_millis());
        for (value, expected) in [
            (expired.to_string(), OpenCodeError::InvalidAuth),
            (
                "not-json-private".into(),
                OpenCodeError::CredentialsUnreadable,
            ),
            (
                " ".repeat(super::MAX_CREDENTIAL_BYTES as usize + 1),
                OpenCodeError::CredentialsUnreadable,
            ),
        ] {
            save(&db, "console", "opencode", true, &value);
            assert!(matches!(paths.go_credentials(now()),Err(error) if error==expected));
        }
    }

    #[test]
    fn custom_console_tokens_are_not_sent_to_public_opencode() {
        let directory = tempdir().unwrap();
        let db = Connection::open(directory.path().join("opencode.db")).unwrap();
        create_store(&db);
        let mut value = oauth();
        value["metadata"]["server"] = json!("https://other-console.example/console");
        save(&db, "console", "opencode", true, &value.to_string());
        let paths = OpenCodePaths::for_data_directory(directory.path().into());
        assert!(load(&paths).is_none());
    }

    #[test]
    fn a_legacy_database_without_a_credential_table_keeps_v1_auth_support() {
        let directory = tempdir().unwrap();
        std::fs::write(
            directory.path().join("auth.json"),
            r#"{"opencode-go":{"type":"api","key":"legacy-key"}}"#,
        )
        .unwrap();
        let db = Connection::open(directory.path().join("opencode.db")).unwrap();
        db.execute("CREATE TABLE session (id TEXT)", []).unwrap();
        let paths = OpenCodePaths::for_data_directory(directory.path().into());
        assert_eq!(load(&paths).unwrap().token.as_str(), "legacy-key");
    }
}
