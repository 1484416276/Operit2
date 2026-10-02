# Node

Node owns node-to-node communication as well as route resolution and execution.
Identity, pairing, session admission and connection lifecycle are node concerns;
there is no separate `access` domain.

## Layout

- `peer-link`: lightweight shared PeerLink engine and HTTP, WebSocket, TCP and
  serial carriers. Depends on foundation contracts and Host APIs, not the full
  application runtime or persistence stores.
- `runtime/src/remote`: node identity/session persistence, pairing, authenticated
  admission, connection supervision, HTTP/WebSocket server and session adapters.
- `runtime`: router, space runtime, discovery, remote-link service and sync.
- `route-macros`: annotation-generated route metadata.
- `edge-contract`, `edge`: compact capability contracts and device runtime.

`proxy/edge` remains a separate compact projection to meet firmware size limits.
That packaging choice must not define another peer identity or routing protocol.

## Remaining cleanup

`edge-transport` still contains a separate pairing authority and peer dispatch
engine. It no longer owns TCP/serial carriers or the LinkChannel contract; those
live only in `peer-link/transport`. Removing its remaining duplicate lifecycle
and session models is unfinished work, not an intended second architecture.
