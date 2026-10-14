use std::{fs::File, io::Read, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use serde_json::Value;
use zeroize::Zeroizing;

use super::bot::BotError;
use crate::providers::{cursor::auth::CursorAuthState, home_directory};

const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub(super) struct BotLogin {
    pub token: String,
    pub team_id: Option<u64>,
}

crate::redacted_debug!(BotLogin { token, team_id });

pub(super) struct BotAuthStore {
    paths: Vec<PathBuf>,
    use_cursor: bool,
    #[cfg(test)]
    login: Option<BotLogin>,
}

impl BotAuthStore {
    pub fn new() -> Self {
        let home = home_directory();
        let mut roots = vec![home.join("Library/Application Support/Grok Bot")];
        if let Some(root) = std::env::var_os("APPDATA") {
            roots.push(PathBuf::from(root).join("Grok Bot"));
        }
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        roots.push(config.join("Grok Bot"));
        roots.push(config.join("sand"));
        Self {
            paths: roots
                .into_iter()
                .map(|root| root.join("sand-secrets.json"))
                .collect(),
            use_cursor: true,
            #[cfg(test)]
            login: None,
        }
    }

    #[cfg(test)]
    pub fn for_login(login: Option<BotLogin>) -> Self {
        Self {
            paths: vec![],
            use_cursor: false,
            login,
        }
    }

    pub fn has_local_credentials(&self) -> bool {
        #[cfg(test)]
        if self.login.is_some() {
            return true;
        }
        // Discovery must not prompt for the encryption key. Decrypt only when the
        // user refreshes this provider, on its existing background worker.
        self.paths.iter().any(|path| {
            read_json(path)
                .ok()
                .is_some_and(|doc| active_record(&doc).ok().flatten().is_some())
        }) || (self.use_cursor && CursorAuthState::has_local_credentials())
    }

    pub fn load(&self, now: DateTime<Utc>) -> Result<Option<BotLogin>, BotError> {
        #[cfg(test)]
        if let Some(login) = &self.login {
            return Ok(Some(login.clone()));
        }
        for path in &self.paths {
            if !path.exists() {
                continue;
            }
            let doc = read_json(path)?;
            let Some(record) = active_record(&doc)? else {
                continue;
            };
            let decrypt = |value: &str| -> Result<String, BotError> {
                let encoded = unscoped_ciphertext(value)?;
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| BotError::Credentials)?;
                let plain = decrypt_os(&bytes, path)?;
                String::from_utf8(plain.to_vec()).map_err(|_| BotError::Credentials)
            };
            let token = decrypt(
                record
                    .get("cursor-access-token")
                    .and_then(Value::as_str)
                    .ok_or(BotError::Credentials)?,
            )?;
            let team_id = record
                .get("cursor-selected-team-id")
                .and_then(Value::as_str)
                .map(decrypt)
                .transpose()?
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|id| *id > 0);
            return validate_login(BotLogin { token, team_id }, now).map(Some);
        }
        if self.use_cursor {
            if let Some(auth) = CursorAuthState::load().map_err(|_| BotError::Credentials)? {
                if let Some(token) = auth.access_token {
                    return validate_login(
                        BotLogin {
                            token,
                            team_id: None,
                        },
                        now,
                    )
                    .map(Some);
                }
            }
        }
        Ok(None)
    }
}

fn read_json(path: &std::path::Path) -> Result<Value, BotError> {
    let file = File::open(path).map_err(|_| BotError::Credentials)?;
    let metadata = file.metadata().map_err(|_| BotError::Credentials)?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(BotError::Credentials);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BotError::Credentials)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(BotError::Credentials);
    }
    serde_json::from_slice(&bytes).map_err(|_| BotError::Credentials)
}

