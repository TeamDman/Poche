// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Protected player-root and independently certified device persistence.

use core::fmt;
use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use facet::Facet;
use keyring_manager::{KeyringError, KeyringManager};
use poche_protocol::{
    CertificateId, DeviceCapabilityWire, DeviceCustodyWire, DeviceId, PlayerRootWire, PrincipalId,
    REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, SignatureAlgorithm,
    SignatureBytes, SignatureIntent, UnsignedDeviceCertificateWire,
    canonical_device_certificate_bytes,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tempfile::Builder as TempFileBuilder;
use zeroize::Zeroizing;

use crate::{DeviceClientError, DeviceProfile, DeviceSigner};

const PROFILE_SCHEMA_VERSION_V1: u16 = 1;
const MAX_PUBLIC_PROFILE_BYTES: u64 = 1024 * 1024;
const KEYRING_APPLICATION: &str = "TeamDman.Poche";
const ROOT_KEY_SERVICE: &str = "player-root-v1";
const DEVICE_KEY_SERVICE: &str = "device-key-v1";

/// Public selector and stable root identity. The key handle is an opaque
/// locator into protected storage, never secret key material.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerRootProfile {
    pub schema_version: u16,
    pub label: String,
    pub root: PlayerRootWire,
    pub signing_key_handle: String,
}

impl PlayerRootProfile {
    /// Validate the local selector, wire root, and protected handle shape.
    ///
    /// # Errors
    ///
    /// Returns a stable profile category without disclosing path or key data.
    pub fn validate(&self) -> Result<(), ProfileStoreError> {
        validate_label(&self.label)?;
        if self.schema_version != PROFILE_SCHEMA_VERSION_V1
            || self.root.validate().is_err()
            || parse_handle(&self.signing_key_handle) != Some((ROOT_KEY_SERVICE, &self.label))
        {
            Err(ProfileStoreError::CorruptPublicProfile)
        } else {
            Ok(())
        }
    }
}

/// Stable, redacted profile persistence failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileStoreError {
    InvalidLabel,
    AlreadyExists,
    NotFound,
    ProtectedStoreUnavailable,
    CorruptProtectedSecret,
    CorruptPublicProfile,
    PublicStoreUnavailable,
    RandomUnavailable,
}

impl fmt::Display for ProfileStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLabel => "profile label is invalid",
            Self::AlreadyExists => "profile already exists",
            Self::NotFound => "profile was not found",
            Self::ProtectedStoreUnavailable => "protected credential store is unavailable",
            Self::CorruptProtectedSecret => {
                "protected credential does not match its public profile"
            }
            Self::CorruptPublicProfile => "public profile is corrupt or unauthenticated",
            Self::PublicStoreUnavailable => "public profile store is unavailable",
            Self::RandomUnavailable => "operating-system randomness is unavailable",
        })
    }
}

impl std::error::Error for ProfileStoreError {}

trait SecretVault: Send {
    fn load(&self, handle: &str) -> Result<Option<Zeroizing<String>>, ProfileStoreError>;
    fn store(&self, handle: &str, value: &str) -> Result<(), ProfileStoreError>;
    fn delete(&self, handle: &str) -> Result<(), ProfileStoreError>;
}

struct OsCredentialVault {
    manager: KeyringManager,
}

impl OsCredentialVault {
    fn open() -> Result<Self, ProfileStoreError> {
        Ok(Self {
            manager: KeyringManager::new_secure(KEYRING_APPLICATION)
                .map_err(|_| ProfileStoreError::ProtectedStoreUnavailable)?,
        })
    }
}

impl SecretVault for OsCredentialVault {
    fn load(&self, handle: &str) -> Result<Option<Zeroizing<String>>, ProfileStoreError> {
        let (service, key) = parse_handle(handle).ok_or(ProfileStoreError::CorruptPublicProfile)?;
        match self
            .manager
            .with_keyring(service, key, |credential| credential.get_value())
        {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(KeyringError::NoPasswordFound) => Ok(None),
            Err(_) => Err(ProfileStoreError::ProtectedStoreUnavailable),
        }
    }

    fn store(&self, handle: &str, value: &str) -> Result<(), ProfileStoreError> {
        let (service, key) = parse_handle(handle).ok_or(ProfileStoreError::CorruptPublicProfile)?;
        self.manager
            .with_keyring(service, key, |credential| credential.set_value(value))
            .map_err(|_| ProfileStoreError::ProtectedStoreUnavailable)
    }

