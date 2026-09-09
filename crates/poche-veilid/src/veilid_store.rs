use crate::{IdentityStore, IdentityStoreError, SecretIdentityBlob, StorageSecurity};

const IDENTITY_KEY: &str = "poche.application-identity.v1";

/// Production adapter over Veilid's platform protected-store abstraction.
pub struct VeilidProtectedIdentityStore {
    api: veilid_core::VeilidAPI,
}

impl VeilidProtectedIdentityStore {
    #[must_use]
    pub const fn new(api: veilid_core::VeilidAPI) -> Self {
        Self { api }
    }
}

impl IdentityStore for VeilidProtectedIdentityStore {
    fn security(&self) -> StorageSecurity {
        StorageSecurity::Protected
    }

    async fn load(&self) -> Result<Option<SecretIdentityBlob>, IdentityStoreError> {
        self.api
            .load_user_secret(IDENTITY_KEY.to_owned())
            .await
            .map(|value| value.map(SecretIdentityBlob::new))
            .map_err(|_| IdentityStoreError::BackendUnavailable)
    }

    async fn save(&self, identity: &SecretIdentityBlob) -> Result<(), IdentityStoreError> {
        if self
            .api
            .load_user_secret(IDENTITY_KEY.to_owned())
            .await
            .map_err(|_| IdentityStoreError::BackendUnavailable)?
            .is_some()
        {
            return Err(IdentityStoreError::ConcurrentIdentityCreation);
        }
        let replaced = self
            .api
            .save_user_secret(IDENTITY_KEY.to_owned(), identity.with_bytes(<[u8]>::to_vec))
            .await
            .map_err(|_| IdentityStoreError::BackendUnavailable)?;
        if replaced {
            Err(IdentityStoreError::ConcurrentIdentityCreation)
        } else {
            Ok(())
        }
    }
}

/// Enforce Poche's native protected-store policy before Veilid startup.
///
/// A nonempty password is required for the device-encryption key, insecure
/// fallback is rejected, and automatic deletion is forbidden. A wrong password
/// subsequently causes Veilid initialization to fail rather than replacing the
/// application identity.
///
/// # Errors
///
/// Returns a stable safe category without copying any credential value.
pub fn validate_protected_store_config(
    config: &veilid_core::VeilidConfigProtectedStore,
) -> Result<(), IdentityStoreError> {
    if config.allow_insecure_fallback || config.always_use_insecure_storage {
        return Err(IdentityStoreError::InsecureOptInRequired);
    }
    if config.delete {
        return Err(IdentityStoreError::DestructiveConfiguration);
    }
    if config.device_encryption_key_password.is_empty() {
        return Err(IdentityStoreError::MissingCredentials);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_config_rejects_missing_credentials_and_insecure_fallback() {
        let mut config = veilid_core::VeilidConfigProtectedStore::default();
        assert_eq!(
            validate_protected_store_config(&config),
            Err(IdentityStoreError::MissingCredentials)
        );
        config.device_encryption_key_password = "present-but-never-logged".to_owned();
        assert_eq!(validate_protected_store_config(&config), Ok(()));
        config.allow_insecure_fallback = true;
        assert_eq!(
            validate_protected_store_config(&config),
            Err(IdentityStoreError::InsecureOptInRequired)
        );
    }
}
