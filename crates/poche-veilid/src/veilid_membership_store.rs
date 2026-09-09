use crate::{MembershipError, MembershipStore, SecretMembershipBlob, StorageSecurity};
use poche_protocol::RoomId;

const MEMBERSHIP_KEY_PREFIX: &str = "poche.membership.v1.";

/// Production durable-membership adapter over Veilid protected storage.
pub struct VeilidProtectedMembershipStore {
    api: veilid_core::VeilidAPI,
}

impl VeilidProtectedMembershipStore {
    #[must_use]
    pub const fn new(api: veilid_core::VeilidAPI) -> Self {
        Self { api }
    }
}

impl MembershipStore for VeilidProtectedMembershipStore {
    fn security(&self) -> StorageSecurity {
        StorageSecurity::Protected
    }

    async fn load(
        &self,
        room_id: &RoomId,
    ) -> Result<Option<SecretMembershipBlob>, MembershipError> {
        self.api
            .load_user_secret(membership_key(room_id))
            .await
            .map(|value| value.map(SecretMembershipBlob::new))
            .map_err(|_| MembershipError::BackendUnavailable)
    }

    async fn save(
        &self,
        room_id: &RoomId,
        membership: &SecretMembershipBlob,
    ) -> Result<(), MembershipError> {
        self.api
            .save_user_secret(
                membership_key(room_id),
                membership.with_bytes(<[u8]>::to_vec),
            )
            .await
            .map(|_| ())
            .map_err(|_| MembershipError::BackendUnavailable)
    }

    async fn remove(&self, room_id: &RoomId) -> Result<bool, MembershipError> {
        self.api
            .remove_user_secret(membership_key(room_id))
            .await
            .map_err(|_| MembershipError::BackendUnavailable)
    }
}

fn membership_key(room_id: &RoomId) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-membership-store-key-v1\0");
    hasher.update(room_id.as_str().as_bytes());
    format!("{MEMBERSHIP_KEY_PREFIX}{}", hasher.finalize().to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_store_key_is_stable_bounded_and_not_the_room_name() {
        let room = RoomId::new("private-human-room-name").unwrap();
        let key = membership_key(&room);
        assert!(key.starts_with(MEMBERSHIP_KEY_PREFIX));
        assert!(!key.contains(room.as_str()));
        assert_eq!(key.len(), MEMBERSHIP_KEY_PREFIX.len() + 64);
    }
}
