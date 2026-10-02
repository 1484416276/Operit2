# operit-node-runtime

The aggregate node runtime owns routing, space execution, pairing, authenticated
session admission and peer connection lifecycle. It uses `operit-peer-link` for
transport I/O and shared PeerLink request/watch/push/heartbeat machinery.

## Remote communication

`src/remote/` contains node-to-node communication orchestration:

- `mod.rs`: identity/session store, pairing endpoints, HTTP/WebSocket server and
  public session types. This large module still needs responsibility-based splits.
- `connections.rs`: connection ownership, reconnect/backoff and shutdown.
- `target.rs`: selects a transport for stored peer connection targets.
- `transport.rs`: HTTP/WebSocket authentication adapters for `operit-peer-link`.
- `channel.rs`: authenticated framed-channel admission for TCP/serial.
- `pairing.rs`: framed-channel pairing orchestration.
- `topology.rs`: adapts PeerLink measurements to the space store.

All shared transport implementations are in `node/peer-link/src/transport`, not
in a separate access domain or duplicated per device role. Lifecycle deadlines
and retry delays use the Host scheduler.

## Other boundaries

The node router receives local execution capabilities from the business runtime.
Proxy code generation remains under `proxy`; compact device projections do not
require linking the full application proxy into firmware.

The separate framed/session record models and Edge pairing authority have not
yet been unified; moving ownership into node does not by itself complete that
cleanup.
