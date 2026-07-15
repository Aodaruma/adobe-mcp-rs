/* Premiere MCP Bridge (CEP) ExtendScript */

if (typeof JSON === "undefined") {
    JSON = {};
}
if (typeof JSON.parse !== "function") {
    JSON.parse = function (text) {
        return eval("(" + text + ")");
    };
}
if (typeof JSON.stringify !== "function") {
    (function () {
        function esc(str) {
            return (str + "")
                .replace(/\\/g, "\\\\")
                .replace(/"/g, '\\"')
                .replace(/\n/g, "\\n")
                .replace(/\r/g, "\\r")
                .replace(/\t/g, "\\t");
        }

        function toJSON(val) {
            if (val === null) {
                return "null";
            }
            var t = typeof val;
            if (t === "number" || t === "boolean") {
                return String(val);
            }
            if (t === "string") {
                return '"' + esc(val) + '"';
            }
            if (val instanceof Array) {
                var items = [];
                for (var i = 0; i < val.length; i++) {
                    items.push(toJSON(val[i]));
                }
                return "[" + items.join(",") + "]";
            }
            if (t === "object") {
                var props = [];
                for (var k in val) {
                    if (val.hasOwnProperty(k) && typeof val[k] !== "function" && typeof val[k] !== "undefined") {
                        props.push('"' + esc(k) + '":' + toJSON(val[k]));
                    }
                }
                return "{" + props.join(",") + "}";
            }
            return "null";
        }

        JSON.stringify = function (value, _replacer, _space) {
            return toJSON(value);
        };
    })();
}

var mcpBridgeState = {
    autoRun: true,
    lastStatus: "idle",
    lastCommand: null,
    lastMessage: null,
    lastError: null,
    lastRunAt: null,
    bridgeRoot: null,
    commandFile: null,
    resultFile: null,
    instanceId: null,
    heartbeatFile: null
};

var mcpBridgeInstanceId = null;

function mcpBridgeRootFolder() {
    var docs = Folder.myDocuments;
    var root = new Folder(docs.fsName + "/pr-mcp-bridge");
    if (!root.exists) {
        root.create();
    }
    return root;
}

function mcpSanitizeBridgePathSegment(value) {
    return String(value || "unknown").replace(/[^A-Za-z0-9_.-]/g, "_");
}

function mcpCreateBridgeInstanceId() {
    var version = "unknown";
    try {
        version = app.version || "unknown";
    } catch (_versionErr) {}
    return "pr-cep-" +
        mcpSanitizeBridgePathSegment(version) +
        "-" +
        (new Date().getTime()) +
        "-" +
        Math.floor(Math.random() * 1000000);
}

function mcpGetBridgeInstanceId() {
    if (!mcpBridgeInstanceId) {
        mcpBridgeInstanceId = mcpCreateBridgeInstanceId();
    }
    return mcpBridgeInstanceId;
}

function mcpBridgeInstancesFolder() {
    var root = mcpBridgeRootFolder();
    var folder = new Folder(root.fsName + "/instances");
    if (!folder.exists) {
        folder.create();
    }
    return folder;
}

function mcpBridgeInstanceFolder() {
    var instances = mcpBridgeInstancesFolder();
    var folder = new Folder(instances.fsName + "/" + mcpGetBridgeInstanceId());
    if (!folder.exists) {
        folder.create();
    }
    return folder;
}

function mcpBridgeCommandFile() {
    var root = mcpBridgeRootFolder();
    return new File(root.fsName + "/pr_command.json");
}

function mcpBridgeResultFile() {
    var root = mcpBridgeRootFolder();
    return new File(root.fsName + "/pr_mcp_result.json");
}

function mcpBridgeInstanceCommandFile() {
    var folder = mcpBridgeInstanceFolder();
    return new File(folder.fsName + "/pr_command.json");
}

function mcpBridgeInstanceResultFile() {
    var folder = mcpBridgeInstanceFolder();
    return new File(folder.fsName + "/pr_mcp_result.json");
}

function mcpBridgeHeartbeatFile() {
    var folder = mcpBridgeInstanceFolder();
    return new File(folder.fsName + "/heartbeat.json");
}

var mcpBridgeAtomicWriteCounter = 0;

function mcpBridgeCleanupAtomicResidues(file) {
    try {
        var prefix = "." + file.name + ".tmp-";
        var backupPrefix = "." + file.name + ".bak-";
        var now = new Date().getTime();
        var entries = file.parent.getFiles();
        for (var i = 0; i < entries.length; i++) {
            var entry = entries[i];
            if (!(entry instanceof File) ||
                (entry.name.indexOf(prefix) !== 0 && entry.name.indexOf(backupPrefix) !== 0)) {
                continue;
            }
            var modified = entry.modified ? entry.modified.getTime() : now;
            if (now - modified >= 60 * 60 * 1000) {
                try { entry.remove(); } catch (_cleanupErr) {}
            }
        }
    } catch (_scanErr) {}
}

function mcpBridgeAtomicWriteText(file, text) {
    if (file.parent && !file.parent.exists && !file.parent.create()) {
        throw new Error("Failed to create directory: " + file.parent.fsName);
    }
    mcpBridgeCleanupAtomicResidues(file);
    mcpBridgeAtomicWriteCounter += 1;
    var suffix = new Date().getTime() + "-" + mcpBridgeAtomicWriteCounter + "-" +
        Math.floor(Math.random() * 0x7fffffff).toString(16);
    var tempFile = new File(file.parent.fsName + "/." + file.name + ".tmp-" + suffix);
    tempFile.encoding = "UTF-8";
    if (!tempFile.open("w")) {
        throw new Error("Failed to open temporary file: " + tempFile.fsName);
    }
    if (!tempFile.write(text)) {
        try { tempFile.close(); } catch (_writeCloseErr) {}
        try { tempFile.remove(); } catch (_writeRemoveErr) {}
        throw new Error("Failed to write temporary file: " + tempFile.fsName);
    }
    if (!tempFile.close()) {
        try { tempFile.remove(); } catch (_closeRemoveErr) {}
        throw new Error("Failed to flush temporary file: " + tempFile.fsName);
    }

    // Some ExtendScript hosts overwrite on rename. Prefer that atomic path first.
    if (tempFile.rename(file.name)) {
        return;
    }

    // Legacy hosts may reject overwrite. Preserve the old valid file as a backup,
    // publish the temp file, then remove the backup. Readers tolerate this short gap.
    var backupFile = new File(file.parent.fsName + "/." + file.name + ".bak-" + suffix);
    var hadTarget = file.exists;
    if (hadTarget && !file.rename(backupFile.name)) {
        try { tempFile.remove(); } catch (_targetRemoveErr) {}
        throw new Error("Failed to preserve previous file: " + file.fsName);
    }
    if (!tempFile.rename(file.name)) {
        if (hadTarget && backupFile.exists) {
            try { backupFile.rename(file.name); } catch (_rollbackErr) {}
        }
        try { tempFile.remove(); } catch (_publishRemoveErr) {}
        throw new Error("Failed to publish temporary file: " + file.fsName);
    }
    if (backupFile.exists) {
        try { backupFile.remove(); } catch (_backupRemoveErr) {}
    }
}

function mcpBridgeWriteJsonFile(file, value) {
    mcpBridgeAtomicWriteText(file, JSON.stringify(value, null, 2));
}

function mcpBridgeWriteHeartbeat() {
    var root = mcpBridgeRootFolder();
    var cmd = mcpBridgeInstanceCommandFile();
    var res = mcpBridgeInstanceResultFile();
    var heartbeat = mcpBridgeHeartbeatFile();
    var appVersion = "";
    try {
        appVersion = app.version ? String(app.version) : "";
    } catch (_versionErr) {}
    var updatedAt = new Date().toISOString();
    var payload = {
        protocolVersion: 1,
        instanceId: mcpGetBridgeInstanceId(),
        hostId: "premiere",
        appName: "Premiere Pro",
        appVersion: appVersion,
        displayName: appVersion ? "Premiere Pro " + appVersion : "Premiere Pro CEP",
        projectPath: null,
        status: mcpBridgeState.lastStatus || "idle",
        currentRequestId: mcpBridgeState.currentRequestId || null,
        bridgeRuntime: "cep-extendscript",
        capabilities: ["run-jsx", "projects.read", "sequences.read", "tracks.read", "export"],
        bridgeRoot: root.fsName,
        commandFile: cmd.fsName,
        resultFile: res.fsName,
        lastHeartbeatAt: updatedAt,
        updatedAt: updatedAt,
        heartbeatPath: heartbeat.fsName
    };
    mcpBridgeWriteJsonFile(heartbeat, payload);
}

function mcpBridgeGetState() {
    var root = mcpBridgeRootFolder();
    var cmd = mcpBridgeInstanceCommandFile();
    var res = mcpBridgeInstanceResultFile();
    var heartbeat = mcpBridgeHeartbeatFile();
    mcpBridgeState.bridgeRoot = root.fsName;
    mcpBridgeState.commandFile = cmd.fsName;
    mcpBridgeState.resultFile = res.fsName;
    mcpBridgeState.instanceId = mcpGetBridgeInstanceId();
    mcpBridgeState.heartbeatFile = heartbeat.fsName;
    mcpBridgeWriteHeartbeat();
    return JSON.stringify(mcpBridgeState);
}

function mcpBridgeSetAutoRun(enabled) {
    mcpBridgeState.autoRun = enabled === true;
    return mcpBridgeGetState();
}

function mcpExecuteCommand(command, args) {
    if (!command) {
        throw new Error("command is required");
    }
    var handler = $.global[command];
    if (typeof handler === "function") {
        return handler(args || {});
    }
    return JSON.stringify({
        status: "error",
        message: "Unknown command: " + command
    });
}

function mcpWriteResult(raw, resultFile) {
    resultFile = resultFile || mcpBridgeResultFile();
    mcpBridgeAtomicWriteText(resultFile, raw);
    var globalResultFile = mcpBridgeResultFile();
    if (globalResultFile.fsName !== resultFile.fsName) {
        try { mcpBridgeAtomicWriteText(globalResultFile, raw); } catch (_rootWriteErr) {}
    }
}

function mcpUpdateCommandStatus(commandFile, payload, status) {
    payload.status = status;
    mcpBridgeWriteJsonFile(commandFile, payload);
}

function mcpReadCommandPayload(commandFile) {
    if (!commandFile.exists) {
        return null;
    }
    commandFile.encoding = "UTF-8";
    if (!commandFile.open("r")) {
        throw new Error("Failed to open command file: " + commandFile.fsName);
    }
    var content = commandFile.read();
    commandFile.close();
    if (!content) {
        return null;
    }
    return JSON.parse(content);
}

function mcpFindCommandContext() {
    var instanceCommandFile = mcpBridgeInstanceCommandFile();
    var instancePayload = mcpReadCommandPayload(instanceCommandFile);
    if (instancePayload && instancePayload.command) {
        return {
            commandFile: instanceCommandFile,
            resultFile: mcpBridgeInstanceResultFile(),
            payload: instancePayload
        };
    }
    var globalCommandFile = mcpBridgeCommandFile();
    var globalPayload = mcpReadCommandPayload(globalCommandFile);
    if (globalPayload && globalPayload.command) {
        return {
            commandFile: globalCommandFile,
            resultFile: mcpBridgeResultFile(),
            payload: globalPayload
        };
    }
    return null;
}

function mcpBridgeCheck() {
    try {
        mcpBridgeWriteHeartbeat();
        if (!mcpBridgeState.autoRun) {
            return mcpBridgeGetState();
        }
        var context = null;
        try {
            context = mcpFindCommandContext();
        } catch (parseErr) {
            mcpBridgeState.lastStatus = "error";
            mcpBridgeState.lastError = "Invalid command JSON";
            return mcpBridgeGetState();
        }

        if (!context) {
            mcpBridgeState.lastStatus = "waiting";
            return mcpBridgeGetState();
        }

        var payload = context.payload;
        if (!payload || !payload.command) {
            mcpBridgeState.lastStatus = "waiting";
            return mcpBridgeGetState();
        }

        var status = payload.status || "";
        if (status.toLowerCase() !== "pending") {
            mcpBridgeState.lastStatus = "waiting";
            return mcpBridgeGetState();
        }

        var command = payload.command;
        var args = payload.args || {};
        var rawResult = "";
        try {
            mcpBridgeState.currentRequestId = payload.requestId || payload.request_id || null;
            mcpUpdateCommandStatus(context.commandFile, payload, "running");
            rawResult = mcpExecuteCommand(command, args);
        } catch (err) {
            rawResult = JSON.stringify({
                status: "error",
                message: err.toString()
            });
        }

        var resultString = (typeof rawResult === "string")
            ? rawResult
            : JSON.stringify(rawResult);

        try {
            var resultObj = JSON.parse(resultString);
            if (resultObj) {
                resultObj._commandExecuted = command;
                if (payload.requestId || payload.request_id) {
                    resultObj._requestId = payload.requestId || payload.request_id;
                }
                resultObj._responseTimestamp = new Date().toISOString();
                resultString = JSON.stringify(resultObj, null, 2);
            }
        } catch (_e) {}

        mcpWriteResult(resultString, context.resultFile);

        var finalStatus = "completed";
        try {
            var parsed = JSON.parse(resultString);
            if (parsed && parsed.status === "error") {
                finalStatus = "error";
            }
        } catch (_e2) {}

        mcpUpdateCommandStatus(context.commandFile, payload, finalStatus);

        mcpBridgeState.lastStatus = finalStatus;
        mcpBridgeState.lastCommand = command;
        mcpBridgeState.lastMessage = "Executed command: " + command;
        mcpBridgeState.lastError = finalStatus === "error" ? "Command failed" : null;
        mcpBridgeState.lastRunAt = new Date().toISOString();
        mcpBridgeState.currentRequestId = null;
    } catch (err) {
        mcpBridgeState.lastStatus = "error";
        mcpBridgeState.lastError = err.toString();
        mcpBridgeState.currentRequestId = null;
    }

    return mcpBridgeGetState();
}

function ping(_args) {
    return JSON.stringify({
        status: "success",
        message: "ok"
    });
}

function mcpGetSequenceCollection() {
    if (!app || !app.project) {
        return null;
    }
    return app.project.sequences;
}

function mcpGetSequenceCount(sequences) {
    if (!sequences) {
        return 0;
    }
    if (typeof sequences.numSequences === "number") {
        return sequences.numSequences;
    }
    if (typeof sequences.length === "number") {
        return sequences.length;
    }
    return 0;
}

function mcpGetSequenceId(sequence) {
    if (!sequence) {
        return null;
    }
    try {
        if (sequence.sequenceID !== undefined && sequence.sequenceID !== null) {
            return sequence.sequenceID;
        }
    } catch (_e) {}
    return null;
}

function mcpGetSequenceByIndex(index) {
    var sequences = mcpGetSequenceCollection();
    var count = mcpGetSequenceCount(sequences);
    if (!sequences || count === 0 || index === null || index === undefined) {
        return null;
    }
    var idxNumber = Number(index);
    if (isNaN(idxNumber)) {
        return null;
    }
    var oneBased = idxNumber - 1;
    if (oneBased >= 0 && oneBased < count) {
        return sequences[oneBased];
    }
    if (idxNumber >= 0 && idxNumber < count) {
        return sequences[idxNumber];
    }
    return null;
}

function mcpFindSequenceByName(name) {
    if (!name) {
        return null;
    }
    var sequences = mcpGetSequenceCollection();
    var count = mcpGetSequenceCount(sequences);
    if (!sequences || count === 0) {
        return null;
    }
    var needle = name.toLowerCase();
    for (var i = 0; i < count; i++) {
        var seq = sequences[i];
        if (seq && seq.name && seq.name.toLowerCase() === needle) {
            return seq;
        }
    }
    return null;
}

function mcpFindSequenceIndex(target) {
    var sequences = mcpGetSequenceCollection();
    var count = mcpGetSequenceCount(sequences);
    if (!sequences || count === 0 || !target) {
        return null;
    }
    var targetId = mcpGetSequenceId(target);
    for (var i = 0; i < count; i++) {
        var seq = sequences[i];
        if (!seq) {
            continue;
        }
        if (seq === target) {
            return i + 1;
        }
        var seqId = mcpGetSequenceId(seq);
        if (targetId !== null && seqId !== null && seqId === targetId) {
            return i + 1;
        }
    }
    return null;
}

function mcpResolveSequence(args) {
    args = args || {};
    if (args.sequenceName) {
        var byName = mcpFindSequenceByName(args.sequenceName);
        if (byName) {
            return byName;
        }
    }
    if (args.sequenceIndex !== undefined && args.sequenceIndex !== null) {
        var byIndex = mcpGetSequenceByIndex(args.sequenceIndex);
        if (byIndex) {
            return byIndex;
        }
    }
    if (app && app.project) {
        return app.project.activeSequence;
    }
    return null;
}

function mcpCollectionLength(collection) {
    if (!collection) {
        return 0;
    }
    var keys = ["length", "numTracks", "numItems", "numSequences"];
    for (var i = 0; i < keys.length; i++) {
        var value = collection[keys[i]];
        if (typeof value === "number" && !isNaN(value)) {
            return value;
        }
    }
    return 0;
}

function mcpGetSequencePlayhead(sequence) {
    if (!sequence || !sequence.getPlayerPosition) {
        return null;
    }
    try {
        var position = sequence.getPlayerPosition();
        return {
            seconds: position && position.seconds !== undefined ? position.seconds : null,
            ticks: position && position.ticks !== undefined ? String(position.ticks) : null
        };
    } catch (_e) {
        return null;
    }
}

function mcpSummarizeSequence(sequence) {
    if (!sequence) {
        return null;
    }
    return {
        name: sequence.name || "",
        index: mcpFindSequenceIndex(sequence),
        id: mcpGetSequenceId(sequence),
        videoTrackCount: mcpCollectionLength(sequence.videoTracks),
        audioTrackCount: mcpCollectionLength(sequence.audioTracks),
        playhead: mcpGetSequencePlayhead(sequence)
    };
}

function getProjectInfo(_args) {
    if (!app || !app.project) {
        return JSON.stringify({
            status: "error",
            message: "Premiere project is not available."
        });
    }
    var sequences = mcpGetSequenceCollection();
    return JSON.stringify({
        status: "success",
        project: {
            name: app.project.name || "",
            path: app.project.path || null,
            sequenceCount: mcpGetSequenceCount(sequences),
            activeSequence: mcpSummarizeSequence(app.project.activeSequence)
        }
    });
}

function getSequenceInfo(args) {
    var seq = mcpResolveSequence(args || {});
    if (!seq) {
        return JSON.stringify({
            status: "error",
            message: "Sequence not found."
        });
    }
    return JSON.stringify({
        status: "success",
        sequence: mcpSummarizeSequence(seq)
    });
}

function listSequences(_args) {
    var sequences = mcpGetSequenceCollection();
    var count = mcpGetSequenceCount(sequences);
    if (!sequences || count === 0) {
        return JSON.stringify({
            status: "error",
            message: "No sequences found in the current project."
        });
    }

    var items = [];
    for (var i = 0; i < count; i++) {
        var seq = sequences[i];
        if (!seq) {
            continue;
        }
        items.push({
            index: i + 1,
            name: seq.name || "",
            id: mcpGetSequenceId(seq)
        });
    }

    return JSON.stringify({
        status: "success",
        total: count,
        sequences: items
    });
}

function getActiveSequence(_args) {
    if (!app || !app.project) {
        return JSON.stringify({
            status: "error",
            message: "Premiere project is not available."
        });
    }
    var seq = app.project.activeSequence;
    if (!seq) {
        return JSON.stringify({
            status: "error",
            message: "No active sequence."
        });
    }
    var index = mcpFindSequenceIndex(seq);
    return JSON.stringify({
        status: "success",
        sequence: {
            name: seq.name || "",
            index: index,
            id: mcpGetSequenceId(seq)
        }
    });
}

function setPlayheadTime(args) {
    args = args || {};
    var seq = mcpResolveSequence(args);
    if (!seq) {
        return JSON.stringify({
            status: "error",
            message: "Sequence not found."
        });
    }

    var timeTicks = null;
    if (args.timeTicks !== undefined && args.timeTicks !== null) {
        var ticks = Number(args.timeTicks);
        if (!isNaN(ticks)) {
            timeTicks = ticks;
        }
    }

    var timeSeconds = null;
    if (args.timeSeconds !== undefined && args.timeSeconds !== null) {
        var seconds = Number(args.timeSeconds);
        if (!isNaN(seconds)) {
            timeSeconds = seconds;
        }
    }

    if (timeTicks === null && timeSeconds === null) {
        return JSON.stringify({
            status: "error",
            message: "timeSeconds or timeTicks is required."
        });
    }

    if (timeTicks === null) {
        var t = new Time();
        t.seconds = timeSeconds;
        timeTicks = t.ticks;
    }

    try {
        seq.setPlayerPosition(timeTicks);
    } catch (err) {
        return JSON.stringify({
            status: "error",
            message: err.toString()
        });
    }

    return JSON.stringify({
        status: "success",
        sequenceName: seq.name || "",
        timeSeconds: timeSeconds,
        timeTicks: timeTicks
    });
}

function exportSequence(args) {
    args = args || {};
    var seq = mcpResolveSequence(args);
    if (!seq) {
        return JSON.stringify({
            status: "error",
            message: "Sequence not found."
        });
    }

    var outputPath = args.outputPath;
    if (!outputPath) {
        return JSON.stringify({
            status: "error",
            message: "outputPath is required."
        });
    }

    var presetPath = args.presetPath;
    if (!presetPath) {
        return JSON.stringify({
            status: "error",
            message: "presetPath is required."
        });
    }

    var workAreaType = 0;
    if (args.workAreaType !== undefined && args.workAreaType !== null) {
        var workArea = Number(args.workAreaType);
        if (!isNaN(workArea)) {
            workAreaType = workArea;
        }
    }

    var result = null;
    try {
        result = seq.exportAsMediaDirect(outputPath, presetPath, workAreaType);
    } catch (err) {
        return JSON.stringify({
            status: "error",
            message: err.toString()
        });
    }

    return JSON.stringify({
        status: "success",
        result: result,
        sequenceName: seq.name || "",
        outputPath: outputPath,
        presetPath: presetPath,
        workAreaType: workAreaType
    });
}
