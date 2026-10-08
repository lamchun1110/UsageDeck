//! ZCode's read-only reset status belongs to its signed-in personal account.
//! Match either its generated key or the server-reported subscription owner.
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
const SUBSCRIPTION_URL: &str = "https://api.z.ai/api/biz/subscription/list";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

struct Credentials {
    personal_key: Zeroizing<String>,
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

fn credentials(data: &Value, secret: &str) -> Option<Credentials> {
    let personal_key = personal_key(data, secret)?;
    let read = |name: &str| decrypt(data.get(name)?.as_str()?, secret);
    let jwt = read("zcodejwttoken")?;
    let access_token = read("oauth:zai:access_token")?;
    if jwt.trim().is_empty() || access_token.trim().is_empty() {
        return None;
    }
    Some(Credentials {
        personal_key,
        jwt,
        access_token,
    })
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

fn local_credentials() -> Option<Credentials> {
    let (data, secret) = local_file()?;
    credentials(&data, &secret)
}

pub(crate) fn local_personal_api_key() -> Option<Zeroizing<String>> {
    let (data, secret) = local_file()?;
    let key = personal_key(&data, &secret)?;
    credentials(&data, &secret)?;
    // A failed/expired reset login must not replace the card's working key.
    fetch(&key, None, Utc::now())?;
    Some(key)
}

pub(super) fn fetch(
    api_key: &str,
    subscription: Option<&Value>,
    now: DateTime<Utc>,
) -> Option<ValueMetric> {
    let credentials = local_credentials()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    fetch_with_credentials(
        &client,
        &credentials,
        api_key,
        subscription,
        (STATUS_URL, SUBSCRIPTION_URL),
        now,
    )
}

fn bearer_token(value: &str) -> &str {
    let value = value.trim();
    if value
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("bearer "))
    {
        value[7..].trim()
    } else {
        value
    }
}

// Different keys can belong to the same account. Subscription responses are
// authenticated by Z.ai, unlike a key prefix, plan name, or local account name.
// Require one unambiguous owner and preserve integer IDs without float rounding.
fn subscription_owner(body: &Value) -> Option<String> {
    if body.get("code")?.as_u64()? != 200 || !body.get("success")?.as_bool()? {
        return None;
    }
    let rows = body.get("data")?.as_array()?;
    let mut owner = None;
    for row in rows {
        let value = row.get("customerId")?;
        let id = if let Some(value) = value.as_str() {
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            value.trim_start_matches('0').to_owned()
        } else {
            value.as_u64()?.to_string()
        };
        if id.is_empty() || id == "0" {
            return None;
        }
        if owner.as_ref().is_some_and(|owner| owner != &id) {
            return None;
        }
        owner = Some(id);
    }
    owner
}

fn fetch_with_credentials(
    client: &reqwest::blocking::Client,
    credentials: &Credentials,
    api_key: &str,
    subscription: Option<&Value>,
    endpoints: (&str, &str),
    now: DateTime<Utc>,
) -> Option<ValueMetric> {
    let (status_url, subscription_url) = endpoints;
    if credentials.personal_key.trim() != api_key.trim() {
        let card_owner = subscription_owner(subscription?)?;
        let response = client
            .get(subscription_url)
            .bearer_auth(bearer_token(&credentials.access_token))
            .header("Accept", "application/json")
            .send()
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let account_subscription: Value = response.json().ok()?;
        if subscription_owner(&account_subscription)? != card_owner {
            return None;
        }
    }
    let response = client
        .get(status_url)
        .bearer_auth(bearer_token(&credentials.jwt))
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
    fn only_loads_the_active_personal_account() {
        let data = json!({"oauth:active_provider":"zai","oauth:zai:user_info":"{\"user_id\":\"test-user\"}","account-provider:coding-plan:account:zai-individual-coding-plan:account:test-user:api-key":"matching-key","zcodejwttoken":"jwt","oauth:zai:access_token":"token"});
        assert_eq!(
            credentials(&data, "unused").unwrap().personal_key.as_str(),
            "matching-key"
        );
        let mut other_account = data.clone();
        other_account["oauth:zai:user_info"] = json!("{\"user_id\":\"another-user\"}");
        assert!(credentials(&other_account, "unused").is_none());
        let mut other_provider = data;
        other_provider["oauth:active_provider"] = json!("bigmodel");
        assert!(credentials(&other_provider, "unused").is_none());
        assert!(decrypt("enc:v2:unknown", "unused").is_none());
    }

    fn subscription(customer_id: Value) -> Value {
        json!({"code":200,"success":true,"data":[{"customerId":customer_id}]})
    }

    fn test_credentials() -> Credentials {
        Credentials {
            personal_key: Zeroizing::new("zcode-key".into()),
            jwt: Zeroizing::new("Bearer zcode-jwt".into()),
            access_token: Zeroizing::new("oauth-access-token".into()),
        }
    }

    fn reset_status(now: DateTime<Utc>) -> Value {
        json!({"code":0,"data":{"available_five_hour_resets":[{"expire_at":(now+chrono::Duration::hours(4)).timestamp_millis()}],"available_week_resets":[]}})
    }

    #[test]
    fn subscription_owner_requires_authenticated_unambiguous_ids() {
        // IDs exceed JavaScript's integer precision; compare their exact digits.
        let id = "12345678901234567";
        assert_eq!(
            subscription_owner(&subscription(json!(id))).as_deref(),
            Some(id)
        );
        assert_eq!(
            subscription_owner(&subscription(json!(12345678901234567_u64))).as_deref(),
            Some(id)
        );
        let repeated = json!({"code":200,"success":true,"data":[{"customerId":id},{"customerId":12345678901234567_u64}]});
        assert_eq!(subscription_owner(&repeated).as_deref(), Some(id));
        for body in [
            json!({"code":401,"success":true,"data":[{"customerId":id}]}),
            json!({"code":200,"success":false,"data":[{"customerId":id}]}),
            json!({"code":200,"data":[{"customerId":id}]}),
            json!({"code":200,"success":true,"data":[]}),
            json!({"code":200,"success":true,"data":[{"customerId":id},{}]}),
            json!({"code":200,"success":true,"data":[{"customerId":id},{"customerId":"999"}]}),
            subscription(Value::Null),
            subscription(json!(true)),
            subscription(json!(0)),
            subscription(json!(-1)),
            subscription(json!(1.5)),
            subscription(json!("")),
            subscription(json!("unknown")),
            subscription(json!("000")),
        ] {
            assert!(subscription_owner(&body).is_none(), "{body}");
        }
    }

    #[test]
    fn different_key_for_same_account_fetches_real_reset_and_keeps_tokens_scoped() {
        use crate::providers::test_http;
        let now = Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap();
        let owner = subscription(json!("12345678901234567"));
        let (subscription_url, subscription_request, subscription_thread) =
            test_http::capture_once(200, &owner.to_string());
        let (status_url, status_request, status_thread) =
            test_http::capture_once(200, &reset_status(now).to_string());
        let metric = fetch_with_credentials(
            &reqwest::blocking::Client::new(),
            &test_credentials(),
            "another-key-for-same-account",
            Some(&owner),
            (&status_url, &subscription_url),
            now,
        )
        .unwrap();
        assert_eq!(metric.values[0].number, 1.0);
        assert_eq!(metric.expiries_at, vec![now + chrono::Duration::hours(4)]);
        let account_request = subscription_request.recv().unwrap().to_ascii_lowercase();
        assert!(account_request.starts_with("get / "));
        assert!(account_request.contains("authorization: bearer oauth-access-token\r\n"));
        assert!(!account_request.contains("zcode-jwt"));
        let reset_request = status_request.recv().unwrap().to_ascii_lowercase();
        assert!(reset_request.starts_with("get / "));
        assert!(reset_request.contains("authorization: bearer zcode-jwt\r\n"));
        assert!(reset_request.contains("x-bigmodel-authorization: oauth-access-token\r\n"));
        assert!(reset_request.contains("bigmodel-target-type: personal\r\n"));
        assert!(!reset_request.contains("another-key-for-same-account"));
        subscription_thread.join().unwrap();
        status_thread.join().unwrap();
    }

    #[test]
    fn exact_zcode_key_does_not_require_an_extra_account_request() {
        use crate::providers::test_http;
        let now = Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap();
        let status_url = test_http::serve_once(200, &[], &reset_status(now).to_string());
        let metric = fetch_with_credentials(
            &reqwest::blocking::Client::new(),
            &test_credentials(),
            " zcode-key ",
            None,
            (&status_url, "http://127.0.0.1:1"),
            now,
        )
        .unwrap();
        assert_eq!(metric.values[0].number, 1.0);
    }

    #[test]
    fn another_account_or_failed_owner_verification_never_returns_resets() {
        use crate::providers::test_http::{serve_sequence, Step};
        let now = Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap();
        let owner = subscription(json!("12345678901234567"));
        for (status, body) in [
            (200, subscription(json!("99999999999999999"))),
            (401, owner.clone()),
            (200, json!({"code":200,"success":true,"data":[]})),
            (200, json!({"code":401,"success":false,"data":null})),
        ] {
            let (base, requests) = serve_sequence(vec![
                Step::Respond {
                    status,
                    headers: vec![],
                    body: body.to_string(),
                },
                Step::Respond {
                    status: 200,
                    headers: vec![],
                    body: reset_status(now).to_string(),
                },
            ]);
            assert!(fetch_with_credentials(
                &reqwest::blocking::Client::new(),
                &test_credentials(),
                "another-key",
                Some(&owner),
                (&format!("{base}/status"), &format!("{base}/subscription")),
                now,
            )
            .is_none());
            assert_eq!(
                *requests.lock().unwrap(),
                vec!["GET /subscription HTTP/1.1"]
            );
        }
        assert!(fetch_with_credentials(
            &reqwest::blocking::Client::new(),
            &test_credentials(),
            "another-key",
            None,
            ("http://127.0.0.1:1", "http://127.0.0.1:1"),
            now,
        )
        .is_none());
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
