# Veilid rendezvous and room codes

Task 5.2 uses exactly one owner-writable subkey in a Veilid `DFLT` record.
Dynamic room membership is application state and is never represented by a
`SMPL` schema. Veilid encrypts the DHT value under the encryption component of
the returned `RecordKey`; only the compact room code carries that full key.
DHT peers see the opaque record key and encrypted value, not the code's record
encryption key or invite secret.

The strict bounded rendezvous value contains only:

- rendezvous and protocol versions;
- network, room ID, session epoch, route epoch, and authority-clock expiry;
- the host's stable public application identity;
- a bounded room label, player limit, and spectator flag; and
- the current publishable private-route blob.

The value has one subkey, a 16 KiB application bound beneath Veilid's 32768-byte
subkey limit, and no invite field. Route bytes are base64url encoded canonically
inside strict JSON. Debug output redacts the route even though Veilid designs
the blob for publication.

A `p3-` room code is canonical base64url without padding over a binary payload:

```text
magic | code version | network | protocol version | rendezvous version
| expiry | encrypted record-key length + bytes | expected host signing key
| 32-byte one-time secret | 8-byte typo checksum
```

The Veilid 0.5.7 VLD0 encrypted record key is 92 ASCII bytes, keeping the
complete code below the protocol's 256-byte invite-proof limit. The code and
its text wrapper have redacted diagnostics and zero their owned byte buffers on
drop. The checksum detects accidental corruption; it is not authorization.
Authorization comes from possession of the random secret, the host's
authority-side verifier, the application signature of the joining stable key,
and one-time consumption by the session reducer.

`VeilidRendezvous` compiles against the released 0.5.7 API. The host allocates a
private route, creates `DHTSchema::dflt(1)`, publishes subkey zero, and flushes
it. A client parses the encrypted record key, force-refreshes and validates the
record, checks network/host/version/expiry bindings, imports the private route,
and uses `app_call` for invite redemption. Both request and response are capped
at Veilid's 32768-byte limit. DHT values are discovery hints, never command or
event authority.

Pure acceptance proves canonical code round-trip, mutation and
expiry rejection, strict/bounded/secret-free record encoding, host/network
binding, stable-key membership creation, and invalid/replayed/cross-room/
revoked denial. The released adapter builds with denied warnings. The opt-in
public-network acceptance additionally used two distinct native Veilid nodes:
the host created the encrypted DHT record and private route, and the client
opened and validated the record, imported the route, redeemed the one-time
code over `AppCall`, and became a durable stable-key member. The complete
game/lifecycle run reused the resulting transport without exposing invite
material in the DHT value, structured output, or diagnostics.

An isolated local probe records an upstream topology limitation instead of
mislabeling local bytes as Veilid success. A loopback or ordinary LAN address
belongs to Veilid's `LocalNetwork` routing domain, while 0.5.7 private-route
allocation requires a ready `PublicInternet` routing domain. Released direct
bootstrap also excludes `LocalNetwork` peers, and the released
`virtual-network` configuration is not an executable substitute: core does not
consume the configuration and the published virtual-router server leaves
machine allocation unimplemented. The diagnostic-only probe enables
`footgun-nodeid-target` solely to make two isolated nodes start; it obtains zero
peers and deliberately performs no application send. Actual byte-delivery
acceptance therefore requires the separately guarded public-network command.
Ordinary unit, CI, and RL workflows never contact the public Veilid network.

Evidence commands:

```text
cargo test -p poche-veilid
cargo test -p poche-veilid --features veilid
cargo clippy -p poche-veilid --all-targets --features veilid -- -D warnings
cargo run -p poche-xtask --offline -- transport test veilid-local
$env:POCHE_ALLOW_VEILID_PUBLIC_TEST='I_ACCEPT_PUBLIC_NETWORK_TRAFFIC'
cargo run -p poche-xtask --offline -- transport test veilid-public
```

See `docs/veilid-native-acceptance.md` and
`evidence/veilid-native-acceptance.json` for measured results and the opt-in
boundary.
