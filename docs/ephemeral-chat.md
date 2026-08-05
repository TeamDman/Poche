# Ephemeral chat

Task 4.4 keeps chat inside the same authorization and event path as every
other session command. A message enters the in-process transport as a strict
typed or canonical-NDJSON command, is authorized and reduced by
`poche-session`, and reaches the runtime tail only when the resulting
`ChatPosted` event is applied for the first time. Duplicate transport delivery
therefore cannot duplicate chat.

`ChatTail` retains only the newest configured number of public, attributed
entries in process memory. The default capacity is 64. A zero-capacity tail is
valid, nothing is persisted across restart, and truncation is intentional. Its
NDJSON export contains only revision, principal ID, and text; invite material,
command envelopes, signatures, capabilities, and viewer-private projections
are outside the type being serialized.

Chat text remains payload data. JSON escaping prevents embedded newlines,
carriage returns, or control-looking JSON text from becoming extra protocol
frames. The protocol boundary rejects empty or oversized messages before
fanout, while the session reducer rejects non-members, closed rooms, and
messages beyond the bounded logical rate allowance. Rejection precedence is
covered by the loopback integration test.

The canonical CLI replay exercises an accepted chat command through the
loopback authority and renders its attributed text. A live `chat send` command
connected to a native room is deliberately deferred to the transport and
client phases; this task does not introduce a second, CLI-local chat semantic
path.

Run the focused evidence with:

```text
cargo test -p poche-runtime --test in_process_chat
cargo test -p poche-cli --test cli_contract
```
