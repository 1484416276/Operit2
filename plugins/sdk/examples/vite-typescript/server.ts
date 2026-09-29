import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { createServer as createViteServer } from "vite";
import {
  OperitApplicationPackageManagerClient,
  OperitPluginSdkClient,
} from "@operit/plugin-sdk";

const port = Number(process.env.PORT ?? 5173);

/** Selects the first localized value in a stable display order. */
function localizedText(value: unknown): string {
  const values = (value as { values?: Record<string, string> } | null)?.values;
  if (!values) {
    throw new Error("Package metadata has no localized values");
  }
  const english = values.en;
  if (typeof english !== "string") {
    throw new Error("Package metadata has no English value");
  }
  return english;
}

/** Writes a JSON response with the SDK example's API headers. */
function writeJson(response: ServerResponse, status: number, value: unknown): void {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(body),
  });
  response.end(body);
}

/** Serves the package catalog queried through the generated SDK client. */
async function handleApi(
  request: IncomingMessage,
  response: ServerResponse,
  packageManager: OperitApplicationPackageManagerClient,
): Promise<boolean> {
  if (request.method !== "GET" || request.url !== "/api/packages") {
    return false;
  }
  const packages = await packageManager.getAvailablePackages();
  const enabled = new Set(await packageManager.getEnabledPackageNames());
  const summary = Object.entries(packages).map(([id, value]) => ({
    id,
    displayName: localizedText(value.display_name),
    description: localizedText(value.description),
    enabled: enabled.has(id),
  }));
  writeJson(response, 200, summary);
  return true;
}

/** Creates the Vite development server and native SDK API bridge. */
async function start(): Promise<void> {
  const sdk = await OperitPluginSdkClient.connect();
  const packageManager = new OperitApplicationPackageManagerClient(sdk);
  const vite = await createViteServer({ server: { middlewareMode: true } });
  const server = createServer(async (request, response) => {
    try {
      if (await handleApi(request, response, packageManager)) {
        return;
      }
      await new Promise<void>((resolve, reject) => {
        vite.middlewares(request, response, (error?: Error) => error ? reject(error) : resolve());
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!response.headersSent) {
        writeJson(response, 500, { error: message });
      } else {
        response.destroy(error instanceof Error ? error : undefined);
      }
    }
  });
  server.listen(port, () => {
    console.log(`Vite SDK example is running at http://localhost:${port}`);
  });
  const close = async (): Promise<void> => {
    await vite.close();
    sdk.close();
    server.close();
  };
  process.once("SIGINT", () => void close());
  process.once("SIGTERM", () => void close());
}

void start();
