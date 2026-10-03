import type { Request } from "../shared/model";
export function call(request: Request): Promise<unknown> { return ToolPkg.ipc.call<Request, unknown>("music.service", request, { targetRuntime: "main" }); }
export interface Revision { projectId: string; revision: number }
export function batch(p: Revision, operations: unknown[]): Promise<unknown> { return call({ action: "batch", ...p, operations }); }
export function json(value: string): unknown { if (typeof value !== "string" || value.length > 2000000) throw new Error("Expected JSON string ≤2MB"); return JSON.parse(value); }
