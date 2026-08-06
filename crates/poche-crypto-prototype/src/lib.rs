// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bounded, research-only wrapper for ADR 0008's hidden-card profile.
//!
//! This crate deliberately does not call itself secure or production-ready.
//! `ziffle` 0.1.0 is experimental and unaudited, and unanimous reveal shares
//! make availability weaker than privacy. Missing shares are handled by the
//! session-level abort/redeal policy implemented in phase 6.3.

use std::collections::BTreeSet;
use std::fmt;

use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use poche_domain::{CardId, FiniteDomain};
use rand::Rng;
use ziffle::{
    AggregatePublicKey, AggregateRevealToken, MaskedDeck, OwnershipProof, PublicKey, RevealToken,
    RevealTokenProof, SecretKey, Shuffle, ShuffleProof, Verified,
};

/// Canonical standard-deck size selected by ADR 0008.
pub const DECK_SIZE: usize = 52;
/// Poche supports at most 51 players because every player must receive a card.
pub const MAX_PLAYERS: usize = 51;
/// Bound for a canonical room identifier in the cryptographic context.
pub const MAX_ROOM_BYTES: usize = 128;
/// Bound for a canonical player identifier in the cryptographic context.
pub const MAX_PLAYER_ID_BYTES: usize = 64;

const DOMAIN: &[u8] = b"POCHE\0MENTAL-POKER\0V0";
const PUBLIC_KEY_BYTES: usize = 33;
const OWNERSHIP_PROOF_BYTES: usize = 65;
const MASKED_DECK_BYTES: usize = 3_432;
const SHUFFLE_PROOF_BYTES: usize = 5_547;
const REVEAL_TOKEN_BYTES: usize = 33;
const REVEAL_PROOF_BYTES: usize = 98;

