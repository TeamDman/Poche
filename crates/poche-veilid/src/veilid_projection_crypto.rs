// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use data_encoding::BASE64URL_NOPAD;
use poche_protocol::{
    PrincipalId, ProjectionEnvelope, ProtocolFrame, RoomId, SignatureBytes, decode_frame_line,
    encode_frame_line,
};
use serde::Serialize;
use veilid_core::{
    BareDecapsulationKey, BareEncapsulationKey, CRYPTO_KIND_VLD0, DecapsulationKey,
    EncapsulationKey, VeilidAPI, bytes::Bytes,
};

use crate::{
    ApplicationIdentity, ApplicationPublicIdentity, ENCRYPTED_PROJECTION_SCHEMA_VERSION,
    EncryptedProjectionPacket, ProjectionCryptoError, ProjectionEncryptionAlgorithm,
    verify_application_bytes,
};

const PACKET_SIGNATURE_DOMAIN: &[u8] = b"poche/encrypted-projection/v1\0";

#[derive(Serialize)]
struct ProjectionAssociatedData<'a> {
    schema_version: u16,
    algorithm: ProjectionEncryptionAlgorithm,
    room_id: &'a RoomId,
    session_epoch: u64,
    host_principal_id: &'a PrincipalId,
    recipient_principal_id: &'a PrincipalId,
    projection_epoch: u64,
    current_revision: u64,
}

#[derive(Serialize)]
struct ProjectionPacketSignature<'a> {
    associated_data: ProjectionAssociatedData<'a>,
    sealed_base64url: &'a str,
}

/// Seal one validated viewer projection using Veilid 0.5.7 VLD0 HPKE base
/// mode, then sign the complete opaque packet with the stable host identity.
///
/// # Errors
///
/// Rejects invalid identities/projections, recipient mismatch, unavailable
/// crypto, encryption failure, or a packet exceeding the safe ceiling.
pub async fn seal_projection(
    api: &VeilidAPI,
    host: &ApplicationIdentity,
    recipient: &ApplicationPublicIdentity,
    projection: &ProjectionEnvelope,
) -> Result<EncryptedProjectionPacket, ProjectionCryptoError> {
    recipient
        .validate()
        .map_err(|_| ProjectionCryptoError::InvalidIdentity)?;
    let host_public = host.public();
    if projection.principal_id != recipient.principal_id {
        return Err(ProjectionCryptoError::WrongRecipient);
    }
    let plaintext = encode_frame_line(&ProtocolFrame::Projection(projection.clone()))
        .map_err(|_| ProjectionCryptoError::InvalidProjection)?;
    let mut packet = EncryptedProjectionPacket {
        schema_version: ENCRYPTED_PROJECTION_SCHEMA_VERSION,
        algorithm: ProjectionEncryptionAlgorithm::Vld0HpkeBase,
        room_id: projection.room_id.clone(),
        session_epoch: projection.session_epoch,
        host_principal_id: host_public.principal_id,
        recipient_principal_id: recipient.principal_id.clone(),
        projection_epoch: projection.projection_epoch,
        current_revision: projection.current_revision,
        sealed_base64url: String::new(),
        host_signature: SignatureBytes::new("0".repeat(128))
            .map_err(|_| ProjectionCryptoError::Unavailable)?,
    };
    let aad = associated_data_bytes(&packet)?;
    let recipient_bytes = recipient
        .encryption_key_bytes()
        .map_err(|_| ProjectionCryptoError::InvalidIdentity)?;
    let recipient_key = EncapsulationKey::new(
        CRYPTO_KIND_VLD0,
        BareEncapsulationKey::new(&recipient_bytes),
    );
    let crypto = api
        .crypto()
        .map_err(|_| ProjectionCryptoError::Unavailable)?;
    let vcrypto = crypto
        .get_async(CRYPTO_KIND_VLD0)
        .ok_or(ProjectionCryptoError::Unavailable)?;
    let sealed = vcrypto
        .hpke_seal(&recipient_key, Bytes::from(aad), Bytes::from(plaintext))
        .await
        .map_err(|_| ProjectionCryptoError::EncryptFailed)?;
    packet.sealed_base64url = BASE64URL_NOPAD.encode(&sealed);
    let signing_bytes = packet_signing_bytes(&packet)?;
    packet.host_signature = host
        .sign_application_bytes(&signing_bytes)
        .map_err(|_| ProjectionCryptoError::Unavailable)?;
    packet.encode()?;
    Ok(packet)
}

