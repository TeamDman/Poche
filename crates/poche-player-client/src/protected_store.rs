// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Protected player-root and independently certified device persistence.

mod recovery;

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
use poche_capture::{
    CaptureTransferKey, WrappedCaptureTransferKey, device_encryption_public_key,
    open_wrapped_capture_transfer_key,
};
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
const TRANSPORT_KEY_SERVICE: &str = "transport-password-v1";
const RECOVERY_KEY_SERVICE: &str = "authority-recovery-v1";

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
    creation_lock_root: PathBuf,
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
        // Vault handles are global to this OS user, even when public metadata
        // roots differ. Therefore serialize creation across all public roots.
        let project = ProjectDirs::from("com", "TeamDman", "Poche")
            .ok_or(ProfileStoreError::PublicStoreUnavailable)?;
        Ok(Self {
            public_root,
            creation_lock_root: project.data_local_dir().join("credential-locks"),
            vault: Box::new(OsCredentialVault::open()?),
        })
    }

    #[must_use]
    pub fn public_root(&self) -> &Path {
        &self.public_root
    }

    /// Borrow the protected transport-storage password only at its startup
    /// boundary. Set allow_create only for a new transport store; recovery of
    /// an existing store must fail if the credential is missing. No plaintext
    /// fallback, user-selected password or recovery-code export is provided.
    pub fn with_transport_password<R>(
        &self,
        label: &str,
        allow_create: bool,
        operation: impl FnOnce(&str) -> R,
    ) -> Result<R, ProfileStoreError> {
        validate_label(label)?;
        let guard = self.lock_creation()?;
        let handle = key_handle(TRANSPORT_KEY_SERVICE, label);
        let password = match self.vault.load(&handle)? {
            Some(password) => {
                if decode_hex::<32>(&password).is_none() {
                    return Err(ProfileStoreError::CorruptProtectedSecret);
                }
                password
            }
            None if allow_create => {
                let seed = random_seed()?;
                let password = Zeroizing::new(hex(&*seed));
                self.vault.store(&handle, &password)?;
                password
            }
            None => return Err(ProfileStoreError::NotFound),
        };
        drop(guard);
        Ok(operation(&password))
    }

    /// Recover the selected local identity, creating only genuinely absent
    /// profiles. Labels are local selectors, never remote proof of identity.
    /// Missing/corrupt keys behind existing metadata fail without replacement.
    /// Call on the connection worker because the protected vault may block.
    pub fn load_or_create_device(
        &self,
        root_label: &str,
        device_label: &str,
    ) -> Result<DeviceProfile, ProfileStoreError> {
        validate_label(root_label)?;
        validate_label(device_label)?;
        let root = match self.load_player_root(root_label) {
            Ok(root) => root,
            Err(ProfileStoreError::NotFound) => match self.create_player_root(root_label) {
                Ok(root) => root,
                Err(ProfileStoreError::AlreadyExists) => self.load_player_root(root_label)?,
                Err(error) => return Err(error),
            },
            Err(error) => return Err(error),
        };
        let _root_key =
            self.load_signing_key(&root.signing_key_handle, &root.root.signing_public_key)?;
        let profile = match self.load_device(device_label) {
            Ok(profile) => profile,
            Err(ProfileStoreError::NotFound) => {
                match self.create_device(root_label, device_label) {
                    Ok(profile) => profile,
                    Err(ProfileStoreError::AlreadyExists) => self.load_device(device_label)?,
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        if profile.player_id != root.root.player_id {
            return Err(ProfileStoreError::CorruptPublicProfile);
        }
        let _device_key = self.load_signing_key(
            &profile.signing_key_handle,
            &profile.certificate.device_signing_public_key,
        )?;
        Ok(profile)
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
        let _creation = self.lock_creation()?;
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
        let _creation = self.lock_creation()?;
        let root = self.load_player_root(root_label)?;
        let root_key = self.load_signing_key(
            &root.signing_key_handle,
            root.root.signing_public_key.as_str(),
        )?;
        let handle = key_handle(DEVICE_KEY_SERVICE, device_label);
        let path = self.device_path(device_label);
        self.require_vacant(&path, &handle)?;

        let existing_devices = self.list_devices()?;
        let sequence = existing_devices
            .iter()
            .filter(|profile| profile.player_id == root.root.player_id)
            .map(|profile| profile.certificate.sequence)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ProfileStoreError::CorruptPublicProfile)?;
        let issue_vote_capability = !existing_devices.iter().any(|profile| {
            profile.player_id == root.root.player_id
                && profile
                    .certificate
                    .has_capability(DeviceCapabilityWire::Vote)
        });
        let seed = random_seed()?;
        let signing = SigningKey::from_bytes(&seed);
        let device_key = hex(&signing.verifying_key().to_bytes());
        let device_encryption_key = hex(&device_encryption_public_key(&seed));
        let device_id = DeviceId::new(device_key.clone())
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        let mut capabilities = vec![DeviceCapabilityWire::Propose];
        if issue_vote_capability {
            capabilities.push(DeviceCapabilityWire::Vote);
        }
        capabilities.extend([
            DeviceCapabilityWire::ReceivePrivateProjection,
            DeviceCapabilityWire::RequestDeviceChange,
            DeviceCapabilityWire::RequestCapture,
            DeviceCapabilityWire::ProvideCapture,
        ]);
        let unsigned = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("device-{device_label}"))
                .map_err(|_| ProfileStoreError::InvalidLabel)?,
            player_id: root.root.player_id.clone(),
            device_id: device_id.clone(),
            device_signing_public_key: device_key,
            device_encryption_public_key: device_encryption_key,
            sequence,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities,
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

    /// Open a short-lived artifact key only for the exact protected device
    /// profile named by a recipient-wrapped envelope. Raw profile seed bytes
    /// never cross this key-store boundary.
    pub fn open_capture_transfer_key(
        &self,
        profile: &DeviceProfile,
        wrapped: &WrappedCaptureTransferKey,
    ) -> Result<CaptureTransferKey, DeviceClientError> {
        profile.validate()?;
        let signing = self
            .load_signing_key(
                &profile.signing_key_handle,
                &profile.certificate.device_signing_public_key,
            )
            .map_err(|error| match error {
                ProfileStoreError::NotFound | ProfileStoreError::ProtectedStoreUnavailable => {
                    DeviceClientError::KeyUnavailable
                }
                _ => DeviceClientError::InvalidProfile,
            })?;
        open_wrapped_capture_transfer_key(wrapped, &profile.certificate, &signing.to_bytes())
            .map_err(|_| DeviceClientError::AuthorizationDenied)
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

    fn lock_creation(&self) -> Result<fs::File, ProfileStoreError> {
        fs::create_dir_all(&self.creation_lock_root)
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.creation_lock_root.join("create.lock"))
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        // OS-managed lock releases even after a process crashes. Keep the file
        // itself: unlinking lock files can let writers lock different inodes.
        file.lock()
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        Ok(file)
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
    if matches!(
        service,
        ROOT_KEY_SERVICE | DEVICE_KEY_SERVICE | TRANSPORT_KEY_SERVICE | RECOVERY_KEY_SERVICE
    ) && validate_label(label).is_ok()
    {
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
            creation_lock_root: directory.path().join("locks"),
            vault: Box::new(vault.clone()),
        };
        (directory, store, vault)
    }

    #[test]
    fn authority_recovery_is_encrypted_scoped_and_replaceable() {
        let (_directory, store, _vault) = fixture_store();
        assert!(store.load_authority_recovery("owner", "room-a").unwrap().is_none());
        let private = b"private hand: ace of spades";
        store.save_authority_recovery("owner", "room-a", private).unwrap();
        let path = fs::read_dir(store.public_root.join("authority-recovery")).unwrap().next().unwrap().unwrap().path();
        let first = fs::read(&path).unwrap();
        assert!(!first.windows(private.len()).any(|window| window == private));
        assert_eq!(store.load_authority_recovery("owner", "room-a").unwrap().unwrap().as_slice(), private);
        store.save_authority_recovery("owner", "room-a", private).unwrap();
        assert_ne!(first, fs::read(&path).unwrap(), "fresh nonces for repeated saves");
        store.save_authority_recovery("owner", "room-a", b"next revision").unwrap();
        assert_eq!(store.load_authority_recovery("owner", "room-a").unwrap().unwrap().as_slice(), b"next revision");
        // Re-labeling a valid envelope cannot authenticate under another room.
        let aad = b"poche.authority-recovery.v1\0owner\0room-b";
        let other = store.public_root.join("authority-recovery").join(format!("{}.sealed", blake3::hash(aad).to_hex()));
        fs::copy(&path, &other).unwrap();
        assert!(matches!(store.load_authority_recovery("owner", "room-b"), Err(ProfileStoreError::CorruptPublicProfile)));
        let mut corrupt = fs::read(&path).unwrap();
        *corrupt.last_mut().unwrap() ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert!(matches!(store.load_authority_recovery("owner", "room-a"), Err(ProfileStoreError::CorruptPublicProfile)));
    }

    #[test]
    fn authority_recovery_missing_key_never_replaces_existing_checkpoint() {
        let (_directory, store, vault) = fixture_store();
        store.save_authority_recovery("owner", "room", b"checkpoint").unwrap();
        vault.delete(&key_handle(RECOVERY_KEY_SERVICE, "owner")).unwrap();
        assert!(matches!(store.load_authority_recovery("owner", "room"), Err(ProfileStoreError::NotFound)));
        assert!(matches!(store.save_authority_recovery("owner", "room", b"replacement"), Err(ProfileStoreError::NotFound)));
        assert!(vault.0.lock().unwrap().is_empty());
        assert!(store.save_authority_recovery("../escape", "room", b"data").is_err());
    }

    #[test]
    fn transport_password_is_stable_and_missing_recovery_never_creates_one() {
        let (_directory, store, vault) = fixture_store();
        assert!(matches!(
            store.with_transport_password("node", false, |_| ()),
            Err(ProfileStoreError::NotFound)
        ));
        assert!(vault.0.lock().unwrap().is_empty());
        let fingerprint = |password: &str| blake3::hash(password.as_bytes());
        let first = store
            .with_transport_password("node", true, fingerprint)
            .unwrap();
        assert_eq!(
            first,
            store
                .with_transport_password("node", false, fingerprint)
                .unwrap()
        );
        assert_ne!(
            first,
            store
                .with_transport_password("other-node", true, fingerprint)
                .unwrap()
        );
        vault
            .delete(&key_handle(TRANSPORT_KEY_SERVICE, "node"))
            .unwrap();
        assert!(matches!(
            store.with_transport_password("node", false, |_| ()),
            Err(ProfileStoreError::NotFound)
        ));
    }

    #[test]
    fn automatic_recovery_keeps_identity_and_never_replaces_missing_keys() {
        let (_directory, store, vault) = fixture_store();
        let first = store
            .load_or_create_device("alice", "alice-desktop")
            .unwrap();
        assert_eq!(
            first,
            store
                .load_or_create_device("alice", "alice-desktop")
                .unwrap()
        );
        let second = store.load_or_create_device("bob", "bob-desktop").unwrap();
        assert_ne!(first.player_id, second.player_id);
        assert!(store.load_or_create_device("bob", "alice-desktop").is_err());
        vault.delete(&first.signing_key_handle).unwrap();
        assert!(
            store
                .load_or_create_device("alice", "alice-desktop")
                .is_err()
        );
        assert!(vault.load(&first.signing_key_handle).unwrap().is_none());
        assert_eq!(store.load_device("alice-desktop").unwrap(), first);
    }

    #[test]
    fn competing_creators_preserve_the_winning_secret() {
        let (_directory, store, vault) = fixture_store();
        let other = ProtectedProfileStore {
            public_root: store.public_root.join("other-public-root"),
            creation_lock_root: store.creation_lock_root.clone(),
            vault: Box::new(vault),
        };
        let ((left, store), (right, other)) = std::thread::scope(|scope| {
            let left = scope.spawn(move || (store.create_player_root("same-player"), store));
            let right = scope.spawn(move || (other.create_player_root("same-player"), other));
            (left.join().unwrap(), right.join().unwrap())
        });
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        let winner = if left.is_ok() { &store } else { &other };
        // This loads and verifies the protected root key, not only metadata.
        let device = winner.create_device("same-player", "survivor").unwrap();
        assert!(winner.sign_device_bytes(&device, b"still-owned").is_ok());
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

        let transfer_id = poche_protocol::CaptureTransferId::new("protected-store-wrap").unwrap();
        let request_hash = poche_protocol::SemanticHash([7; 32]);
        let (sender_key, wrapped) = poche_capture::generate_wrapped_capture_transfer_key(
            &device.certificate,
            transfer_id.clone(),
            request_hash,
        )
        .unwrap();
        let receiver_key = store.open_capture_transfer_key(&device, &wrapped).unwrap();
        let bytes = b"exact-recipient-artifact".to_vec();
        let descriptor =
            poche_capture::capture_transfer_descriptor(transfer_id, &bytes, 1024).unwrap();
        let mut sender = poche_capture::CaptureTransferSender::new(
            descriptor.clone(),
            request_hash,
            1_000,
            bytes.clone(),
            sender_key,
            1,
        )
        .unwrap();
        let mut receiver = poche_capture::CaptureTransferReceiver::new(
            descriptor,
            request_hash,
            1_000,
            receiver_key,
        )
        .unwrap();
        let chunk = sender.next_chunk(1).unwrap().unwrap();
        receiver.accept(1, &chunk).unwrap();
        assert_eq!(receiver.finish().unwrap(), bytes);
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

    #[test]
    fn sibling_profiles_retain_agency_without_minting_player_vote_weight() {
        let (_directory, store, _vault) = fixture_store();
        store.create_player_root("alice").unwrap();
        let first = store.create_device("alice", "alice-native").unwrap();
        let sibling = store.create_device("alice", "alice-browser").unwrap();

        assert!(first.certificate.has_capability(DeviceCapabilityWire::Vote));
        assert!(
            !sibling
                .certificate
                .has_capability(DeviceCapabilityWire::Vote)
        );
        for profile in [&first, &sibling] {
            assert!(
                profile
                    .certificate
                    .has_capability(DeviceCapabilityWire::Propose)
            );
            assert!(
                profile
                    .certificate
                    .has_capability(DeviceCapabilityWire::ReceivePrivateProjection)
            );
            assert!(
                profile
                    .certificate
                    .has_capability(DeviceCapabilityWire::RequestCapture)
            );
            assert!(
                profile
                    .certificate
                    .has_capability(DeviceCapabilityWire::ProvideCapture)
            );
        }
        assert_eq!(
            store
                .list_devices()
                .unwrap()
                .iter()
                .filter(|profile| profile
                    .certificate
                    .has_capability(DeviceCapabilityWire::Vote))
                .count(),
            1
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
