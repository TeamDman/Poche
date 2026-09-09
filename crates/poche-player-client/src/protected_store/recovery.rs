// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Authority-private recovery storage. This is not a player transcript.
use super::*;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use std::io::Read as _;

const HEADER: &[u8] = b"POCHE-AUTHORITY-RECOVERY-1\0";
const MAX_BYTES: usize = 32 * 1024 * 1024;

impl ProtectedProfileStore {
    /// Atomically replace an encrypted authority checkpoint. Callers must
    /// serialize room operations and save before acknowledging a mutation.
    /// The key is independent of player signing and transport credentials.
    /// This does not prevent rollback to an older authenticated checkpoint.
    pub fn save_authority_recovery(
        &self,
        label: &str,
        room: &str,
        plaintext: &[u8],
    ) -> Result<(), ProfileStoreError> {
        let (path, aad) = self.recovery_location(label, room)?;
        if plaintext.len() > MAX_BYTES {
            return Err(ProfileStoreError::CorruptPublicProfile);
        }
        let _guard = self.lock_creation()?;
        // Missing keys behind an existing checkpoint must never be replaced.
        let exists = path
            .try_exists()
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let key = self.recovery_key(label, !exists)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| ProfileStoreError::CorruptProtectedSecret)?;
        let mut nonce = [0_u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| ProfileStoreError::RandomUnavailable)?;
        let ciphertext = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| ProfileStoreError::CorruptProtectedSecret)?;
        let parent = path
            .parent()
            .ok_or(ProfileStoreError::PublicStoreUnavailable)?;
        fs::create_dir_all(parent).map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let mut temporary = TempFileBuilder::new()
            .prefix(".poche-recovery-")
            .tempfile_in(parent)
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        temporary
            .write_all(HEADER)
            .and_then(|()| temporary.write_all(&nonce))
            .and_then(|()| temporary.write_all(&ciphertext))
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        temporary
            .persist(&path)
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        Ok(())
    }

    /// Authenticate and decrypt private recovery data; absence is distinct
    /// from corruption. No credential is created during a read. The returned
    /// plaintext is zeroized on drop and must never be logged or shared.
    pub fn load_authority_recovery(
        &self,
        label: &str,
        room: &str,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, ProfileStoreError> {
        let (path, aad) = self.recovery_location(label, room)?;
        let _guard = self.lock_creation()?;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ProfileStoreError::PublicStoreUnavailable),
        };
        let limit = MAX_BYTES + HEADER.len() + 24 + 16;
        let mut bytes = Vec::new();
        file.take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        if bytes.len() > limit || bytes.len() < HEADER.len() + 24 + 16 || !bytes.starts_with(HEADER)
        {
            return Err(ProfileStoreError::CorruptPublicProfile);
        }
        let key = self.recovery_key(label, false)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| ProfileStoreError::CorruptProtectedSecret)?;
        let nonce: [u8; 24] = bytes[HEADER.len()..HEADER.len() + 24]
            .try_into()
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)?;
        cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &bytes[HEADER.len() + 24..],
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map(Some)
            .map_err(|_| ProfileStoreError::CorruptPublicProfile)
    }

    fn recovery_location(
        &self,
        label: &str,
        room: &str,
    ) -> Result<(PathBuf, Vec<u8>), ProfileStoreError> {
        validate_label(label)?;
        validate_label(room)?;
        // Hash both selectors: '.' and '..' can never become path components.
        let aad = format!("poche.authority-recovery.v1\0{label}\0{room}").into_bytes();
        let path = self
            .public_root
            .join("authority-recovery")
            .join(format!("{}.sealed", blake3::hash(&aad).to_hex()));
        Ok((path, aad))
    }

    fn recovery_key(
        &self,
        label: &str,
        allow_create: bool,
    ) -> Result<Zeroizing<[u8; 32]>, ProfileStoreError> {
        let handle = key_handle(RECOVERY_KEY_SERVICE, label);
        match self.vault.load(&handle)? {
            Some(value) => decode_hex::<32>(&value)
                .map(Zeroizing::new)
                .ok_or(ProfileStoreError::CorruptProtectedSecret),
            None if allow_create => {
                let key = random_seed()?;
                let encoded = Zeroizing::new(hex(key.as_ref()));
                self.vault.store(&handle, &encoded)?;
                Ok(key)
            }
            None => Err(ProfileStoreError::NotFound),
        }
    }
}
