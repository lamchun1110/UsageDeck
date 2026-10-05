//! ZCode's read-only reset status belongs to its signed-in personal account,
//! rather than to every API key. Never send a token until the key matches.
use std::{fs::File, io::Read, path::Path};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::models::{MetricValue, MetricValueKind, ValueMetric};

const STATUS_URL: &str = "https://zcode.z.ai/api/v1/coding-plan/reset/status";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

struct Credentials {
    jwt: Zeroizing<String>,
    access_token: Zeroizing<String>,
}

fn decrypt(value: &str, secret: &str) -> Option<Zeroizing<String>> {
    let Some(payload) = value.strip_prefix("enc:v1:") else {
        return (!value.starts_with("enc:")).then(|| Zeroizing::new(value.to_owned()));
    };
    let parts: Vec<_> = payload.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let iv: [u8; 12] = URL_SAFE_NO_PAD.decode(parts[0]).ok()?.try_into().ok()?;
    let tag = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    if tag.len() != 16 {
        return None;
    }
    let mut bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(parts[2]).ok()?);
    bytes.extend_from_slice(&tag);
    let key =
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &Sha256::digest(secret.as_bytes())).ok()?);
    let plain = key
        .open_in_place(Nonce::assume_unique_for_key(iv), Aad::empty(), &mut bytes)
        .ok()?;
    Some(Zeroizing::new(std::str::from_utf8(plain).ok()?.to_owned()))
}

fn read_json(path: &Path) -> Option<Value> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn personal_key(data: &Value, secret: &str) -> Option<Zeroizing<String>> {
    let read = |name: &str| decrypt(data.get(name)?.as_str()?, secret);
    if read("oauth:active_provider")?.as_str() != "zai" {
        return None;
    }
    let user = read("oauth:zai:user_info")?;
    let user: Value = serde_json::from_str(&user).ok()?;
    let user_id = user.get("user_id")?.as_str()?;
    // ZCode encodes the account identity in the key name. Match the personal
    // plan key for this login, not a team/start-plan key or another account.
    let encoded_id = percent_encode(user_id);
    let key_name = format!("account-provider:coding-plan:account:zai-individual-coding-plan:account:{encoded_id}:api-key");
    let key = read(&key_name)?;
    (!key.trim().is_empty()).then_some(key)
}

fn matching_credentials(data: &Value, api_key: &str, secret: &str) -> Option<Credentials> {
    if personal_key(data, secret)?.trim() != api_key.trim() {
        return None;
    }
    let read = |name: &str| decrypt(data.get(name)?.as_str()?, secret);
    let jwt = read("zcodejwttoken")?;
    let access_token = read("oauth:zai:access_token")?;
    if jwt.trim().is_empty() || access_token.trim().is_empty() {
        return None;
    }
    Some(Credentials { jwt, access_token })
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn local_file() -> Option<(Value, Zeroizing<String>)> {
    let home = super::super::paths::home_directory();
    let root = std::env::var_os("ZCODE_HOME")
        .or_else(|| std::env::var_os("ZCODE_DESKTOP_HOME_DIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".zcode"));
    let data = read_json(&root.join("v2/credentials.json"))?;
    let platform = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    };
    let secret = Zeroizing::new(
        std::env::var("ZCODE_CREDENTIAL_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "zcode-credential-fallback:{platform}:{}:{}",
                    home.display(),
                    whoami::username().unwrap_or_default()
                )
            }),
    );
    Some((data, secret))
}

fn local_credentials(api_key: &str) -> Option<Credentials> {
    let (data, secret) = local_file()?;
    matching_credentials(&data, api_key, &secret)
}

pub(crate) fn local_personal_api_key() -> Option<Zeroizing<String>> {
    let (data, secret) = local_file()?;
    let key = personal_key(&data, &secret)?;
    matching_credentials(&data, &key, &secret)?;
    // A failed/expired reset login must not replace the card's working key.
    fetch(&key, Utc::now())?;
    Some(key)
}

pub(super) fn fetch(api_key: &str, now: DateTime<Utc>) -> Option<ValueMetric> {
    let credentials = local_credentials(api_key)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let jwt = credentials.jwt.trim();
    let jwt = if jwt
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("bearer "))
    {
        &jwt[7..]
    } else {
        jwt
    };
    let response = client
        .get(STATUS_URL)
        .bearer_auth(jwt.trim())
        .header("X-Bigmodel-Authorization", credentials.access_token.trim())
        .header("Bigmodel-Target-Type", "PERSONAL")
        .header("Accept", "application/json")
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().ok()?;
    map_status(&body, now)
}

