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
    resultFile: null
};

function mcpBridgeRootFolder() {
    var docs = Folder.myDocuments;
    var root = new Folder(docs.fsName + "/pr-mcp-bridge");
    if (!root.exists) {
        root.create();
    }
    return root;
}

function mcpBridgeCommandFile() {
    var root = mcpBridgeRootFolder();
    return new File(root.fsName + "/pr_command.json");
}

function mcpBridgeResultFile() {
    var root = mcpBridgeRootFolder();
    return new File(root.fsName + "/pr_mcp_result.json");
}

function mcpBridgeGetState() {
    var root = mcpBridgeRootFolder();
    var cmd = mcpBridgeCommandFile();
    var res = mcpBridgeResultFile();
    mcpBridgeState.bridgeRoot = root.fsName;
    mcpBridgeState.commandFile = cmd.fsName;
    mcpBridgeState.resultFile = res.fsName;
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

function mcpWriteResult(raw) {
    var resultFile = mcpBridgeResultFile();
    resultFile.encoding = "UTF-8";
    if (!resultFile.open("w")) {
        throw new Error("Failed to open result file: " + resultFile.fsName);
    }
    resultFile.write(raw);
    resultFile.close();
}

function mcpUpdateCommandStatus(payload, status) {
    payload.status = status;
    var cmdFile = mcpBridgeCommandFile();
    cmdFile.encoding = "UTF-8";
    if (!cmdFile.open("w")) {
        throw new Error("Failed to open command file: " + cmdFile.fsName);
    }
    cmdFile.write(JSON.stringify(payload, null, 2));
    cmdFile.close();
}

function mcpBridgeCheck() {
    try {
        if (!mcpBridgeState.autoRun) {
            return mcpBridgeGetState();
        }
        var cmdFile = mcpBridgeCommandFile();
        if (!cmdFile.exists) {
            mcpBridgeState.lastStatus = "waiting";
            return mcpBridgeGetState();
        }

        cmdFile.encoding = "UTF-8";
        if (!cmdFile.open("r")) {
            mcpBridgeState.lastStatus = "error";
            mcpBridgeState.lastError = "Failed to open command file";
            return mcpBridgeGetState();
        }
        var content = cmdFile.read();
        cmdFile.close();
        if (!content) {
            mcpBridgeState.lastStatus = "waiting";
            return mcpBridgeGetState();
        }

        var payload = null;
        try {
            payload = JSON.parse(content);
        } catch (parseErr) {
            mcpBridgeState.lastStatus = "error";
            mcpBridgeState.lastError = "Invalid command JSON";
            return mcpBridgeGetState();
        }

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
            mcpUpdateCommandStatus(payload, "running");
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
                resultObj._responseTimestamp = new Date().toISOString();
                resultString = JSON.stringify(resultObj, null, 2);
            }
        } catch (_e) {}

        mcpWriteResult(resultString);

        var finalStatus = "completed";
        try {
            var parsed = JSON.parse(resultString);
            if (parsed && parsed.status === "error") {
                finalStatus = "error";
            }
        } catch (_e2) {}

        mcpUpdateCommandStatus(payload, finalStatus);

        mcpBridgeState.lastStatus = finalStatus;
        mcpBridgeState.lastCommand = command;
        mcpBridgeState.lastMessage = "Executed command: " + command;
        mcpBridgeState.lastError = finalStatus === "error" ? "Command failed" : null;
        mcpBridgeState.lastRunAt = new Date().toISOString();
    } catch (err) {
        mcpBridgeState.lastStatus = "error";
        mcpBridgeState.lastError = err.toString();
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
