# operit-node-edge

Lightweight device-side Edge Core node.

The crate owns typed Edge Services and adapts them to the internal
`CoreLinkSharedClient` boundary. It consumes `operit-host-api::HostManager` and
does not represent the full `CoreNode` defined by the Space architecture. It
does not own `OperitApplication`, providers, ToolPkg, chat orchestration,
persistence, identity, Access sessions, Space sync, or Proxy implementations.

## Native plugins

Firmware registers small `EdgePlugin` implementations at startup with
`EdgeNode::withPlugin`. Each declares a stable id and action names. Authenticated
PeerLink callers use the `edge.plugins` target: `list` returns manifests;
`invoke` accepts `pluginId`, `action`, and `args`. Core callers can use
`EdgeProxy::plugins()` for the same operations.

Only firmware-registered actions run on the device. ToolPkg JavaScript and
unadapted plugin UI remain on Core and are not transferred as executable code.

Transport carriers are composition concerns. A future ESP32 Wi-Fi, BLE, or
serial carrier should feed the node through a transport adapter while keeping
Link request and event types out of the device app layer.
