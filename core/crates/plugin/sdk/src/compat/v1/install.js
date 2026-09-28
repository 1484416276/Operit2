/** Installs v1 adapters beside the generated v2 implementations without replacing their contracts. */
(function installV1Adapters() {
    var api = globalThis.__operitToolPkgApi;
    var currentFiles = Object.assign({}, Tools.Files);
    var legacyFiles = __operitCreateV1Files(currentFiles);
    for (var method of Object.keys(legacyFiles)) {
        Tools.Files[method] = api.method()
            .between('1.0.0', '2.0.0', legacyFiles[method])
            .since('2.0.0', currentFiles[method])
            .build('Tools.Files.' + method);
    }
    // Legacy workflow calls use the declared prerequisite rather than built-in tools.
    var workflow = __operitCreateV1Workflow(function(method, payload) {
        return api.callDependency('com.operit.workflow', method, payload);
    });
    Tools.Workflow = {};
    for (var name of Object.keys(workflow)) {
        Tools.Workflow[name] = api.method()
            .between('1.0.0', '2.0.0', workflow[name])
            .build('Tools.Workflow.' + name);
    }
    var currentCall = Tools.Chat.call;
    Tools.Chat.call = api.method()
        .between('1.0.1', '2.0.0', __operitCreateV1ChatCall(currentCall))
        .since('2.0.0', currentCall)
        .build('Tools.Chat.call');
})();
