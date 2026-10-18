use std::time::Duration;

use crate::providers::http::{self, TransportError};

use super::{auth::GoCredentials, OpenCodeError};

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
            client: reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .timeout(timeout)
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("UsageDeck/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| OpenCodeError::ConnectionFailed)?,
            usage_url: usage_url.into(),
        })
    }

    pub(super) fn fetch_go_usage(
        &self,
        credentials: &GoCredentials,
    ) -> Result<UsageResponse, OpenCodeError> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(organization) = &credentials.organization {
            headers.insert(
                "x-org-id",
                organization
                    .parse()
                    .map_err(|_| OpenCodeError::InvalidAuth)?,
            );
        }
        http::get_json_with_headers(
            &self.client,
            &self.usage_url,
            &credentials.token,
            "opencode",
            "go usage",
            &headers,
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

#[cfg(test)]
mod tests {
    #[test]
    fn quota_requests_do_not_follow_redirects() {
        let url = crate::providers::test_http::serve_once(
            302,
            &[("Location", "http://127.0.0.1:1")],
            "{}",
        );
        let client = super::OpenCodeClient::for_test(&url, std::time::Duration::from_secs(1));
        let credentials = super::GoCredentials {
            token: zeroize::Zeroizing::new("redirect-test-token".into()),
            organization: Some("org-test".into()),
        };
        let response = client.fetch_go_usage(&credentials).unwrap();
        assert_eq!(response.status, reqwest::StatusCode::FOUND);
    }
}