    fn delete(&self, handle: &str) -> Result<(), ProfileStoreError> {
        let (service, key) = parse_handle(handle).ok_or(ProfileStoreError::CorruptPublicProfile)?;
        match self
            .manager
            .with_keyring(service, key, |credential| credential.delete_value())
        {
            Ok(()) | Err(KeyringError::NoPasswordFound) => Ok(()),
            Err(_) => Err(ProfileStoreError::ProtectedStoreUnavailable),
        }
    }
}

/// Public profile registry paired with an operating-system credential vault.
///
/// This type intentionally has no `Debug`, serialization, or secret-export
/// API. It implements [`DeviceSigner`] by resolving only a profile's opaque
/// protected handle for the duration of one signature.
pub struct ProtectedProfileStore {
    public_root: PathBuf,
    vault: Box<dyn SecretVault>,
}

impl ProtectedProfileStore {
    /// Open the platform's normal application-data and protected-credential
    /// locations.
    ///
    /// # Errors
    ///
    /// Fails when no protected OS credential backend or application data
    /// location is available. It never falls back to plaintext key files.
    pub fn open_default() -> Result<Self, ProfileStoreError> {
        if let Some(public_root) = std::env::var_os("POCHE_PROFILE_ROOT") {
            return Self::open_at(PathBuf::from(public_root));
        }
        let project = ProjectDirs::from("com", "TeamDman", "Poche")
            .ok_or(ProfileStoreError::PublicStoreUnavailable)?;
        Self::open_at(project.data_local_dir().join("profiles"))
    }

    /// Open protected credentials with an explicit public metadata root.
    /// This is useful for portable public-profile placement; secrets still
    /// remain in the OS vault.
    ///
    /// # Errors
    ///
    /// Fails closed if the protected credential backend cannot be opened.
    pub fn open_at(public_root: PathBuf) -> Result<Self, ProfileStoreError> {
        Ok(Self {
            public_root,
            vault: Box::new(OsCredentialVault::open()?),
        })
    }

    #[must_use]
    pub fn public_root(&self) -> &Path {
        &self.public_root
    }

