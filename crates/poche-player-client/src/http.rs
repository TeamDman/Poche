// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synchronous HTTPS/explicit-loopback adapter for CLI and native devices.

use std::{io::Read as _, time::Duration};

use poche_protocol::{
    CorrelationId, DeviceCertificateWire, DeviceId, DeviceObservationModeWire, InviteProof, RoomId,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    DeviceActionRequest, DeviceActionResult, DeviceClientError, DeviceCooperationRequest,
    DeviceCooperationResult, DeviceObservation, DeviceProfile, DeviceSigner, DeviceTransport,
    sign_observation_request, sign_observation_request_with_invite,
};

const MAX_HTTP_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

/// Cooperation call carrier. Capture requests remain independently signed;
/// this wrapper supplies the requester's root-certified profile and exact
/// target so a stateless gateway can locate the normal cooperation route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpDeviceCooperationCall {
    pub certificate: DeviceCertificateWire,
    pub target_device: DeviceId,
    pub request: DeviceCooperationRequest,
}

/// Configured network transport whose signer owns the protected device key.
pub struct HttpDeviceTransport<S> {
    endpoint: String,
    session_epoch: u64,
    signer: S,
    agent: ureq::Agent,
    request_namespace: String,
    next_request: u64,
    join_invite: Option<InviteProof>,
}

impl<S> HttpDeviceTransport<S> {
    /// Construct an adapter for HTTPS or an explicit loopback HTTP origin.
    ///
    /// # Errors
    ///
    /// Rejects non-HTTP, insecure non-loopback, or path/query configuration
    /// before any request is attempted. Epoch zero is the constrained pending-
    /// room bootstrap; ordinary requests are validated by their signed wire.
    pub fn new(
        endpoint: impl Into<String>,
        session_epoch: u64,
        signer: S,
    ) -> Result<Self, DeviceClientError> {
        let endpoint = endpoint.into();
        if !valid_endpoint(&endpoint) {
            return Err(DeviceClientError::InvalidProfile);
        }
        let mut namespace = [0_u8; 16];
        getrandom::fill(&mut namespace).map_err(|_| DeviceClientError::KeyUnavailable)?;
        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            session_epoch,
            signer,
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(5))
                .timeout_read(Duration::from_secs(35))
                .timeout_write(Duration::from_secs(10))
                .build(),
            request_namespace: namespace.iter().fold(String::new(), |mut output, byte| {
                use std::fmt::Write as _;
                let _ = write!(output, "{byte:02x}");
                output
            }),
            next_request: 0,
            join_invite: None,
        })
    }

    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    #[must_use]
    pub const fn session_epoch(&self) -> u64 {
        self.session_epoch
    }

    /// Replace the configured membership epoch after a certified transition.
    pub const fn set_session_epoch(&mut self, session_epoch: u64) {
        self.session_epoch = session_epoch;
    }

    /// Bind one bearer invite into the next signed immediate observation.
    /// It is cleared after a committed join and is never sent on waits.
    #[must_use]
    pub fn with_join_invite(mut self, join_invite: InviteProof) -> Self {
        self.join_invite = Some(join_invite);
        self
    }

    fn request_id(&mut self) -> Result<CorrelationId, DeviceClientError> {
        let sequence = self.next_request;
        self.next_request = self.next_request.saturating_add(1);
        CorrelationId::new(format!("device-http-{}-{sequence}", self.request_namespace))
            .map_err(|_| DeviceClientError::ProtocolViolation)
    }

    fn post<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, DeviceClientError> {
        let response = self
            .agent
            .post(&format!("{}{path}", self.endpoint))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|error| map_http_error(&error))?;
        let announced = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok());
        if announced.is_some_and(|length| length > MAX_HTTP_RESPONSE_BYTES) {
            return Err(DeviceClientError::ProtocolViolation);
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_HTTP_RESPONSE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if u64::try_from(bytes.len()).map_or(true, |length| length > MAX_HTTP_RESPONSE_BYTES) {
            return Err(DeviceClientError::ProtocolViolation);
        }
        serde_json::from_slice(&bytes).map_err(|_| DeviceClientError::ProtocolViolation)
    }
}

