//! Signed physical motion, deliberately not a logical card-play command.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalPoseState {
    pub device: DeviceId,
    pub generation: u64,
    pub sequence: u64,
    pub position_mm: [i32; 3],
    /// Canonical yaw/pitch/roll in [0, 360000), rendered in YXZ order.
    pub rotation_millidegrees: [i32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalPoseRequest {
    pub certificate: DeviceCertificateWire,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub card_id: String,
    /// Claim compares the previous generation; update uses the current one.
    pub generation: u64,
    pub claim: bool,
    pub sequence: u64,
    pub position_mm: [i32; 3],
    pub rotation_millidegrees: [i32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPhysicalPose {
    pub request: PhysicalPoseRequest,
    pub signature: SignatureBytes,
}

impl PhysicalPoseRequest {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, DeviceClientError> {
        let mut bytes = b"poche.physical-pose.v1\0".to_vec();
        bytes.extend(serde_json::to_vec(self).map_err(|_| DeviceClientError::ProtocolViolation)?);
        Ok(bytes)
    }

    pub fn sign(
        self,
        profile: &DeviceProfile,
        signer: &impl DeviceSigner,
    ) -> Result<SignedPhysicalPose, DeviceClientError> {
        profile.validate()?;
        if self.certificate != profile.certificate {
            return Err(DeviceClientError::InvalidProfile);
        }
        let signature = signer.sign_device_bytes(profile, &self.canonical_bytes()?)?;
        Ok(SignedPhysicalPose {
            request: self,
            signature,
        })
    }
}