/// Stable failures emitted before untrusted material becomes protocol state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// A bounded string was empty or too large.
    InvalidIdentifier(&'static str),
    /// A roster did not use the exact canonical order or supported size.
    InvalidRoster(&'static str),
    /// A player was absent from the frozen round roster.
    UnknownPlayer,
    /// A deck position was outside `0..52`.
    InvalidPosition,
    /// The record count or order did not match the frozen roster.
    UnexpectedContribution(&'static str),
    /// A fixed-size canonical artifact had the wrong byte length.
    InvalidArtifactLength {
        /// Artifact kind.
        artifact: &'static str,
        /// Required exact byte length.
        expected: usize,
        /// Received byte length.
        actual: usize,
    },
    /// Arkworks rejected a noncanonical or malformed artifact.
    InvalidEncoding(&'static str),
    /// A proof did not validate in this exact round context.
    InvalidProof(&'static str),
    /// A digest did not bind to the expected predecessor or transcript.
    DigestMismatch(&'static str),
    /// Two deal or reveal records named the same encrypted position.
    DuplicatePosition,
    /// The complete audit revealed a duplicate or omitted plaintext card.
    DeckConservation,
    /// A complete reveal set did not decrypt to a standard card.
    RevealFailed,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(kind) => write!(formatter, "invalid bounded {kind}"),
            Self::InvalidRoster(reason) => write!(formatter, "invalid roster: {reason}"),
            Self::UnknownPlayer => formatter.write_str("player is not in the round roster"),
            Self::InvalidPosition => formatter.write_str("deck position is outside 0..52"),
            Self::UnexpectedContribution(kind) => {
                write!(formatter, "unexpected {kind} contribution")
            }
            Self::InvalidArtifactLength {
                artifact,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid {artifact} length: expected {expected}, got {actual}"
            ),
            Self::InvalidEncoding(kind) => write!(formatter, "invalid canonical {kind} encoding"),
            Self::InvalidProof(kind) => write!(formatter, "invalid {kind} proof"),
            Self::DigestMismatch(kind) => write!(formatter, "{kind} digest mismatch"),
            Self::DuplicatePosition => formatter.write_str("duplicate encrypted deck position"),
            Self::DeckConservation => {
                formatter.write_str("reveals do not conserve the 52-card deck")
            }
            Self::RevealFailed => formatter.write_str("complete shares did not reveal a card"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Canonically bounded player identity used only by the cryptographic profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(String);

impl PlayerId {
    /// Validate a nonempty UTF-8 identity of at most 64 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::InvalidIdentifier`] for an empty or oversized ID.
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_PLAYER_ID_BYTES {
            return Err(ProtocolError::InvalidIdentifier("player id"));
        }
        Ok(Self(value))
    }

    /// Return the canonical identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An exact encrypted-deck position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(u8);

impl Position {
    /// Validate `0..52`.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::InvalidPosition`] outside the standard deck.
    pub fn new(value: usize) -> Result<Self, ProtocolError> {
        if value >= DECK_SIZE {
            return Err(ProtocolError::InvalidPosition);
        }
        Ok(Self(
            u8::try_from(value).map_err(|_| ProtocolError::InvalidPosition)?,
        ))
    }

    /// Return the zero-based position.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// Length-framed context bound into every proof in one cryptographic round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundContext {
    room: String,
    session: [u8; 32],
    membership_epoch: u64,
    round: u64,
    roster: Vec<PlayerId>,
    deck_schema_hash: [u8; 32],
    canonical: Vec<u8>,
    digest: [u8; 32],
}

impl RoundContext {
    /// Freeze a room/session/epoch/round and strictly sorted roster.
    ///
    /// # Errors
    ///
    /// Rejects unbounded room text, unsupported player counts, duplicate players,
    /// or a roster that is not strictly sorted by canonical player ID.
    pub fn new(
        room: impl Into<String>,
        session: [u8; 32],
        membership_epoch: u64,
        round: u64,
        roster: Vec<PlayerId>,
    ) -> Result<Self, ProtocolError> {
        let room = room.into();
        if room.is_empty() || room.len() > MAX_ROOM_BYTES {
            return Err(ProtocolError::InvalidIdentifier("room id"));
        }
        if !(2..=MAX_PLAYERS).contains(&roster.len()) {
            return Err(ProtocolError::InvalidRoster("expected 2..=51 players"));
        }
        if roster.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ProtocolError::InvalidRoster(
                "player ids must be unique and strictly increasing",
            ));
        }

        let deck_schema_hash =
            *blake3::hash(b"poche-standard-deck:suit-major-rank-minor:v1").as_bytes();
        let mut canonical = Vec::with_capacity(256 + roster.len() * 68);
        canonical.extend_from_slice(DOMAIN);
        push_bytes(&mut canonical, room.as_bytes());
        canonical.extend_from_slice(&session);
        canonical.extend_from_slice(&membership_epoch.to_be_bytes());
        canonical.extend_from_slice(&round.to_be_bytes());
        canonical.extend_from_slice(&deck_schema_hash);
        push_len(&mut canonical, roster.len());
        for player in &roster {
            push_bytes(&mut canonical, player.as_str().as_bytes());
        }
        let digest = *blake3::hash(&canonical).as_bytes();
        Ok(Self {
            room,
            session,
            membership_epoch,
            round,
            roster,
            deck_schema_hash,
            canonical,
            digest,
        })
    }

    /// Exact bytes supplied as the proof context.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Stable BLAKE3 digest of the length-framed context.
    #[must_use]
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Ordered player roster.
    #[must_use]
    pub fn roster(&self) -> &[PlayerId] {
        &self.roster
    }

    /// Human-readable room identity.
    #[must_use]
    pub fn room(&self) -> &str {
        &self.room
    }

    /// Session identity.
    #[must_use]
    pub const fn session(&self) -> [u8; 32] {
        self.session
    }

    /// Membership epoch.
    #[must_use]
    pub const fn membership_epoch(&self) -> u64 {
        self.membership_epoch
    }

    /// Cryptographic round number.
    #[must_use]
    pub const fn round(&self) -> u64 {
        self.round
    }

    /// Canonical standard-deck schema hash.
    #[must_use]
    pub const fn deck_schema_hash(&self) -> [u8; 32] {
        self.deck_schema_hash
    }
}

/// Secret key capability owned by one player device.
pub struct PlayerSecret {
    player: PlayerId,
    secret_key: SecretKey,
    public_key: PublicKey,
}

impl fmt::Debug for PlayerSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlayerSecret")
            .field("player", &self.player)
            .field("secret_key", &"<redacted and zeroized on drop>")
            .finish_non_exhaustive()
    }
}

impl PlayerSecret {
    /// Player that owns this capability.
    #[must_use]
    pub fn player(&self) -> &PlayerId {
        &self.player
    }
}

/// Public key and ownership proof attributed to one roster player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRecord {
    player: PlayerId,
    public_key: Vec<u8>,
    ownership_proof: Vec<u8>,
}

impl KeyRecord {
    /// Attributed player.
    #[must_use]
    pub fn player(&self) -> &PlayerId {
        &self.player
    }

    /// Stable length-framed record bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(180);
        push_bytes(&mut bytes, self.player.as_str().as_bytes());
        push_bytes(&mut bytes, &self.public_key);
        push_bytes(&mut bytes, &self.ownership_proof);
        bytes
    }

    /// Decode one exact canonical key record without trusting its proofs.
    ///
    /// # Errors
    ///
    /// Rejects truncated, trailing, non-UTF-8, unbounded, or wrong-sized fields.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = Cursor::new(bytes);
        let player = cursor.player_id("key record")?;
        let public_key = cursor.exact_vec(PUBLIC_KEY_BYTES, "public key")?;
        let ownership_proof = cursor.exact_vec(OWNERSHIP_PROOF_BYTES, "ownership proof")?;
        cursor.finish("key record")?;
        Ok(Self {
            player,
            public_key,
            ownership_proof,
        })
    }
}

