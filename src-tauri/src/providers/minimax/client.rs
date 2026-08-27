use std::time::Duration;

use crate::providers::http::{self, TransportError};

use super::MiniMaxError;

const REMAINS_URL: &str = "https://www.minimax.io/v1/token_plan/remains";

pub(super) type EndpointResponse = crate::providers::http::EndpointResponse;

impl From<TransportError> for MiniMaxError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::ConnectionFailed => MiniMaxError::ConnectionFailed,
            TransportError::InvalidResponse => MiniMaxError::InvalidResponse,
            TransportError::RateLimited => MiniMaxError::RequestFailed(429),
        }
    }
}

pub struct MiniMaxClient {
    client: reqwest::blocking::Client,
    url: String,
}

impl MiniMaxClient {
    pub fn new() -> Result<Self, MiniMaxError> {
        Self::with_endpoint(REMAINS_URL, Duration::from_secs(15))
    }

    fn with_endpoint(url: &str, timeout: Duration) -> Result<Self, MiniMaxError> {
        Ok(Self {
            client: http::client(timeout)?,
            url: url.to_owned(),
        })
    }

    pub fn fetch(&self, api_key: &str) -> Result<EndpointResponse, MiniMaxError> {
        http::get_json(&self.client, &self.url, api_key, "minimax", "token_plan")
            .map_err(From::from)
    }
}

#[cfg(test)]
impl MiniMaxClient {
    pub fn for_test(url: &str, timeout: Duration) -> Self {
        Self::with_endpoint(url, timeout).unwrap()
    }
}
