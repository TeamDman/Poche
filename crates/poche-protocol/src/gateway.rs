// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned browser-gateway trust disclosure.

use core::fmt;

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::DeviceCustodyWire;

/// Stable disclosure schema version.
pub const GATEWAY_DISCLOSURE_VERSION_V1: u16 = 1;
/// Maximum canonical disclosure size.
pub const MAX_GATEWAY_DISCLOSURE_BYTES: usize = 2_048;

/// Which shared-state authority receives browser traffic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAuthorityModeWire {
    /// Compatibility mode in which the gateway process is the room authority.
    HostAuthoritative,
    /// Experimental ADR-0007 mode in which the gateway is one transport peer.
    ReplicatedFacilitator,
}

/// Whether exact-recipient state is visible to the gateway process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GatewayProjectionProtectionWire {
    /// The authority/gateway constructs a plaintext recipient projection.
    GatewayPlaintext,
    /// The gateway transports device-encrypted projection bytes.
    DeviceEndToEndEncrypted,
}

/// Required user recovery for the selected device-secret custody.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GatewayCustodyRecoveryWire {
    /// Revoke/replace a browser-local device through another root/device path.
    RotateBrowserDevice,
    /// Create a local device, verify it, then revoke the gateway-held device.
    ExportRotateRevokeGateway,
}

/// Whether a data class is visible in plaintext to the gateway operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GatewayVisibilityWire {
    Visible,
    Opaque,
}

/// Whether the gateway can perform one named action in the selected profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAbilityWire {
    Present,
    Absent,
}

/// Whether the experimental replicated path still needs a consensus quorum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GatewayQuorumWire {
    Required,
    NotRequired,
}

/// Machine-readable inventory shown before a browser joins.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayTrustDisclosureWire {
    pub schema_version: u16,
    pub authority_mode: GatewayAuthorityModeWire,
    pub device_custody: DeviceCustodyWire,
    pub projection_protection: GatewayProjectionProtectionWire,
    pub transport_metadata: GatewayVisibilityWire,
    pub signed_command_plaintext: GatewayVisibilityWire,
    pub projection_plaintext: GatewayVisibilityWire,
    pub censor_delay_reorder: GatewayAbilityWire,
    pub replay_transport_bytes: GatewayAbilityWire,
    pub impersonate_this_device: GatewayAbilityWire,
    pub impersonate_other_devices: GatewayAbilityWire,
    pub player_root_access: GatewayAbilityWire,
    pub unilateral_room_ordering: GatewayAbilityWire,
    pub consensus_quorum: GatewayQuorumWire,
    pub host_hidden_state: GatewayVisibilityWire,
    pub custody_recovery: GatewayCustodyRecoveryWire,
}

