# Poche session protocol v1

The normative inspectable transport is one canonical JSON envelope followed by
one LF byte. The implementation lives in `poche-protocol`; this document records
the interoperability constraints rather than duplicating its Rust types.

The checked protocol transcript is replayed both directly from typed envelopes
and through this canonical NDJSON decoder. Full semantic transcripts—including
errors, events, snapshots, projections, and hashes—must match exactly. Phon is
not currently an enabled protocol codec.

## Identity

- Protocol version: `1`
- Signature-domain version: `1`
- Schema descriptor: `poche.protocol.v1`
- Schema BLAKE3:
  `1489b2887acc117dd1a9e98d2891b8640fa4d901621b734d1f7654940c4e11ad`
- Canonical command fixture: `fixtures/protocol/command-chat-v1.ndjson`
- Maximum complete frame including LF: 30,000 bytes
- Maximum chat payload: 2,048 UTF-8 bytes

Every command, event, snapshot, projection, and error root derives Facet. The
checked-in schema descriptor fixes root fields and payload tags; its pinned hash
and reflected root type-name tests make accidental schema drift visible.
Projection v1 includes a typed public game state, an ordered public game-event
prefix, an exact own-hand field, and exact granted-hand fields. Private hands
do not occur in public event payloads; `game_transitioned` carries only a
semantic state hash.

## Canonical NDJSON

The encoder emits compact Serde JSON in declared field order followed by exactly
one LF. The decoder accepts a frame only when decode, semantic validation, and
re-encoding produce the exact original bytes. It rejects:

- a missing LF, CRLF, an interior raw CR/LF, or multiple frames;
- leading/trailing whitespace, alternate field order, unknown fields/tags, or
  another otherwise-equivalent JSON spelling;
- invalid UTF-8 or JSON, unknown protocol/signature-domain versions, invalid
  identifiers/signatures/payload bounds, and frames over the size limit.

JSON string escapes such as chat `\n` remain payload data and cannot create a
control frame. A deterministic fuzz corpus exercises the checked-in valid seed,
single-byte mutations, and 10,000 bounded arbitrary byte sequences. Any input
the decoder accepts must re-encode byte-for-byte to itself.

## Signing and diagnostics

Diagnostic NDJSON is not signed as-is. Commands and host events have separate
unsigned types and domain-separated, length-framed binary encodings. The
canonical command vector is pinned in the protocol tests. A signature binds the
protocol/domain version, room, session epoch, object ID, principal/key ID,
revision, correlation/causation, stable-tagged payload, and algorithm.

`SecretKeyMaterial` implements neither Facet, Serde, cloning, `Debug`, nor
`Display`. Wire types contain only public key IDs and public signatures. Codec
errors return stable categories and never retain or echo rejected bytes.

## Semantic boundary

`poche-session` fixes the pure sequence
`authorize(state, attempt) -> decision`,
`decide(state, authorized command) -> events`, and
`apply(state, event) -> state`. A bare decoded command cannot be passed to
`decide`; it must be paired with an allow decision as `AuthorizedCommand`.

`poche-runtime` owns clock and transport ports around that pure boundary. Clock
adapters deliver logical tokens; reducers do not read wall time. Transport
adapters receive already viewer-scoped typed frames. Direct RL use may call the
policy-neutral `GameEnvironment` without JSON or network traffic.

## Phase 3 protocols remain separate and versioned

Host-authoritative session protocol v1 is still supported byte-for-byte. Phase
3 adds contracts around it rather than silently changing its schema:

| Contract | Purpose | Authority boundary |
| --- | --- | --- |
| `poche.governance-command.v1` | One Facet-reflected AST for slash text, HTML/native controls, classified drags, votes, and replay | Parsing grants nothing. The typed command still requires direct capability or an approved proposal, and structural invariants are never voteable. |
| `poche-replicated-v1` | Player-root/device certificates, semantic proposals, player-deduplicated vote certificates, hash-linked events, snapshots, and fork evidence | Separate experimental authority mode; a player with several devices still has one quorum vote. Host protocol v1 does not acquire consensus by using these bytes. |
| `poche.gateway-command.v1` | A bounded browser-device HTTP command signed by a browser-local Ed25519 key | The signature prevents another client from impersonating the device; it does not hide command/projection plaintext from the disclosed host gateway. |
| `poche-spatial-v1` | Exact integer scene realization, viewer-authorized semantic text, classified interaction, and deterministic animation endpoints | Spatial records propose/visualize typed meaning. Geometry, ECS state, DOM layout, and Slug glyphs are not game authority. |
| `poche-mental-poker-bg12-v0` | Membership-bound key, shuffle, private-deal, play-reveal, and final-audit artifacts | Research-only proof context. Recipient encryption belongs to transport; unanimous reveal and independent security review remain requirements. |

The canonical transport for inspectable fixtures remains NDJSON, but semantic
commands are not text strings internally. The original spelling is discarded
after strict parsing; signing, authorization, proposal identity, idempotency,
and replay use the canonical typed representation. HTTP request/response plus
SSE, direct typed loopback, and Veilid AppCall/event delivery are adapters for
the same reducer boundaries. SSE is a server-to-browser projection stream, so
browser commands still use bounded HTTP POST; no WebSocket/WebTransport path is
claimed merely because another transport could carry the same records.

Route credentials are also distinct from protocol authority. Legacy `p3-`
retains its direct-Veilid meaning. `p3r-` may identify direct Veilid, an exact
HTTPS gateway origin, or explicit loopback HTTP, while binding versions,
expiry, expected host principal, and invitation secret. Redemption still must
produce membership; membership still must certify devices; an accepted event
still must satisfy the selected host or replicated authority mode.
