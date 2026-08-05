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

Pure acceptance currently proves canonical code round-trip, mutation and
expiry rejection, strict/bounded/secret-free record encoding, host/network
binding, stable-key membership creation, and invalid/replayed/cross-room/
revoked denial. The released adapter builds with denied warnings.

The remaining Task 5.2 gate is a real two-native-node execution. A loopback or
ordinary LAN address belongs to Veilid's `LocalNetwork` routing domain, while
0.5.7 private-route allocation requires a ready `PublicInternet` routing
domain. The released `virtual-network` configuration is not an executable
substitute: the configuration is not consumed by core networking and the
published virtual-router server still leaves machine allocation unimplemented.
Consequently Poche does not label the codec/unit evidence as a native transport
pass. Task 5.6 will provide an isolated routable local topology or require an
explicit opt-in public-network run; ordinary unit tests never contact the
public Veilid network.

Evidence commands:

```text
cargo test -p poche-veilid
cargo test -p poche-veilid --features veilid
cargo clippy -p poche-veilid --all-targets --features veilid -- -D warnings
```
