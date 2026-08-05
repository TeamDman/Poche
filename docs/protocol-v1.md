# Poche session protocol v1

The normative inspectable transport is one canonical JSON envelope followed by
one LF byte. The implementation lives in `poche-protocol`; this document records
the interoperability constraints rather than duplicating its Rust types.

## Identity

- Protocol version: `1`
- Signature-domain version: `1`
- Schema descriptor: `poche.protocol.v1`
- Schema BLAKE3:
  `a3c01a792a99303adabef2ca3c8c49fb6288c0ca1ea876162c1655360a82ca0c`
- Canonical command fixture: `fixtures/protocol/command-chat-v1.ndjson`
- Maximum complete frame including LF: 30,000 bytes
- Maximum chat payload: 2,048 UTF-8 bytes

Every command, event, snapshot, projection, and error root derives Facet. The
checked-in schema descriptor fixes root fields and payload tags; its pinned hash
and reflected root type-name tests make accidental schema drift visible.

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