/// Generate one context-bound key contribution.
///
/// # Errors
///
/// Rejects a player outside the frozen roster or an unexpectedly sized
/// canonical artifact produced by the selected dependency.
pub fn generate_key<R: Rng>(
    rng: &mut R,
    context: &RoundContext,
    player: &PlayerId,
) -> Result<(PlayerSecret, KeyRecord), ProtocolError> {
    if !context.roster.contains(player) {
        return Err(ProtocolError::UnknownPlayer);
    }
    let shuffle = Shuffle::<DECK_SIZE>::default();
    let (secret_key, public_key, proof) = shuffle.keygen(rng, context.canonical_bytes());
    let record = KeyRecord {
        player: player.clone(),
        public_key: encode_exact(&public_key, PUBLIC_KEY_BYTES, "public key")?,
        ownership_proof: encode_exact(&proof, OWNERSHIP_PROOF_BYTES, "ownership proof")?,
    };
    Ok((
        PlayerSecret {
            player: player.clone(),
            secret_key,
            public_key,
        },
        record,
    ))
}

/// Verified exact roster and aggregate encryption key.
pub struct VerifiedRoster {
    context: RoundContext,
    verified_keys: Vec<Verified<PublicKey>>,
    aggregate_key: AggregatePublicKey,
    transcript_digest: [u8; 32],
}

impl fmt::Debug for VerifiedRoster {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedRoster")
            .field("context_digest", &hex(self.context.digest()))
            .field("players", &self.context.roster)
            .field("transcript_digest", &hex(self.transcript_digest))
            .finish_non_exhaustive()
    }
}

impl VerifiedRoster {
    /// Frozen context.
    #[must_use]
    pub const fn context(&self) -> &RoundContext {
        &self.context
    }

    /// Digest binding ordered key records.
    #[must_use]
    pub const fn transcript_digest(&self) -> [u8; 32] {
        self.transcript_digest
    }
}

/// Verify exact record count/order, encodings, and ownership proofs.
///
/// # Errors
///
/// Rejects missing, extra, reordered, malformed, wrong-context, or invalid key
/// contributions.
pub fn verify_roster(
    context: &RoundContext,
    records: &[KeyRecord],
) -> Result<VerifiedRoster, ProtocolError> {
    if records.len() != context.roster.len() {
        return Err(ProtocolError::UnexpectedContribution("key"));
    }
    let mut verified_keys = Vec::with_capacity(records.len());
    let mut digest_input = Vec::new();
    digest_input.extend_from_slice(&context.digest());
    for (expected, record) in context.roster.iter().zip(records) {
        if expected != &record.player {
            return Err(ProtocolError::UnexpectedContribution("key"));
        }
        let public_key: PublicKey =
            decode_exact(&record.public_key, PUBLIC_KEY_BYTES, "public key")?;
        let proof: OwnershipProof = decode_exact(
            &record.ownership_proof,
            OWNERSHIP_PROOF_BYTES,
            "ownership proof",
        )?;
        let verified = proof
            .verify(public_key, context.canonical_bytes())
            .ok_or(ProtocolError::InvalidProof("ownership"))?;
        verified_keys.push(verified);
        push_bytes(&mut digest_input, &record.canonical_bytes());
    }
    let aggregate_key = AggregatePublicKey::new(&verified_keys);
    Ok(VerifiedRoster {
        context: context.clone(),
        verified_keys,
        aggregate_key,
        transcript_digest: *blake3::hash(&digest_input).as_bytes(),
    })
}

/// One attributed, serialized, hash-linked full-deck shuffle contribution.
#[derive(Clone, PartialEq, Eq)]
pub struct ShuffleRecord {
    ordinal: u8,
    player: PlayerId,
    parent_digest: [u8; 32],
    deck: Vec<u8>,
    proof: Vec<u8>,
    digest: [u8; 32],
}

impl fmt::Debug for ShuffleRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShuffleRecord")
            .field("ordinal", &self.ordinal)
            .field("player", &self.player)
            .field("parent_digest", &hex(self.parent_digest))
            .field("digest", &hex(self.digest))
            .field("deck_bytes", &self.deck.len())
            .field("proof_bytes", &self.proof.len())
            .finish()
    }
}

impl ShuffleRecord {
    /// Zero-based roster contribution order.
    #[must_use]
    pub const fn ordinal(&self) -> u8 {
        self.ordinal
    }

    /// Contributing roster player.
    #[must_use]
    pub fn player(&self) -> &PlayerId {
        &self.player
    }

    /// Digest of this contribution.
    #[must_use]
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Stable length-framed bytes for signatures and storage.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        shuffle_record_bytes(
            self.ordinal,
            &self.player,
            self.parent_digest,
            &self.deck,
            &self.proof,
        )
    }

    /// Decode one exact canonical shuffle record and derive its record digest.
    ///
    /// # Errors
    ///
    /// Rejects truncated, trailing, non-UTF-8, unbounded, or wrong-sized fields.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = Cursor::new(bytes);
        let ordinal = cursor.byte("shuffle record")?;
        let player = cursor.player_id("shuffle record")?;
        let parent_digest = cursor.array_32("shuffle parent digest")?;
        let deck = cursor.exact_vec(MASKED_DECK_BYTES, "masked deck")?;
        let proof = cursor.exact_vec(SHUFFLE_PROOF_BYTES, "shuffle proof")?;
        cursor.finish("shuffle record")?;
        Ok(Self {
            ordinal,
            player,
            parent_digest,
            deck,
            proof,
            digest: *blake3::hash(bytes).as_bytes(),
        })
    }
}

