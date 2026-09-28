/**
 * Adapts the v1.0.1 model-call contract without changing the current Chat namespace.
 * @param {typeof import('../../../../../../../plugins/types/chat').Chat.call} currentCall
 * @returns {typeof import('../../../../../../../plugins/types-v1/chat').Chat.call}
 */
function __operitCreateV1ChatCall(currentCall) {
    /**
     * Validates a JSON boundary value without widening the legacy metadata contract.
     * @param {unknown} value
     * @returns {import('../../../../../../../plugins/types-v1/toolpkg').ToolPkg.JsonValue}
     */
    function jsonValue(value) {
        if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
        if (typeof value === 'number' && Number.isFinite(value)) return value;
        if (Array.isArray(value)) return value.map(jsonValue);
        if (typeof value === 'object' && value !== null) return jsonObject(value);
        throw new Error('ToolPkg v1 Chat.call received non-JSON metadata.');
    }

    /**
     * Converts a metadata object while retaining every declared JSON field.
     * @param {object} value
     * @returns {import('../../../../../../../plugins/types-v1/toolpkg').ToolPkg.JsonObject}
     */
    function jsonObject(value) {
        if (Object.prototype.toString.call(value) !== '[object Object]') {
            throw new Error('ToolPkg v1 Chat.call metadata must be a JSON object.');
        }
        /** @type {import('../../../../../../../plugins/types-v1/toolpkg').ToolPkg.JsonObject} */
        var output = {};
        for (var [key, entry] of Object.entries(value)) {
            Object.defineProperty(output, key, { value: jsonValue(entry), enumerable: true });
        }
        return output;
    }

    /**
     * Validates the explicit v1 prompt-role vocabulary.
     * @param {string} kind
     * @returns {import('../../../../../../../plugins/types-v1/toolpkg').ToolPkg.PromptTurnKind}
     */
    function promptKind(kind) {
        switch (kind) {
            case 'SYSTEM': case 'USER': case 'ASSISTANT':
            case 'TOOL_CALL': case 'TOOL_RESULT': case 'SUMMARY': return kind;
            default: throw new Error('Unsupported ToolPkg v1 prompt turn kind: ' + kind);
        }
    }

    /**
     * Converts nullable tool names to the optional current field and validates returned enums.
     * @param {import('../../../../../../../plugins/types-v1/chat').Chat.ChatCallOptions} options
     * @returns {Promise<import('../../../../../../../plugins/types-v1/results').ChatCallResultData>}
     */
    return async function call(options) {
        /** @type {import('../../../../../../../plugins/types/chat').Chat.CallOptions} */
        var request = {
            functionType: options.functionType,
            /** Copies each turn so the legacy caller's data stays unchanged. */
            turns: options.turns.map(function(turn) {
                var { toolName, ...rest } = turn;
                if (toolName === null) return rest;
                return Object.assign({}, rest, { toolName: toolName });
            }),
            recordTokenUsage: options.recordTokenUsage,
            enableThinking: options.enableThinking
        };
        var response = await currentCall(request);
        if (response.finishReason !== 'stop' && response.finishReason !== 'tool_call') {
            throw new Error('Unsupported ToolPkg v1 model finish reason: ' + response.finishReason);
        }
        return {
            text: response.text,
            /** Converts the current open role string to the closed legacy role contract. */
            turns: response.turns.map(function(turn) {
                /** @type {import('../../../../../../../plugins/types-v1/toolpkg').ToolPkg.PromptTurn} */
                var converted = {
                    kind: promptKind(turn.kind),
                    content: turn.content,
                    metadata: jsonObject(turn.metadata)
                };
                if (turn.toolName !== undefined) converted.toolName = turn.toolName;
                return converted;
            }),
            finishReason: response.finishReason,
            metadata: jsonObject(response.metadata),
            receivedAt: response.receivedAt
        };
    };
}
