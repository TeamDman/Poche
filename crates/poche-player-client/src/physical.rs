//! Signed physical motion, deliberately not a logical card-play command.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalPublicCard {
    pub id: String,
    pub face: u8,
    pub pose: PhysicalPoseState,
}

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

impl PhysicalPoseState {
    /// Check wire bounds before a received pose reaches a renderer.
    /// This is structural validation, not proof of signature or authorization.
    #[must_use]
    pub fn validate(&self) -> bool {
        self.device.validate()
            && self.generation > 0
            && self.sequence > 0
            && self
                .position_mm
                .iter()
                .all(|value| (-10_000..=10_000).contains(value))
            && self
                .rotation_millidegrees
                .iter()
                .all(|value| (0..360_000).contains(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn received_pose_bounds_and_lease_counters_are_checked() {
        let pose = PhysicalPoseState {
            device: DeviceId::new("device").unwrap(),
            generation: 1,
            sequence: 1,
            position_mm: [-10_000, 0, 10_000],
            rotation_millidegrees: [0, 180_000, 359_999],
        };
        assert!(pose.validate());
        for value in [i32::MIN, -10_001, 10_001, i32::MAX] {
            let mut invalid = pose.clone();
            invalid.position_mm[0] = value;
            assert!(!invalid.validate());
        }
        for value in [-1, 360_000] {
            let mut invalid = pose.clone();
            invalid.rotation_millidegrees[2] = value;
            assert!(!invalid.validate());
        }
        let mut invalid = pose.clone();
        invalid.generation = 0;
        assert!(!invalid.validate());
        invalid = pose;
        invalid.sequence = 0;
        assert!(!invalid.validate());
    }
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