impl GatewayTrustDisclosureWire {
    /// Construct the exact supported threat inventory.
    ///
    /// # Errors
    ///
    /// Rejects native/legacy custody because this disclosure describes a
    /// browser path, and rejects end-to-end projection claims in host mode.
    pub fn for_browser(
        authority_mode: GatewayAuthorityModeWire,
        device_custody: DeviceCustodyWire,
        projection_protection: GatewayProjectionProtectionWire,
    ) -> Result<Self, GatewayDisclosureError> {
        if !matches!(
            device_custody,
            DeviceCustodyWire::BrowserLocal | DeviceCustodyWire::GatewayCustodied
        ) {
            return Err(GatewayDisclosureError::UnsupportedProfile);
        }
        if authority_mode == GatewayAuthorityModeWire::HostAuthoritative
            && projection_protection != GatewayProjectionProtectionWire::GatewayPlaintext
        {
            return Err(GatewayDisclosureError::UnsupportedProfile);
        }
        let gateway_custodied = device_custody == DeviceCustodyWire::GatewayCustodied;
        let host = authority_mode == GatewayAuthorityModeWire::HostAuthoritative;
        let sees_projection_plaintext = host
            || projection_protection == GatewayProjectionProtectionWire::GatewayPlaintext
            || gateway_custodied;
        let result = Self {
            schema_version: GATEWAY_DISCLOSURE_VERSION_V1,
            authority_mode,
            device_custody,
            projection_protection,
            transport_metadata: GatewayVisibilityWire::Visible,
            // Phase 7 HTTP ingress is signed, but not application-encrypted.
            signed_command_plaintext: GatewayVisibilityWire::Visible,
            projection_plaintext: visibility(sees_projection_plaintext),
            censor_delay_reorder: GatewayAbilityWire::Present,
            replay_transport_bytes: GatewayAbilityWire::Present,
            impersonate_this_device: ability(gateway_custodied),
            impersonate_other_devices: GatewayAbilityWire::Absent,
            player_root_access: GatewayAbilityWire::Absent,
            unilateral_room_ordering: ability(host),
            consensus_quorum: if host {
                GatewayQuorumWire::NotRequired
            } else {
                GatewayQuorumWire::Required
            },
            host_hidden_state: visibility(host),
            custody_recovery: if gateway_custodied {
                GatewayCustodyRecoveryWire::ExportRotateRevokeGateway
            } else {
                GatewayCustodyRecoveryWire::RotateBrowserDevice
            },
        };
        result.validate()?;
        Ok(result)
    }

    /// Fail closed if decoded booleans soften the selected profile.
    ///
    /// # Errors
    ///
    /// Rejects an unknown schema or any field inconsistent with the canonical
    /// constructor for its three profile selectors.
    pub fn validate(&self) -> Result<(), GatewayDisclosureError> {
        if self.schema_version != GATEWAY_DISCLOSURE_VERSION_V1 {
            return Err(GatewayDisclosureError::UnknownVersion);
        }
        let expected = Self::for_browser_unchecked(
            self.authority_mode,
            self.device_custody,
            self.projection_protection,
        )?;
        if *self == expected {
            Ok(())
        } else {
            Err(GatewayDisclosureError::InconsistentClaims)
        }
    }

    fn for_browser_unchecked(
        authority_mode: GatewayAuthorityModeWire,
        device_custody: DeviceCustodyWire,
        projection_protection: GatewayProjectionProtectionWire,
    ) -> Result<Self, GatewayDisclosureError> {
        if !matches!(
            device_custody,
            DeviceCustodyWire::BrowserLocal | DeviceCustodyWire::GatewayCustodied
        ) || (authority_mode == GatewayAuthorityModeWire::HostAuthoritative
            && projection_protection != GatewayProjectionProtectionWire::GatewayPlaintext)
        {
            return Err(GatewayDisclosureError::UnsupportedProfile);
        }
        let gateway_custodied = device_custody == DeviceCustodyWire::GatewayCustodied;
        let host = authority_mode == GatewayAuthorityModeWire::HostAuthoritative;
        Ok(Self {
            schema_version: GATEWAY_DISCLOSURE_VERSION_V1,
            authority_mode,
            device_custody,
            projection_protection,
            transport_metadata: GatewayVisibilityWire::Visible,
            signed_command_plaintext: GatewayVisibilityWire::Visible,
            projection_plaintext: visibility(
                host || projection_protection == GatewayProjectionProtectionWire::GatewayPlaintext
                    || gateway_custodied,
            ),
            censor_delay_reorder: GatewayAbilityWire::Present,
            replay_transport_bytes: GatewayAbilityWire::Present,
            impersonate_this_device: ability(gateway_custodied),
            impersonate_other_devices: GatewayAbilityWire::Absent,
            player_root_access: GatewayAbilityWire::Absent,
            unilateral_room_ordering: ability(host),
            consensus_quorum: if host {
                GatewayQuorumWire::NotRequired
            } else {
                GatewayQuorumWire::Required
            },
            host_hidden_state: visibility(host),
            custody_recovery: if gateway_custodied {
                GatewayCustodyRecoveryWire::ExportRotateRevokeGateway
            } else {
                GatewayCustodyRecoveryWire::RotateBrowserDevice
            },
        })
    }
}