fn active_record(doc: &Value) -> Result<Option<Value>, BotError> {
    if doc
        .get("version")
        .and_then(Value::as_u64)
        .is_some_and(|version| version != 1)
    {
        return Err(BotError::Credentials);
    }
    if let Some(encoded) = doc.get("cursor-accounts").and_then(Value::as_str) {
        let accounts: Value = serde_json::from_str(encoded).map_err(|_| BotError::Credentials)?;
        let Some(active) = accounts.get("active").and_then(Value::as_str) else {
            return Ok(None);
        };
        return accounts
            .get("accounts")
            .and_then(|records| records.get(active))
            .filter(|record| {
                record
                    .get("cursor-access-token")
                    .and_then(Value::as_str)
                    .is_some_and(|token| !token.is_empty())
            })
            .cloned()
            .map(Some)
            .ok_or(BotError::Credentials);
    }
    // Older desktop versions keep the active pair at the document root.
    Ok(doc
        .get("cursor-access-token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(|_| doc.clone()))
}

fn unscoped_ciphertext(value: &str) -> Result<&str, BotError> {
    if let Some(scoped) = value.strip_prefix("scoped:v1:") {
        let (scope, ciphertext) = scoped.split_once(':').ok_or(BotError::Credentials)?;
        if scope.len() != 64
            || !scope.bytes().all(|byte| byte.is_ascii_hexdigit())
            || ciphertext.is_empty()
        {
            return Err(BotError::Credentials);
        }
        Ok(ciphertext)
    } else {
        Ok(value)
    }
}

fn validate_login(login: BotLogin, now: DateTime<Utc>) -> Result<BotLogin, BotError> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let payload = login.token.split('.').nth(1).ok_or(BotError::Credentials)?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| BotError::Credentials)?;
    let claims: Value = serde_json::from_slice(&payload).map_err(|_| BotError::Credentials)?;
    let expires = claims
        .get("exp")
        .and_then(Value::as_i64)
        .ok_or(BotError::Credentials)?;
    if expires <= now.timestamp() {
        return Err(BotError::Expired);
    }
    Ok(login)
}

#[cfg(not(target_os = "windows"))]
fn decrypt_cbc(
    bytes: &[u8],
    password: &[u8],
    iterations: u32,
) -> Result<Zeroizing<Vec<u8>>, BotError> {
    use aes::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};
    use ring::pbkdf2;
    let ciphertext = bytes
        .get(3..)
        .filter(|_| bytes.starts_with(b"v10") || bytes.starts_with(b"v11"))
        .ok_or(BotError::Credentials)?;
    let mut key = Zeroizing::new([0u8; 16]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA1,
        std::num::NonZeroU32::new(iterations).ok_or(BotError::Credentials)?,
        b"saltysalt",
        password,
        key.as_mut(),
    );
    let mut plain = Zeroizing::new(ciphertext.to_vec());
    let len = cbc::Decryptor::<aes::Aes128>::new((&*key).into(), (&[b' '; 16]).into())
        .decrypt_padded::<Pkcs7>(&mut plain)
        .map_err(|_| BotError::Credentials)?
        .len();
    plain.truncate(len);
    Ok(plain)
}

#[cfg(target_os = "macos")]
fn decrypt_os(bytes: &[u8], _path: &std::path::Path) -> Result<Zeroizing<Vec<u8>>, BotError> {
    let password = Zeroizing::new(
        crate::providers::credential_store::read_generic_password_via_security(
            "Grok Bot Safe Storage",
            "Grok Bot Key",
        )
        .map_err(|_| BotError::Credentials)?
        .ok_or(BotError::Credentials)?,
    );
    // `security -w` adds a newline to the stored safe-storage password.
    let password = password.strip_suffix(b"\n").unwrap_or(&password);
    decrypt_cbc(bytes, password, 1003)
}

#[cfg(target_os = "linux")]
fn decrypt_os(bytes: &[u8], _path: &std::path::Path) -> Result<Zeroizing<Vec<u8>>, BotError> {
    let password = if bytes.starts_with(b"v10") {
        Zeroizing::new(b"peanuts".to_vec()) // Electron's legacy basic-text backend.
    } else {
        Zeroizing::new(
            crate::providers::credential_store::read_electron_password("Grok Bot")
                .map_err(|_| BotError::Credentials)?
                .ok_or(BotError::Credentials)?,
        )
    };
    decrypt_cbc(bytes, &password, 1)
}

