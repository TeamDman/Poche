#![cfg(feature = "device-service")]

use ed25519_dalek::{Signer, SigningKey};
use poche_player_client::{
    DeviceClientError, DeviceProfile, DeviceSigner, sign_observation_request,
};
use poche_protocol::*;
use poche_runtime::{
    CertifiedDeviceRoom, LoopbackCodec, OracleRoomActionSource, OracleSessionGame,
    RuntimeLoopbackDeviceAdapter,
};
use poche_session::SessionState;
use poche_veilid::{VeilidDeviceReply, VeilidDeviceRequest, VeilidDeviceService};

struct TestSigner(SigningKey);
impl DeviceSigner for TestSigner {
    fn sign_device_bytes(
        &self,
        _profile: &DeviceProfile,
        bytes: &[u8],
    ) -> Result<SignatureBytes, DeviceClientError> {
        Ok(signature(&self.0, bytes))
    }
}
fn signature(key: &SigningKey, bytes: &[u8]) -> SignatureBytes {
    SignatureBytes::new(data_encoding::HEXLOWER.encode(&key.sign(bytes).to_bytes())).unwrap()
}

#[test]
fn device_dispatch_authenticates_before_returning_observations() {
    let root = SigningKey::from_bytes(&[31; 32]);
    let device = SigningKey::from_bytes(&[32; 32]);
    let player_id =
        PrincipalId::new(data_encoding::HEXLOWER.encode(&root.verifying_key().to_bytes())).unwrap();
    let device_public = data_encoding::HEXLOWER.encode(&device.verifying_key().to_bytes());
    let device_id = DeviceId::new(device_public.clone()).unwrap();
    let unsigned = UnsignedDeviceCertificateWire {
        schema_version: REPLICATION_SCHEMA_VERSION_V1,
        certificate_id: CertificateId::new("service-test").unwrap(),
        player_id: player_id.clone(),
        device_id: device_id.clone(),
        device_signing_public_key: device_public,
        device_encryption_public_key: "ee".repeat(32),
        sequence: 1,
        valid_from_membership_epoch: 1,
        valid_through_membership_epoch: None,
        capabilities: vec![
            DeviceCapabilityWire::Propose,
            DeviceCapabilityWire::ReceivePrivateProjection,
        ],
        custody: DeviceCustodyWire::NativeLocal,
        signature_intent: SignatureIntent {
            domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: player_id.clone(),
        },
    };
    let sig = signature(
        &root,
        &canonical_device_certificate_bytes(&unsigned).unwrap(),
    );
    let profile = DeviceProfile {
        schema_version: DeviceProfile::SCHEMA_VERSION_V1,
        label: "test".to_owned(),
        player_id,
        device_id,
        certificate: unsigned.attach_signature(sig).unwrap(),
        signing_key_handle: "test-key".to_owned(),
    };
    let room_id = RoomId::new("device-service-test").unwrap();
    let state = SessionState::<OracleSessionGame<2>>::pending(
        room_id.clone(),
        PrincipalId::new("clock").unwrap(),
        PrincipalId::new("game").unwrap(),
    );
    let adapter = RuntimeLoopbackDeviceAdapter::new(
        state,
        OracleRoomActionSource::new(29, 2, "test-invite", 30, "test-countdown").unwrap(),
        LoopbackCodec::CanonicalNdjson,
    );
    let service = VeilidDeviceService::new(CertifiedDeviceRoom::new(adapter));
    let request = sign_observation_request(
        &profile,
        &room_id,
        0,
        CorrelationId::new("request-one").unwrap(),
        DeviceObservationModeWire::Snapshot,
        &TestSigner(device),
    )
    .unwrap();
    let bytes = VeilidDeviceRequest::Observe(request.clone())
        .encode()
        .unwrap();
    let reply = service.dispatch(&bytes).unwrap();
    assert!(matches!(
        VeilidDeviceReply::decode(&reply).unwrap(),
        VeilidDeviceReply::Observation(_)
    ));
    // Mutating a signed field must not return a projection or enroll new data.
    let mut tampered = request;
    tampered.room_id = RoomId::new("other-room").unwrap();
    let reply = service
        .dispatch(&VeilidDeviceRequest::Observe(tampered).encode().unwrap())
        .unwrap();
    assert!(matches!(
        VeilidDeviceReply::decode(&reply).unwrap(),
        VeilidDeviceReply::Denied
    ));
    assert!(service.dispatch(b"{}").is_err());

    #[cfg(feature = "veilid-mock-test")]
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use std::{sync::Arc, time::Duration};
        use veilid_core::{Target, VeilidConfig, VeilidUpdate, api_startup};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_str().unwrap();
        let mut config = VeilidConfig::new(
            "poche_device_rpc_test",
            "teamdman",
            "org",
            Some(path),
            Some(path),
        );
        config.namespace = "server".to_owned();
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let server = api_startup(
            Arc::new(move |update| {
                if let VeilidUpdate::AppCall(call) = update {
                    send.try_send(call).unwrap();
                }
            }),
            config.clone(),
        )
        .await
        .unwrap();
        config.namespace = "client".to_owned();
        let client = api_startup(Arc::new(drop), config).await.unwrap();
        server.attach().await.unwrap();
        client.attach().await.unwrap();
        let route = server.new_private_route().await.unwrap();
        let remote = client.import_remote_private_route(route.blob).unwrap();
        let server_api = server.clone();
        let handler = tokio::spawn(async move {
            let call = receive.recv().await.unwrap();
            service.answer_app_call(&server_api, &call).await.unwrap();
        });
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            client
                .routing_context()
                .unwrap()
                .app_call(Target::RouteId(remote), bytes),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(
            VeilidDeviceReply::decode(&response).unwrap(),
            VeilidDeviceReply::Observation(_)
        ));
        handler.await.unwrap();
        client.shutdown().await;
        server.shutdown().await;
    });
}