/// Verify the stable host signature, open one HPKE packet with the exact
/// recipient identity, and revalidate every duplicated projection field.
///
/// # Errors
///
/// Wrong recipients, host forgery, ciphertext tampering, stale metadata, and
/// malformed plaintext all fail closed with redacted categories.
pub async fn open_projection(
    api: &VeilidAPI,
    expected_host: &ApplicationPublicIdentity,
    recipient: &ApplicationIdentity,
    expected_projection_epoch: u64,
    packet: &EncryptedProjectionPacket,
) -> Result<ProjectionEnvelope, ProjectionCryptoError> {
    packet.validate()?;
    expected_host
        .validate()
        .map_err(|_| ProjectionCryptoError::InvalidIdentity)?;
    let recipient_public = recipient.public();
    if packet.recipient_principal_id != recipient_public.principal_id {
        return Err(ProjectionCryptoError::WrongRecipient);
    }
    if packet.projection_epoch != expected_projection_epoch {
        return Err(ProjectionCryptoError::StaleProjection);
    }
    if packet.host_principal_id != expected_host.principal_id {
        return Err(ProjectionCryptoError::BadHostSignature);
    }
    verify_application_bytes(
        expected_host,
        &packet_signing_bytes(packet)?,
        &packet.host_signature,
    )
    .map_err(|_| ProjectionCryptoError::BadHostSignature)?;

    let secret = recipient.with_encryption_secret(|bytes| {
        DecapsulationKey::new(CRYPTO_KIND_VLD0, BareDecapsulationKey::new(bytes))
    });
    let crypto = api
        .crypto()
        .map_err(|_| ProjectionCryptoError::Unavailable)?;
    let vcrypto = crypto
        .get_async(CRYPTO_KIND_VLD0)
        .ok_or(ProjectionCryptoError::Unavailable)?;
    let plaintext = vcrypto
        .hpke_open(
            &secret,
            Bytes::from(associated_data_bytes(packet)?),
            Bytes::from(packet.sealed_bytes()?),
        )
        .await
        .map_err(|_| ProjectionCryptoError::DecryptFailed)?;
    let ProtocolFrame::Projection(projection) =
        decode_frame_line(&plaintext).map_err(|_| ProjectionCryptoError::InvalidProjection)?
    else {
        return Err(ProjectionCryptoError::InvalidProjection);
    };
    if projection.room_id != packet.room_id
        || projection.session_epoch != packet.session_epoch
        || projection.principal_id != packet.recipient_principal_id
        || projection.projection_epoch != packet.projection_epoch
        || projection.current_revision != packet.current_revision
    {
        return Err(ProjectionCryptoError::InvalidProjection);
    }
    Ok(projection)
}

fn associated_data_bytes(
    packet: &EncryptedProjectionPacket,
) -> Result<Vec<u8>, ProjectionCryptoError> {
    serde_json::to_vec(&ProjectionAssociatedData {
        schema_version: packet.schema_version,
        algorithm: packet.algorithm,
        room_id: &packet.room_id,
        session_epoch: packet.session_epoch,
        host_principal_id: &packet.host_principal_id,
        recipient_principal_id: &packet.recipient_principal_id,
        projection_epoch: packet.projection_epoch,
        current_revision: packet.current_revision,
    })
    .map_err(|_| ProjectionCryptoError::InvalidPacket)
}

