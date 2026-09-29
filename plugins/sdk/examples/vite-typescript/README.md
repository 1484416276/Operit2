# Vite + TypeScript SDK example

This example uses Vite for the browser UI and development server. The server-side
route imports `@operit/plugin-sdk`, connects through the native SDK host, and
exposes the available ToolPkg catalog at `/api/packages`. The browser renders
that typed SDK response without importing Node-only SDK transport code.

## Run

Build the local SDK client, install this example's dependencies, and start Vite:

```powershell
npm install
npm run dev
```

Open `http://localhost:5173`. The native SDK carrier must be staged at
`plugins/sdk/clients/typescript/native/liboperit_plugin_sdk.so`; build it with
`python plugins/sdk/build_native.py --target <desktop-target>` as described in
[`plugins/sdk/README.md`](../../README.md).

`npm run build` creates the browser assets and runs the TypeScript check.
