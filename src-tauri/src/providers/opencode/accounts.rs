//! Discovery of additional OpenCode data directories — one card per login.
//!
//! Everything an OpenCode account needs (its `opencode-go` credential and its
//! usage databases) lives inside one data directory, so sibling directories
//! following the `opencode-<name>` convention next to the default directory
//! are separate logins, exactly like alternate `CLAUDE_CONFIG_DIR`s for
//! Claude. Accounts appear when the directory holds a subscribed login; no
//! credential ever passes through UsageDeck.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use super::paths::{data_directory, OpenCodePaths};
use crate::hashing::sha256_hex;

const DISCOVERY_BUDGET: Duration = Duration::from_millis(400);

pub(crate) struct OpenCodeAccount {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) data_directory: PathBuf,
}

pub(crate) fn discover() -> Vec<OpenCodeAccount> {
    discover_with(
        |name| std::env::var(name).ok(),
        &crate::providers::home_directory(),
        Instant::now(),
    )
}

pub(crate) fn discover_with(
    environment: impl Fn(&str) -> Option<String>,
    home: &std::path::Path,
    started: Instant,
) -> Vec<OpenCodeAccount> {
    let default_directory = data_directory(&environment, home);
    let Some(parent) = default_directory.parent() else {
        return Vec::new();
    };
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut accounts = Vec::new();
    for entry in entries.flatten() {
        if started.elapsed() > DISCOVERY_BUDGET {
            break;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(label) = name.strip_prefix("opencode-") else {
            continue;
        };
        if label.is_empty() || entry.path() == default_directory {
            continue;
        }
        // Only a directory with a subscribed login is an account; a data
        // directory without an `opencode-go` key is just a data directory.
        let paths = OpenCodePaths::for_data_directory(entry.path());
        if !matches!(paths.go_api_key(), Ok(Some(_))) {
            continue;
        }
        let canonical =
            std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().clone());
        let display_name = label.trim();
        accounts.push(OpenCodeAccount {
            id: format!(
                "opencode@{}",
                &sha256_hex(canonical.to_string_lossy().as_bytes())[..8]
            ),
            display_name: if display_name.is_empty() {
                "OpenCode".to_owned()
            } else {
                display_name.to_owned()
            },
            data_directory: entry.path(),
        });
    }
    accounts
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, time::Instant};

    use tempfile::tempdir;

    use super::discover_with;

    fn write_login(directory: &std::path::Path, key: Option<&str>) {
        fs::create_dir_all(directory).unwrap();
        match key {
            Some(key) => fs::write(
                directory.join("auth.json"),
                format!(r#"{{"opencode-go":{{"type":"api","key":"{key}"}}}}"#),
            )
            .unwrap(),
            None => {
                let path = directory.join("auth.json");
                if path.exists() {
                    fs::remove_file(path).unwrap();
                }
            }
        }
    }

    #[test]
    fn discovers_subscribed_sibling_directories_only() {
        let home = tempdir().unwrap();
        let share = home.path().join(".local").join("share");
        write_login(&share.join("opencode"), Some("default-key"));
        write_login(&share.join("opencode-work"), Some("work-key"));
        // A sibling without a subscribed login is just a data directory.
        write_login(&share.join("opencode-personal"), None);
        // Unrelated and malformed names never match.
        write_login(&share.join("opencode-"), Some("dash-only"));
        write_login(&share.join("other-work"), Some("other"));
        fs::create_dir_all(share.join("opencode.db-wal")).unwrap();

        let accounts = discover_with(|_| None, home.path(), Instant::now());

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].display_name, "work");
        assert!(accounts[0].id.starts_with("opencode@"));
        assert_eq!(accounts[0].id.len(), "opencode@".len() + 8);
        assert_eq!(accounts[0].data_directory, share.join("opencode-work"));
    }

    #[test]
    fn a_custom_data_directory_still_uses_its_parent_for_siblings() {
        let home = tempdir().unwrap();
        let custom_root = home.path().join("custom");
        write_login(&custom_root.join("opencode"), Some("default-key"));
        write_login(&custom_root.join("opencode-alt"), Some("alt-key"));

        let environment = HashMap::from([(
            "OPENCODE_DATA_DIR".to_owned(),
            custom_root.join("opencode").to_string_lossy().into_owned(),
        )]);
        let accounts = discover_with(
            |name| environment.get(name).cloned(),
            home.path(),
            Instant::now(),
        );

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].display_name, "alt");
    }
}
