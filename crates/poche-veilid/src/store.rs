use std::future::Future;

/// Storage security selected by an identity-store adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageSecurity {
    Protected,
    ExplicitInsecureDevelopment,
}

/// Explicit acknowledgement required by every insecure development store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplicitInsecureDevelopment {
    AcknowledgeSecretsAreNotProtected,
}

/// Caller policy for opening application identity storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityStoragePolicy {
    RequireProtected,
    AllowExplicitInsecure(ExplicitInsecureDevelopment),
}

/// Stable fail-closed storage categories. No backend diagnostic carries values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityStoreError {
    MissingCredentials,
    CredentialsRejected,
    BackendUnavailable,
    CorruptIdentity,
    ConcurrentIdentityCreation,
    InsecureOptInRequired,
    DestructiveConfiguration,
    RandomUnavailable,
}

/// Secret blob crossing only the protected-store boundary.
///
/// This type intentionally has no `Debug`, `Display`, `Clone`, or serialization
/// implementation and zeroes its allocation on drop.
pub struct SecretIdentityBlob(Vec<u8>);

impl SecretIdentityBlob {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn with_bytes<R>(&self, operation: impl FnOnce(&[u8]) -> R) -> R {
        operation(&self.0)
    }
}

impl Drop for SecretIdentityBlob {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// Minimal application identity store; transport metadata belongs elsewhere.
pub trait IdentityStore {
    fn security(&self) -> StorageSecurity;

    fn load(&self) -> impl Future<Output = Result<Option<SecretIdentityBlob>, IdentityStoreError>>;

    fn save(
        &self,
        identity: &SecretIdentityBlob,
    ) -> impl Future<Output = Result<(), IdentityStoreError>>;
}

/// Memory-only development store. Merely enabling its feature is insufficient:
/// construction and use both require explicit acknowledgement.
#[cfg(any(test, feature = "insecure-development"))]
pub struct InsecureMemoryIdentityStore {
    identity: std::sync::Mutex<Option<Vec<u8>>>,
}

#[cfg(any(test, feature = "insecure-development"))]
impl InsecureMemoryIdentityStore {
    #[must_use]
    pub const fn new(_acknowledgement: ExplicitInsecureDevelopment) -> Self {
        Self {
            identity: std::sync::Mutex::new(None),
        }
    }
}

#[cfg(any(test, feature = "insecure-development"))]
impl IdentityStore for InsecureMemoryIdentityStore {
    fn security(&self) -> StorageSecurity {
        StorageSecurity::ExplicitInsecureDevelopment
    }

    async fn load(&self) -> Result<Option<SecretIdentityBlob>, IdentityStoreError> {
        Ok(self
            .identity
            .lock()
            .map_err(|_| IdentityStoreError::BackendUnavailable)?
            .as_ref()
            .map(|bytes| SecretIdentityBlob::new(bytes.clone())))
    }

    async fn save(&self, identity: &SecretIdentityBlob) -> Result<(), IdentityStoreError> {
        let mut stored = self
            .identity
            .lock()
            .map_err(|_| IdentityStoreError::BackendUnavailable)?;
        if stored.is_some() {
            return Err(IdentityStoreError::ConcurrentIdentityCreation);
        }
        *stored = Some(identity.with_bytes(<[u8]>::to_vec));
        Ok(())
    }
}
