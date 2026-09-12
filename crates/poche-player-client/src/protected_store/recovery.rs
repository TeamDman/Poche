// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Authority-private recovery storage. This is not a player transcript.
use super::*;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use std::{io::Read as _, time::Instant};

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
        let total_started = Instant::now();
        let location_started = Instant::now();
        let (path, aad) = self.recovery_location(label, room)?;
        let location_us = elapsed_micros(location_started);
        if plaintext.len() > MAX_BYTES {
            return Err(ProfileStoreError::CorruptPublicProfile);
        }
        let lock_started = Instant::now();
        let _guard = self.lock_creation()?;
        let lock_us = elapsed_micros(lock_started);
        // Missing keys behind an existing checkpoint must never be replaced.
        let key_started = Instant::now();
        let exists = path
            .try_exists()
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let key = self.recovery_key(label, !exists)?;
        let key_us = elapsed_micros(key_started);
        let encrypt_started = Instant::now();
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
        let encrypt_us = elapsed_micros(encrypt_started);
        let temporary_started = Instant::now();
        let parent = path
            .parent()
            .ok_or(ProfileStoreError::PublicStoreUnavailable)?;
        fs::create_dir_all(parent).map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let mut temporary = TempFileBuilder::new()
            .prefix(".poche-recovery-")
            .tempfile_in(parent)
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let temporary_us = elapsed_micros(temporary_started);
        let write_started = Instant::now();
        temporary
            .write_all(HEADER)
            .and_then(|()| temporary.write_all(&nonce))
            .and_then(|()| temporary.write_all(&ciphertext))
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let write_us = elapsed_micros(write_started);
        let sync_started = Instant::now();
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let sync_us = elapsed_micros(sync_started);
        let persist_started = Instant::now();
        temporary
            .persist(&path)
            .map_err(|_| ProfileStoreError::PublicStoreUnavailable)?;
        let persist_us = elapsed_micros(persist_started);
        tracing::trace!(
            target: "poche_latency",
            event = "authority_checkpoint_saved",
            plaintext_bytes = plaintext.len(),
            ciphertext_bytes = ciphertext.len(),
            location_us,
            lock_us,
            key_us,
            encrypt_us,
            temporary_us,
            write_us,
            sync_us,
            persist_us,
            total_us = elapsed_micros(total_started),
        );
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

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
