use std::time::Duration;

use crate::providers::http::{self, EndpointResponse, TransportError};

use super::ZaiError;

const SUBSCRIPTION_URL: &str = "https://api.z.ai/api/biz/subscription/list";
const QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

pub(super) type ZaiResponse = EndpointResponse;

impl From<TransportError> for ZaiError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::ConnectionFailed => ZaiError::ConnectionFailed,
            TransportError::InvalidResponse => ZaiError::InvalidResponse,
            TransportError::RateLimited => ZaiError::RequestFailed(429),
        }
    }
}

pub struct ZaiClient {
    client: reqwest::blocking::Client,
    subscription_url: String,
    quota_url: String,
}

impl ZaiClient {
    pub fn new() -> Result<Self, ZaiError> {
        Self::with_endpoints(SUBSCRIPTION_URL, QUOTA_URL, Duration::from_secs(15))
    }

    fn with_endpoints(
        subscription_url: &str,
        quota_url: &str,
        timeout: Duration,
    ) -> Result<Self, ZaiError> {
        Ok(Self {
            client: http::client(timeout)?,
            subscription_url: subscription_url.to_owned(),
            quota_url: quota_url.to_owned(),
        })
    }

    pub fn fetch_quota(&self, api_key: &str) -> Result<ZaiResponse, ZaiError> {
        http::get_json(&self.client, &self.quota_url, api_key, "zai", "quota").map_err(From::from)
    }

    pub fn fetch_subscription(&self, api_key: &str) -> Result<ZaiResponse, ZaiError> {
        http::get_json(
            &self.client,
            &self.subscription_url,
            api_key,
            "zai",
            "subscription",
        )
        .map_err(From::from)
    }
}

#[cfg(test)]
impl ZaiClient {
    pub fn for_test(subscription_url: &str, quota_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(subscription_url, quota_url, timeout).unwrap()
    }
}
