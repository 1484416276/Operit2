import { getAll, create, get, updateWorkflow, patchWorkflow, setEnabled, deleteWorkflow, triggerWorkflow } from "./public-api";
export { getAll, create, get, updateWorkflow, patchWorkflow, setEnabled, deleteWorkflow, triggerWorkflow } from "./public-api";
import screen from "./ui/web";
import { dispatch, receive, replaceManifestTemplates, trigger } from "./service";
import { parseWorkflow } from "./validation";
import type { ManifestWorkflowTemplate } from "./model";

export const ROUTE = "toolpkg:com.operit.workflow:ui:workflow";
export const EVENT_TOPICS: ToolPkg.BroadcastTopic[] = ["app.lifecycle.resumed", "system.network.changed", "system.power.connected", "system.power.disconnected", "system.screen.on", "system.screen.off", "system.battery.low", "system.battery.okay"];

// Main-runtime IPC loads this module without invoking the metadata registration function.
ToolPkg.ipc.on("workflow.service", receive);
ToolPkg.ipc.on("workflow.web", (request, meta) => receive(request as import("./service").Request, meta));

/** Registers the plugin UI, public service and host-owned trigger sources. */
export function registerToolPkg(): boolean {
  ToolPkg.registerApi({ name: "getAll", function: getAll });
  ToolPkg.registerApi({ name: "create", function: create });
  ToolPkg.registerApi({ name: "get", function: get });
  ToolPkg.registerApi({ name: "update", function: updateWorkflow });
  ToolPkg.registerApi({ name: "patch", function: patchWorkflow });
  ToolPkg.registerApi({ name: "setEnabled", function: setEnabled });
  ToolPkg.registerApi({ name: "delete", function: deleteWorkflow });
  ToolPkg.registerApi({ name: "trigger", function: triggerWorkflow });
  ToolPkg.registerUiRoute({ id: "workflow", route: ROUTE, screen, runtime: "compose_dsl", keepAlive: true, title: { zh: "工作流", en: "Workflow" } });
  ToolPkg.registerNavigationEntry({ id: "workflow_sidebar", route: ROUTE, surface: "main_sidebar_plugins", title: { zh: "工作流", en: "Workflow" }, icon: "AccountTree", order: 140 });
  ToolPkg.registerNavigationEntry({ id: "workflow_toolbox", route: ROUTE, surface: "toolbox", title: { zh: "工作流", en: "Workflow" }, icon: "AccountTree", order: 140 });
  ToolPkg.registerHostEventHook({ id: "workflow_clock", source: "interval", trigger: { kind: "interval", intervalMs: 60000 }, function: onClock });
  ToolPkg.registerAppLifecycleHook({ id: "workflow_open", event: "application_on_create", function: onOpen });
  ToolPkg.registerManifestExtension({ key: "workflow_templates", function: onManifestExtension });
  for (const topic of EVENT_TOPICS) ToolPkg.registerHostEventHook({ id: `workflow_${topic}`, source: "broadcast", trigger: { kind: "broadcast", topic }, function: onEvent });
  return true;
}

/** Imports workflow template declarations from dependent ToolPkg manifests. */
export async function onManifestExtension(event: ToolPkg.ManifestExtensionHookEvent): Promise<void> {
  if (event.eventPayload.extensionKey !== "workflow_templates") throw new Error(`不支持的 manifest 扩展：${event.eventPayload.extensionKey}`);
  const entries = event.eventPayload.extension;
  if (!Array.isArray(entries)) throw new Error("workflow_templates 必须是数组");
  const useEnglish = getLang().toLowerCase().startsWith("en");
  const templates: ManifestWorkflowTemplate[] = [];
  for (const item of entries) {
    if (item === null || typeof item !== "object" || Array.isArray(item)) throw new Error("workflow_templates 项必须是对象");
    const record = item as ToolPkg.JsonObject;
    const templateId = requireManifestString(record.id, "id");
    const resourceKey = requireManifestString(record.resource_key, "resource_key");
    const displayName = localizedManifestString(record.display_name, useEnglish, "display_name");
    const description = localizedManifestString(record.description, useEnglish, "description");
    const resourcePath = await ToolPkg.readResourceFromPackage(event.eventPayload.sourceToolPkgId, resourceKey);
    const resourceText = (await Tools.Files.read(resourcePath)).content;
    const workflow = parseWorkflow(JSON.parse(resourceText));
    templates.push({ sourceToolPkgId: event.eventPayload.sourceToolPkgId, sourceVersion: event.eventPayload.sourceVersion, templateId, displayName, description, resourceKey, workflow });
  }
  await replaceManifestTemplates(event.eventPayload.sourceToolPkgId, templates);
}

/** Reads a required string field from a manifest extension object. */
function requireManifestString(value: ToolPkg.JsonValue | undefined, field: string): string {
  if (typeof value !== "string" || value.trim() === "") throw new Error(`workflow_templates.${field} 必须是非空字符串`);
  return value.trim();
}

/** Resolves one localized manifest value for the active language. */
function localizedManifestString(value: ToolPkg.JsonValue | undefined, useEnglish: boolean, field: string): string {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error(`workflow_templates.${field} 必须是本地化对象`);
  const selected = (value as ToolPkg.JsonObject)[useEnglish ? "en" : "zh"];
  return requireManifestString(selected, `${field}.${useEnglish ? "en" : "zh"}`);
}

/** Checks persisted schedules when the host emits the plugin clock event. */
export async function onClock(): Promise<void> { await trigger("schedule"); }

/** Runs cold-start entry nodes through the application lifecycle contract. */
export async function onOpen(): Promise<void> { await trigger("app_open"); }

/** Forwards normalized host events to matching workflow entry nodes. */
export async function onEvent(event: ToolPkg.HostEventBroadcastHookEvent<ToolPkg.BroadcastTopic>): Promise<void> {
  await trigger("event", event.eventPayload.payload.topic, { event: JSON.stringify(event.eventPayload.payload) });
}