#[cfg(target_os = "windows")]
fn decrypt_os(bytes: &[u8], path: &std::path::Path) -> Result<Zeroizing<Vec<u8>>, BotError> {
    use ring::aead;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let root = path.parent().ok_or(BotError::Credentials)?;
    let state = read_json(&root.join("Local State"))?;
    let encrypted = STANDARD
        .decode(
            state
                .pointer("/os_crypt/encrypted_key")
                .and_then(Value::as_str)
                .ok_or(BotError::Credentials)?,
        )
        .map_err(|_| BotError::Credentials)?;
    let encrypted = encrypted
        .strip_prefix(b"DPAPI")
        .ok_or(BotError::Credentials)?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted
            .len()
            .try_into()
            .map_err(|_| BotError::Credentials)?,
        pbData: encrypted.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI allocates the master key; copy, wipe, and release that allocation.
    let key = unsafe {
        if CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        ) == 0
        {
            return Err(BotError::Credentials);
        }
        let key = Zeroizing::new(
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec(),
        );
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData.cast());
        key
    };
    let data = bytes.strip_prefix(b"v10").ok_or(BotError::Credentials)?;
    let nonce: [u8; 12] = data
        .get(..12)
        .ok_or(BotError::Credentials)?
        .try_into()
        .map_err(|_| BotError::Credentials)?;
    let mut plain = Zeroizing::new(data[12..].to_vec());
    let key = aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| BotError::Credentials)?;
    let len = aead::LessSafeKey::new(key)
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::empty(),
            &mut plain,
        )
        .map_err(|_| BotError::Credentials)?
        .len();
    plain.truncate(len);
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selects_only_the_active_account_and_accepts_legacy_storage() {
        let accounts = json!({"active":"second","accounts":{"first":{"cursor-access-token":"a"},"second":{"cursor-access-token":"b"}}});
        assert_eq!(
            active_record(&json!({"version":1,"cursor-accounts":accounts.to_string()}))
                .unwrap()
                .unwrap()["cursor-access-token"],
            "b"
        );
        assert!(
            active_record(&json!({"cursor-accounts":"{\"active\":null}"}))
                .unwrap()
                .is_none()
        );
        assert!(active_record(&json!({"version":2,"cursor-access-token":"a"})).is_err());
        assert!(active_record(&json!({"cursor-access-token":"a"}))
            .unwrap()
            .is_some());
    }

    #[test]
    fn validates_scoped_ciphertext_without_exposing_secrets() {
        assert_eq!(
            unscoped_ciphertext(&format!("scoped:v1:{}:cipher", "a".repeat(64))).unwrap(),
            "cipher"
        );
        assert!(unscoped_ciphertext("scoped:v1:bad:cipher").is_err());
    }

    #[test]
    fn expired_and_malformed_access_tokens_are_never_used() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let now = Utc::now();
        let token = |expires| {
            format!(
                "header.{}.signature",
                URL_SAFE_NO_PAD.encode(json!({"exp":expires}).to_string())
            )
        };
        let login = |token| BotLogin {
            token,
            team_id: None,
        };
        assert!(matches!(
            validate_login(login(token(now.timestamp() - 1)), now),
            Err(BotError::Expired)
        ));
        assert!(validate_login(login(token(now.timestamp() + 60)), now).is_ok());
        assert!(validate_login(login("invalid".into()), now).is_err());
        assert!(!format!("{:?}", login("sensitive-token".into())).contains("sensitive-token"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn decrypts_windows_electron_dpapi_wrapped_master_key() {
        use ring::aead;
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::Cryptography::{
                CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
            },
        };
        let key = [7u8; 32];
        let input = CRYPT_INTEGER_BLOB {
            cbData: 32,
            pbData: key.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let mut wrapped = b"DPAPI".to_vec();
        unsafe {
            assert_ne!(
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output
                ),
                0
            );
            wrapped.extend_from_slice(std::slice::from_raw_parts(
                output.pbData,
                output.cbData as usize,
            ));
            LocalFree(output.pbData.cast());
        }
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("Local State"),
            json!({"os_crypt":{"encrypted_key":STANDARD.encode(wrapped)}}).to_string(),
        )
        .unwrap();
        let nonce = [9u8; 12];
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &key).unwrap());
        let mut ciphertext = b"fake-token".to_vec();
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::empty(),
            &mut ciphertext,
        )
        .unwrap();
        let mut blob = b"v10".to_vec();
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        assert_eq!(
            &*decrypt_os(&blob, &directory.path().join("sand-secrets.json")).unwrap(),
            b"fake-token"
        );
        let last = blob.len() - 1;
        blob[last] ^= 1;
        assert!(decrypt_os(&blob, &directory.path().join("sand-secrets.json")).is_err());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn decrypts_electron_mac_and_linux_envelopes_and_rejects_bad_keys() {
        use aes::cipher::{block_padding::Pkcs7, BlockModeEncrypt, KeyIvInit};
        for iterations in [1003, 1] {
            let mut key = [0u8; 16];
            ring::pbkdf2::derive(
                ring::pbkdf2::PBKDF2_HMAC_SHA1,
                std::num::NonZeroU32::new(iterations).unwrap(),
                b"saltysalt",
                b"fake-password",
                &mut key,
            );
            let mut buffer = [0u8; 64];
            buffer[..10].copy_from_slice(b"fake-token");
            let encrypted = cbc::Encryptor::<aes::Aes128>::new((&key).into(), (&[b' '; 16]).into())
                .encrypt_padded::<Pkcs7>(&mut buffer, 10)
                .unwrap();
            let mut blob = b"v10".to_vec();
            blob.extend_from_slice(encrypted);
            assert_eq!(
                &*decrypt_cbc(&blob, b"fake-password", iterations).unwrap(),
                b"fake-token"
            );
            assert!(decrypt_cbc(&blob, b"wrong-password", iterations).is_err());
            assert!(decrypt_cbc(b"unsupported", b"fake-password", iterations).is_err());
        }
    }
}
