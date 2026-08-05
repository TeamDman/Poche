# Veilid recipient-private projections

Task 5.5 uses the released Veilid 0.5.7 VLD0 HPKE base-mode API rather than
inventing a Poche cipher. The stable application identity already contains a
separate X25519 recipient key. Veilid's `hpke_seal` and `hpke_open` consume that
key directly; Veilid node IDs and replaceable private routes are not encryption
identities.

## Packet boundary

`EncryptedProjectionPacket` exposes only bounded routing/application metadata:

- room and session epoch;
- stable host and exact recipient principal IDs;
- projection epoch and authority revision;
- the `vld0_hpke_base` algorithm identifier;
- an opaque base64url HPKE blob; and
- a stable-host signature over all metadata and ciphertext.

The HPKE associated data repeats every visible semantic field. After opening,
the client decodes exactly one canonical `ProjectionEnvelope` and requires all
duplicated fields to match. The expected local projection epoch is also an
input to opening, so a pre-revoke packet cannot overwrite a post-revoke view.
Previously observed plaintext cannot be erased and is not claimed to be.

HPKE base mode authenticates the ciphertext to the recipient but does not by
itself authenticate the sender. The outer Ed25519 signature therefore binds
the complete packet to the room's stable host identity before decryption.
Forgery, metadata edits, ciphertext edits, wrong recipient keys, stale epochs,
noncanonical JSON, malformed plaintext, and packets above the 30,000-byte safe
ceiling all fail closed with redacted categories.

`TransportCommandReply` permits only authority events and public error frames
in its plaintext `frames` field. It has one exact-recipient
`encrypted_projection` field, and rejects a plaintext `Projection` frame. This
makes the object graph—not a renderer or convention—the network privacy
boundary.

## Grant, future delivery, and revoke

The session reducer already advances `projection_epoch` on a grant, revoke, or
expiry that removes a relevant capability. Seat-role and membership changes
emit capability-expiry events for affected player/recipient pairs. Projection
construction looks up the current hand only while an exact current grant is
active; it does not retain cards inside the capability.

The feature-enabled acceptance test starts a released Veilid API instance and
captures serialized packets and transport replies. It proves:

- an ungranted spectator opens a projection with no own or granted hand;
- captured JSON contains neither projection field names nor card plaintext;
- the granted spectator receives only the selected player's current hand and
  a later update at the same grant epoch;
- another seated player's hand is absent;
- another spectator and the other player cannot open the packet;
- a deliberately re-signed wrong-recipient/wrong-key packet fails HPKE open;
- revoke advances the expected epoch and all later output omits the hand;
- replaying the old packet at the new epoch fails as stale; and
- host-signature or metadata tampering fails before plaintext is accepted.

Run the evidence with:

```powershell
cargo test -p poche-veilid --features veilid --offline
cargo clippy -p poche-veilid --all-targets --features veilid --offline -- -D warnings
```

This is end-to-end application cryptography over the real released VLD0
implementation. Task 5.6 separately retains proof that the same bytes cross
separate native Veilid nodes under the selected public/topology setup.