/// Stable disclosure codec failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayDisclosureError {
    UnknownVersion,
    UnsupportedProfile,
    InconsistentClaims,
    InvalidJson,
    NonCanonical,
    Oversize,
}

impl fmt::Display for GatewayDisclosureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownVersion => "unknown gateway disclosure version",
            Self::UnsupportedProfile => "unsupported gateway profile",
            Self::InconsistentClaims => "gateway disclosure understates its selected profile",
            Self::InvalidJson => "invalid gateway disclosure JSON",
            Self::NonCanonical => "noncanonical gateway disclosure JSON",
            Self::Oversize => "gateway disclosure exceeds its bound",
        })
    }
}

impl std::error::Error for GatewayDisclosureError {}

const fn visibility(value: bool) -> GatewayVisibilityWire {
    if value {
        GatewayVisibilityWire::Visible
    } else {
        GatewayVisibilityWire::Opaque
    }
}

const fn ability(value: bool) -> GatewayAbilityWire {
    if value {
        GatewayAbilityWire::Present
    } else {
        GatewayAbilityWire::Absent
    }
}

/// Encode one validated canonical JSON disclosure.
///
/// # Errors
///
/// Rejects inconsistent, unserializable, or oversized disclosures.
pub fn encode_gateway_disclosure(
    disclosure: &GatewayTrustDisclosureWire,
) -> Result<Vec<u8>, GatewayDisclosureError> {
    disclosure.validate()?;
    let bytes = serde_json::to_vec(disclosure).map_err(|_| GatewayDisclosureError::InvalidJson)?;
    if bytes.len() > MAX_GATEWAY_DISCLOSURE_BYTES {
        Err(GatewayDisclosureError::Oversize)
    } else {
        Ok(bytes)
    }
}