impl<S: DeviceSigner> DeviceTransport for HttpDeviceTransport<S> {
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError> {
        let request_id = self.request_id()?;
        let request = sign_observation_request_with_invite(
            profile,
            room_id,
            self.session_epoch,
            request_id,
            DeviceObservationModeWire::Snapshot,
            self.join_invite.clone(),
            &self.signer,
        )?;
        self.post("/device/v1/observe", &request)
    }

    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        let bootstrap = request.session_epoch == 0
            && matches!(request.payload, poche_protocol::CommandPayload::CreateRoom);
        let joins = matches!(
            request.payload,
            poche_protocol::CommandPayload::RedeemInvite { .. }
        );
        let signed = request.sign(profile, &self.signer)?;
        let result = self.post("/device/v1/invoke", &signed)?;
        if bootstrap && matches!(result, DeviceActionResult::Committed { .. }) {
            self.session_epoch = 1;
        }
        if joins && matches!(result, DeviceActionResult::Committed { .. }) {
            self.join_invite = None;
        }
        Ok(result)
    }

    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        let request_id = self.request_id()?;
        let request = sign_observation_request(
            profile,
            room_id,
            self.session_epoch,
            request_id,
            DeviceObservationModeWire::Wait { after_revision },
            &self.signer,
        )?;
        self.post("/device/v1/observe", &request)
    }

    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        self.post(
            "/device/v1/cooperate",
            &HttpDeviceCooperationCall {
                certificate: profile.certificate.clone(),
                target_device: target_device.clone(),
                request,
            },
        )
    }
}

fn valid_endpoint(endpoint: &str) -> bool {
    let endpoint = endpoint.trim_end_matches('/');
    if let Some(authority) = endpoint.strip_prefix("https://") {
        return !authority.is_empty() && !authority.contains('/') && !endpoint.contains(['?', '#']);
    }
    ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|origin| {
            endpoint.strip_prefix(origin).is_some_and(|tail| {
                tail.is_empty()
                    || tail.strip_prefix(':').is_some_and(|port| {
                        !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
                    })
            })
        })
}

fn map_http_error(error: &ureq::Error) -> DeviceClientError {
    match error {
        ureq::Error::Status(403, _) => DeviceClientError::AuthorizationDenied,
        ureq::Error::Status(409, _) => DeviceClientError::StaleRevision,
        ureq::Error::Status(400 | 413 | 422, _) => DeviceClientError::ProtocolViolation,
        ureq::Error::Status(_, _) | ureq::Error::Transport(_) => {
            DeviceClientError::TransportUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoSigner;

    impl DeviceSigner for NoSigner {
        fn sign_device_bytes(
            &self,
            _profile: &DeviceProfile,
            _canonical_bytes: &[u8],
        ) -> Result<poche_protocol::SignatureBytes, DeviceClientError> {
            Err(DeviceClientError::KeyUnavailable)
        }
    }

    #[test]
    fn endpoint_policy_allows_https_and_explicit_loopback_only() {
        for accepted in [
            "https://poche.example",
            "https://poche.example:8443/",
            "http://127.0.0.1:4174",
            "http://localhost:4174/",
            "http://[::1]:4174",
        ] {
            assert!(HttpDeviceTransport::new(accepted, 1, NoSigner).is_ok());
        }
        for rejected in [
            "http://poche.example",
            "ftp://127.0.0.1",
            "https://poche.example/path",
            "https://poche.example?secret=1",
            "http://127.0.0.1:not-a-port",
        ] {
            assert!(HttpDeviceTransport::new(rejected, 1, NoSigner).is_err());
        }
    }
}
