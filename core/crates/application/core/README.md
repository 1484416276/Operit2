# operit-core-application

`operit-core-application` is the composition root for the Core tree.

It creates and owns the current runtime application, local proxy client, node router, route runtime installation, and access services. Host surfaces should receive handles from this crate instead of constructing runtime, proxy, node, and access pieces independently.

## Boundary

- Owns `CoreApplication`, `CoreApplicationConfig`, and lifecycle-facing handles.
- Creates `OperitApplication`, `LocalCoreProxy`, `CoreNodeLocalRuntime`, `CoreNodeRouter`, and `RuntimeRemoteLinkService`.
- Accepts a preconfigured local client through `startWithLocalClient()` or `startWithSharedLocalClient()` when a host factory must finish platform wiring first.
- Exposes local Core calls through `localClient()`.
- Exposes node routing through `nodeRouter()`.
- Exposes access and device-space control through `accessServices()`.
- Reads the original persisted device metadata and updates it through `updateDeviceInfo()`.
- Injects one shared `NodeServices` through `installNodeServices()`; pairing and listening belong to that service.
- Clears the global route runtime through `shutdown()` or `shutdownNow()`.

## Call Paths

- Local host calls: `CoreApplication.localClient()` -> generated proxy -> local runtime.
- Rust route calls: route macro wrapper -> shared link route runtime -> `CoreNodeRouter`.
- Access control: application -> NodeServices -> RuntimePeerService -> PeerLink -> Host.

## Host Surfaces

- CLI command, TUI, and Link surfaces create `CoreApplication` and consume its handles.
- Flutter holds `CoreApplication` over its shared local client and uses generated typed runtime services; it does not own a separate pairing server.
