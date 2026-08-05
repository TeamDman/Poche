/// Application secret-key material.
///
/// This type intentionally implements neither `Debug`, `Display`, Facet,
/// serialization, nor cloning. Network and protocol types carry public key IDs
/// and signatures only.
///
/// ```compile_fail
/// use poche_protocol::SecretKeyMaterial;
/// let key = SecretKeyMaterial::new([0; 32]);
/// println!("{key:?}");
/// ```
///
/// ```compile_fail
/// use poche_protocol::SecretKeyMaterial;
/// let key = SecretKeyMaterial::new([0; 32]);
/// println!("{key}");
/// ```
///
/// ```compile_fail
/// use poche_protocol::SecretKeyMaterial;
/// let key = SecretKeyMaterial::new([0; 32]);
/// let copy = key.clone();
/// ```
pub struct SecretKeyMaterial([u8; 32]);

impl SecretKeyMaterial {
    /// Take ownership of secret bytes at the storage/signing boundary.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow bytes only for a caller-supplied signing/storage operation.
    pub fn with_bytes<R>(&self, operation: impl FnOnce(&[u8; 32]) -> R) -> R {
        operation(&self.0)
    }
}