#[derive(Deserialize)]
struct Status {
    available_five_hour_resets: Vec<Card>,
    available_week_resets: Vec<Card>,
}

#[derive(Deserialize)]
struct Card {
    expire_at: i64,
}

pub(super) fn map_status(body: &Value, now: DateTime<Utc>) -> Option<ValueMetric> {
    if body.get("code")?.as_i64()? != 0 {
        return None;
    }
    let status: Status = serde_json::from_value(body.get("data")?.clone()).ok()?;
    let mut expiries = Vec::new();
    for card in status
        .available_five_hour_resets
        .into_iter()
        .chain(status.available_week_resets)
    {
        // Reject malformed timestamps instead of quietly reporting zero.
        if card.expire_at <= 0 {
            return None;
        }
        let expiry = DateTime::from_timestamp_millis(card.expire_at)?;
        if expiry > now {
            expiries.push(expiry);
        }
    }
    expiries.sort();
    Some(ValueMetric {
        id: "rateLimitResets".into(),
        label: "Rate Limit Resets".into(),
        values: vec![MetricValue {
            number: expiries.len() as f64,
            kind: MetricValueKind::Count,
            label: None,
            estimated: false,
        }],
        expiries_at: expiries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    #[test]
    fn combines_both_card_types_and_ignores_expired_cards() {
        let now = Utc.with_ymd_and_hms(2026, 10, 5, 0, 0, 0).unwrap();
        let early = now + chrono::Duration::hours(4);
        let late = now + chrono::Duration::days(7);
        let body = json!({"code":0,"data":{"available_five_hour_resets":[{"expire_at":late.timestamp_millis()},{"expire_at":now.timestamp_millis()}],"available_week_resets":[{"expire_at":early.timestamp_millis()}]}});
        let metric = map_status(&body, now).unwrap();
        assert_eq!(metric.values[0].number, 2.0);
        assert_eq!(metric.expiries_at, vec![early, late]);
        for body in [
            json!({"code":401,"data":{}}),
            json!({"code":0,"data":{}}),
            json!({"code":0,"data":{"available_five_hour_resets":[{"expire_at":"tomorrow"}],"available_week_resets":[]}}),
        ] {
            assert!(map_status(&body, now).is_none());
        }
        assert_eq!(map_status(&json!({"code":0,"data":{"available_five_hour_resets":[],"available_week_resets":[]}}),now).unwrap().values[0].number,0.0);
    }

    #[test]
    fn only_uses_the_matching_personal_account() {
        let data = json!({"oauth:active_provider":"zai","oauth:zai:user_info":"{\"user_id\":\"test-user\"}","account-provider:coding-plan:account:zai-individual-coding-plan:account:test-user:api-key":"matching-key","zcodejwttoken":"jwt","oauth:zai:access_token":"token"});
        assert!(matching_credentials(&data, "matching-key", "unused").is_some());
        assert!(matching_credentials(&data, "another-key", "unused").is_none());
        assert!(decrypt("enc:v2:unknown", "unused").is_none());
    }

    #[test]
    fn decrypts_zcode_format_and_rejects_tampering() {
        let key = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &Sha256::digest(b"test-secret")).unwrap(),
        );
        let iv = [7; 12];
        let mut encrypted = b"a test credential".to_vec();
        let tag = key
            .seal_in_place_separate_tag(
                Nonce::assume_unique_for_key(iv),
                Aad::empty(),
                &mut encrypted,
            )
            .unwrap();
        let payload = format!(
            "enc:v1:{}.{}.{}",
            URL_SAFE_NO_PAD.encode(iv),
            URL_SAFE_NO_PAD.encode(tag),
            URL_SAFE_NO_PAD.encode(encrypted)
        );
        assert_eq!(
            decrypt(&payload, "test-secret").unwrap().as_str(),
            "a test credential"
        );
        assert!(decrypt(&payload, "wrong-secret").is_none());
        assert!(decrypt("enc:v1:broken", "test-secret").is_none());
    }
}
