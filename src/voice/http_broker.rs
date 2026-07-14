//! HTTP-backed voice broker client.
//!
//! This client talks only to the trusted `sb-webrtc` service. Public clients
//! receive its results through the netplay WebSocket join path.

use crate::voice::{
    CloseVoiceRoomRequest, CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse,
    IssueVoiceTokenBrokerRequest, RemoveVoiceParticipantRequest, VoiceBroker, VoiceBrokerError,
    VoiceBrokerGrant,
};
use reqwest::Url;
use std::time::Duration;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// HTTP voice broker client using bearer service authentication.
pub struct HttpVoiceBroker {
    client: reqwest::Client,
    base_url: Url,
    bearer_token: String,
}

impl HttpVoiceBroker {
    /// Creates a broker client from a base URL and service bearer token.
    pub fn new(
        base_url: impl AsRef<str>,
        bearer_token: impl Into<String>,
        request_timeout: Duration,
    ) -> Result<Self, VoiceBrokerError> {
        let client = reqwest::Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .timeout(request_timeout)
            .build()
            .map_err(|_| VoiceBrokerError::RequestFailed)?;
        let base_url = parse_base_url(base_url.as_ref())?;

        Ok(Self {
            client,
            base_url,
            bearer_token: bearer_token.into(),
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, VoiceBrokerError> {
        self.base_url
            .join(path)
            .map_err(|_| VoiceBrokerError::InvalidUrl)
    }

    fn participant_endpoint(
        &self,
        voice_room_id: &str,
        participant_identity: &str,
    ) -> Result<Url, VoiceBrokerError> {
        let mut endpoint = self.endpoint("v1/voice/rooms/")?;
        endpoint
            .path_segments_mut()
            .map_err(|_| VoiceBrokerError::InvalidUrl)?
            .pop_if_empty()
            .push(voice_room_id)
            .push("participants")
            .push(participant_identity);

        Ok(endpoint)
    }
}

#[async_trait::async_trait]
impl VoiceBroker for HttpVoiceBroker {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn create_room(
        &self,
        request: CreateVoiceRoomBrokerRequest,
    ) -> Result<CreateVoiceRoomBrokerResponse, VoiceBrokerError> {
        let response = self
            .client
            .post(self.endpoint("v1/voice/rooms")?)
            .bearer_auth(&self.bearer_token)
            .json(&request)
            .send()
            .await
            .map_err(|_| VoiceBrokerError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(VoiceBrokerError::UnexpectedStatus(
                response.status().as_u16(),
            ));
        }

        response
            .json::<CreateVoiceRoomBrokerResponse>()
            .await
            .map_err(|_| VoiceBrokerError::InvalidResponse)
    }

    async fn issue_token(
        &self,
        voice_room_id: &str,
        request: IssueVoiceTokenBrokerRequest,
    ) -> Result<VoiceBrokerGrant, VoiceBrokerError> {
        let path = format!("v1/voice/rooms/{voice_room_id}/tokens");
        let response = self
            .client
            .post(self.endpoint(&path)?)
            .bearer_auth(&self.bearer_token)
            .json(&request)
            .send()
            .await
            .map_err(|_| VoiceBrokerError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(VoiceBrokerError::UnexpectedStatus(
                response.status().as_u16(),
            ));
        }

        response
            .json::<VoiceBrokerGrant>()
            .await
            .map_err(|_| VoiceBrokerError::InvalidResponse)
    }

    async fn close_room(&self, voice_room_id: &str, reason: &str) -> Result<(), VoiceBrokerError> {
        let path = format!("v1/voice/rooms/{voice_room_id}");
        let response = self
            .client
            .delete(self.endpoint(&path)?)
            .bearer_auth(&self.bearer_token)
            .json(&CloseVoiceRoomRequest {
                reason: reason.to_string(),
            })
            .send()
            .await
            .map_err(|_| VoiceBrokerError::RequestFailed)?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(VoiceBrokerError::UnexpectedStatus(
                response.status().as_u16(),
            ))
        }
    }

    async fn remove_participant(
        &self,
        voice_room_id: &str,
        participant_identity: &str,
        reason: &str,
    ) -> Result<(), VoiceBrokerError> {
        let response = self
            .client
            .delete(self.participant_endpoint(voice_room_id, participant_identity)?)
            .bearer_auth(&self.bearer_token)
            .json(&RemoveVoiceParticipantRequest {
                reason: reason.to_string(),
            })
            .send()
            .await
            .map_err(|_| VoiceBrokerError::RequestFailed)?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(VoiceBrokerError::UnexpectedStatus(
                response.status().as_u16(),
            ))
        }
    }
}

fn parse_base_url(value: &str) -> Result<Url, VoiceBrokerError> {
    let mut url = Url::parse(value).map_err(|_| VoiceBrokerError::InvalidUrl)?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::HttpVoiceBroker;
    use crate::voice::VoiceBroker;
    use std::time::Duration;

    #[test]
    fn normalizes_base_url_for_route_joins() {
        let broker = HttpVoiceBroker::new(
            "https://voice.shadowboy.app",
            "secret",
            Duration::from_secs(1),
        )
        .expect("broker");

        assert!(broker.is_enabled());
        assert_eq!(
            broker
                .endpoint("v1/voice/rooms")
                .expect("endpoint")
                .as_str(),
            "https://voice.shadowboy.app/v1/voice/rooms"
        );
        assert_eq!(
            broker
                .participant_endpoint("room-id", "lobby/player-2")
                .expect("participant endpoint")
                .as_str(),
            "https://voice.shadowboy.app/v1/voice/rooms/room-id/participants/lobby%2Fplayer-2"
        );
    }
}
