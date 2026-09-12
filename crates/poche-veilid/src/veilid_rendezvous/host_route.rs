// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. https://mozilla.org/MPL/2.0/

//! Owned, recoverable route publication. A failed flush is not a failed write.
use super::{
    DHT_FLUSH_TIMEOUT, PublishedRoom, ResumedHostRoom, VeilidRendezvous, VeilidRendezvousError,
    map_veilid_error, retry_private_route,
};
use crate::{RENDEZVOUS_DHT_SUBKEY, RendezvousRecord};
use std::{future::Future, str::FromStr};
use veilid_core::{KeyPair, RecordKey, RouteBlob, RouteId, RoutingContext, VeilidAPI};

struct OwnedRoute {
    id: RouteId,
    record: RendezvousRecord,
    dead: bool,
}

struct Publication {
    current: OwnedRoute,
    // At most one candidate. Keep it alive across uncertain writes/flushes.
    pending: Option<OwnedRoute>,
}

/// Sole local owner of an existing room publication, not a new game authority.
/// No Debug implementation: record keys and route blobs are capabilities.
/// Explicit close closes the DHT handle; Drop always releases known routes.
pub struct HostRouteOwner {
    publication: Publication,
    io: LiveRouteIo,
}

struct LiveRouteIo {
    api: VeilidAPI,
    routing: RoutingContext,
    key: RecordKey,
    allocating: tokio::sync::Mutex<Option<AllocationReply>>,
}

type AllocationReply =
    tokio::sync::oneshot::Receiver<Result<AllocatedRoute, VeilidRendezvousError>>;

// Upstream allocation awaits route tests after allocating an internal route.
// Do not cancel that future on our deadline. Retain one receiver for retry;
// if the owner disappears, release the eventual result instead of leaking it.
struct AllocatedRoute {
    api: VeilidAPI,
    route: Option<RouteBlob>,
}
impl Drop for AllocatedRoute {
    fn drop(&mut self) {
        if let Some(route) = self.route.take() {
            let _ = self.api.release_private_route(route.route_id);
        }
    }
}

impl LiveRouteIo {
    async fn await_allocation(&self) -> Result<RouteBlob, VeilidRendezvousError> {
        let mut allocating = self.allocating.lock().await;
        let receiver = allocating.get_or_insert_with(|| {
            let (send, receive) = tokio::sync::oneshot::channel();
            let api = self.api.clone();
            tokio::spawn(async move {
                let result =
                    retry_private_route(|| api.new_private_route(), veilid_core::tools::sleep)
                        .await
                        .map(|route| AllocatedRoute {
                            api: api.clone(),
                            route: Some(route),
                        })
                        .map_err(|error| map_veilid_error(&error));
                // Failed delivery drops the result's cleanup guard.
                let _ = send.send(result);
            });
            receive
        });
        let result = receiver.await;
        *allocating = None;
        let mut allocated = result.map_err(|_| VeilidRendezvousError::Unavailable)??;
        allocated
            .route
            .take()
            .ok_or(VeilidRendezvousError::Unavailable)
    }
}

// This private seam exercises the production transaction with deterministic
// failures. It is not a replacement network or an application transport.
trait RouteIo: Sync {
    fn read(&self) -> impl Future<Output = Result<RendezvousRecord, VeilidRendezvousError>> + Send;
    fn allocate(&self) -> impl Future<Output = Result<RouteBlob, VeilidRendezvousError>> + Send;
    fn write(
        &self,
        record: &RendezvousRecord,
    ) -> impl Future<Output = Result<(), VeilidRendezvousError>> + Send;
    fn flush(&self) -> impl Future<Output = Result<(), VeilidRendezvousError>> + Send;
    fn release(&self, route: RouteId);
}

