# Operit Plugin SDK clients

The clients in this directory are generated from the same `operit-proxy-scan`
model used by the Core Rust and Flutter proxies. Generation is driven only by
methods explicitly marked for the external SDK in their source route
annotation. Each generated object client forwards the existing Link `call`,
`watch`, and `push` primitives through the Plugin SDK IPC session. The IPC
server enforces the same generated route allowlist; the client has no tool
catalog, discovery API, or user-provided transport contract.

The generated artifacts are refreshed by the `operit-proxy-local` build script:

- `clients/rust/src/generated.rs`
- `clients/dart/lib/src/generated.dart`
- `clients/kotlin/src/main/kotlin/OperitPluginSdkGenerated.kt`
- `clients/typescript/src/generated.ts`

Generated route methods retain the declared argument names and types. For example,
Dart calls `disablePackage(packageName: 'example')` and
`getToolPkgContainerDetails(packageName: 'example', useEnglish: true)`; a route
without arguments has an empty parameter list. The generated wrapper serializes
the Link argument map. Input-only DTOs are generated alongside return models.

Regenerate only these SDK artifacts with:

```sh
cargo run --manifest-path core/Cargo.toml -p operit-plugin-sdk-codegen --example generate_plugin_sdk_clients
```

## Connecting

The clients include platform activation. Android starts an empty Activity and
binds an IBinder service; Linux uses D-Bus activation and pipe descriptor passing;
Windows launches the registered Operit URI and waits for its named pipe. There
are no TCP/Unix endpoint arguments in the public language clients.

- Rust: `operit_plugin_sdk_client::connect()`.
- TypeScript/Node: `await OperitPluginSdkClient.connect()`.
- Dart/Flutter: `await OperitPluginSdkClient.connect()`.
- Kotlin desktop: `OperitPluginSdkClient.connect()` (suspending).
- Kotlin Android: `OperitPluginSdkClient.connect(context)` with the Android
  source set. Flutter registers the Android context through its packaged plugin.

The shared native implementation is in `native`, with platform dispatch in
`hosts/plugin-sdk-client`. It uses the same host carrier source as the server;
language clients retain their generated MessagePack models and call/watch/push
API. Native code handles activation, frame limits and pipe ownership. Dart and
Node perform native work on workers so startup and writes do not block UI/event
loops. Close each language client when finished.

## Native SDK packaging

Build and stage the native library for the distribution target, for example:

```sh
python plugins/sdk/build_native.py --target x86_64-pc-windows-msvc
python plugins/sdk/build_native.py --target x86_64-unknown-linux-gnu
python plugins/sdk/build_native.py --target aarch64-linux-android
```

Use the repository Python venv. Android cross-builds require the NDK compiler
environment. `--no-build` stages an already-built release artifact. Node loads
the packaged `native/liboperit_plugin_sdk.so`; this is a fixed artifact name on
desktop platforms, including Windows. Dart deployments put that artifact in
the application's native library search path or supply `nativeLibraryPath` to
`connect`. Node accepts an explicit library path for custom packaging too.
These paths locate the SDK library, not an IPC endpoint. JVM deployments put
their staged library on `java.library.path`. Android AAR/Flutter packages bundle
the library in `jniLibs` and retain JNI classes via consumer ProGuard rules.

Operit must have been opened once to configure local storage and register desktop
activation. Android activation must originate while the integrating app is
visible, in accordance with Android Activity-launch rules. Apple retains its
existing host transport; automatic Apple activation is not implemented here.

## Examples

- [`examples/vite-typescript`](examples/vite-typescript) is a small Vite app that keeps the browser UI in Vite while its server route calls the generated TypeScript SDK and displays the live ToolPkg catalog.