    /// Generate a new player root in protected storage and persist only its
    /// public projection.
    ///
    /// # Errors
    ///
    /// Rejects invalid/colliding labels and rolls the protected secret back if
    /// public metadata cannot be committed.
    pub fn create_player_root(&self, label: &str) -> Result<PlayerRootProfile, ProfileStoreError> {
        validate_label(label)?;
        let handle = key_handle(ROOT_KEY_SERVICE, label);
        let path = self.root_path(label);
        self.require_vacant(&path, &handle)?;

        let seed = random_seed()?;
        let signing = SigningKey::from_bytes(&seed);
        let public_key = hex(&signing.verifying_key().to_bytes());
        let root = PlayerRootWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            player_id: PrincipalId::new(public_key.clone())
                .map_err(|_| ProfileStoreError::CorruptPublicProfile)?,
            signing_public_key: public_key,
        };
        let profile = PlayerRootProfile {
            schema_version: PROFILE_SCHEMA_VERSION_V1,
            label: label.to_owned(),
            root,
            signing_key_handle: handle.clone(),
        };
        profile.validate()?;
        let encoded_secret = Zeroizing::new(hex(&*seed));
        self.vault.store(&handle, &encoded_secret)?;
        if let Err(error) = persist_public(&path, &profile) {
            let _ = self.vault.delete(&handle);
            return Err(error);
        }
        Ok(profile)
    }

    /// Root-certify a fresh independent native device key.
    ///
    /// # Errors
    ///
    /// Rejects missing/corrupt roots, invalid/colliding labels, or unavailable
    /// protected/public storage without exposing either secret.
    pub fn create_device(
        &self,
        root_label: &str,
        device_label: &str,
    ) -> Result<DeviceProfile, ProfileStoreError> {
        validate_label(device_label)?;
        let root = self.load_player_root(root_label)?;
        let root_key = self.load_signing_key(
            &root.signing_key_handle,
            root.root.signing_public_key.as_str(),
        )?;
        let handle = key_handle(DEVICE_KEY_SERVICE, device_label);
        let path = self.device_path(device_label);
        self.require_vacant(&path, &handle)?;

        let sequence = self
            .list_devices()?
            .into_iter()
            .filter(|profile| profile.player_id == root.root.player_id)
            .map(|profile| profile.certificate.sequence)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ProfileStoreError::CorruptPublicProfile)?;
        let seed = random_seed()?;
        let signing = SigningKey::from_bytes(&seed);
        let device_key = hex(&signing.verifying_key().to_bytes());
        let device_id = DeviceId::new(device_key.clone())
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        let unsigned = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("device-{device_label}"))
                .map_err(|_| ProfileStoreError::InvalidLabel)?,
            player_id: root.root.player_id.clone(),
            device_id: device_id.clone(),
            device_signing_public_key: device_key,
            sequence,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![
                DeviceCapabilityWire::Propose,
                DeviceCapabilityWire::Vote,
                DeviceCapabilityWire::ReceivePrivateProjection,
                DeviceCapabilityWire::RequestDeviceChange,
                DeviceCapabilityWire::RequestCapture,
                DeviceCapabilityWire::ProvideCapture,
            ],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: root.root.player_id,
            },
        };
        let certificate_bytes = canonical_device_certificate_bytes(&unsigned)
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        let signature = SignatureBytes::new(hex(&root_key.sign(&certificate_bytes).to_bytes()))
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        let certificate = unsigned
            .attach_signature(signature)
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        let profile = DeviceProfile {
            schema_version: DeviceProfile::SCHEMA_VERSION_V1,
            label: device_label.to_owned(),
            player_id: certificate.player_id.clone(),
            device_id,
            certificate,
            signing_key_handle: handle.clone(),
        };
        profile
            .validate()
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        verify_certificate_signature(&profile)?;

        let encoded_secret = Zeroizing::new(hex(&*seed));
        self.vault.store(&handle, &encoded_secret)?;
        if let Err(error) = persist_public(&path, &profile) {
            let _ = self.vault.delete(&handle);
            return Err(error);
        }
        Ok(profile)
    }

    /// Load and authenticate one public player-root profile.
    pub fn load_player_root(&self, label: &str) -> Result<PlayerRootProfile, ProfileStoreError> {
        validate_label(label)?;
        let profile: PlayerRootProfile = read_public(&self.root_path(label))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Load and authenticate one public root-certified device profile.
    pub fn load_device(&self, label: &str) -> Result<DeviceProfile, ProfileStoreError> {
        validate_label(label)?;
        let profile: DeviceProfile = read_public(&self.device_path(label))?;
        if profile.label != label
            || parse_handle(&profile.signing_key_handle) != Some((DEVICE_KEY_SERVICE, label))
            || profile.validate().is_err()
        {
            return Err(ProfileStoreError::CorruptPublicProfile);
        }
        verify_certificate_signature(&profile)?;
        Ok(profile)
    }

    /// Return all authenticated public player roots in label order.
    pub fn list_player_roots(&self) -> Result<Vec<PlayerRootProfile>, ProfileStoreError> {
        list_public(&self.public_root.join("identities"), |label| {
            self.load_player_root(label)
        })
    }

    /// Return all authenticated public device profiles in label order.
    pub fn list_devices(&self) -> Result<Vec<DeviceProfile>, ProfileStoreError> {
        list_public(&self.public_root.join("devices"), |label| {
            self.load_device(label)
        })
    }

    fn root_path(&self, label: &str) -> PathBuf {
        self.public_root
            .join("identities")
            .join(format!("{label}.json"))
    }

    fn device_path(&self, label: &str) -> PathBuf {
        self.public_root
            .join("devices")
            .join(format!("{label}.json"))
    }

    fn require_vacant(&self, path: &Path, handle: &str) -> Result<(), ProfileStoreError> {
        if path.exists() || self.vault.load(handle)?.is_some() {
            Err(ProfileStoreError::AlreadyExists)
        } else {
            Ok(())
        }
    }

    fn load_signing_key(
        &self,
        handle: &str,
        expected_public: &str,
    ) -> Result<SigningKey, ProfileStoreError> {
        let encoded = self
            .vault
            .load(handle)?
            .ok_or(ProfileStoreError::NotFound)?;
        let secret = Zeroizing::new(
            decode_hex::<32>(&encoded).ok_or(ProfileStoreError::CorruptProtectedSecret)?,
        );
        let signing = SigningKey::from_bytes(&secret);
        if hex(&signing.verifying_key().to_bytes()) == expected_public {
            Ok(signing)
        } else {
            Err(ProfileStoreError::CorruptProtectedSecret)
        }
    }
}

