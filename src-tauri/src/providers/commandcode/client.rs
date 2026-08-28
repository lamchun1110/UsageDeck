use std::time::Duration;

use crate::providers::http::{self, TransportError};

use super::CommandCodeError;

const CREDITS_URL: &str = "https://api.commandcode.ai/alpha/billing/credits";
const SUBSCRIPTION_URL: &str = "https://api.commandcode.ai/alpha/billing/subscriptions";

pub(super) type EndpointResponse = crate::providers::http::EndpointResponse;

impl From<TransportError> for CommandCodeError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::ConnectionFailed => CommandCodeError::ConnectionFailed,
            TransportError::InvalidResponse => CommandCodeError::InvalidResponse,
            TransportError::RateLimited => CommandCodeError::RequestFailed(429),
        }
    }
}

pub struct CommandCodeClient {
    client: reqwest::blocking::Client,
    credits_url: String,
    subscription_url: String,
}

impl CommandCodeClient {
    pub fn new() -> Result<Self, CommandCodeError> {
        Self::with_endpoints(CREDITS_URL, SUBSCRIPTION_URL, Duration::from_secs(15))
    }

    fn with_endpoints(
        credits_url: &str,
        subscription_url: &str,
        timeout: Duration,
    ) -> Result<Self, CommandCodeError> {
        Ok(Self {
            client: http::client(timeout)?,
            credits_url: credits_url.into(),
            subscription_url: subscription_url.into(),
        })
    }

    pub fn fetch(
        &self,
        api_key: &str,
    ) -> Result<(EndpointResponse, EndpointResponse), CommandCodeError> {
        Ok((
            http::get_json(
                &self.client,
                &self.credits_url,
                api_key,
                "command-code",
                "credits",
            )?,
            http::get_json(
                &self.client,
                &self.subscription_url,
                api_key,
                "command-code",
                "subscriptions",
            )?,
        ))
    }
}

#[cfg(test)]
impl CommandCodeClient {
    pub fn for_test(credits_url: &str, subscription_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(credits_url, subscription_url, timeout).unwrap()
    }
}