/// A deck that passed every accepted shuffle proof in roster order.
pub struct VerifiedDeck {
    deck: Verified<MaskedDeck<DECK_SIZE>>,
    transcript_digest: [u8; 32],
}

impl fmt::Debug for VerifiedDeck {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedDeck")
            .field("transcript_digest", &hex(self.transcript_digest))
            .finish_non_exhaustive()
    }
}

impl VerifiedDeck {
    /// Digest of the final verified shuffle contribution.
    #[must_use]
    pub const fn transcript_digest(&self) -> [u8; 32] {
        self.transcript_digest
    }
}

/// Run the honest sequential contribution path used by deterministic vectors.
///
/// Real peers call the same primitive independently with their own CSPRNG and
/// publish each record through consensus. A seeded RNG must only be used by
/// explicitly labelled tests.
///
/// # Errors
///
/// Returns an error if locally produced artifacts violate exact bounds or any
/// generated shuffle proof fails immediate verification.
pub fn create_shuffle_transcript<R: Rng>(
    rng: &mut R,
    roster: &VerifiedRoster,
) -> Result<(Vec<ShuffleRecord>, VerifiedDeck), ProtocolError> {
    let shuffle = Shuffle::<DECK_SIZE>::default();
    let parent0 = genesis_shuffle_digest(roster);
    let mut records = Vec::with_capacity(roster.context.roster.len());

    let (first_deck, first_proof) =
        shuffle.shuffle_initial_deck(rng, roster.aggregate_key, roster.context.canonical_bytes());
    let verified = shuffle
        .verify_initial_shuffle(
            roster.aggregate_key,
            first_deck,
            first_proof,
            roster.context.canonical_bytes(),
        )
        .ok_or(ProtocolError::InvalidProof("initial shuffle"))?;
    let first_record = make_shuffle_record(
        0,
        roster.context.roster[0].clone(),
        parent0,
        &first_deck,
        &first_proof,
    )?;
    let mut current = VerifiedDeck {
        deck: verified,
        transcript_digest: first_record.digest,
    };
    records.push(first_record);

    for ordinal in 1..roster.context.roster.len() {
        let (next_deck, proof) = shuffle.shuffle_deck(
            rng,
            roster.aggregate_key,
            &current.deck,
            roster.context.canonical_bytes(),
        );
        let verified = shuffle
            .verify_shuffle(
                roster.aggregate_key,
                &current.deck,
                next_deck,
                proof,
                roster.context.canonical_bytes(),
            )
            .ok_or(ProtocolError::InvalidProof("shuffle"))?;
        let record = make_shuffle_record(
            u8::try_from(ordinal)
                .map_err(|_| ProtocolError::InvalidRoster("too many shuffle contributors"))?,
            roster.context.roster[ordinal].clone(),
            current.transcript_digest,
            &next_deck,
            &proof,
        )?;
        current = VerifiedDeck {
            deck: verified,
            transcript_digest: record.digest,
        };
        records.push(record);
    }
    Ok((records, current))
}

/// Rebuild and verify a serialized shuffle transcript without trusting its producer.
///
/// # Errors
///
/// Rejects wrong count/order/parent hashes, malformed canonical artifacts,
/// record digest changes, and invalid shuffle proofs.
pub fn verify_shuffle_transcript(
    roster: &VerifiedRoster,
    records: &[ShuffleRecord],
) -> Result<VerifiedDeck, ProtocolError> {
    if records.len() != roster.context.roster.len() {
        return Err(ProtocolError::UnexpectedContribution("shuffle"));
    }
    let shuffle = Shuffle::<DECK_SIZE>::default();
    let mut expected_parent = genesis_shuffle_digest(roster);
    let mut current: Option<Verified<MaskedDeck<DECK_SIZE>>> = None;
    for (ordinal, record) in records.iter().enumerate() {
        if usize::from(record.ordinal) != ordinal || record.player != roster.context.roster[ordinal]
        {
            return Err(ProtocolError::UnexpectedContribution("shuffle"));
        }
        if record.parent_digest != expected_parent {
            return Err(ProtocolError::DigestMismatch("shuffle parent"));
        }
        let actual_digest = *blake3::hash(&record.canonical_bytes()).as_bytes();
        if actual_digest != record.digest {
            return Err(ProtocolError::DigestMismatch("shuffle record"));
        }
        let deck: MaskedDeck<DECK_SIZE> =
            decode_exact(&record.deck, MASKED_DECK_BYTES, "masked deck")?;
        let proof: ShuffleProof<DECK_SIZE> =
            decode_exact(&record.proof, SHUFFLE_PROOF_BYTES, "shuffle proof")?;
        let verified = match &current {
            None => shuffle.verify_initial_shuffle(
                roster.aggregate_key,
                deck,
                proof,
                roster.context.canonical_bytes(),
            ),
            Some(previous) => shuffle.verify_shuffle(
                roster.aggregate_key,
                previous,
                deck,
                proof,
                roster.context.canonical_bytes(),
            ),
        }
        .ok_or(ProtocolError::InvalidProof("shuffle"))?;
        expected_parent = record.digest;
        current = Some(verified);
    }
    Ok(VerifiedDeck {
        deck: current.ok_or(ProtocolError::UnexpectedContribution("shuffle"))?,
        transcript_digest: expected_parent,
    })
}

