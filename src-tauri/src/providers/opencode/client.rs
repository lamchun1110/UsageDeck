use std::time::Duration;

use crate::providers::http::{self, TransportError};

use super::OpenCodeError;

const GO_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

pub(super) type UsageResponse = crate::providers::http::EndpointResponse;

impl From<TransportError> for OpenCodeError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::ConnectionFailed => OpenCodeError::ConnectionFailed,
            TransportError::InvalidResponse => OpenCodeError::InvalidResponse,
            TransportError::RateLimited => OpenCodeError::RequestFailed(429),
        }
    }
}

pub(super) struct OpenCodeClient {
    client: reqwest::blocking::Client,
    usage_url: String,
}

impl OpenCodeClient {
    pub(super) fn new() -> Result<Self, OpenCodeError> {
        Self::with_endpoint(GO_USAGE_URL, Duration::from_secs(15))
    }

    fn with_endpoint(usage_url: &str, timeout: Duration) -> Result<Self, OpenCodeError> {
        Ok(Self {
            client: http::client(timeout)?,
            usage_url: usage_url.into(),
        })
    }

    pub(super) fn fetch_go_usage(&self, api_key: &str) -> Result<UsageResponse, OpenCodeError> {
        http::get_json(
            &self.client,
            &self.usage_url,
            api_key,
            "opencode",
            "go usage",
        )
        .map_err(From::from)
    }
}

#[cfg(test)]
impl OpenCodeClient {
    pub(super) fn for_test(url: &str, timeout: Duration) -> Self {
        Self::with_endpoint(url, timeout).expect("test OpenCode endpoint should be valid")
    }
}
