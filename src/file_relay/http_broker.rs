//! HTTP-backed file relay client.
//!
//! This client talks only to the trusted `sb-file-relay-serv` service. Public
//! clients receive transfer grants through netplay/lobby state.

use crate::file_relay::{
    CreateFileRelayTransferRequest, CreateFileRelayTransferResponse, FileRelayBroker,
    FileRelayBrokerError,
};
use reqwest::Url;
use std::time::Duration;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// HTTP file relay broker client using bearer service authentication.
pub struct HttpFileRelayBroker {
    client: reqwest::Client,
    base_url: Url,
    bearer_token: String,
}

impl HttpFileRelayBroker {
    /// Creates a broker client from a base URL and service bearer token.
    pub fn new(
        base_url: impl AsRef<str>,
        bearer_token: impl Into<String>,
        request_timeout: Duration,
    ) -> Result<Self, FileRelayBrokerError> {
        let client = reqwest::Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .timeout(request_timeout)
            .build()
            .map_err(|_| FileRelayBrokerError::RequestFailed)?;
        let base_url = parse_base_url(base_url.as_ref())?;

        Ok(Self {
            client,
            base_url,
            bearer_token: bearer_token.into(),
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, FileRelayBrokerError> {
        self.base_url
            .join(path)
            .map_err(|_| FileRelayBrokerError::InvalidUrl)
    }
}

#[async_trait::async_trait]
impl FileRelayBroker for HttpFileRelayBroker {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn create_transfer(
        &self,
        request: CreateFileRelayTransferRequest,
    ) -> Result<CreateFileRelayTransferResponse, FileRelayBrokerError> {
        let response = self
            .client
            .post(self.endpoint("v1/transfers")?)
            .bearer_auth(&self.bearer_token)
            .json(&request)
            .send()
            .await
            .map_err(|_| FileRelayBrokerError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(FileRelayBrokerError::UnexpectedStatus(
                response.status().as_u16(),
            ));
        }

        response
            .json::<CreateFileRelayTransferResponse>()
            .await
            .map_err(|_| FileRelayBrokerError::InvalidResponse)
    }
}

fn parse_base_url(value: &str) -> Result<Url, FileRelayBrokerError> {
    let mut url = Url::parse(value).map_err(|_| FileRelayBrokerError::InvalidUrl)?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::HttpFileRelayBroker;
    use crate::file_relay::FileRelayBroker;
    use std::time::Duration;

    #[test]
    fn normalizes_base_url_for_transfer_route() {
        let broker = HttpFileRelayBroker::new(
            "https://relay.shadowboy.app",
            "secret",
            Duration::from_secs(1),
        )
        .expect("broker");

        assert!(broker.is_enabled());
        assert_eq!(
            broker.endpoint("v1/transfers").expect("endpoint").as_str(),
            "https://relay.shadowboy.app/v1/transfers"
        );
    }
}