/// Unique assignment of one encrypted position to a holder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    /// Intended holder.
    pub holder: PlayerId,
    /// Opaque encrypted-deck position.
    pub position: Position,
}

/// Validate holder membership and globally unique encrypted positions.
///
/// # Errors
///
/// Rejects unknown holders, more than 52 assignments, or duplicate positions.
pub fn validate_assignments(
    roster: &VerifiedRoster,
    assignments: &[Assignment],
) -> Result<(), ProtocolError> {
    if assignments.len() > DECK_SIZE {
        return Err(ProtocolError::DuplicatePosition);
    }
    let mut positions = BTreeSet::new();
    for assignment in assignments {
        if !roster.context.roster.contains(&assignment.holder) {
            return Err(ProtocolError::UnknownPlayer);
        }
        if !positions.insert(assignment.position) {
            return Err(ProtocolError::DuplicatePosition);
        }
    }
    Ok(())
}

/// Card-specific decryption share and DLEQ proof.
#[derive(Clone, PartialEq, Eq)]
pub struct RevealShareRecord {
    player: PlayerId,
    position: Position,
    token: Vec<u8>,
    proof: Vec<u8>,
}

impl fmt::Debug for RevealShareRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RevealShareRecord")
            .field("player", &self.player)
            .field("position", &self.position)
            .field("material", &"<private until public reveal>")
            .field(
                "digest",
                &hex(*blake3::hash(&self.canonical_bytes()).as_bytes()),
            )
            .finish_non_exhaustive()
    }
}

impl RevealShareRecord {
    /// Player who produced the share.
    #[must_use]
    pub fn player(&self) -> &PlayerId {
        &self.player
    }

    /// Encrypted position bound by the proof.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.position
    }

    /// Stable length-framed bytes. Treat as confidential until public reveal.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(220);
        push_bytes(&mut bytes, self.player.as_str().as_bytes());
        bytes.push(self.position.0);
        push_bytes(&mut bytes, &self.token);
        push_bytes(&mut bytes, &self.proof);
        bytes
    }

    /// Decode one exact private/public reveal-share record.
    ///
    /// # Errors
    ///
    /// Rejects truncated, trailing, non-UTF-8, unbounded, wrong-position, or
    /// wrong-sized fields. Cryptographic verification remains a separate step.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = Cursor::new(bytes);
        let player = cursor.player_id("reveal share")?;
        let position = Position::new(usize::from(cursor.byte("reveal share")?))?;
        let token = cursor.exact_vec(REVEAL_TOKEN_BYTES, "reveal token")?;
        let proof = cursor.exact_vec(REVEAL_PROOF_BYTES, "reveal proof")?;
        cursor.finish("reveal share")?;
        Ok(Self {
            player,
            position,
            token,
            proof,
        })
    }
}

/// Create one card-specific reveal share with the owning player's secret key.
///
/// # Errors
///
/// Rejects an unknown player/position or an unexpectedly sized canonical
/// artifact produced by the selected dependency.
pub fn create_reveal_share<R: Rng>(
    rng: &mut R,
    roster: &VerifiedRoster,
    deck: &VerifiedDeck,
    secret: &PlayerSecret,
    position: Position,
) -> Result<RevealShareRecord, ProtocolError> {
    if !roster.context.roster.contains(&secret.player) {
        return Err(ProtocolError::UnknownPlayer);
    }
    let card = deck
        .deck
        .get(position.get())
        .ok_or(ProtocolError::InvalidPosition)?;
    let (token, proof) = card.reveal_token(
        rng,
        &secret.secret_key,
        secret.public_key,
        roster.context.canonical_bytes(),
    );
    Ok(RevealShareRecord {
        player: secret.player.clone(),
        position,
        token: encode_exact(&token, REVEAL_TOKEN_BYTES, "reveal token")?,
        proof: encode_exact(&proof, REVEAL_PROOF_BYTES, "reveal proof")?,
    })
}

struct VerifiedRevealSet {
    tokens: Vec<Verified<RevealToken>>,
    digest: [u8; 32],
}

fn verify_reveal_set(
    roster: &VerifiedRoster,
    deck: &VerifiedDeck,
    position: Position,
    shares: &[RevealShareRecord],
) -> Result<VerifiedRevealSet, ProtocolError> {
    if shares.len() != roster.context.roster.len() {
        return Err(ProtocolError::UnexpectedContribution("reveal share"));
    }
    let card = deck
        .deck
        .get(position.get())
        .ok_or(ProtocolError::InvalidPosition)?;
    let mut tokens = Vec::with_capacity(shares.len());
    let mut digest_input = Vec::new();
    digest_input.extend_from_slice(&roster.context.digest());
    digest_input.extend_from_slice(&deck.transcript_digest);
    digest_input.push(position.0);
    for (index, share) in shares.iter().enumerate() {
        if share.player != roster.context.roster[index] || share.position != position {
            return Err(ProtocolError::UnexpectedContribution("reveal share"));
        }
        let token: RevealToken = decode_exact(&share.token, REVEAL_TOKEN_BYTES, "reveal token")?;
        let proof: RevealTokenProof =
            decode_exact(&share.proof, REVEAL_PROOF_BYTES, "reveal proof")?;
        let verified = proof
            .verify(
                roster.verified_keys[index],
                token,
                card,
                roster.context.canonical_bytes(),
            )
            .ok_or(ProtocolError::InvalidProof("reveal"))?;
        tokens.push(verified);
        push_bytes(&mut digest_input, &share.canonical_bytes());
    }
    Ok(VerifiedRevealSet {
        tokens,
        digest: *blake3::hash(&digest_input).as_bytes(),
    })
}

