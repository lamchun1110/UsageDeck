//! Shared transport for the API-key provider clients.
//!
//! Every provider that queries a plain authenticated JSON endpoint used to
//! hand-roll the same `Client` builder, request timing, log sandwich, and
//! body decode. The helpers here declare that policy once: one connect
//! budget, one total-timeout budget, one user agent, identical logging
//! (`"{provider} {endpoint} HTTP <code> (<ms>)"`), a single retry for
//! transient failures, and a per-key 429 cooldown shared by every endpoint
//! of a provider.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

/// A completed provider request: the HTTP status plus the decoded JSON body.
/// A body that is not valid JSON decodes to `Value::Null`, matching the
/// per-provider clients this module replaces; providers that must tell a
/// malformed 200 apart from missing data check `body.is_null()` like Codex
/// does.
#[derive(Debug)]
pub(crate) struct EndpointResponse {
    pub(crate) status: StatusCode,
    pub(crate) body: Value,
}

/// Transport-level failure modes shared by every provider client. Each
/// provider maps these onto its own error enum so user-facing classification
/// stays local to the provider.
#[derive(Debug)]
pub(crate) enum TransportError {
    ConnectionFailed,
    InvalidResponse,
    /// The provider is in a shared rate-limit cooldown; the request was not
    /// sent. Maps to each provider's HTTP 429 classification.
    RateLimited,
}

/// Pause before the single retry of a transiently failed request.
const RETRY_DELAY: Duration = Duration::from_millis(250);
/// Cooldown applied when a 429 arrives without a usable `Retry-After`.
const RATE_LIMIT_DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);
/// Upper bound for an advertised `Retry-After`, so a misbehaving endpoint
/// cannot park a provider for hours.
const RATE_LIMIT_MAX_COOLDOWN: Duration = Duration::from_secs(600);

/// Parses the `Retry-After` header: delay seconds or an HTTP-date (RFC 2822),
/// as sent by some gateways. A past date cools down for zero seconds.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?.to_utc();
    let milliseconds = date
        .signed_duration_since(chrono::Utc::now())
        .num_milliseconds()
        .max(0) as u64;
    Some(Duration::from_secs(milliseconds.div_ceil(1000)))
}

fn cooldowns() -> &'static Mutex<HashMap<String, Instant>> {
    static COOLDOWNS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    COOLDOWNS.get_or_init(|| Mutex::new(HashMap::new()))
}

// The map is keyed by a hash of the API key, never the key itself: entries
// outlive requests and must not hold a credential.
fn cooldown_key(provider: &str, api_key: &str) -> String {
    format!(
        "{provider}:{}",
        crate::hashing::sha256_hex(api_key.as_bytes())
    )
}

fn rate_limit_active(provider: &str, api_key: &str) -> bool {
    let now = Instant::now();
    let mut cooldowns = cooldowns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match cooldowns.get(&cooldown_key(provider, api_key)) {
        Some(until) if *until > now => true,
        _ => {
            cooldowns.remove(&cooldown_key(provider, api_key));
            false
        }
    }
}

fn record_rate_limit(provider: &str, api_key: &str, retry_after: Option<Duration>) -> Duration {
    let cooldown = retry_after
        .unwrap_or(RATE_LIMIT_DEFAULT_COOLDOWN)
        .min(RATE_LIMIT_MAX_COOLDOWN);
    let now = Instant::now();
    let mut cooldowns = cooldowns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cooldowns.retain(|_, until| *until > now);
    cooldowns.insert(cooldown_key(provider, api_key), now + cooldown);
    cooldown
}

/// Clears every rate-limit cooldown. Mock-server tests share one process and
/// one cooldown map; resetting per test keeps a 429 case from silencing the
/// next test's requests for the same provider and key.
#[cfg(test)]
pub(crate) fn clear_rate_limits() {
    cooldowns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

/// How much longer this key's rate-limit cooldown runs; for tests and logs.
#[cfg(test)]
pub(crate) fn rate_limit_remaining(provider: &str, api_key: &str) -> Option<Duration> {
    let now = Instant::now();
    cooldowns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&cooldown_key(provider, api_key))
        .and_then(|until| until.checked_duration_since(now))
}