impl RouteIo for LiveRouteIo {
    async fn read(&self) -> Result<RendezvousRecord, VeilidRendezvousError> {
        // Veilid's open-record state is node-wide, not reference-counted per
        // RoutingContext. A same-node client may have closed its view.
        let _ = self
            .routing
            .open_dht_record(self.key.clone(), None)
            .await
            .map_err(|error| map_veilid_error(&error))?;
        let value = self
            .routing
            .get_dht_value(self.key.clone(), RENDEZVOUS_DHT_SUBKEY, true)
            .await
            .map_err(|error| map_veilid_error(&error))?
            .ok_or(VeilidRendezvousError::NotFound)?;
        RendezvousRecord::decode(value.data()).map_err(Into::into)
    }

    async fn allocate(&self) -> Result<RouteBlob, VeilidRendezvousError> {
        self.await_allocation().await
    }

    async fn write(&self, record: &RendezvousRecord) -> Result<(), VeilidRendezvousError> {
        // Opening this record for the creator's own client replaces the node's
        // default writer with None. Never rely on that shared mutable default:
        // recover and bind the protected owner capability for this exact write.
        let adapter = VeilidRendezvous {
            api: self.api.clone(),
            routing: self.routing.clone(),
        };
        let capability = adapter.load_host_capability(&record.room_id).await?;
        let key = capability
            .with_record_key(RecordKey::from_str)
            .map_err(|_| VeilidRendezvousError::InvalidRecord)?;
        if capability.host_principal != record.host_identity.principal_id
            || capability.room_id != record.room_id
            || key != self.key
        {
            return Err(VeilidRendezvousError::InvalidMembership);
        }
        let writer = capability
            .with_owner_keypair(KeyPair::from_str)
            .map_err(|_| VeilidRendezvousError::InvalidRecord)?;
        match self
            .routing
            .set_dht_value(
                self.key.clone(),
                RENDEZVOUS_DHT_SUBKEY,
                record.encode()?,
                Some(veilid_core::SetDHTValueOptions {
                    writer: Some(writer),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|error| map_veilid_error(&error))?
        {
            None => Ok(()),
            Some(_) => Err(VeilidRendezvousError::Conflict),
        }
    }

    async fn flush(&self) -> Result<(), VeilidRendezvousError> {
        if self
            .routing
            .flush_dht_record(self.key.clone(), Some(DHT_FLUSH_TIMEOUT))
            .await
            .map_err(|error| map_veilid_error(&error))?
        {
            Ok(())
        } else {
            Err(VeilidRendezvousError::Timeout)
        }
    }

    fn release(&self, route: RouteId) {
        // Dead routes may already have been released upstream.
        let _ = self.api.release_private_route(route);
    }
}

impl VeilidRendezvous {
    /// Transfer a publication to live route maintenance. Preserve its invite
    /// separately before this call; maintenance never issues a new invitation.
    #[must_use]
    pub fn own_published_route(&self, room: PublishedRoom) -> HostRouteOwner {
        self.own_route(room.record_key, room.route_id, room.record)
    }

    /// Transfer a restored room to the same maintenance used for new rooms.
    #[must_use]
    pub fn own_resumed_route(&self, room: ResumedHostRoom) -> HostRouteOwner {
        self.own_route(room.record_key, room.route_id, room.record)
    }

    fn own_route(&self, key: RecordKey, id: RouteId, record: RendezvousRecord) -> HostRouteOwner {
        HostRouteOwner {
            publication: Publication {
                current: OwnedRoute {
                    id,
                    record,
                    dead: false,
                },
                pending: None,
            },
            io: LiveRouteIo {
                api: self.api.clone(),
                routing: self.routing.clone(),
                key,
                allocating: tokio::sync::Mutex::new(None),
            },
        }
    }
}

impl HostRouteOwner {
    /// Deliberate availability fault for opt-in acceptance. Do not mark the
    /// route dead here: recovery must be driven by Veilid's actual callback.
    #[cfg(feature = "native-input-test")]
    pub(crate) async fn retire_for_acceptance(&self) -> Result<u64, VeilidRendezvousError> {
        #[cfg(not(feature = "veilid-mock-test"))]
        let before = self.owned_route_presence_for_acceptance().await?;
        #[cfg(not(feature = "veilid-mock-test"))]
        if !before.0 {
            eprintln!(
                "poche route fault: precondition failed; allocated={} imported={}",
                before.0, before.1
            );
            return Err(VeilidRendezvousError::Unavailable);
        }
        self.io
            .api
            .release_private_route(self.publication.current.id.clone())
            .map_err(|error| map_veilid_error(&error))?;
        #[cfg(not(feature = "veilid-mock-test"))]
        {
            // Real Veilid 99c9616 gives a self-import the allocated route ID,
            // unlike mock-api. Release prefers the imported entry. Inspect the
            // exact owned ID and retire its allocation too, never another ID.
            let after_first = self.owned_route_presence_for_acceptance().await?;
            eprintln!(
                "poche route fault: before allocated={} imported={}; after first release allocated={} imported={}",
                before.0, before.1, after_first.0, after_first.1
            );
            if after_first.0 {
                self.io
                    .api
                    .release_private_route(self.publication.current.id.clone())
                    .map_err(|error| map_veilid_error(&error))?;
            }
            let after = self.owned_route_presence_for_acceptance().await?;
            if after.0 {
                // Concurrent reimport can race the second release; do not
                // claim successful fault injection or retry without a bound.
                return Err(VeilidRendezvousError::TryAgain);
            }
            eprintln!(
                "poche route fault: allocated route confirmed absent; awaiting actual callback"
            );
        }
        Ok(self.record().route_epoch)
    }

    // Pinned-upstream diagnostic format, acceptance only. Never log the dump:
    // it includes other route IDs and network details. Format drift fails the
    // probe closed rather than manufacturing a successful retirement.
    #[cfg(all(feature = "native-input-test", not(feature = "veilid-mock-test")))]
    async fn owned_route_presence_for_acceptance(
        &self,
    ) -> Result<(bool, bool), VeilidRendezvousError> {
        let listing = self
            .io
            .api
            .debug("route list".to_owned())
            .await
            .map_err(|error| {
                eprintln!("poche route fault: diagnostic route-list call failed");
                map_veilid_error(&error)
            })?;
        let (allocated, imported) = listing
            .split_once("\nRemote Routes: (count = ")
            .filter(|(allocated, _)| allocated.starts_with("Allocated Routes: (count = "))
            .ok_or_else(|| {
                eprintln!(
                    "poche route fault: diagnostic route-list format did not match pinned source"
                );
                VeilidRendezvousError::Unavailable
            })?;
        let prefix = format!("{}: ", self.publication.current.id);
        Ok((
            allocated.lines().any(|line| line.starts_with(&prefix)),
            imported.lines().any(|line| line.starts_with(&prefix)),
        ))
    }

    #[must_use]
    pub fn record(&self) -> &RendezvousRecord {
        &self.publication.current.record
    }

    /// Local route deaths are hints about availability, never membership.
    pub fn routes_died(&mut self, routes: &[RouteId]) {
        self.publication.invalidate(|id| routes.contains(id));
    }

    /// A bounded notification queue overflow cannot silently lose recovery.
    pub fn notifications_lagged(&mut self) {
        self.publication.invalidate(|_| true);
    }

    #[must_use]
    pub fn needs_recovery(&self) -> bool {
        self.publication.current.dead || self.publication.pending.is_some()
    }

    /// Reconcile one bounded-size replacement transaction. Retryable failures
    /// retain its exact candidate; no additional route is allocated on retry.
    ///
    /// # Errors
    /// Foreign/conflicting publication, expiry and API errors are redacted.
    /// Success may still need another pass if the candidate itself died.
    pub async fn recover(&mut self, now_unix_ms: u64) -> Result<(), VeilidRendezvousError> {
        self.publication.recover(&self.io, now_unix_ms).await
    }

    /// Close only this owner's open record and release its routes, preserving
    /// the protected restart capability. This does not disband the game.
    ///
    /// # Errors
    /// Returns a redacted close failure; Drop still releases known routes.
    pub async fn close(self) -> Result<(), VeilidRendezvousError> {
        self.io
            .routing
            .close_dht_record(self.io.key.clone())
            .await
            .map_err(|error| map_veilid_error(&error))
    }
}

impl Drop for HostRouteOwner {
    fn drop(&mut self) {
        self.io.release(self.publication.current.id.clone());
        if let Some(pending) = &self.publication.pending {
            self.io.release(pending.id.clone());
        }
    }
}

impl Publication {
    fn invalidate(&mut self, matches: impl Fn(&RouteId) -> bool) {
        self.current.dead |= matches(&self.current.id);
        if let Some(pending) = &mut self.pending {
            pending.dead |= matches(&pending.id);
        }
    }

    async fn recover(&mut self, io: &impl RouteIo, now: u64) -> Result<(), VeilidRendezvousError> {
        if !self.current.dead && self.pending.is_none() {
            return Ok(());
        }
        if self.current.record.expires_at_unix_ms <= now {
            return Err(VeilidRendezvousError::InvalidRecord);
        }
        let observed = io.read().await?;
        // Validate the entire record, not merely epoch or host name. Never
        // overwrite a concurrent owner, different session or third candidate.
        let candidate_observed = self.pending.as_ref().is_some_and(|p| p.record == observed);
        if observed != self.current.record && !candidate_observed {
            return Err(VeilidRendezvousError::Conflict);
        }
        if self.pending.is_none() {
            let epoch = self
                .current
                .record
                .route_epoch
                .checked_add(1)
                .ok_or(VeilidRendezvousError::InvalidRecord)?;
            let route = io.allocate().await?;
            let old = &self.current.record;
            let record = match RendezvousRecord::new(
                old.network.into(),
                old.room_id.clone(),
                old.host_identity.clone(),
                old.metadata.clone(),
                old.session_epoch,
                epoch,
                old.expires_at_unix_ms,
                now,
                &route.blob,
            ) {
                Ok(record) => record,
                Err(error) => {
                    io.release(route.route_id);
                    return Err(error.into());
                }
            };
            self.pending = Some(OwnedRoute {
                id: route.route_id,
                record,
                dead: false,
            });
        }
        let candidate = self
            .pending
            .as_ref()
            .expect("candidate allocated or retained");
        if !candidate_observed {
            // If the last write had an uncertain result, repeat only these
            // exact metadata bytes after observing the exact prior record.
            io.write(&candidate.record).await?;
        }
        io.flush().await?;
        let confirmed = io.read().await?;
        if confirmed != candidate.record {
            return Err(if confirmed == self.current.record {
                VeilidRendezvousError::TryAgain
            } else {
                VeilidRendezvousError::Conflict
            });
        }
        let committed = self.pending.take().expect("flushed candidate retained");
        let previous = std::mem::replace(&mut self.current, committed);
        io.release(previous.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApplicationIdentity, IdentityStoragePolicy, PublicRoomMetadata, RoomNetwork};
    use poche_protocol::RoomId;
    use std::{sync::Mutex, time::Duration};
    use veilid_core::{BareRouteId, CRYPTO_KIND_VLD0};

    #[derive(Default)]
    struct Counters {
        allocations: u8,
        writes: usize,
        released: Vec<RouteId>,
        fail_allocate: bool,
        write_failure: Option<bool>, // true: committed, but reply lost
        fail_flush: bool,
    }
    struct FakeIo {
        record: Mutex<RendezvousRecord>,
        counters: Mutex<Counters>,
    }
    fn route(id: u8) -> RouteId {
        RouteId::new(CRYPTO_KIND_VLD0, BareRouteId::new(&[id; 32]))
    }

    impl RouteIo for FakeIo {
        async fn read(&self) -> Result<RendezvousRecord, VeilidRendezvousError> {
            Ok(self.record.lock().unwrap().clone())
        }
        async fn allocate(&self) -> Result<RouteBlob, VeilidRendezvousError> {
            let mut state = self.counters.lock().unwrap();
            if std::mem::take(&mut state.fail_allocate) {
                return Err(VeilidRendezvousError::TryAgain);
            }
            state.allocations += 1;
            Ok(RouteBlob {
                route_id: route(state.allocations),
                blob: vec![state.allocations; 32],
            })
        }
        async fn write(&self, record: &RendezvousRecord) -> Result<(), VeilidRendezvousError> {
            let mut state = self.counters.lock().unwrap();
            state.writes += 1;
            let failure = state.write_failure.take();
            if failure != Some(false) {
                *self.record.lock().unwrap() = record.clone();
            }
            if failure.is_some() {
                Err(VeilidRendezvousError::Timeout)
            } else {
                Ok(())
            }
        }
        async fn flush(&self) -> Result<(), VeilidRendezvousError> {
            if std::mem::take(&mut self.counters.lock().unwrap().fail_flush) {
                Err(VeilidRendezvousError::Timeout)
            } else {
                Ok(())
            }
        }
        fn release(&self, id: RouteId) {
            self.counters.lock().unwrap().released.push(id);
        }
    }

    async fn fixture() -> (Publication, FakeIo) {
        let acknowledgement = crate::ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected;
        let identity = ApplicationIdentity::load_or_create(
            &crate::InsecureMemoryIdentityStore::new(acknowledgement),
            IdentityStoragePolicy::AllowExplicitInsecure(acknowledgement),
        )
        .await
        .unwrap();
        let record = RendezvousRecord::new(
            RoomNetwork::VeilidLocal,
            RoomId::new("live-route-test").unwrap(),
            identity.public(),
            PublicRoomMetadata::new("route test", 2, true).unwrap(),
            4,
            7,
            10_000,
            100,
            &[9; 32],
        )
        .unwrap();
        (
            Publication {
                current: OwnedRoute {
                    id: route(0),
                    record: record.clone(),
                    dead: true,
                },
                pending: None,
            },
            FakeIo {
                record: Mutex::new(record),
                counters: Mutex::new(Counters::default()),
            },
        )
    }

    #[tokio::test]
    async fn failed_allocation_preserves_publication_then_advances_only_route_epoch() {
        let (mut state, io) = fixture().await;
        let original = state.current.record.clone();
        io.counters.lock().unwrap().fail_allocate = true;
        assert_eq!(
            state.recover(&io, 101).await,
            Err(VeilidRendezvousError::TryAgain)
        );
        assert!(state.pending.is_none());
        assert_eq!(*io.record.lock().unwrap(), original);
        state.recover(&io, 102).await.unwrap();
        assert_eq!(state.current.record.route_epoch, 8);
        assert_eq!(state.current.record.room_id, original.room_id);
        assert_eq!(state.current.record.host_identity, original.host_identity);
        assert_eq!(state.current.record.session_epoch, original.session_epoch);
        assert_eq!(state.current.record.metadata, original.metadata);
        assert_eq!(state.current.record.network, original.network);
        assert_eq!(
            state.current.record.expires_at_unix_ms,
            original.expires_at_unix_ms
        );
        assert!(!state.current.dead);
        let counts = io.counters.lock().unwrap();
        assert_eq!(counts.released, [route(0)]);
        assert_eq!(counts.allocations, 1);
    }

    #[tokio::test]
    async fn uncertain_writes_and_flushes_reuse_exact_candidate_without_releasing_it() {
        for (write_failure, fail_flush) in [(Some(false), false), (Some(true), false), (None, true)]
        {
            let (mut state, io) = fixture().await;
            {
                let mut counters = io.counters.lock().unwrap();
                counters.write_failure = write_failure;
                counters.fail_flush = fail_flush;
            }
            assert_eq!(
                state.recover(&io, 101).await,
                Err(VeilidRendezvousError::Timeout)
            );
            let candidate = state.pending.as_ref().unwrap().record.clone();
            assert_eq!(state.current.record.route_epoch, 7);
            assert!(io.counters.lock().unwrap().released.is_empty());
            state.recover(&io, 102).await.unwrap();
            assert_eq!(state.current.record, candidate);
            let counts = io.counters.lock().unwrap();
            assert_eq!(counts.allocations, 1);
            assert_eq!(
                counts.writes,
                if write_failure == Some(false) { 2 } else { 1 }
            );
            assert_eq!(counts.released, [route(0)]);
        }
    }

    #[tokio::test]
    async fn a_dead_pending_route_is_not_mistaken_for_completed_recovery() {
        let (mut state, io) = fixture().await;
        io.counters.lock().unwrap().fail_flush = true;
        assert!(state.recover(&io, 101).await.is_err());
        state.invalidate(|id| *id == route(1));
        state.recover(&io, 102).await.unwrap();
        assert!(state.current.dead);
        state.recover(&io, 103).await.unwrap();
        assert!(!state.current.dead);
        assert_eq!(state.current.record.route_epoch, 9);
        assert_eq!(io.counters.lock().unwrap().released, [route(0), route(1)]);
    }

    #[tokio::test]
    async fn foreign_publication_and_epoch_exhaustion_fail_before_allocation_or_write() {
        for pending in [false, true] {
            let (mut state, io) = fixture().await;
            if pending {
                io.counters.lock().unwrap().fail_flush = true;
                assert!(state.recover(&io, 101).await.is_err());
            }
            io.record.lock().unwrap().session_epoch += 1;
            let writes = io.counters.lock().unwrap().writes;
            assert_eq!(
                state.recover(&io, 102).await,
                Err(VeilidRendezvousError::Conflict)
            );
            assert_eq!(io.counters.lock().unwrap().writes, writes);
            assert_eq!(io.counters.lock().unwrap().allocations, u8::from(pending));
        }
        let (mut state, io) = fixture().await;
        state.current.record.route_epoch = u64::MAX;
        *io.record.lock().unwrap() = state.current.record.clone();
        assert_eq!(
            state.recover(&io, 102).await,
            Err(VeilidRendezvousError::InvalidRecord)
        );
        assert_eq!(io.counters.lock().unwrap().allocations, 0);
    }

    #[tokio::test]
    async fn healthy_routes_ignore_stale_or_unrelated_notifications_and_expiry_never_renews() {
        let (mut state, io) = fixture().await;
        state.current.dead = false;
        state.invalidate(|id| *id == route(10));
        state.recover(&io, 101).await.unwrap();
        assert_eq!(io.counters.lock().unwrap().allocations, 0);
        state.invalidate(|_| true); // queue overflow is conservative
        assert_eq!(
            state.recover(&io, 10_000).await,
            Err(VeilidRendezvousError::InvalidRecord)
        );
        assert_eq!(io.counters.lock().unwrap().allocations, 0);
    }

    #[cfg(feature = "veilid-mock-test")]
    async fn mock_owner(
        namespace: &str,
    ) -> (tempfile::TempDir, crate::VeilidDeviceNode, HostRouteOwner) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_str().unwrap();
        let mut config = veilid_core::VeilidConfig::new(
            "poche_route_cleanup",
            "teamdman",
            "org",
            Some(path),
            Some(path),
        );
        config.namespace = namespace.into();
        let node = crate::VeilidDeviceNode::start(config, std::sync::Arc::new(drop)).unwrap();
        node.api().attach().await.unwrap();
        let identity = ApplicationIdentity::load_or_create(
            &crate::VeilidProtectedIdentityStore::new(node.api().clone()),
            IdentityStoragePolicy::RequireProtected,
        )
        .await
        .unwrap();
        let adapter = VeilidRendezvous::new(node.api().clone()).unwrap();
        let published = adapter
            .publish_room(
                &identity,
                RoomNetwork::VeilidLocal,
                RoomId::new(namespace).unwrap(),
                PublicRoomMetadata::new("cleanup test", 2, true).unwrap(),
                1,
                7,
                u64::MAX,
                100,
            )
            .await
            .unwrap();
        let mut owner = adapter.own_published_route(published);
        owner.notifications_lagged();
        (directory, node, owner)
    }

    #[cfg(feature = "veilid-mock-test")]
    #[tokio::test]
    async fn allocation_deadline_retains_one_result_and_owner_drop_releases_late_result() {
        let (_directory, node, mut owner) = mock_owner("allocation-cancel").await;
        let (send, receive) = tokio::sync::oneshot::channel();
        *owner.io.allocating.lock().await = Some(receive);
        // Stand in only for a delayed allocation result, then complete it with
        // a genuinely allocated mock route. The transaction/cleanup are real.
        assert!(
            tokio::time::timeout(Duration::from_millis(2), owner.recover(101))
                .await
                .is_err()
        );
        assert!(owner.io.allocating.lock().await.is_some());
        assert!(owner.publication.pending.is_none());
        let route = node.api().new_private_route().await.unwrap();
        let id = route.route_id.clone();
        assert!(
            send.send(Ok(AllocatedRoute {
                api: node.api().clone(),
                route: Some(route)
            }))
            .is_ok()
        );
        owner.recover(102).await.unwrap();
        assert_eq!(owner.publication.current.id, id);
        assert_eq!(owner.record().route_epoch, 8);
        owner.close().await.unwrap();
        assert!(node.api().release_private_route(id).is_err());
        node.shutdown().unwrap();

        let (_directory, node, owner) = mock_owner("late-allocation-drop").await;
        let (send, receive) = tokio::sync::oneshot::channel();
        *owner.io.allocating.lock().await = Some(receive);
        owner.close().await.unwrap();
        let route = node.api().new_private_route().await.unwrap();
        let id = route.route_id.clone();
        // This is the same failed-delivery guard used by the allocation task.
        drop(send.send(Ok(AllocatedRoute {
            api: node.api().clone(),
            route: Some(route),
        })));
        assert!(node.api().release_private_route(id).is_err());
        node.shutdown().unwrap();
    }

    #[cfg(feature = "veilid-mock-test")]
    #[tokio::test]
    async fn failed_real_dht_write_retains_candidate_until_explicit_cleanup() {
        let (_directory, node, mut owner) = mock_owner("failed-write-cleanup").await;
        node.api()
            .debug(format!(
                "fault arm set timeout --count 1 --record {}",
                owner.io.key
            ))
            .await
            .unwrap();
        assert_eq!(
            owner.recover(101).await,
            Err(VeilidRendezvousError::Timeout)
        );
        let original = owner.publication.current.id.clone();
        let pending = owner.publication.pending.as_ref().unwrap().id.clone();
        owner.close().await.unwrap();
        assert!(node.api().release_private_route(original).is_err());
        assert!(node.api().release_private_route(pending).is_err());
        node.shutdown().unwrap();
    }

    #[cfg(feature = "veilid-mock-test")]
    #[tokio::test]
    async fn closed_shared_record_handle_is_reopened_with_explicit_owner_write() {
        let (_directory, node, mut owner) = mock_owner("shared-record-close").await;
        owner
            .io
            .routing
            .close_dht_record(owner.io.key.clone())
            .await
            .unwrap();
        owner.recover(101).await.unwrap();
        assert_eq!(owner.record().route_epoch, 8);
        owner.close().await.unwrap();
        node.shutdown().unwrap();
    }

    #[cfg(feature = "veilid-mock-test")]
    #[tokio::test]
    async fn successful_but_silent_write_is_not_mistaken_for_a_published_route() {
        let (_directory, node, mut owner) = mock_owner("silent-write-reconcile").await;
        node.api()
            .debug(format!(
                "fault arm set silent --count 1 --record {}",
                owner.io.key
            ))
            .await
            .unwrap();
        assert_eq!(
            owner.recover(101).await,
            Err(VeilidRendezvousError::TryAgain)
        );
        assert_eq!(owner.record().route_epoch, 7);
        let candidate = owner.publication.pending.as_ref().unwrap().id.clone();
        owner.recover(102).await.unwrap();
        assert_eq!(owner.record().route_epoch, 8);
        assert_eq!(owner.publication.current.id, candidate);
        owner.close().await.unwrap();
        node.shutdown().unwrap();
    }
}