/// Holder-only result. Debug output intentionally redacts the plaintext card.
#[derive(Clone, PartialEq, Eq)]
pub struct HolderPrivateCard {
    holder: PlayerId,
    position: Position,
    card: CardId,
    reveal_set_digest: [u8; 32],
}

impl fmt::Debug for HolderPrivateCard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HolderPrivateCard")
            .field("holder", &self.holder)
            .field("position", &self.position)
            .field("card", &"<holder-only>")
            .field("reveal_set_digest", &hex(self.reveal_set_digest))
            .finish()
    }
}

impl HolderPrivateCard {
    /// Read the plaintext identity inside the holder's private projection.
    #[must_use]
    pub const fn card(&self) -> CardId {
        self.card
    }

    /// Intended holder.
    #[must_use]
    pub const fn holder(&self) -> &PlayerId {
        &self.holder
    }

    /// Opaque position that was dealt.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.position
    }
}

/// Replicable receipt for a private deal. It contains neither shares nor card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicDealReceipt {
    /// Intended holder.
    pub holder: PlayerId,
    /// Assigned opaque position.
    pub position: Position,
    /// Digest of the complete privately delivered reveal set.
    pub reveal_set_digest: [u8; 32],
}

/// Verify complete privately delivered shares and split holder/public views.
///
/// # Errors
///
/// Rejects an invalid assignment, incomplete/reordered/malformed share set,
/// invalid DLEQ proof, or a set that does not reveal a standard card.
pub fn deal_private(
    roster: &VerifiedRoster,
    deck: &VerifiedDeck,
    assignment: &Assignment,
    shares: &[RevealShareRecord],
) -> Result<(HolderPrivateCard, PublicDealReceipt), ProtocolError> {
    validate_assignments(roster, std::slice::from_ref(assignment))?;
    let set = verify_reveal_set(roster, deck, assignment.position, shares)?;
    let card = reveal_from_set(deck, assignment.position, &set)?;
    Ok((
        HolderPrivateCard {
            holder: assignment.holder.clone(),
            position: assignment.position,
            card,
            reveal_set_digest: set.digest,
        },
        PublicDealReceipt {
            holder: assignment.holder.clone(),
            position: assignment.position,
            reveal_set_digest: set.digest,
        },
    ))
}

/// Public reveal material, suitable for every replica to audit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicRevealEvidence {
    /// Revealed encrypted position.
    pub position: Position,
    /// Claimed canonical Poche card.
    pub card: CardId,
    /// Complete roster-ordered token/proof set.
    pub shares: Vec<RevealShareRecord>,
    /// Digest binding position, identity, and shares.
    pub evidence_digest: [u8; 32],
}

impl PublicRevealEvidence {
    /// Stable bytes containing the now-public complete reveal evidence.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(self.position.0);
        bytes.push(self.card.code());
        push_len(&mut bytes, self.shares.len());
        for share in &self.shares {
            push_bytes(&mut bytes, &share.canonical_bytes());
        }
        bytes.extend_from_slice(&self.evidence_digest);
        bytes
    }

    /// Decode one exact public reveal record without trusting its digest/proofs.
    ///
    /// # Errors
    ///
    /// Rejects truncated, trailing, out-of-range, oversized, or malformed
    /// fields. [`verify_public_reveal`] must still authenticate its meaning.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = Cursor::new(bytes);
        let position = Position::new(usize::from(cursor.byte("public reveal")?))?;
        let card = CardId::decode(u128::from(cursor.byte("public reveal")?))
            .map_err(|_| ProtocolError::InvalidEncoding("public reveal card"))?;
        let count = cursor.length("public reveal")?;
        if count == 0 || count > MAX_PLAYERS {
            return Err(ProtocolError::InvalidEncoding("public reveal share count"));
        }
        let mut shares = Vec::with_capacity(count);
        for _ in 0..count {
            let encoded = cursor.bounded_bytes(256, "reveal share")?;
            shares.push(RevealShareRecord::from_canonical_bytes(encoded)?);
        }
        let evidence_digest = cursor.array_32("public reveal digest")?;
        cursor.finish("public reveal")?;
        Ok(Self {
            position,
            card,
            shares,
            evidence_digest,
        })
    }
}

