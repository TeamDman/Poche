# Application identity and protected storage

Task 5.1 introduces `poche-veilid` without putting Veilid on ordinary game,
session, CLI, formal, or RL paths. Its default features contain only the stable
application identity boundary. The non-default `veilid` feature adds the exact
released `veilid-core = 0.5.7` production protected-store adapter.

An application identity has two independent 32-byte secrets:

- an Ed25519 signing key whose lowercase public-key hex is the stable
  `PrincipalId`; and
- an X25519 recipient key produced from a separately generated secret using
  Curve25519 clamped base multiplication.

Neither value is a Veilid node ID, DHT owner key, or private route. Those
transport identities can rotate without changing room membership, policy
principal, command signer, or private-projection recipient.

Only `ApplicationPublicIdentity` is serializable or diagnostic. The application
identity and its versioned protected-store blob deliberately implement no
`Debug`, `Display`, `Clone`, Facet, or serialization traits. The blob is zeroed
on drop. Its internal checksum detects truncation/corruption but is not treated
as authentication; the platform protected store supplies the security boundary.

`VeilidProtectedIdentityStore` calls the public protected-store API from the
released crate. Before Veilid startup, `validate_protected_store_config`
requires a nonempty device-encryption password and rejects insecure fallback,
forced insecure storage, and delete-on-start. Wrong credentials fail Veilid
startup. The identity loader propagates any storage error and generates a new
identity only after a successful, credentialed `None` result; it never responds
to an error by silently replacing a principal.

Veilid's released protected-store write API reports whether it overwrote an
existing value but does not provide compare-and-set. Poche therefore requires a
single identity-initialization writer for a protected-store profile. It checks
for an existing identity before writing and treats a reported replacement as a
concurrent-creation error; callers must not start two first-run processes over
the same profile. Existing identities are read-only through this adapter.

The memory-only development store exists only in tests or under the explicit
`insecure-development` feature. Both its constructor and load policy require
`AcknowledgeSecretsAreNotProtected`; merely compiling the feature cannot select
it accidentally.

There is no secret export or backup format in identity schema v1. Losing the
platform credential or protected-store entry loses the application identity
and therefore access to memberships bound to that principal. Restoring an
unprotected copy is intentionally unsupported. A future backup design must be
versioned, encrypted, authenticated, and explicitly user initiated rather than
changing these semantics silently.

Commands and authority events are signed over the existing protocol canonical
domains. Verification checks the expected public identity, principal/key ID,
algorithm, and strict Ed25519 signature. Tests show that a different key is
denied and that changing the signed expected revision invalidates the
signature, so a captured signature cannot authorize modified replay material.

The released and local APIs differ despite both reporting 0.5.7: the crates.io
release exposes synchronous methods through `VeilidAPI::protected_store()`,
whereas the inspected local `main` checkout adds async convenience methods on
`VeilidAPI`. Poche compiles against and documents the released crates.io API;
the checkout remains reference-only.

Evidence commands:

```text
cargo test -p poche-veilid
cargo test -p poche-veilid --features veilid
cargo clippy -p poche-veilid --all-targets --features veilid -- -D warnings
```