fn packet_signing_bytes(
    packet: &EncryptedProjectionPacket,
) -> Result<Vec<u8>, ProjectionCryptoError> {
    let body = serde_json::to_vec(&ProjectionPacketSignature {
        associated_data: ProjectionAssociatedData {
            schema_version: packet.schema_version,
            algorithm: packet.algorithm,
            room_id: &packet.room_id,
            session_epoch: packet.session_epoch,
            host_principal_id: &packet.host_principal_id,
            recipient_principal_id: &packet.recipient_principal_id,
            projection_epoch: packet.projection_epoch,
            current_revision: packet.current_revision,
        },
        sealed_base64url: &packet.sealed_base64url,
    })
    .map_err(|_| ProjectionCryptoError::InvalidPacket)?;
    let mut signing = Vec::with_capacity(PACKET_SIGNATURE_DOMAIN.len() + body.len());
    signing.extend_from_slice(PACKET_SIGNATURE_DOMAIN);
    signing.extend_from_slice(&body);
    Ok(signing)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use poche_protocol::{
        CommandId, CorrelationId, EventId, HandProjection, MemberProjection, PROTOCOL_VERSION_V1,
        ProjectionId, ProjectionPayload, RoomPhase, SIGNATURE_DOMAIN_V1, SignatureAlgorithm,
        SignatureMetadata,
    };

    use super::*;
    use crate::{
        ExplicitInsecureDevelopment, IdentityStoragePolicy, InsecureMemoryIdentityStore,
        TransportCommandReply, TransportDisposition, TransportWireError,
    };

    async fn identity() -> ApplicationIdentity {
        let store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        ApplicationIdentity::load_or_create(
            &store,
            IdentityStoragePolicy::AllowExplicitInsecure(
                ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
            ),
        )
        .await
        .unwrap()
    }

    fn projection(
        recipient: &ApplicationPublicIdentity,
        player: &ApplicationPublicIdentity,
        other_player: &ApplicationPublicIdentity,
        epoch: u64,
        granted_cards: Vec<u8>,
    ) -> ProjectionEnvelope {
        let granted_hands = if granted_cards.is_empty() {
            Vec::new()
        } else {
            vec![HandProjection {
                player: player.principal_id.clone(),
                grant_epoch: epoch,
                cards: granted_cards,
            }]
        };
        ProjectionEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("projection-privacy-room").unwrap(),
            session_epoch: 1,
            projection_id: ProjectionId::new(format!("projection-{epoch}")).unwrap(),
            principal_id: recipient.principal_id.clone(),
            current_revision: 40 + epoch,
            projection_epoch: epoch,
            correlation_id: CorrelationId::new(format!("correlation-{epoch}")).unwrap(),
            causation_id: EventId::new(format!("cause-{epoch}")).unwrap(),
            payload: ProjectionPayload {
                phase: RoomPhase::Running,
                members: vec![
                    MemberProjection {
                        principal_id: player.principal_id.clone(),
                        connected: true,
                        seat: Some(0),
                        ready: false,
                        host: true,
                    },
                    MemberProjection {
                        principal_id: recipient.principal_id.clone(),
                        connected: true,
                        seat: None,
                        ready: false,
                        host: false,
                    },
                    MemberProjection {
                        principal_id: other_player.principal_id.clone(),
                        connected: true,
                        seat: Some(1),
                        ready: false,
                        host: false,
                    },
                ],
                public_game_state: None,
                own_hand: None,
                granted_hands,
                public_history: Vec::new(),
            },
            signature: SignatureMetadata {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: recipient.principal_id.clone(),
                signature: SignatureBytes::new("0".repeat(128)).unwrap(),
            },
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(
        clippy::too_many_lines,
        reason = "one Veilid startup captures the complete grant/revoke privacy scenario"
    )]
    async fn capture_is_opaque_exact_recipient_only_and_revoke_stops_future_hands() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_string_lossy();
        let mut config = veilid_core::VeilidConfig::new(
            "poche_projection_crypto_test",
            "teamdman",
            "org",
            Some(path.as_ref()),
            Some(path.as_ref()),
        );
        config.namespace = "single_crypto_acceptance".to_owned();
        config.protected_store.always_use_insecure_storage = true;
        config.protected_store.device_encryption_key_password = "test-only-password".to_owned();
        let api = veilid_core::api_startup(Arc::new(drop), config)
            .await
            .unwrap();

        let host = identity().await;
        let spectator = identity().await;
        let other_spectator = identity().await;
        let other_player = identity().await;
        let host_public = host.public();
        let spectator_public = spectator.public();
        let other_player_public = other_player.public();
        let granted = projection(
            &spectator_public,
            &host_public,
            &other_player_public,
            1,
            vec![7, 8, 9],
        );
        let packet = seal_projection(&api, &host, &spectator_public, &granted)
            .await
            .unwrap();
        let capture = packet.encode().unwrap();
        assert_eq!(EncryptedProjectionPacket::decode(&capture).unwrap(), packet);
        assert!(
            !capture
                .windows(b"granted_hands".len())
                .any(|window| window == b"granted_hands")
        );
        assert!(
            !capture
                .windows(b"\"cards\":[7,8,9]".len())
                .any(|window| window == b"\"cards\":[7,8,9]")
        );
        assert!(!format!("{packet:?}").contains(&packet.sealed_base64url));
        let reply = TransportCommandReply::new(
            CommandId::new("encrypted-delivery").unwrap(),
            TransportDisposition::Duplicate,
            None,
            packet.current_revision,
            packet.current_revision,
            Vec::new(),
            Some(packet.clone()),
        )
        .unwrap();
        let reply_capture = reply.encode().unwrap();
        assert!(
            !reply_capture
                .windows(b"granted_hands".len())
                .any(|window| window == b"granted_hands")
        );
        assert_eq!(
            TransportCommandReply::decode(&reply_capture).unwrap(),
            reply
        );
        assert_eq!(
            TransportCommandReply::new(
                CommandId::new("plaintext-rejected").unwrap(),
                TransportDisposition::RecoveryRequired,
                None,
                granted.current_revision,
                granted.current_revision,
                vec![ProtocolFrame::Projection(granted.clone())],
                None,
            ),
            Err(TransportWireError::InvalidReply)
        );

        let opened = open_projection(&api, &host_public, &spectator, 1, &packet)
            .await
            .unwrap();
        assert_eq!(opened.payload.granted_hands.len(), 1);
        assert_eq!(
            opened.payload.granted_hands[0].player,
            host_public.principal_id
        );
        assert_eq!(opened.payload.granted_hands[0].cards, vec![7, 8, 9]);
        assert!(
            opened
                .payload
                .granted_hands
                .iter()
                .all(|hand| hand.player != other_player_public.principal_id)
        );
        assert_eq!(
            open_projection(&api, &host_public, &other_spectator, 1, &packet).await,
            Err(ProjectionCryptoError::WrongRecipient)
        );
        assert_eq!(
            open_projection(&api, &host_public, &other_player, 1, &packet).await,
            Err(ProjectionCryptoError::WrongRecipient)
        );

        let mut future = granted.clone();
        future.current_revision += 1;
        future.projection_id = ProjectionId::new("projection-1-future").unwrap();
        future.causation_id = EventId::new("cause-1-future").unwrap();
        future.payload.granted_hands[0].cards = vec![10, 11];
        let future_packet = seal_projection(&api, &host, &spectator_public, &future)
            .await
            .unwrap();
        let opened_future = open_projection(&api, &host_public, &spectator, 1, &future_packet)
            .await
            .unwrap();
        assert_eq!(opened_future.projection_epoch, 1);
        assert_eq!(opened_future.payload.granted_hands[0].cards, vec![10, 11]);

        let other_public = other_spectator.public();
        let other_packet = seal_projection(
            &api,
            &host,
            &other_public,
            &projection(
                &other_public,
                &host_public,
                &other_player_public,
                1,
                Vec::new(),
            ),
        )
        .await
        .unwrap();
        let opened_ungranted =
            open_projection(&api, &host_public, &other_spectator, 1, &other_packet)
                .await
                .unwrap();
        assert!(opened_ungranted.payload.own_hand.is_none());
        assert!(opened_ungranted.payload.granted_hands.is_empty());
        assert!(
            open_projection(&api, &host_public, &spectator, 1, &other_packet)
                .await
                .is_err()
        );

        let revoked = projection(
            &spectator_public,
            &host_public,
            &other_player_public,
            2,
            Vec::new(),
        );
        let revoked_packet = seal_projection(&api, &host, &spectator_public, &revoked)
            .await
            .unwrap();
        let opened_revoked = open_projection(&api, &host_public, &spectator, 2, &revoked_packet)
            .await
            .unwrap();
        assert_eq!(opened_revoked.projection_epoch, 2);
        assert!(opened_revoked.payload.granted_hands.is_empty());
        assert_eq!(
            open_projection(&api, &host_public, &spectator, 2, &packet).await,
            Err(ProjectionCryptoError::StaleProjection)
        );

        let mut wrong_key_packet = packet.clone();
        wrong_key_packet.recipient_principal_id = other_public.principal_id;
        wrong_key_packet.host_signature = host
            .sign_application_bytes(&packet_signing_bytes(&wrong_key_packet).unwrap())
            .unwrap();
        assert_eq!(
            open_projection(&api, &host_public, &other_spectator, 1, &wrong_key_packet,).await,
            Err(ProjectionCryptoError::DecryptFailed)
        );

        let mut tampered = packet.clone();
        tampered.current_revision += 1;
        assert_eq!(
            open_projection(&api, &host_public, &spectator, 1, &tampered).await,
            Err(ProjectionCryptoError::BadHostSignature)
        );

        api.clone().shutdown().await;
        assert!(api.is_shutdown());
    }
}
