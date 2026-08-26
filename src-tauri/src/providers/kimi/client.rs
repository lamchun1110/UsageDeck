use std::time::Duration;

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use super::KimiError;

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

/// The transport for Kimi Code requests. The URL is supplied per call because the user chooses
/// which Kimi domain to query, and that choice can change while the app is running.
pub struct KimiClient {
    client: Client,
    #[cfg(test)]
    endpoint_override: Option<String>,
}

impl KimiClient {
    pub fn new() -> Result<Self, KimiError> {
        Self::with_timeout(Duration::from_secs(15))
    }

    fn with_timeout(timeout: Duration) -> Result<Self, KimiError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(timeout)
            .user_agent(concat!("UsageDeck/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| KimiError::ConnectionFailed)?;
        Ok(Self {
            client,
            #[cfg(test)]
            endpoint_override: None,
        })
    }

    pub fn fetch(&self, url: &str, api_key: &str) -> Result<EndpointResponse, KimiError> {
        #[cfg(test)]
        let url = self.endpoint_override.as_deref().unwrap_or(url);

        let started = std::time::Instant::now();
        let response = self
            .client
            .get(url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "kimi usages request failed (transport)");
                KimiError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "kimi usages HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response.text().map_err(|_| KimiError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(EndpointResponse { status, body })
    }
}

#[cfg(test)]
impl KimiClient {
    /// Pins every request to a local test server, whatever URL the provider resolves.
    pub fn for_test(url: &str, timeout: Duration) -> Self {
        let mut client = Self::with_timeout(timeout).unwrap();
        client.endpoint_override = Some(url.to_owned());
        client
    }
}
