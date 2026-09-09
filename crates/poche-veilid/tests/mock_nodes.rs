#![cfg(feature = "veilid-mock-test")]

use poche_veilid::{
    ApplicationIdentity, IdentityStoragePolicy, IdentityStore, PublicRoomMetadata, RoomNetwork,
    SecretIdentityBlob, VeilidProtectedIdentityStore, VeilidRendezvous,
};
use std::sync::Arc;
use veilid_core::{CRYPTO_KIND_VLD0, DHTSchema, VeilidConfig, api_startup};

/// Upstream's process-local simulation, not evidence of socket connectivity.
#[tokio::test]
async fn two_mock_nodes_share_dht_but_not_device_secrets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_str().unwrap();
    let mut first = VeilidConfig::new(
        "poche_mock_adapter",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    first.namespace = "writer".to_owned();
    let mut second = first.clone();
    second.namespace = "reader".to_owned();
    let writer = api_startup(Arc::new(drop), first).await.unwrap();
    let reader = api_startup(Arc::new(drop), second).await.unwrap();
    writer.attach().await.unwrap();
    reader.attach().await.unwrap();

    let writer_store = VeilidProtectedIdentityStore::new(writer.clone());
    let reader_store = VeilidProtectedIdentityStore::new(reader.clone());
    writer_store
        .save(&SecretIdentityBlob::new(vec![7; 32]))
        .await
        .unwrap();
    assert!(reader_store.load().await.unwrap().is_none());
    assert_eq!(
        writer_store
            .load()
            .await
            .unwrap()
            .unwrap()
            .with_bytes(<[u8]>::to_vec),
        vec![7; 32]
    );

    let outgoing = writer.routing_context().unwrap();
    let incoming = reader.routing_context().unwrap();
    let record = outgoing
        .create_dht_record(CRYPTO_KIND_VLD0, DHTSchema::dflt(1).unwrap(), None)
        .await
        .unwrap();
    outgoing
        .set_dht_value(record.key(), 0, b"poche-room-test".to_vec(), None)
        .await
        .unwrap();
    let _ = incoming.open_dht_record(record.key(), None).await.unwrap();
    let value = incoming
        .get_dht_value(record.key(), 0, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value.data(), b"poche-room-test");

    // Exercise Poche's real rendezvous adapter, not only raw upstream calls.
    let host_identity =
        ApplicationIdentity::load_or_create(&reader_store, IdentityStoragePolicy::RequireProtected)
            .await
            .unwrap();
    let host = VeilidRendezvous::new(reader.clone()).unwrap();
    let joiner = VeilidRendezvous::new(writer.clone()).unwrap();
    let published = host
        .publish_room(
            &host_identity,
            RoomNetwork::VeilidLocal,
            poche_protocol::RoomId::new("mock-room").unwrap(),
            PublicRoomMetadata::new("Mock room", 2, true).unwrap(),
            1,
            1,
            10_000,
            100,
        )
        .await
        .unwrap();
    let resolved = joiner
        .resolve_room(published.room_code(), 101)
        .await
        .unwrap();
    assert_eq!(resolved.record().room_id, published.record().room_id);

    let route = published.route_id().clone();
    published.close(&reader).await.unwrap();
    assert!(
        reader.release_private_route(route).is_err(),
        "publication cleanup must release its private route before shutdown"
    );

    reader.shutdown().await;
    writer.shutdown().await;
}