/// Publish a previously private complete set and bind its revealed card.
///
/// # Errors
///
/// Rejects incomplete, reordered, malformed, wrong-position, or invalid shares,
/// and a complete set that does not reveal a standard card.
pub fn create_public_reveal(
    roster: &VerifiedRoster,
    deck: &VerifiedDeck,
    position: Position,
    shares: Vec<RevealShareRecord>,
) -> Result<PublicRevealEvidence, ProtocolError> {
    let set = verify_reveal_set(roster, deck, position, &shares)?;
    let card = reveal_from_set(deck, position, &set)?;
    let evidence_digest = public_reveal_digest(position, card, &shares);
    Ok(PublicRevealEvidence {
        position,
        card,
        shares,
        evidence_digest,
    })
}

/// Verify a public reveal from serialized card-specific evidence.
///
/// # Errors
///
/// Rejects an altered evidence digest, invalid shares, or a plaintext claim that
/// differs from cryptographic revelation.
pub fn verify_public_reveal(
    roster: &VerifiedRoster,
    deck: &VerifiedDeck,
    evidence: &PublicRevealEvidence,
) -> Result<CardId, ProtocolError> {
    if public_reveal_digest(evidence.position, evidence.card, &evidence.shares)
        != evidence.evidence_digest
    {
        return Err(ProtocolError::DigestMismatch("public reveal"));
    }
    let set = verify_reveal_set(roster, deck, evidence.position, &evidence.shares)?;
    let card = reveal_from_set(deck, evidence.position, &set)?;
    if card != evidence.card {
        return Err(ProtocolError::RevealFailed);
    }
    Ok(card)
}

/// Verify one reveal for every encrypted position and exact plaintext conservation.
///
/// # Errors
///
/// Rejects missing/duplicate positions, any invalid public reveal, or a
/// plaintext result other than the exact canonical 52-card set.
pub fn audit_complete_round(
    roster: &VerifiedRoster,
    deck: &VerifiedDeck,
    evidence: &[PublicRevealEvidence],
) -> Result<[CardId; DECK_SIZE], ProtocolError> {
    if evidence.len() != DECK_SIZE {
        return Err(ProtocolError::DeckConservation);
    }
    let mut by_position: Vec<Option<CardId>> = vec![None; DECK_SIZE];
    let mut cards = BTreeSet::new();
    for reveal in evidence {
        let position = reveal.position.get();
        if by_position[position].is_some() {
            return Err(ProtocolError::DuplicatePosition);
        }
        let card = verify_public_reveal(roster, deck, reveal)?;
        if !cards.insert(card.code()) {
            return Err(ProtocolError::DeckConservation);
        }
        by_position[position] = Some(card);
    }
    if cards.len() != DECK_SIZE || cards.iter().copied().ne(0_u8..52) {
        return Err(ProtocolError::DeckConservation);
    }
    let cards = by_position
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ProtocolError::DeckConservation)?;
    cards
        .try_into()
        .map_err(|_| ProtocolError::DeckConservation)
}

/// Public projection for an in-progress private deal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicRoundProjection {
    /// Frozen context digest.
    pub context_digest: [u8; 32],
    /// Verified shuffle transcript digest.
    pub deck_digest: [u8; 32],
    /// Unique opaque assignments.
    pub assignments: Vec<Assignment>,
    /// Private delivery receipts without token material or card identities.
    pub private_deals: Vec<PublicDealReceipt>,
}

impl PublicRoundProjection {
    /// Construct after revalidating assignment uniqueness and receipt matching.
    ///
    /// # Errors
    ///
    /// Rejects invalid assignments or a receipt without an exact assignment.
    pub fn new(
        roster: &VerifiedRoster,
        deck: &VerifiedDeck,
        assignments: Vec<Assignment>,
        private_deals: Vec<PublicDealReceipt>,
    ) -> Result<Self, ProtocolError> {
        validate_assignments(roster, &assignments)?;
        for receipt in &private_deals {
            if !assignments.iter().any(|assignment| {
                assignment.holder == receipt.holder && assignment.position == receipt.position
            }) {
                return Err(ProtocolError::UnexpectedContribution(
                    "private deal receipt",
                ));
            }
        }
        Ok(Self {
            context_digest: roster.context.digest(),
            deck_digest: deck.transcript_digest,
            assignments,
            private_deals,
        })
    }
}

fn reveal_from_set(
    deck: &VerifiedDeck,
    position: Position,
    set: &VerifiedRevealSet,
) -> Result<CardId, ProtocolError> {
    let card = deck
        .deck
        .get(position.get())
        .ok_or(ProtocolError::InvalidPosition)?;
    let aggregate = AggregateRevealToken::new(&set.tokens);
    let code = Shuffle::<DECK_SIZE>::default()
        .reveal_card(aggregate, card)
        .ok_or(ProtocolError::RevealFailed)?;
    CardId::decode(code as u128).map_err(|_| ProtocolError::RevealFailed)
}

fn public_reveal_digest(
    position: Position,
    card: CardId,
    shares: &[RevealShareRecord],
) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"poche-public-reveal-v0");
    bytes.push(position.0);
    bytes.push(card.code());
    push_len(&mut bytes, shares.len());
    for share in shares {
        push_bytes(&mut bytes, &share.canonical_bytes());
    }
    *blake3::hash(&bytes).as_bytes()
}

