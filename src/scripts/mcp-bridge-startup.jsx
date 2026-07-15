// mcp-bridge-startup.jsx
// Headless After Effects startup bootstrap for adobe-mcp-rs.
#target aftereffects
#targetengine "ae_mcp_bridge"

(function () {
    var BOOTSTRAP_VERSION = "1";
    var runtimeName = "__adobeMcpBridgeRuntime";
    var stateName = "__adobeMcpBridgeBootstrapState";

    function nowIso() {
        var date = new Date();
        function pad(value, width) {
            var text = String(value);
            while (text.length < width) text = "0" + text;
            return text;
        }
        return date.getUTCFullYear() + "-" +
            pad(date.getUTCMonth() + 1, 2) + "-" +
            pad(date.getUTCDate(), 2) + "T" +
            pad(date.getUTCHours(), 2) + ":" +
            pad(date.getUTCMinutes(), 2) + ":" +
            pad(date.getUTCSeconds(), 2) + "." +
            pad(date.getUTCMilliseconds(), 3) + "Z";
    }

    function publish(status, details) {
        $.global[stateName] = {
            bootstrapVersion: BOOTSTRAP_VERSION,
            status: status,
            updatedAt: nowIso(),
            details: details || null
        };
    }

    function clearBootstrapConfig() {
        try {
            delete $.global.__adobeMcpBridgeBootstrapConfig;
        } catch (_deleteBootstrapConfigError) {
            $.global.__adobeMcpBridgeBootstrapConfig = null;
        }
    }

    function findRuntimeFile() {
        var startupFile = new File($.fileName);
        var startupFolder = startupFile.parent;
        var scriptsFolder = startupFolder ? startupFolder.parent : null;
        var candidates = [];
        if (scriptsFolder) {
            candidates.push(new File(scriptsFolder.fsName + "/ScriptUI Panels/mcp-bridge-auto.jsx"));
            candidates.push(new File(scriptsFolder.fsName + "/mcp-bridge-auto.jsx"));
        }
        if (startupFolder) {
            candidates.push(new File(startupFolder.fsName + "/mcp-bridge-auto.jsx"));
        }
        for (var i = 0; i < candidates.length; i++) {
            if (candidates[i].exists) {
                return candidates[i];
            }
        }
        return null;
    }

    try {
        var existing = $.global[runtimeName];
        if (existing && existing.getState) {
            var existingState = existing.getState();
            if (!existingState.running && existing.start) {
                existingState = existing.start();
            } else if (existing.writeHeartbeat) {
                existing.writeHeartbeat();
            }
            publish("already-running", existingState);
            return;
        }

        var runtimeFile = findRuntimeFile();
        if (!runtimeFile) {
            publish("runtime-not-found", {
                startupScript: $.fileName,
                expectedName: "mcp-bridge-auto.jsx"
            });
            return;
        }

        $.global.__adobeMcpBridgeBootstrapConfig = {
            headless: true,
            source: "scripts-startup",
            bootstrapVersion: BOOTSTRAP_VERSION,
            startupScriptPath: $.fileName,
            runtimeScriptPath: runtimeFile.fsName
        };
        publish("loading", { runtimeScriptPath: runtimeFile.fsName });
        try {
            $.evalFile(runtimeFile);
        } finally {
            // The runtime copies this configuration during evaluation. Leaving
            // it global would force later manual launches to stay headless and
            // prevent the optional ScriptUI diagnostics panel from appearing.
            clearBootstrapConfig();
        }

        var loaded = $.global[runtimeName];
        if (!loaded || !loaded.getState) {
            publish("runtime-api-missing", { runtimeScriptPath: runtimeFile.fsName });
            return;
        }
        var loadedState = loaded.getState();
        if (!loadedState.running && loaded.start) {
            loadedState = loaded.start();
        }
        publish("running", loadedState);
    } catch (error) {
        publish("error", {
            message: error.toString(),
            line: error.line || null,
            fileName: error.fileName || null
        });
    }
})();
