/**
 * Binds the handwritten v1 workflow surface to the workflow prerequisite's public API.
 * @param {<T>(method: string, payload: object) => Promise<T>} invoke
 * @returns {import('../../../../../../../plugins/types-v1/workflow').Workflow.Runtime}
 */
function __operitCreateV1Workflow(invoke) {
    return {
        /** Lists workflows owned by the prerequisite service. */
        getAll: () => invoke('getAll', {}),
        /** Preserves the legacy positional create signature and graph spellings. */
        create: (name, description, nodes, connections, enabled) => invoke('create', { name, description, nodes, connections, enabled }),
        /** Fetches a workflow by its stable identity. */
        get: workflowId => invoke('get', { workflow_id: workflowId }),
        /** Sends only explicitly supplied update fields. */
        update: (workflowId, updates = {}) => invoke('update', Object.assign({}, updates, { workflow_id: workflowId })),
        /** Applies explicit graph patch operations in the provider. */
        patch: (workflowId, patch = {}) => invoke('patch', Object.assign({}, patch, { workflow_id: workflowId })),
        /** Changes the enabled flag. */
        setEnabled: (workflowId, enabled) => invoke('setEnabled', { workflow_id: workflowId, enabled }),
        /** Enables the selected workflow. */
        enable: workflowId => invoke('setEnabled', { workflow_id: workflowId, enabled: true }),
        /** Disables the selected workflow. */
        disable: workflowId => invoke('setEnabled', { workflow_id: workflowId, enabled: false }),
        /** Deletes the selected workflow. */
        delete: workflowId => invoke('delete', { workflow_id: workflowId }),
        /** Waits for the provider's execution result. */
        trigger: workflowId => invoke('trigger', { workflow_id: workflowId })
    };
}