fn genesis_shuffle_digest(roster: &VerifiedRoster) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"poche-shuffle-genesis-v0");
    bytes.extend_from_slice(&roster.context.digest());
    bytes.extend_from_slice(&roster.transcript_digest);
    *blake3::hash(&bytes).as_bytes()
}

fn make_shuffle_record(
    ordinal: u8,
    player: PlayerId,
    parent_digest: [u8; 32],
    deck: &MaskedDeck<DECK_SIZE>,
    proof: &ShuffleProof<DECK_SIZE>,
) -> Result<ShuffleRecord, ProtocolError> {
    let deck = encode_exact(deck, MASKED_DECK_BYTES, "masked deck")?;
    let proof = encode_exact(proof, SHUFFLE_PROOF_BYTES, "shuffle proof")?;
    let canonical = shuffle_record_bytes(ordinal, &player, parent_digest, &deck, &proof);
    let digest = *blake3::hash(&canonical).as_bytes();
    Ok(ShuffleRecord {
        ordinal,
        player,
        parent_digest,
        deck,
        proof,
        digest,
    })
}

fn shuffle_record_bytes(
    ordinal: u8,
    player: &PlayerId,
    parent_digest: [u8; 32],
    deck: &[u8],
    proof: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9_200);
    bytes.push(ordinal);
    push_bytes(&mut bytes, player.as_str().as_bytes());
    bytes.extend_from_slice(&parent_digest);
    push_bytes(&mut bytes, deck);
    push_bytes(&mut bytes, proof);
    bytes
}

fn encode_exact<T: CanonicalSerialize>(
    value: &T,
    expected: usize,
    artifact: &'static str,
) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = Vec::with_capacity(expected);
    value
        .serialize_compressed(&mut bytes)
        .map_err(|_| ProtocolError::InvalidEncoding(artifact))?;
    if bytes.len() != expected {
        return Err(ProtocolError::InvalidArtifactLength {
            artifact,
            expected,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

fn decode_exact<T: CanonicalDeserialize>(
    bytes: &[u8],
    expected: usize,
    artifact: &'static str,
) -> Result<T, ProtocolError> {
    if bytes.len() != expected {
        return Err(ProtocolError::InvalidArtifactLength {
            artifact,
            expected,
            actual: bytes.len(),
        });
    }
    let mut input = bytes;
    let value = T::deserialize_compressed(&mut input)
        .map_err(|_| ProtocolError::InvalidEncoding(artifact))?;
    if !input.is_empty() {
        return Err(ProtocolError::InvalidEncoding(artifact));
    }
    Ok(value)
}

fn push_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    push_len(output, bytes.len());
    output.extend_from_slice(bytes);
}

fn push_len(output: &mut Vec<u8>, length: usize) {
    output.extend_from_slice(
        &u32::try_from(length)
            .expect("all protocol collections are bounded below u32::MAX")
            .to_be_bytes(),
    );
}

struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize, artifact: &'static str) -> Result<&'a [u8], ProtocolError> {
        if self.remaining.len() < count {
            return Err(ProtocolError::InvalidEncoding(artifact));
        }
        let (head, tail) = self.remaining.split_at(count);
        self.remaining = tail;
        Ok(head)
    }

    fn byte(&mut self, artifact: &'static str) -> Result<u8, ProtocolError> {
        self.take(1, artifact).map(|bytes| bytes[0])
    }

    fn length(&mut self, artifact: &'static str) -> Result<usize, ProtocolError> {
        let bytes: [u8; 4] = self
            .take(4, artifact)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidEncoding(artifact))?;
        Ok(u32::from_be_bytes(bytes) as usize)
    }

    fn bounded_bytes(
        &mut self,
        maximum: usize,
        artifact: &'static str,
    ) -> Result<&'a [u8], ProtocolError> {
        let length = self.length(artifact)?;
        if length > maximum {
            return Err(ProtocolError::InvalidEncoding(artifact));
        }
        self.take(length, artifact)
    }

    fn exact_vec(
        &mut self,
        expected: usize,
        artifact: &'static str,
    ) -> Result<Vec<u8>, ProtocolError> {
        let length = self.length(artifact)?;
        if length != expected {
            return Err(ProtocolError::InvalidArtifactLength {
                artifact,
                expected,
                actual: length,
            });
        }
        Ok(self.take(length, artifact)?.to_vec())
    }

    fn player_id(&mut self, artifact: &'static str) -> Result<PlayerId, ProtocolError> {
        let bytes = self.bounded_bytes(MAX_PLAYER_ID_BYTES, artifact)?;
        let value =
            std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidEncoding(artifact))?;
        PlayerId::new(value)
    }

    fn array_32(&mut self, artifact: &'static str) -> Result<[u8; 32], ProtocolError> {
        self.take(32, artifact)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidEncoding(artifact))
    }

    fn finish(self, artifact: &'static str) -> Result<(), ProtocolError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidEncoding(artifact))
        }
    }
}

fn hex(bytes: [u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests;
