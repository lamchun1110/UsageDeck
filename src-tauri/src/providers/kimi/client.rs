use std::time::Duration;

use crate::providers::http::{self, TransportError};

use super::KimiError;

pub(super) type EndpointResponse = crate::providers::http::EndpointResponse;

impl From<TransportError> for KimiError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::ConnectionFailed => KimiError::ConnectionFailed,
            TransportError::InvalidResponse => KimiError::InvalidResponse,
            TransportError::RateLimited => KimiError::RequestFailed(429),
        }
    }
}

/// The transport for Kimi Code requests. The URL is supplied per call because the user chooses
/// which Kimi domain to query, and that choice can change while the app is running.
pub struct KimiClient {
    client: reqwest::blocking::Client,
    #[cfg(test)]
    endpoint_override: Option<String>,
}

impl KimiClient {
    pub fn new() -> Result<Self, KimiError> {
        Self::with_timeout(Duration::from_secs(15))
    }

    fn with_timeout(timeout: Duration) -> Result<Self, KimiError> {
        Ok(Self {
            client: http::client(timeout)?,
            #[cfg(test)]
            endpoint_override: None,
        })
    }

    pub fn fetch(&self, url: &str, api_key: &str) -> Result<EndpointResponse, KimiError> {
        #[cfg(test)]
        let url = self.endpoint_override.as_deref().unwrap_or(url);

        http::get_json(&self.client, url, api_key, "kimi", "usages").map_err(From::from)
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
