# Durable membership and reconnect

Task 5.3 separates three identities that must not be conflated:

- the stable application signing/encryption key identifies a person;
- a host-signed `MembershipCredential` identifies that stable key's membership
  in one room and session epoch;
- Veilid node IDs and private routes are replaceable transport coordinates.

Changing a route therefore never changes `PrincipalId`, membership, or seat.
Conversely, presenting a valid old credential never recreates a member that the
current authoritative session has removed. The session reducer remains the
final authorization boundary.

## Join-to-reconnect handoff

Successful one-time code redemption produces a host-signed credential. The
client then replaces the code with a `MembershipLocator` containing exactly:

- network and room ID;
- the encrypted Veilid DHT record key;
- the stable host public application identity; and
- the host-signed member credential bound to the stable member public identity.

The random invite secret and full `p3-` code are structurally absent. The
locator can only be persisted through a `MembershipStore`. Production uses
Veilid protected storage under a hashed, room-name-free key. The in-memory
store is available only for tests or an explicit insecure-development feature
and policy acknowledgement. Secret-bearing buffers are redacted from `Debug`
and overwritten on replacement, removal, and drop.

After restart, `resolve_membership` opens the same encrypted DHT record without
the invite, validates its network/room/host/session/expiry bindings, and imports
the current host private route. The member sends a stable-key-signed
`Reconnect` command plus a separately signed fresh recipient route and monotonic
route epoch. This proves that changing a recipient route did not change the
member principal.

## Host restart

Creating a room also writes the encrypted DHT record key and DHT owner keypair
to Veilid protected storage. Neither capability is exposed by a public type or
diagnostic. `resume_host_room` requires the same stable host application key,
reopens the owner record, validates the existing room and host binding,
allocates a new private route, increments `route_epoch`, and republishes the
rendezvous record. It does not retain or recreate the old invite code.

The authoritative game/session state is recovered by the deterministic
snapshot/transcript machinery established in Task 2.4; the transport layer
does not invent a second game-state persistence format. Task 5.4 wires this
recovery exchange into request handling; Task 5.6 subsequently exercised the
same boundary in separate native processes.

## Recovery and revocation

`RecoveryBundle` contains a viewer-scoped, host-signed snapshot followed by a
host-signed, gap-free authority event tail. Verification binds every item to
the credential room/session and stable host key and rejects gaps, reordering,
wrong keys, or signature mutation. Its diagnostic representation hides the
snapshot payload because that payload may contain an authorized private hand.

A reconnect admission has two independent gates:

1. cryptographic/transport verification validates the host credential, member
   command, and fresh recipient route;
2. current session authorization decides whether that principal is still a
   disconnected member permitted to reconnect.

Removal releases the seat and removes the principal from active membership, so
the same still-valid certificate and stable key are denied as
`UnknownPrincipal`. In the v1 session vocabulary, `RemoveMember` is the durable
membership-revocation operation; there is no weaker parallel "banned but still
active" state. Any future richer ban representation must feed the same
current-session gate. A credential is deliberately not a self-authorizing
bearer token.

## Executable evidence

The following ordinary offline checks cover the boundary without contacting a
public network:

```powershell
cargo test -p poche-veilid --features veilid --offline
cargo test -p poche-session --offline removed_member_cannot_reconnect_with_the_former_stable_principal
cargo clippy -p poche-veilid --all-targets --features veilid --offline -- -D warnings
```

They exercise host and client application-key restart, protected locator
round-trip, explicit absence of the invite secret, host and recipient route
rotation, wrong-key/tamper rejection, snapshot-tail gap/reorder rejection,
strict host DHT capability persistence, and the current-authority removal gate.
The separately guarded two-native-process DHT/private-route lifecycle is
recorded in [`veilid-native-acceptance.md`](veilid-native-acceptance.md); these
ordinary offline checks do not contact or substitute for that public run.