/// Builds the standard provider HTTP client.
pub(crate) fn client(timeout: Duration) -> Result<Client, TransportError> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(timeout)
        .user_agent(concat!("UsageDeck/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| TransportError::ConnectionFailed)
}

/// Sends one authenticated JSON GET under the provider's tag, retrying a
/// single time on transient failures and enforcing the per-key 429 cooldown.
pub(crate) fn get_json(
    client: &Client,
    url: &str,
    api_key: &str,
    provider: &str,
    endpoint: &str,
) -> Result<EndpointResponse, TransportError> {
    if rate_limit_active(provider, api_key) {
        crate::app_debug!(
            "http",
            "{provider} {endpoint} skipped: rate-limit cooldown active"
        );
        return Err(TransportError::RateLimited);
    }
    let mut retries_left = 1_u8;
    loop {
        let started = Instant::now();
        let result = client
            .get(url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send();
        let response = match result {
            Ok(response) => response,
            Err(error) if retries_left > 0 => {
                retries_left -= 1;
                crate::app_debug!(
                    "http",
                    "{provider} {endpoint} transport failure ({error}); retrying once"
                );
                thread::sleep(RETRY_DELAY);
                continue;
            }
            Err(error) => {
                crate::app_warn!(
                    "http",
                    "{provider} {endpoint} request failed (transport): {error}"
                );
                return Err(TransportError::ConnectionFailed);
            }
        };
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(parse_retry_after);
            let cooldown = record_rate_limit(provider, api_key, retry_after);
            crate::app_warn!(
                "http",
                "{provider} {endpoint} rate limited; cooling down for {}s",
                cooldown.as_secs()
            );
        } else if retries_left > 0 && is_transient(status) {
            retries_left -= 1;
            crate::app_debug!(
                "http",
                "{provider} {endpoint} HTTP {} ({}ms); retrying once",
                status.as_u16(),
                started.elapsed().as_millis()
            );
            thread::sleep(RETRY_DELAY);
            continue;
        }
        crate::app_debug!(
            "http",
            "{provider} {endpoint} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response.text().map_err(|error| {
            crate::app_debug!("http", "{provider} {endpoint} body read failed: {error}");
            TransportError::InvalidResponse
        })?;
        let body = serde_json::from_str(&text).unwrap_or_else(|error| {
            crate::app_debug!(
                "http",
                "{provider} {endpoint} returned a non-JSON body: {error}"
            );
            Value::Null
        });
        return Ok(EndpointResponse { status, body });
    }
}

/// Gateway-style statuses that are worth exactly one retry.
fn is_transient(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_retry_after_accepts_seconds_and_http_dates() {
        assert_eq!(
            super::parse_retry_after("120"),
            Some(Duration::from_secs(120))
        );
        let future = (chrono::Utc::now() + chrono::Duration::seconds(90))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let parsed = super::parse_retry_after(&future).map(|d| d.as_secs());
        assert!(
            parsed.is_some_and(|secs| (85..=90).contains(&secs)),
            "{parsed:?}"
        );
        assert_eq!(
            super::parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(Duration::ZERO)
        );
        assert_eq!(super::parse_retry_after("garbage"), None);
    }

    use std::time::Duration;

    use super::{client, get_json, rate_limit_remaining};
    use crate::providers::test_http::{self, Step};

    // The cooldown map is process-global; give every test its own key.
    fn key(tag: &str) -> String {
        format!("{tag}-key")
    }

    #[test]
    fn transient_connection_failures_retry_once_and_succeed() {
        let (url, requests) = test_http::serve_sequence(vec![
            Step::Close,
            Step::Respond {
                status: 200,
                headers: Vec::new(),
                body: r#"{"ok":true}"#.to_owned(),
            },
        ]);

        let response = get_json(
            &client(Duration::from_secs(2)).unwrap(),
            &url,
            key("retry").as_str(),
            "test-retry",
            "endpoint",
        )
        .unwrap();

        assert_eq!(response.status.as_u16(), 200);
        assert_eq!(response.body["ok"], true);
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn gateway_errors_retry_once_before_surfacing() {
        let (url, requests) = test_http::serve_sequence(vec![
            Step::Respond {
                status: 503,
                headers: Vec::new(),
                body: "{}".to_owned(),
            },
            Step::Respond {
                status: 200,
                headers: Vec::new(),
                body: "{}".to_owned(),
            },
        ]);

        let response = get_json(
            &client(Duration::from_secs(2)).unwrap(),
            &url,
            key("gateway").as_str(),
            "test-gateway",
            "endpoint",
        )
        .unwrap();

        assert_eq!(response.status.as_u16(), 200);
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_429_starts_a_cooldown_that_skips_the_next_request() {
        let (url, requests) = test_http::serve_sequence(vec![Step::Respond {
            status: 429,
            headers: Vec::new(),
            body: "{}".to_owned(),
        }]);

        let provider = "test-cooldown";
        let api_key = key("cooldown");
        let first = get_json(
            &client(Duration::from_secs(2)).unwrap(),
            &url,
            api_key.as_str(),
            provider,
            "endpoint",
        )
        .unwrap();
        assert_eq!(first.status.as_u16(), 429);
        assert!(rate_limit_remaining(provider, api_key.as_str()).is_some());

        let second = get_json(
            &client(Duration::from_secs(2)).unwrap(),
            &url,
            api_key.as_str(),
            provider,
            "endpoint",
        )
        .unwrap_err();
        assert!(matches!(second, super::TransportError::RateLimited));
        // The cooldown short-circuited before any second connection.
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_advertised_retry_after_extends_the_cooldown_within_bounds() {
        let (url, _requests) = test_http::serve_sequence(vec![Step::Respond {
            status: 429,
            headers: vec![("Retry-After".to_owned(), "120".to_owned())],
            body: "{}".to_owned(),
        }]);

        let provider = "test-retry-after";
        let api_key = key("retry-after");
        get_json(
            &client(Duration::from_secs(2)).unwrap(),
            &url,
            api_key.as_str(),
            provider,
            "endpoint",
        )
        .unwrap();

        let remaining = rate_limit_remaining(provider, api_key.as_str()).unwrap();
        assert!(
            remaining > Duration::from_secs(60),
            "Retry-After: 120 must extend the cooldown past the 60s default"
        );
    }
}
