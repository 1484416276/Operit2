# Operit2 Web Access

This directory owns the Web Access frontend boundary.

The deployment server must send these headers for every application asset so
threaded Sherpa ONNX WebAssembly can run local STT and TTS:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Resource-Policy: same-origin
```

The deployment server or reverse proxy for `web.operit.app` must emit these headers. The Link Access server exposed by the CLI does not serve the Web frontend.

## Flutter Web development

`fvm flutter run -d edge` uses Flutter's built-in development server, which
does not provide the required response headers. Run Flutter as a Web Server and
open the isolated proxy origin instead:

```powershell
cd apps/flutter/app
fvm flutter run -d web-server --web-hostname 127.0.0.1 --web-port 4835
```

In a second terminal at the repository root:

```powershell
node tools/dev_web_access_proxy.mjs --upstream-port 4835 --listen-port 4836
```

Open `http://127.0.0.1:4836`. The proxy forwards Flutter's HTTP and debug
WebSocket traffic, and sends the cross-origin isolation headers for every
response. Flutter hot reload remains available through the Web Server session.

For VS Code, select `Operit2: Web (isolated)` from Run and Debug and press F5.
That launch configuration starts the proxy task, runs Edge through the FVM
Flutter SDK, and opens the isolated origin automatically.

## Runtime layout

This is the standard Flutter Web entry directory for `web.operit.app`. The
standalone browser runtime is kept under `runtime/` and is split by purpose:

- `runtime/src/` contains TypeScript sources and its compiler configuration.
- `runtime/generated/` contains generated JavaScript/WASM artifacts and the v86
  guest runtime.
- `runtime/vendor/` contains third-party static files such as MessagePack.

Native apps and the CLI do not package or serve this directory. They expose only
the Link Access protocol; the browser connects through `https://web.operit.app/`
with the `accessUrl` and `token` query parameters.