impl DeviceSigner for ProtectedProfileStore {
    fn sign_device_bytes(
        &self,
        profile: &DeviceProfile,
        canonical_bytes: &[u8],
    ) -> Result<SignatureBytes, DeviceClientError> {
        let signing = self
            .load_signing_key(&profile.signing_key_handle, profile.device_id.as_str())
            .map_err(|error| match error {
                ProfileStoreError::NotFound | ProfileStoreError::ProtectedStoreUnavailable => {
                    DeviceClientError::KeyUnavailable
                }
                _ => DeviceClientError::InvalidProfile,
            })?;
        SignatureBytes::new(hex(&signing.sign(canonical_bytes).to_bytes()))
            .map_err(|_| DeviceClientError::SigningFailed)
    }
}

fn verify_certificate_signature(profile: &DeviceProfile) -> Result<(), ProfileStoreError> {
    let public = decode_hex::<32>(profile.player_id.as_str())
        .ok_or(ProfileStoreError::CorruptPublicProfile)?;
    let signature = decode_hex::<64>(profile.certificate.signature.signature.as_str())
        .ok_or(ProfileStoreError::CorruptPublicProfile)?;
    let verifying =
        VerifyingKey::from_bytes(&public).map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
    let bytes = canonical_device_certificate_bytes(&profile.certificate.unsigned())
        .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
    verifying
        .verify_strict(&bytes, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| ProfileStoreError::CorruptPublicProfile)
}

fn validate_label(label: &str) -> Result<(), ProfileStoreError> {
    if label.is_empty()
        || label.len() > 64
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err(ProfileStoreError::InvalidLabel)
    } else {
        Ok(())
    }
}

fn key_handle(service: &str, label: &str) -> String {
    format!("keyring:{service}:{label}")
}

fn parse_handle(handle: &str) -> Option<(&str, &str)> {
    let value = handle.strip_prefix("keyring:")?;
    let (service, label) = value.split_once(':')?;
    if matches!(service, ROOT_KEY_SERVICE | DEVICE_KEY_SERVICE) && validate_label(label).is_ok() {
        Some((service, label))
    } else {
        None
    }
}

fn random_seed() -> Result<Zeroizing<[u8; 32]>, ProfileStoreError> {
    let mut seed = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *seed).map_err(|_| ProfileStoreError::RandomUnavailable)?;
    Ok(seed)
}

fn persist_public<T: Serialize>(path: &Path, value: &T) -> Result<(), ProfileStoreError> {
    let parent = path
        .parent()
        .ok_or(ProfileStoreError::PublicStoreUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
    let mut temporary = TempFileBuilder::new()
        .prefix(".poche-public-")
        .tempfile_in(parent)
        .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
    temporary.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            ProfileStoreError::AlreadyExists
        } else {
            ProfileStoreError::PublicStoreUnavailable
        }
    })?;
    Ok(())
}

fn read_public<T: DeserializeOwned>(path: &Path) -> Result<T, ProfileStoreError> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ProfileStoreError::NotFound
        } else {
            ProfileStoreError::PublicStoreUnavailable
        }
    })?;
    if !metadata.is_file() || metadata.len() > MAX_PUBLIC_PROFILE_BYTES {
        return Err(ProfileStoreError::CorruptPublicProfile);
    }
    let bytes = fs::read(path).map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
    serde_json::from_slice(&bytes).map_err(|_| ProfileStoreError::CorruptPublicProfile)
}

fn list_public<T>(
    directory: &Path,
    mut load: impl FnMut(&str) -> Result<T, ProfileStoreError>,
) -> Result<Vec<T>, ProfileStoreError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(ProfileStoreError::PublicStoreUnavailable),
    };
    let mut labels = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        if entry
            .file_type()
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?
            .is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("json")
        {
            let label = entry
                .path()
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or(ProfileStoreError::CorruptPublicProfile)?
                .to_owned();
            validate_label(&label)?;
            labels.push(label);
        }
    }
    labels.sort();
    labels.into_iter().map(|label| load(&label)).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len().saturating_mul(2)),
        |mut output, byte| {
            use std::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
            output
        },
    )
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N.checked_mul(2)? {
        return None;
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(bytes)
}

const fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use ed25519_dalek::Verifier as _;

    use super::*;

    #[derive(Clone, Default)]
    struct MemoryVault(Arc<Mutex<BTreeMap<String, String>>>);

    impl SecretVault for MemoryVault {
        fn load(&self, handle: &str) -> Result<Option<Zeroizing<String>>, ProfileStoreError> {
            Ok(self
                .0
                .lock()
                .map_err(|_| ProfileStoreError::ProtectedStoreUnavailable)?
                .get(handle)
                .cloned()
                .map(Zeroizing::new))
        }

        fn store(&self, handle: &str, value: &str) -> Result<(), ProfileStoreError> {
            self.0
                .lock()
                .map_err(|_| ProfileStoreError::ProtectedStoreUnavailable)?
                .insert(handle.to_owned(), value.to_owned());
            Ok(())
        }

        fn delete(&self, handle: &str) -> Result<(), ProfileStoreError> {
            self.0
                .lock()
                .map_err(|_| ProfileStoreError::ProtectedStoreUnavailable)?
                .remove(handle);
            Ok(())
        }
    }

    fn fixture_store() -> (tempfile::TempDir, ProtectedProfileStore, MemoryVault) {
        let directory = tempfile::tempdir().unwrap();
        let vault = MemoryVault::default();
        let store = ProtectedProfileStore {
            public_root: directory.path().to_path_buf(),
            vault: Box::new(vault.clone()),
        };
        (directory, store, vault)
    }

    #[test]
    fn roots_and_devices_persist_only_authenticated_public_material() {
        let (directory, store, vault) = fixture_store();
        let root = store.create_player_root("alice").unwrap();
        let device = store.create_device("alice", "alice-cli").unwrap();
        assert_eq!(store.list_player_roots().unwrap(), vec![root.clone()]);
        assert_eq!(store.list_devices().unwrap(), vec![device.clone()]);
        assert_eq!(store.load_device("alice-cli").unwrap(), device);
        assert_eq!(device.certificate.capabilities.len(), 6);

        let public_files = format!(
            "{}{}",
            fs::read_to_string(directory.path().join("identities/alice.json")).unwrap(),
            fs::read_to_string(directory.path().join("devices/alice-cli.json")).unwrap()
        );
        for secret in vault.0.lock().unwrap().values() {
            assert!(!public_files.contains(secret));
        }

        let payload = b"device-signature-does-not-export-the-key";
        let signature = store.sign_device_bytes(&device, payload).unwrap();
        let verifying =
            VerifyingKey::from_bytes(&decode_hex(device.device_id.as_str()).unwrap()).unwrap();
        verifying
            .verify(
                payload,
                &ed25519_dalek::Signature::from_bytes(&decode_hex(signature.as_str()).unwrap()),
            )
            .unwrap();
    }

    #[test]
    fn collisions_corruption_and_secret_mismatch_fail_without_replacement() {
        let (_directory, store, vault) = fixture_store();
        let root = store.create_player_root("alice").unwrap();
        assert_eq!(
            store.create_player_root("alice"),
            Err(ProfileStoreError::AlreadyExists)
        );
        let original = vault
            .0
            .lock()
            .unwrap()
            .get(&root.signing_key_handle)
            .cloned()
            .unwrap();
        assert_eq!(
            store.create_device("missing", "device"),
            Err(ProfileStoreError::NotFound)
        );
        assert_eq!(
            vault.0.lock().unwrap().get(&root.signing_key_handle),
            Some(&original)
        );

        let device = store.create_device("alice", "alice-cli").unwrap();
        vault
            .0
            .lock()
            .unwrap()
            .insert(device.signing_key_handle.clone(), "00".repeat(32));
        assert_eq!(
            store.sign_device_bytes(&device, b"payload"),
            Err(DeviceClientError::InvalidProfile)
        );
        assert_eq!(
            validate_label("../escape"),
            Err(ProfileStoreError::InvalidLabel)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_credential_vault_round_trip_deletes_its_probe() {
        let vault = OsCredentialVault::open().unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let handle = key_handle(DEVICE_KEY_SERVICE, &format!("test-{nonce}"));
        let value = "5a".repeat(32);
        assert!(vault.load(&handle).unwrap().is_none());
        vault.store(&handle, &value).unwrap();
        let loaded = vault.load(&handle).unwrap();
        assert_eq!(
            loaded.as_ref().map(|secret| secret.as_str()),
            Some(value.as_str())
        );
        vault.delete(&handle).unwrap();
        assert!(vault.load(&handle).unwrap().is_none());
    }
}
