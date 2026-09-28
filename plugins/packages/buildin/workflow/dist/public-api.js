"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.getAll = getAll;
exports.create = create;
exports.get = get;
exports.updateWorkflow = updateWorkflow;
exports.patchWorkflow = patchWorkflow;
exports.setEnabled = setEnabled;
exports.deleteWorkflow = deleteWorkflow;
exports.triggerWorkflow = triggerWorkflow;
const model_1 = require("./model");
const validation_1 = require("./validation");
const service_1 = require("./service");
/** Decodes either the array or serialized-array spelling used by the public graph API. */
function array(value, label) {
    const parsed = typeof value === "string" ? JSON.parse(value) : value;
    if (!Array.isArray(parsed))
        throw new Error(`${label} must be an array`);
    return parsed;
}
/** Converts the documented scalar/reference shorthand into the workflow engine's value model. */
function inputValue(value) {
    if (value === null)
        return { value: "" };
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean")
        return { value: String(value) };
    const raw = (0, validation_1.object)(value, "parameter");
    if (raw.ref !== undefined)
        return { nodeId: (0, validation_1.string)(raw.ref, "ref") };
    if (raw.refNodeId !== undefined)
        return { nodeId: (0, validation_1.string)(raw.refNodeId, "refNodeId") };
    return (0, validation_1.parameter)(raw);
}
/** Converts legacy shorthand fields before invoking the existing strict node validator. */
function nodeInput(value) {
    const raw = { ...(0, validation_1.object)(value, "node") };
    switch (raw.type) {
        case "execute":
            if (raw.actionConfig !== undefined)
                raw.actionConfig = Object.fromEntries(Object.entries((0, validation_1.object)(raw.actionConfig, "actionConfig")).map(([key, entry]) => [key, inputValue(entry)]));
            break;
        case "condition":
            if (raw.left !== undefined)
                raw.left = inputValue(raw.left);
            if (raw.right !== undefined)
                raw.right = inputValue(raw.right);
            break;
        case "extract":
            if (raw.defaultValue !== undefined && raw.defaultValue !== "")
                throw new Error("Non-empty extract.defaultValue is not supported by the current workflow engine");
            if (raw.source !== undefined)
                raw.source = inputValue(raw.source);
            if (raw.others !== undefined)
                raw.others = array(raw.others, "others").map(inputValue);
            break;
    }
    return (0, validation_1.parseNode)(raw);
}
/** Decodes a public connection while assigning an identity only for a new connection. */
function connectionInput(value) {
    const raw = (0, validation_1.object)(value, "connection");
    return {
        id: raw.id === undefined ? `connection_${Date.now()}_${Math.random().toString(36).slice(2)}` : (0, validation_1.string)(raw.id, "id"),
        sourceNodeId: (0, validation_1.string)(raw.sourceNodeId, "sourceNodeId"),
        targetNodeId: (0, validation_1.string)(raw.targetNodeId, "targetNodeId"),
        condition: raw.condition === undefined || raw.condition === null ? null : (0, validation_1.string)(raw.condition, "condition"),
    };
}
/** Selects a stored workflow without manufacturing a missing record. */
function find(workflows, id) {
    const workflow = workflows.find(item => item.id === id);
    if (workflow === undefined)
        throw new Error(`Workflow not found: ${id}`);
    return workflow;
}
/** Applies supplied public fields without resetting workflow identity, revision, or statistics. */
function update(workflow, request) {
    if (request.name !== undefined)
        workflow.name = (0, validation_1.string)(request.name, "name");
    if (request.description !== undefined)
        workflow.description = (0, validation_1.string)(request.description, "description");
    if (request.enabled !== undefined) {
        if (typeof request.enabled !== "boolean")
            throw new Error("enabled must be boolean");
        workflow.enabled = request.enabled;
    }
    if (request.nodes !== undefined && request.nodes !== null)
        workflow.nodes = array(request.nodes, "nodes").map(nodeInput);
    if (request.connections !== undefined && request.connections !== null)
        workflow.connections = array(request.connections, "connections").map(connectionInput);
    (0, validation_1.validateGraph)(workflow, false);
}
/** Applies explicit add/update/remove patches while preserving unspecified node properties. */
function patch(workflow, request) {
    if (request.node_patches !== undefined)
        for (const value of array(request.node_patches, "node_patches")) {
            const entry = (0, validation_1.object)(value, "node patch");
            if (entry.op === "add") {
                workflow.nodes.push(nodeInput(entry.node));
                continue;
            }
            const id = (0, validation_1.string)(entry.id, "node patch id");
            const index = workflow.nodes.findIndex(node => node.id === id);
            if (index < 0)
                throw new Error(`Node not found: ${id}`);
            if (entry.op === "remove") {
                workflow.nodes.splice(index, 1);
                workflow.connections = workflow.connections.filter(edge => edge.sourceNodeId !== id && edge.targetNodeId !== id);
            }
            else if (entry.op === "update") {
                workflow.nodes[index] = nodeInput({ ...workflow.nodes[index], ...(0, validation_1.object)(entry.node, "node"), id });
            }
            else
                throw new Error("Invalid node patch operation");
        }
    if (request.connection_patches !== undefined)
        for (const value of array(request.connection_patches, "connection_patches")) {
            const entry = (0, validation_1.object)(value, "connection patch");
            if (entry.op === "add") {
                workflow.connections.push(connectionInput(entry.connection));
                continue;
            }
            const id = (0, validation_1.string)(entry.id, "connection patch id");
            const index = workflow.connections.findIndex(edge => edge.id === id);
            if (index < 0)
                throw new Error(`Connection not found: ${id}`);
            if (entry.op === "remove")
                workflow.connections.splice(index, 1);
            else if (entry.op === "update")
                workflow.connections[index] = connectionInput({ ...workflow.connections[index], ...(0, validation_1.object)(entry.connection, "connection"), id });
            else
                throw new Error("Invalid connection patch operation");
        }
    update(workflow, request);
}
/** Exports the public graph shape without leaking internal optimistic-lock metadata. */
function detail(workflow) {
    return {
        id: workflow.id, name: workflow.name, description: workflow.description, enabled: workflow.enabled,
        createdAt: workflow.createdAt, updatedAt: workflow.updatedAt,
        lastExecutionTime: workflow.lastExecutionTime, lastExecutionStatus: workflow.lastExecutionStatus,
        totalExecutions: workflow.totalExecutions, successfulExecutions: workflow.successfulExecutions, failedExecutions: workflow.failedExecutions,
        nodes: workflow.nodes.map(node => node.type === "extract" ? { ...(0, model_1.copy)(node), defaultValue: "" } : (0, model_1.copy)(node)),
        connections: (0, model_1.copy)(workflow.connections),
    };
}
/** Lists public workflow summaries using the shared plugin-owned database. */
async function getAll(_event) {
    const snapshot = await (0, service_1.dispatch)({ action: "list" });
    const workflows = snapshot.workflows.map(workflow => {
        const { nodes, connections, ...summary } = detail(workflow);
        return { ...summary, nodeCount: nodes.length, connectionCount: connections.length };
    });
    return { workflows, totalCount: workflows.length };
}
/** Creates a complete graph atomically after validating every public field. */
async function create(event) {
    const request = (0, validation_1.object)(event.payload, "create");
    return (0, service_1.publicTransaction)(workflows => {
        const workflow = (0, model_1.newWorkflow)((0, validation_1.string)(request.name, "name"));
        update(workflow, request);
        workflows.push(workflow);
        return detail(workflow);
    });
}
/** Reads one stored workflow through its public identifier. */
async function get(event) {
    const request = (0, validation_1.object)(event.payload, "get");
    const snapshot = await (0, service_1.dispatch)({ action: "list" });
    return detail(find(snapshot.workflows, (0, validation_1.string)(request.workflow_id, "workflow_id")));
}
/** Updates fields within one serialized database mutation rather than read/save IPC calls. */
async function updateWorkflow(event) {
    const request = (0, validation_1.object)(event.payload, "update");
    const id = (0, validation_1.string)(request.workflow_id, "workflow_id");
    return (0, service_1.publicTransaction)(workflows => {
        const workflow = find(workflows, id);
        update(workflow, request);
        workflow.revision++;
        workflow.updatedAt = Date.now();
        return detail(workflow);
    }, id);
}
/** Applies a graph patch inside the same database mutation boundary. */
async function patchWorkflow(event) {
    const request = (0, validation_1.object)(event.payload, "patch");
    const id = (0, validation_1.string)(request.workflow_id, "workflow_id");
    return (0, service_1.publicTransaction)(workflows => {
        const workflow = find(workflows, id);
        patch(workflow, request);
        workflow.revision++;
        workflow.updatedAt = Date.now();
        return detail(workflow);
    }, id);
}
/** Changes only the enabled field through the normal update path. */
async function setEnabled(event) {
    const request = (0, validation_1.object)(event.payload, "setEnabled");
    if (typeof request.enabled !== "boolean")
        throw new Error("enabled must be boolean");
    return updateWorkflow(event);
}
/** Deletes the selected workflow and associated execution records through the existing service. */
async function deleteWorkflow(event) {
    const id = (0, validation_1.string)((0, validation_1.object)(event.payload, "delete").workflow_id, "workflow_id");
    find((await (0, service_1.dispatch)({ action: "list" })).workflows, id);
    await (0, service_1.dispatch)({ action: "delete", ids: [id] });
    return `Deleted workflow ${id}`;
}
/** Waits for real execution and reports failure rather than claiming a successful trigger. */
async function triggerWorkflow(event) {
    const id = (0, validation_1.string)((0, validation_1.object)(event.payload, "trigger").workflow_id, "workflow_id");
    const run = await (0, service_1.publicRun)(id);
    if (run.status !== "SUCCESS")
        throw new Error(`Workflow ${id} finished with ${run.status}`);
    return `Workflow ${id} completed successfully`;
}
