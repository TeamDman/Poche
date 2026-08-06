<!--
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
-->

# Hidden-card research prototype

The `poche-crypto-prototype` crate implements the bounded executable profile
from [ADR 0008](decisions/0008-hidden-card-prototype.md). It pins experimental,
unaudited `ziffle` 0.1.0. Do not use this mode for money, high-stakes play, or a
production security claim.

## Typed boundary

The proof context length-frames the protocol domain, room, session, membership
epoch, round, ordered roster, and canonical 52-card schema hash. Key,
ownership-proof, encrypted-deck, shuffle-proof, reveal-token, and reveal-proof
artifacts have exact encoded sizes and are canonically decoded before proof
verification. Shuffle records are roster ordered and hash linked. Deal
assignments reject duplicate encrypted positions.

Private reveal shares are directed transport payloads. `PublicRoundProjection`
contains only opaque assignments and delivery-receipt digests; it contains no
share bytes and no plaintext card. Recipient encryption remains the transport
layer's responsibility. Once a card is played, `PublicRevealEvidence` carries
the complete roster-ordered shares so every replica can verify the identity.
The final audit requires all 52 positions and exactly card codes `0..52`.

The `PlayerSecret` debug representation is redacted and its underlying ziffle
secret uses zeroization-on-drop. Seeded `ChaCha20Rng` appears only in labelled
deterministic tests; a real contributor must supply a production CSPRNG.

## Evidence and deliberate limits

The test corpus covers:

- an honest three-player shuffle, private deal, public reveal, and complete
  52-card audit;
- a deterministic transcript/reveal digest;
- independently replaying and verifying serialized shuffle records;
- tampered shuffle proof, wrong-position reveal, duplicate assignment, and
  wrong-round rejection; and
- failure of the complete-reveal gate with `n-1` shares.

The last item tests the wrapper's fail-closed boundary; it is not a proof of the
underlying cryptography. The crate does not supply a same-hand dropout recovery
threshold. A withheld share is handled as a typed, governable cryptographic
abort in phase 6.3. The mandatory independent review in ADR 0008 remains open.

The explicit unanimous-share abort and vote-authorized redeal/end behavior is
documented in [Trustless-round abort and recovery](trustless-recovery.md).
