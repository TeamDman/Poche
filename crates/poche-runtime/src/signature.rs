// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared strict Ed25519 verification for runtime protocol adapters.

use ed25519_dalek::{Signature, VerifyingKey};

pub(crate) fn verify_ed25519_hex(public_key: &str, signature: &str, bytes: &[u8]) -> bool {
    let Some(public_key) = decode_hex::<32>(public_key) else {
        return false;
    };
    let Some(signature) = decode_hex::<64>(signature) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key) else {
        return false;
    };
    verifying_key
        .verify_strict(bytes, &Signature::from_bytes(&signature))
        .is_ok()
}

fn decode_hex<const BYTES: usize>(encoded: &str) -> Option<[u8; BYTES]> {
    if encoded.len() != BYTES * 2 {
        return None;
    }
    let mut decoded = [0_u8; BYTES];
    for (index, byte) in decoded.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(encoded.get(offset..offset + 2)?, 16).ok()?;
    }
    Some(decoded)
}
