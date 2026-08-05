# In-process authoritative runtime

Task 4.2 composes the existing pure session reducer with deterministic adapters.
It does not introduce a second implementation of room or game semantics.

The boundary is deliberately narrow:

- `ScriptedClient` owns a connection handle and stable application principal.
- `InProcessTransport` binds that principal to the connection before accepting
  a command, queues typed inputs, and stores exact-recipient output frames.
- `InProcessAuthority` is the sole state owner and delegates commands to
  `authorize`, `decide`, and `apply`; an entire event batch is validated on a
  cloned state before it is committed.
- `ManualClock` emits only explicit logical countdown tokens. It never reads
  wall time.

`LoopbackCodec::Typed` is the fast default. `CanonicalNdjson` encodes and
strictly decodes both ingress and egress through `poche-protocol`, making
framing parity testable without putting text parsing in the semantic path.

The fault queue supports duplicate-next, reorder-next-pair, and
disconnect-next. Fault order is deterministic. A disconnected stable
principal may later open a new connection and issue the reducer's explicit
reconnect command; transport route identity is not application identity.

Loopback signatures are fixed structural placeholders. Security comes from
the local connection binding in this adapter. They are not a cryptographic
claim and must not be used by Veilid/native transport; Task 5 replaces them
with stable application-key signing and verification.

The acceptance test drives two players and a spectator from room creation to
`PostGame`, including the full 13-round two-player oracle schedule. No socket,
renderer, or CLI text codec participates in those transitions, preserving the
future RL fast path.
