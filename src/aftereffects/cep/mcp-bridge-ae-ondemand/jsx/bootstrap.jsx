// Host-side bootstrap for the on-demand bridge. Evaluated by main.js through
// CSInterface.evalScript. It loads mcp-bridge-auto.jsx in on-demand mode (no
// app.scheduleTask timers) and reports "ok|instanceId|runtimeId|engine|bridgeRoot".
(function () {
    function fail(message) {
        return "error|" + message;
    }

    function findRuntimeFile() {
        var roots = [];
        var i;
        var candidate;
        try {
            roots.push(Folder.appPackage);
            if (Folder.appPackage.parent) {
                roots.push(Folder.appPackage.parent);
            }
        } catch (rootError) {}
        for (i = 0; i < roots.length; i++) {
            candidate = new File(roots[i].fsName + "/Scripts/ScriptUI Panels/mcp-bridge-auto.jsx");
            if (candidate.exists) {
                return candidate;
            }
            candidate = new File(roots[i].fsName + "/Scripts/mcp-bridge-auto.jsx");
            if (candidate.exists) {
                return candidate;
            }
        }
        return null;
    }

    try {
        var runtimeName = "__adobeMcpBridgeRuntime";
        var runtime = $.global[runtimeName];
        if (!runtime || !runtime.getState) {
            var runtimeFile = findRuntimeFile();
            if (!runtimeFile) {
                return fail("runtime-not-found");
            }
            $.global.__adobeMcpBridgeBootstrapConfig = {
                headless: true,
                onDemand: true,
                source: "cep-on-demand",
                runtimeScriptPath: runtimeFile.fsName
            };
            try {
                $.evalFile(runtimeFile);
            } finally {
                try {
                    delete $.global.__adobeMcpBridgeBootstrapConfig;
                } catch (deleteError) {
                    $.global.__adobeMcpBridgeBootstrapConfig = null;
                }
            }
            runtime = $.global[runtimeName];
            if (!runtime || !runtime.getState) {
                return fail("runtime-api-missing");
            }
        }
        var state = runtime.getState();
        if (String(state.schedulerMode) !== "on-demand") {
            // The Startup loader (timer mode) already owns the runtime.
            return fail("timer-runtime-already-loaded:" + state.schedulerMode);
        }
        if (!state.running && runtime.start) {
            state = runtime.start();
        } else if (runtime.writeHeartbeat) {
            runtime.writeHeartbeat();
        }
        return "ok|" + state.instanceId + "|" + state.runtimeId + "|" + String($.engineName) +
            "|" + Folder.myDocuments.fsName + "/ae-mcp-bridge";
    } catch (err) {
        return fail(String(err) + " (line " + err.line + ")");
    }
})()
