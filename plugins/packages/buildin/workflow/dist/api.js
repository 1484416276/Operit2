"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.workflow = void 0;
/** Calls the package's explicitly registered public method through the host dependency boundary. */
function call(method, payload) {
    return ToolPkg.callDependency("com.operit.workflow", method, payload);
}
/** Public client for dependent packages; this file has no imports or business implementation. */
exports.workflow = {
    /** Lists stored workflows. */
    getAll: () => call("getAll", {}),
    /** Creates a workflow using the public graph input contract. */
    create: (name, description, nodes, connections, enabled) => call("create", { name, description, nodes, connections, enabled }),
    /** Reads a workflow by stable identifier. */
    get: workflowId => call("get", { workflow_id: workflowId }),
    /** Updates only supplied fields. */
    update: (workflowId, updates = {}) => call("update", { ...updates, workflow_id: workflowId }),
    /** Applies an explicit graph patch. */
    patch: (workflowId, patch = {}) => call("patch", { ...patch, workflow_id: workflowId }),
    /** Changes the enabled state. */
    setEnabled: (workflowId, enabled) => call("setEnabled", { workflow_id: workflowId, enabled }),
    /** Enables a workflow. */
    enable: workflowId => call("setEnabled", { workflow_id: workflowId, enabled: true }),
    /** Disables a workflow. */
    disable: workflowId => call("setEnabled", { workflow_id: workflowId, enabled: false }),
    /** Deletes a workflow. */
    delete: workflowId => call("delete", { workflow_id: workflowId }),
    /** Runs a workflow and reports the actual completion status. */
    trigger: workflowId => call("trigger", { workflow_id: workflowId }),
};