/// Decode canonical JSON and revalidate every threat claim.
///
/// # Errors
///
/// Rejects oversized, malformed, noncanonical, or softened disclosures.
pub fn decode_gateway_disclosure(
    bytes: &[u8],
) -> Result<GatewayTrustDisclosureWire, GatewayDisclosureError> {
    if bytes.len() > MAX_GATEWAY_DISCLOSURE_BYTES {
        return Err(GatewayDisclosureError::Oversize);
    }
    let value: GatewayTrustDisclosureWire =
        serde_json::from_slice(bytes).map_err(|_| GatewayDisclosureError::InvalidJson)?;
    value.validate()?;
    if encode_gateway_disclosure(&value)? != bytes {
        return Err(GatewayDisclosureError::NonCanonical);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_profiles_disclose_exact_authority_custody_and_visibility() {
        let host = GatewayTrustDisclosureWire::for_browser(
            GatewayAuthorityModeWire::HostAuthoritative,
            DeviceCustodyWire::BrowserLocal,
            GatewayProjectionProtectionWire::GatewayPlaintext,
        )
        .unwrap();
        assert_eq!(host.unilateral_room_ordering, GatewayAbilityWire::Present);
        assert_eq!(host.host_hidden_state, GatewayVisibilityWire::Visible);
        assert_eq!(host.projection_plaintext, GatewayVisibilityWire::Visible);
        assert_eq!(host.impersonate_this_device, GatewayAbilityWire::Absent);
        assert_eq!(host.consensus_quorum, GatewayQuorumWire::NotRequired);

        let replicated = GatewayTrustDisclosureWire::for_browser(
            GatewayAuthorityModeWire::ReplicatedFacilitator,
            DeviceCustodyWire::BrowserLocal,
            GatewayProjectionProtectionWire::DeviceEndToEndEncrypted,
        )
        .unwrap();
        assert_eq!(
            replicated.unilateral_room_ordering,
            GatewayAbilityWire::Absent
        );
        assert_eq!(replicated.consensus_quorum, GatewayQuorumWire::Required);
        assert_eq!(
            replicated.projection_plaintext,
            GatewayVisibilityWire::Opaque
        );
        assert_eq!(
            replicated.signed_command_plaintext,
            GatewayVisibilityWire::Visible
        );
        assert_eq!(replicated.censor_delay_reorder, GatewayAbilityWire::Present);
        assert_eq!(
            replicated.impersonate_this_device,
            GatewayAbilityWire::Absent
        );

        let degraded = GatewayTrustDisclosureWire::for_browser(
            GatewayAuthorityModeWire::ReplicatedFacilitator,
            DeviceCustodyWire::GatewayCustodied,
            GatewayProjectionProtectionWire::DeviceEndToEndEncrypted,
        )
        .unwrap();
        assert_eq!(
            degraded.impersonate_this_device,
            GatewayAbilityWire::Present
        );
        assert_eq!(
            degraded.projection_plaintext,
            GatewayVisibilityWire::Visible
        );
        assert_eq!(
            degraded.custody_recovery,
            GatewayCustodyRecoveryWire::ExportRotateRevokeGateway
        );

        let mut corpus = Vec::new();
        for disclosure in [&host, &replicated, &degraded] {
            let bytes = encode_gateway_disclosure(disclosure).unwrap();
            corpus.extend_from_slice(&u16::try_from(bytes.len()).unwrap().to_be_bytes());
            corpus.extend_from_slice(&bytes);
        }
        assert_eq!(
            blake3::hash(&corpus).to_hex().as_str(),
            "b07ec7cb405aaa5fbfd14f82ba7368b4fbe981eb697914f9b06b197fcb499df9"
        );
    }

    #[test]
    fn gateway_codec_rejects_softened_or_noncanonical_claims() {
        let value = GatewayTrustDisclosureWire::for_browser(
            GatewayAuthorityModeWire::ReplicatedFacilitator,
            DeviceCustodyWire::BrowserLocal,
            GatewayProjectionProtectionWire::DeviceEndToEndEncrypted,
        )
        .unwrap();
        let bytes = encode_gateway_disclosure(&value).unwrap();
        assert_eq!(decode_gateway_disclosure(&bytes), Ok(value.clone()));

        let mut softened = value.clone();
        softened.censor_delay_reorder = GatewayAbilityWire::Absent;
        assert_eq!(
            encode_gateway_disclosure(&softened),
            Err(GatewayDisclosureError::InconsistentClaims)
        );
        let pretty = serde_json::to_string_pretty(&value).unwrap();
        assert_eq!(
            decode_gateway_disclosure(pretty.as_bytes()),
            Err(GatewayDisclosureError::NonCanonical)
        );
    }

    #[test]
    fn gateway_host_cannot_claim_unreadable_projection_and_roots_stay_elsewhere() {
        assert_eq!(
            GatewayTrustDisclosureWire::for_browser(
                GatewayAuthorityModeWire::HostAuthoritative,
                DeviceCustodyWire::BrowserLocal,
                GatewayProjectionProtectionWire::DeviceEndToEndEncrypted,
            ),
            Err(GatewayDisclosureError::UnsupportedProfile)
        );
        for custody in [
            DeviceCustodyWire::BrowserLocal,
            DeviceCustodyWire::GatewayCustodied,
        ] {
            let value = GatewayTrustDisclosureWire::for_browser(
                GatewayAuthorityModeWire::ReplicatedFacilitator,
                custody,
                GatewayProjectionProtectionWire::DeviceEndToEndEncrypted,
            )
            .unwrap();
            assert_eq!(value.player_root_access, GatewayAbilityWire::Absent);
            assert_eq!(value.impersonate_other_devices, GatewayAbilityWire::Absent);
        }
    }
}
