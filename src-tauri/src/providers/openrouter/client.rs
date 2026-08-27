use std::time::Duration;

use crate::providers::http::{self, TransportError};

use super::OpenRouterError;

const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";
const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

pub(super) type EndpointResponse = crate::providers::http::EndpointResponse;

impl From<TransportError> for OpenRouterError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::ConnectionFailed => OpenRouterError::ConnectionFailed,
            TransportError::InvalidResponse => OpenRouterError::InvalidResponse,
            TransportError::RateLimited => OpenRouterError::RequestFailed(429),
        }
    }
}

pub struct OpenRouterClient {
    client: reqwest::blocking::Client,
    credits_url: String,
    key_url: String,
}

impl OpenRouterClient {
    pub fn new() -> Result<Self, OpenRouterError> {
        Self::with_endpoints(CREDITS_URL, KEY_URL, Duration::from_secs(15))
    }

    fn with_endpoints(
        credits_url: &str,
        key_url: &str,
        timeout: Duration,
    ) -> Result<Self, OpenRouterError> {
        Ok(Self {
            client: http::client(timeout)?,
            credits_url: credits_url.to_owned(),
            key_url: key_url.to_owned(),
        })
    }

    pub fn fetch_credits(&self, api_key: &str) -> Result<EndpointResponse, OpenRouterError> {
        http::get_json(
            &self.client,
            &self.credits_url,
            api_key,
            "openrouter",
            "credits",
        )
        .map_err(From::from)
    }

    pub fn fetch_key(&self, api_key: &str) -> Result<EndpointResponse, OpenRouterError> {
        http::get_json(&self.client, &self.key_url, api_key, "openrouter", "key")
            .map_err(From::from)
    }
}

#[cfg(test)]
impl OpenRouterClient {
    pub fn for_test(credits_url: &str, key_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(credits_url, key_url, timeout).unwrap()
    }
}
