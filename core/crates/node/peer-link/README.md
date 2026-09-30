# operit-peer-link

Node-owned, lightweight peer communication machinery. A device role is not a
transport type. This crate does not depend on node runtime, persistence, business
services or the full proxy, so constrained devices can use the same engine.

## Source map

- `src/connection.rs`: PeerLink registration, request correlation, routed calls,
  watches, push streams, heartbeat and connection teardown.
- `src/transport/http.rs`: ordered HTTP batching and Host byte-stream receive.
- `src/transport/websocket.rs`: authenticated WebSocket adapter.
- `src/transport/duplex.rs`: extracted WebSocket ordered receive, registration and teardown lifecycle.
- `src/transport/serial.rs`: serial carrier over Host serial APIs (`serial` feature).
- `src/transport/serial_codec.rs`: bounded framing and checksum resynchronization.
- `src/transport/mod.rs`: the shared bidirectional `LinkChannel` contract.
- `src/timing.rs`: deadlines driven by Host delay, not a Tokio timer driver.
- `src/observer.rs`: optional topology/measurement callbacks without store coupling.

HTTP/WebSocket use authenticated session interfaces supplied by node runtime.
The TCP-specific LinkFrame adapter has been deleted, not relocated.
Host TCP byte I/O remains a separate platform capability.

## Integration status

Both `node/runtime/remote/channel.rs` and `peer-link/transport/tcp.rs` are removed.
The old firmware and simulator still reference the deleted TCP adapter and are
not buildable until their ingress is replaced by the standard Link interface.
Runtime TCP/serial post-pairing opening currently returns an error. Serial
LinkFrame adaptation and device-side SpaceContext bootstrap still need removal.
The lightweight EdgeNode accepts CoreLinkClient call/watch/push; current board
services do not declare push methods, so these return the standard method error.
