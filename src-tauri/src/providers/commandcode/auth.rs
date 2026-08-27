use std::{fs, io::Read, path::PathBuf};

use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};

use super::CommandCodeError;
use crate::providers::paths::home_directory;

const MAX_AUTH_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct CommandCodeAuthStore {
    path: PathBuf,
}

impl CommandCodeAuthStore {
    pub fn new() -> Self {
        Self {
            path: home_directory().join(".commandcode").join("auth.json"),
        }
    }

    #[cfg(test)]
    pub(super) fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Zeroizing<String>, CommandCodeError> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(CommandCodeError::NotLoggedIn);
            }
            Err(_) => return Err(CommandCodeError::InvalidAuth),
        };
        let metadata = file.metadata().map_err(|_| CommandCodeError::InvalidAuth)?;
        if !metadata.is_file() || metadata.len() > MAX_AUTH_FILE_BYTES {
            return Err(CommandCodeError::InvalidAuth);
        }

        let mut text = Zeroizing::new(String::with_capacity(metadata.len() as usize));
        file.take(MAX_AUTH_FILE_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|_| CommandCodeError::InvalidAuth)?;
        if text.len() as u64 > MAX_AUTH_FILE_BYTES {
            return Err(CommandCodeError::InvalidAuth);
        }

        let mut document =
            serde_json::from_str::<Value>(&text).map_err(|_| CommandCodeError::InvalidAuth)?;
        let key_value = document
            .as_object_mut()
            .and_then(|object| object.get_mut("apiKey"))
            .ok_or(CommandCodeError::InvalidAuth)?;
        let api_key = key_value
            .as_str()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(|key| Zeroizing::new(key.to_owned()))
            .ok_or(CommandCodeError::InvalidAuth)?;
        if let Value::String(value) = key_value {
            value.zeroize();
        }
        Ok(api_key)
    }

    pub fn has_local_credentials(&self) -> bool {
        self.load().is_ok()
    }
}

impl Default for CommandCodeAuthStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{CommandCodeAuthStore, MAX_AUTH_FILE_BYTES};
    use crate::providers::commandcode::CommandCodeError;

    #[test]
    fn loads_and_trims_the_api_key() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("auth.json");
        fs::write(&path, r#"{"apiKey":"  secret-key  ","unrelated":true}"#).unwrap();

        let api_key = CommandCodeAuthStore::with_path(path).load().unwrap();
        assert_eq!(api_key.as_str(), "secret-key");
    }

    #[test]
    fn distinguishes_missing_credentials_from_invalid_storage() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("auth.json");
        let store = CommandCodeAuthStore::with_path(path.clone());
        assert!(matches!(
            store.load().unwrap_err(),
            CommandCodeError::NotLoggedIn
        ));

        fs::write(&path, r#"{"apiKey":"  "}"#).unwrap();
        assert!(matches!(
            store.load().unwrap_err(),
            CommandCodeError::InvalidAuth
        ));

        fs::write(&path, vec![b' '; MAX_AUTH_FILE_BYTES as usize + 1]).unwrap();
        assert!(matches!(
            store.load().unwrap_err(),
            CommandCodeError::InvalidAuth
        ));
    }
}
