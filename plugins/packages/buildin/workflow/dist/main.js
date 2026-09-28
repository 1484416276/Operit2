"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.EVENT_TOPICS = exports.ROUTE = exports.triggerWorkflow = exports.deleteWorkflow = exports.setEnabled = exports.patchWorkflow = exports.updateWorkflow = exports.get = exports.create = exports.getAll = void 0;
exports.registerToolPkg = registerToolPkg;
exports.onManifestExtension = onManifestExtension;
exports.onClock = onClock;
exports.onOpen = onOpen;
exports.onEvent = onEvent;
const public_api_1 = require("./public-api");
var public_api_2 = require("./public-api");
Object.defineProperty(exports, "getAll", { enumerable: true, get: function () { return public_api_2.getAll; } });
Object.defineProperty(exports, "create", { enumerable: true, get: function () { return public_api_2.create; } });
Object.defineProperty(exports, "get", { enumerable: true, get: function () { return public_api_2.get; } });
Object.defineProperty(exports, "updateWorkflow", { enumerable: true, get: function () { return public_api_2.updateWorkflow; } });
Object.defineProperty(exports, "patchWorkflow", { enumerable: true, get: function () { return public_api_2.patchWorkflow; } });
Object.defineProperty(exports, "setEnabled", { enumerable: true, get: function () { return public_api_2.setEnabled; } });
Object.defineProperty(exports, "deleteWorkflow", { enumerable: true, get: function () { return public_api_2.deleteWorkflow; } });
Object.defineProperty(exports, "triggerWorkflow", { enumerable: true, get: function () { return public_api_2.triggerWorkflow; } });
const web_1 = __importDefault(require("./ui/web"));
const service_1 = require("./service");
const validation_1 = require("./validation");
exports.ROUTE = "toolpkg:com.operit.workflow:ui:workflow";
exports.EVENT_TOPICS = ["app.lifecycle.resumed", "system.network.changed", "system.power.connected", "system.power.disconnected", "system.screen.on", "system.screen.off", "system.battery.low", "system.battery.okay"];
// Main-runtime IPC loads this module without invoking the metadata registration function.
ToolPkg.ipc.on("workflow.service", service_1.receive);
ToolPkg.ipc.on("workflow.web", (request, meta) => (0, service_1.receive)(request, meta));
/** Registers the plugin UI, public service and host-owned trigger sources. */
function registerToolPkg() {
    ToolPkg.registerApi({ name: "getAll", function: public_api_1.getAll });
    ToolPkg.registerApi({ name: "create", function: public_api_1.create });
    ToolPkg.registerApi({ name: "get", function: public_api_1.get });
    ToolPkg.registerApi({ name: "update", function: public_api_1.updateWorkflow });
    ToolPkg.registerApi({ name: "patch", function: public_api_1.patchWorkflow });
    ToolPkg.registerApi({ name: "setEnabled", function: public_api_1.setEnabled });
    ToolPkg.registerApi({ name: "delete", function: public_api_1.deleteWorkflow });
    ToolPkg.registerApi({ name: "trigger", function: public_api_1.triggerWorkflow });
    ToolPkg.registerUiRoute({ id: "workflow", route: exports.ROUTE, screen: web_1.default, runtime: "compose_dsl", keepAlive: true, title: { zh: "工作流", en: "Workflow" } });
    ToolPkg.registerNavigationEntry({ id: "workflow_sidebar", route: exports.ROUTE, surface: "main_sidebar_plugins", title: { zh: "工作流", en: "Workflow" }, icon: "AccountTree", order: 140 });
    ToolPkg.registerNavigationEntry({ id: "workflow_toolbox", route: exports.ROUTE, surface: "toolbox", title: { zh: "工作流", en: "Workflow" }, icon: "AccountTree", order: 140 });
    ToolPkg.registerHostEventHook({ id: "workflow_clock", source: "interval", trigger: { kind: "interval", intervalMs: 60000 }, function: onClock });
    ToolPkg.registerAppLifecycleHook({ id: "workflow_open", event: "application_on_create", function: onOpen });
    ToolPkg.registerManifestExtension({ key: "workflow_templates", function: onManifestExtension });
    for (const topic of exports.EVENT_TOPICS)
        ToolPkg.registerHostEventHook({ id: `workflow_${topic}`, source: "broadcast", trigger: { kind: "broadcast", topic }, function: onEvent });
    return true;
}
/** Imports workflow template declarations from dependent ToolPkg manifests. */
async function onManifestExtension(event) {
    if (event.eventPayload.extensionKey !== "workflow_templates")
        throw new Error(`不支持的 manifest 扩展：${event.eventPayload.extensionKey}`);
    const entries = event.eventPayload.extension;
    if (!Array.isArray(entries))
        throw new Error("workflow_templates 必须是数组");
    const useEnglish = getLang().toLowerCase().startsWith("en");
    const templates = [];
    for (const item of entries) {
        if (item === null || typeof item !== "object" || Array.isArray(item))
            throw new Error("workflow_templates 项必须是对象");
        const record = item;
        const templateId = requireManifestString(record.id, "id");
        const resourceKey = requireManifestString(record.resource_key, "resource_key");
        const displayName = localizedManifestString(record.display_name, useEnglish, "display_name");
        const description = localizedManifestString(record.description, useEnglish, "description");
        const resourcePath = await ToolPkg.readResourceFromPackage(event.eventPayload.sourceToolPkgId, resourceKey);
        const resourceText = (await Tools.Files.read(resourcePath)).content;
        const workflow = (0, validation_1.parseWorkflow)(JSON.parse(resourceText));
        templates.push({ sourceToolPkgId: event.eventPayload.sourceToolPkgId, sourceVersion: event.eventPayload.sourceVersion, templateId, displayName, description, resourceKey, workflow });
    }
    await (0, service_1.replaceManifestTemplates)(event.eventPayload.sourceToolPkgId, templates);
}
/** Reads a required string field from a manifest extension object. */
function requireManifestString(value, field) {
    if (typeof value !== "string" || value.trim() === "")
        throw new Error(`workflow_templates.${field} 必须是非空字符串`);
    return value.trim();
}
/** Resolves one localized manifest value for the active language. */
function localizedManifestString(value, useEnglish, field) {
    if (value === null || typeof value !== "object" || Array.isArray(value))
        throw new Error(`workflow_templates.${field} 必须是本地化对象`);
    const selected = value[useEnglish ? "en" : "zh"];
    return requireManifestString(selected, `${field}.${useEnglish ? "en" : "zh"}`);
}
/** Checks persisted schedules when the host emits the plugin clock event. */
async function onClock() { await (0, service_1.trigger)("schedule"); }
/** Runs cold-start entry nodes through the application lifecycle contract. */
async function onOpen() { await (0, service_1.trigger)("app_open"); }
/** Forwards normalized host events to matching workflow entry nodes. */
async function onEvent(event) {
    await (0, service_1.trigger)("event", event.eventPayload.payload.topic, { event: JSON.stringify(event.eventPayload.payload) });
}
