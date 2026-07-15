/* Illustrator MCP Bridge (CEP) ExtendScript */

var AI_MCP_BRIDGE_VERSION = "0.4.4";

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

var aiMcpBridgeState = {
    autoRun: true,
    lastStatus: "idle",
    lastCommand: null,
    lastMessage: null,
    lastError: null,
    lastRunAt: null,
    bridgeRoot: null,
    commandFile: null,
    resultFile: null,
    heartbeatFile: null,
    instanceId: null
};

var aiMcpInstanceId = null;
var aiMcpCurrentRequestId = "";
var aiMcpCurrentCommand = "";

function aiPad2(value) {
    return value < 10 ? "0" + value : String(value);
}

function aiIsoDate(date) {
    return (
        date.getUTCFullYear() +
        "-" +
        aiPad2(date.getUTCMonth() + 1) +
        "-" +
        aiPad2(date.getUTCDate()) +
        "T" +
        aiPad2(date.getUTCHours()) +
        ":" +
        aiPad2(date.getUTCMinutes()) +
        ":" +
        aiPad2(date.getUTCSeconds()) +
        "Z"
    );
}

function aiString(value) {
    if (value === null || typeof value === "undefined") {
        return null;
    }
    try {
        return String(value);
    } catch (_e) {
        return null;
    }
}

function aiEnsureFolder(folder) {
    if (!folder.exists) {
        if (!folder.create()) {
            throw new Error("Failed to create folder: " + folder.fsName);
        }
    }
    return folder;
}

function aiBridgeRootFolder() {
    var root = new Folder(Folder.myDocuments.fsName + "/ai-mcp-bridge");
    aiEnsureFolder(root);
    aiEnsureFolder(new Folder(root.fsName + "/instances"));
    aiEnsureFolder(new Folder(root.fsName + "/registry"));
    return root;
}

function aiRootCommandFile() {
    return new File(aiBridgeRootFolder().fsName + "/ai_command.json");
}

function aiRootResultFile() {
    return new File(aiBridgeRootFolder().fsName + "/ai_mcp_result.json");
}

function aiSanitizeIdPart(value) {
    var raw = aiString(value) || "unknown";
    var out = "";
    for (var i = 0; i < raw.length; i++) {
        var ch = raw.charAt(i);
        if (
            (ch >= "a" && ch <= "z") ||
            (ch >= "A" && ch <= "Z") ||
            (ch >= "0" && ch <= "9") ||
            ch === "-" ||
            ch === "_"
        ) {
            out += ch;
        } else {
            out += "-";
        }
    }
    return out;
}

function aiGetInstanceId() {
    if (!aiMcpInstanceId) {
        var version = "unknown";
        try {
            version = app.version || "unknown";
        } catch (_e) {}
        var rand = Math.floor(Math.random() * 1000000);
        aiMcpInstanceId =
            "ai-" +
            aiSanitizeIdPart(version) +
            "-" +
            new Date().getTime() +
            "-" +
            rand;
    }
    return aiMcpInstanceId;
}

function aiInstanceFolder() {
    var folder = new Folder(aiBridgeRootFolder().fsName + "/instances/" + aiGetInstanceId());
    aiEnsureFolder(folder);
    return folder;
}

function aiInstanceCommandFile() {
    return new File(aiInstanceFolder().fsName + "/ai_command.json");
}

function aiInstanceResultFile() {
    return new File(aiInstanceFolder().fsName + "/ai_mcp_result.json");
}

function aiHeartbeatFile() {
    return new File(aiInstanceFolder().fsName + "/heartbeat.json");
}

function aiReadFile(file) {
    file.encoding = "UTF-8";
    if (!file.open("r")) {
        throw new Error("Failed to open file for reading: " + file.fsName);
    }
    var text = file.read();
    file.close();
    return text;
}

var aiAtomicWriteCounter = 0;

function aiCleanupAtomicResidues(file) {
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

function aiWriteFile(file, text) {
    if (file.parent && !file.parent.exists) {
        aiEnsureFolder(file.parent);
    }
    aiCleanupAtomicResidues(file);
    aiAtomicWriteCounter += 1;
    var suffix = new Date().getTime() + "-" + aiAtomicWriteCounter + "-" +
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

    if (tempFile.rename(file.name)) {
        return;
    }

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

function aiGetActiveDocumentPath() {
    try {
        if (app.documents.length > 0 && app.activeDocument && app.activeDocument.fullName) {
            return app.activeDocument.fullName.fsName;
        }
    } catch (_e) {}
    return null;
}

function aiGetInstanceMetadata() {
    var root = aiBridgeRootFolder();
    var commandFile = aiInstanceCommandFile();
    var resultFile = aiInstanceResultFile();
    var heartbeatFile = aiHeartbeatFile();
    var version = null;
    try {
        version = app.version || null;
    } catch (_versionErr) {}

    return {
        protocolVersion: 1,
        instanceId: aiGetInstanceId(),
        appName: "Illustrator",
        appVersion: version,
        displayName: version ? "Adobe Illustrator " + version : "Adobe Illustrator",
        projectPath: aiGetActiveDocumentPath(),
        status: aiMcpCurrentRequestId ? "running" : "idle",
        currentRequestId: aiMcpCurrentRequestId || null,
        bridgeRoot: root.fsName,
        commandFile: commandFile.fsName,
        resultFile: resultFile.fsName,
        lastHeartbeatAt: aiIsoDate(new Date()),
        updatedAt: aiIsoDate(new Date()),
        hostId: "illustrator",
        bridgeRuntime: "cep-extendscript",
        bridgeVersion: AI_MCP_BRIDGE_VERSION,
        capabilities: [
            "run-jsx",
            "documents.list",
            "documents.active",
            "artboards.list",
            "layers.list",
            "documents.export"
        ],
        heartbeatPath: heartbeatFile.fsName
    };
}

function aiWriteHeartbeat() {
    var metadata = aiGetInstanceMetadata();
    aiWriteFile(aiHeartbeatFile(), JSON.stringify(metadata, null, 2));
    return metadata;
}

function aiMcpBridgeGetState() {
    try {
        var metadata = aiWriteHeartbeat();
        var rootCommandFile = aiRootCommandFile();
        var rootResultFile = aiRootResultFile();
        aiMcpBridgeState.bridgeRoot = metadata.bridgeRoot;
        aiMcpBridgeState.commandFile = metadata.commandFile || rootCommandFile.fsName;
        aiMcpBridgeState.resultFile = metadata.resultFile || rootResultFile.fsName;
        aiMcpBridgeState.heartbeatFile = metadata.heartbeatPath;
        aiMcpBridgeState.instanceId = metadata.instanceId;
    } catch (err) {
        aiMcpBridgeState.lastStatus = "error";
        aiMcpBridgeState.lastError = err.toString();
    }
    return JSON.stringify(aiMcpBridgeState);
}

function aiMcpBridgeSetAutoRun(enabled) {
    aiMcpBridgeState.autoRun = enabled === true;
    return aiMcpBridgeGetState();
}

function aiReadCommandPayload(file) {
    if (!file.exists) {
        return null;
    }
    var text = aiReadFile(file);
    if (!text) {
        return null;
    }
    return JSON.parse(text);
}

function aiFindPendingCommand() {
    var candidates = [aiInstanceCommandFile(), aiRootCommandFile()];
    for (var i = 0; i < candidates.length; i++) {
        var file = candidates[i];
        var payload = null;
        try {
            payload = aiReadCommandPayload(file);
        } catch (readErr) {
            aiMcpBridgeState.lastStatus = "error";
            aiMcpBridgeState.lastError = "Invalid command JSON: " + readErr.toString();
            return null;
        }
        if (!payload || !payload.command) {
            continue;
        }
        var status = (payload.status || "").toLowerCase();
        if (status === "pending") {
            return {
                file: file,
                payload: payload,
                isInstance: file.fsName === aiInstanceCommandFile().fsName
            };
        }
    }
    return null;
}

function aiUpdateCommandStatus(commandFile, payload, status) {
    payload.status = status;
    aiWriteFile(commandFile, JSON.stringify(payload, null, 2));
}

function aiWriteResult(targetFile, resultString) {
    aiWriteFile(targetFile, resultString);

    var rootResult = aiRootResultFile();
    if (targetFile.fsName !== rootResult.fsName) {
        try {
            aiWriteFile(rootResult, resultString);
        } catch (_rootWriteErr) {}
    }
}

function aiResultToString(value) {
    if (typeof value === "string") {
        try {
            JSON.parse(value);
            return value;
        } catch (_parseErr) {
            return JSON.stringify({
                status: "success",
                result: value
            });
        }
    }
    if (typeof value === "undefined") {
        return JSON.stringify({
            status: "success",
            result: null
        });
    }
    if (value && typeof value === "object") {
        return JSON.stringify(value);
    }
    return JSON.stringify({
        status: "success",
        result: value
    });
}

function aiError(message) {
    return JSON.stringify({
        status: "error",
        message: message
    });
}

function aiGetDocumentCount() {
    try {
        return app.documents.length || 0;
    } catch (_e) {
        return 0;
    }
}

function aiGetDocumentByIndex(index) {
    var count = aiGetDocumentCount();
    if (index === null || typeof index === "undefined" || count === 0) {
        return null;
    }
    var n = Number(index);
    if (isNaN(n)) {
        return null;
    }
    if (n >= 1 && n <= count) {
        return app.documents[n - 1];
    }
    if (n >= 0 && n < count) {
        return app.documents[n];
    }
    return null;
}

function aiFindDocumentByName(name) {
    if (!name) {
        return null;
    }
    var needle = String(name).toLowerCase();
    var count = aiGetDocumentCount();
    for (var i = 0; i < count; i++) {
        var doc = app.documents[i];
        if (doc && doc.name && String(doc.name).toLowerCase() === needle) {
            return doc;
        }
    }
    return null;
}

function aiResolveDocument(args) {
    args = args || {};
    if (args.documentName) {
        var byName = aiFindDocumentByName(args.documentName);
        if (byName) {
            return byName;
        }
    }
    if (args.documentIndex !== null && typeof args.documentIndex !== "undefined") {
        var byIndex = aiGetDocumentByIndex(args.documentIndex);
        if (byIndex) {
            return byIndex;
        }
    }
    if (aiGetDocumentCount() > 0) {
        return app.activeDocument;
    }
    return null;
}

function aiFindDocumentIndex(target) {
    if (!target) {
        return null;
    }
    var count = aiGetDocumentCount();
    for (var i = 0; i < count; i++) {
        if (app.documents[i] === target) {
            return i + 1;
        }
    }
    return null;
}

function aiCollectionLength(collection) {
    if (!collection) {
        return 0;
    }
    try {
        if (typeof collection.length === "number") {
            return collection.length;
        }
    } catch (_e) {}
    return 0;
}

function aiDocumentPath(doc) {
    try {
        if (doc.fullName) {
            return doc.fullName.fsName;
        }
    } catch (_e) {}
    return null;
}

function aiDocumentFolder(doc) {
    try {
        if (doc.path) {
            return doc.path.fsName;
        }
    } catch (_e) {}
    return null;
}

function aiDocumentSummary(doc) {
    if (!doc) {
        return null;
    }
    var active = false;
    try {
        active = app.activeDocument === doc;
    } catch (_activeErr) {}
    var colorSpace = null;
    try {
        colorSpace = String(doc.documentColorSpace);
    } catch (_colorErr) {}
    var rulerUnits = null;
    try {
        rulerUnits = String(doc.rulerUnits);
    } catch (_unitsErr) {}
    var saved = null;
    try {
        saved = doc.saved === true;
    } catch (_savedErr) {}
    var selectionCount = 0;
    try {
        selectionCount = doc.selection ? doc.selection.length : 0;
    } catch (_selectionErr) {}

    return {
        index: aiFindDocumentIndex(doc),
        name: doc.name || "",
        path: aiDocumentPath(doc),
        folder: aiDocumentFolder(doc),
        saved: saved,
        active: active,
        width: Number(doc.width || 0),
        height: Number(doc.height || 0),
        colorSpace: colorSpace,
        rulerUnits: rulerUnits,
        artboardCount: aiCollectionLength(doc.artboards),
        layerCount: aiCollectionLength(doc.layers),
        selectionCount: selectionCount
    };
}

function ping(_args) {
    return JSON.stringify({
        status: "success",
        message: "ok",
        appName: "Illustrator",
        appVersion: app.version || null
    });
}

function getAppInfo(_args) {
    return JSON.stringify({
        status: "success",
        app: {
            name: "Illustrator",
            version: app.version || null,
            locale: app.locale || null,
            userInteractionLevel: aiString(app.userInteractionLevel),
            documentCount: aiGetDocumentCount(),
            activeDocument: aiDocumentSummary(aiResolveDocument({}))
        }
    });
}

function listDocuments(_args) {
    var count = aiGetDocumentCount();
    var docs = [];
    for (var i = 0; i < count; i++) {
        docs.push(aiDocumentSummary(app.documents[i]));
    }
    return JSON.stringify({
        status: "success",
        total: count,
        documents: docs
    });
}

function getActiveDocument(_args) {
    var doc = aiResolveDocument({});
    if (!doc) {
        return aiError("No active Illustrator document.");
    }
    return JSON.stringify({
        status: "success",
        document: aiDocumentSummary(doc)
    });
}

function listArtboards(args) {
    var doc = aiResolveDocument(args || {});
    if (!doc) {
        return aiError("No Illustrator document found.");
    }
    var artboards = [];
    var activeIndex = null;
    try {
        activeIndex = doc.artboards.getActiveArtboardIndex() + 1;
    } catch (_activeErr) {}
    var count = aiCollectionLength(doc.artboards);
    for (var i = 0; i < count; i++) {
        var artboard = doc.artboards[i];
        var rect = null;
        try {
            rect = artboard.artboardRect;
        } catch (_rectErr) {}
        artboards.push({
            index: i + 1,
            name: artboard.name || "",
            active: activeIndex === i + 1,
            rect: rect,
            width: rect ? Math.abs(rect[2] - rect[0]) : null,
            height: rect ? Math.abs(rect[1] - rect[3]) : null
        });
    }
    return JSON.stringify({
        status: "success",
        document: aiDocumentSummary(doc),
        activeArtboardIndex: activeIndex,
        total: count,
        artboards: artboards
    });
}

function aiLayerSummary(layer, index, parentPath, depth, recursive, maxDepth) {
    var childCount = 0;
    try {
        childCount = aiCollectionLength(layer.layers);
    } catch (_childErr) {}
    var pageItemCount = 0;
    try {
        pageItemCount = aiCollectionLength(layer.pageItems);
    } catch (_pageErr) {}
    var opacity = null;
    try {
        opacity = Number(layer.opacity);
    } catch (_opacityErr) {}
    var item = {
        index: index,
        name: layer.name || "",
        path: parentPath ? parentPath + "/" + (layer.name || "") : (layer.name || ""),
        typename: layer.typename || "Layer",
        visible: layer.visible === true,
        locked: layer.locked === true,
        printable: layer.printable !== false,
        opacity: opacity,
        pageItemCount: pageItemCount,
        childLayerCount: childCount,
        depth: depth
    };
    if (recursive && childCount > 0 && depth < maxDepth) {
        item.layers = [];
        for (var i = 0; i < childCount; i++) {
            item.layers.push(aiLayerSummary(layer.layers[i], i + 1, item.path, depth + 1, recursive, maxDepth));
        }
    }
    return item;
}

function listLayers(args) {
    args = args || {};
    var doc = aiResolveDocument(args);
    if (!doc) {
        return aiError("No Illustrator document found.");
    }
    var recursive = args.recursive === true;
    var maxDepth = args.maxDepth === null || typeof args.maxDepth === "undefined" ? 4 : Number(args.maxDepth);
    if (isNaN(maxDepth) || maxDepth < 0) {
        maxDepth = 4;
    }
    var count = aiCollectionLength(doc.layers);
    var layers = [];
    for (var i = 0; i < count; i++) {
        layers.push(aiLayerSummary(doc.layers[i], i + 1, "", 0, recursive, maxDepth));
    }
    return JSON.stringify({
        status: "success",
        document: aiDocumentSummary(doc),
        total: count,
        recursive: recursive,
        layers: layers
    });
}

function aiInferExportFormat(outputPath, provided) {
    if (provided) {
        return String(provided).toLowerCase();
    }
    var lower = String(outputPath || "").toLowerCase();
    if (lower.match(/\.png$/)) {
        return "png24";
    }
    if (lower.match(/\.jpe?g$/)) {
        return "jpg";
    }
    if (lower.match(/\.svg$/)) {
        return "svg";
    }
    if (lower.match(/\.pdf$/)) {
        return "pdf";
    }
    return "png24";
}

function aiEnsureOutputParent(file) {
    try {
        if (file.parent && !file.parent.exists) {
            aiEnsureFolder(file.parent);
        }
    } catch (_e) {}
}

function exportDocument(args) {
    args = args || {};
    var doc = aiResolveDocument(args);
    if (!doc) {
        return aiError("No Illustrator document found.");
    }
    var outputPath = args.outputPath || args.path;
    if (!outputPath) {
        return aiError("outputPath is required.");
    }
    var format = aiInferExportFormat(outputPath, args.format);
    var file = new File(outputPath);
    aiEnsureOutputParent(file);

    try {
        if (format === "png24" || format === "png") {
            var png24 = new ExportOptionsPNG24();
            png24.artBoardClipping = args.artBoardClipping !== false;
            png24.transparency = args.transparency !== false;
            png24.antiAliasing = args.antiAliasing !== false;
            if (args.horizontalScale !== null && typeof args.horizontalScale !== "undefined") {
                png24.horizontalScale = Number(args.horizontalScale);
            }
            if (args.verticalScale !== null && typeof args.verticalScale !== "undefined") {
                png24.verticalScale = Number(args.verticalScale);
            }
            doc.exportFile(file, ExportType.PNG24, png24);
        } else if (format === "png8") {
            var png8 = new ExportOptionsPNG8();
            png8.artBoardClipping = args.artBoardClipping !== false;
            png8.transparency = args.transparency !== false;
            png8.antiAliasing = args.antiAliasing !== false;
            doc.exportFile(file, ExportType.PNG8, png8);
        } else if (format === "jpg" || format === "jpeg") {
            var jpg = new ExportOptionsJPEG();
            jpg.artBoardClipping = args.artBoardClipping !== false;
            jpg.antiAliasing = args.antiAliasing !== false;
            if (args.quality !== null && typeof args.quality !== "undefined") {
                jpg.qualitySetting = Number(args.quality);
            }
            doc.exportFile(file, ExportType.JPEG, jpg);
        } else if (format === "svg") {
            var svg = new ExportOptionsSVG();
            svg.embedRasterImages = args.embedRasterImages !== false;
            svg.fontSubsetting = SVGFontSubsetting.GLYPHSUSED;
            doc.exportFile(file, ExportType.SVG, svg);
        } else if (format === "pdf") {
            var pdf = new PDFSaveOptions();
            pdf.preserveEditability = args.preserveEditability === true;
            doc.saveAs(file, pdf);
        } else {
            return aiError("Unsupported export format: " + format);
        }
    } catch (err) {
        return aiError(err.toString());
    }

    return JSON.stringify({
        status: "success",
        document: aiDocumentSummary(doc),
        outputPath: file.fsName,
        format: format,
        message: "Document exported."
    });
}

function executeJsx(payload) {
    payload = payload || {};
    var code = payload.code || "";
    if (!code) {
        return aiError("code is required.");
    }
    var userArgs = payload.args || {};
    try {
        var result = (function (args) {
            return eval(code);
        })(userArgs);
        return aiResultToString(result);
    } catch (err) {
        return aiError(err.toString());
    }
}

function executeJsxFile(payload) {
    payload = payload || {};
    var path = payload.path || payload.sourcePath;
    if (!path) {
        return aiError("path is required.");
    }
    var file = new File(path);
    if (!file.exists) {
        return aiError("JSX file not found: " + path);
    }
    var code = aiReadFile(file);
    return executeJsx({
        code: code,
        args: payload.args || {},
        sourcePath: file.fsName
    });
}

function aiExecuteCommand(command, args) {
    switch (command) {
        case "executeJsx":
            return executeJsx(args);
        case "executeJsxFile":
            return executeJsxFile(args);
        case "ping":
            return ping(args);
        case "getAppInfo":
            return getAppInfo(args);
        case "listDocuments":
            return listDocuments(args);
        case "getActiveDocument":
            return getActiveDocument(args);
        case "listArtboards":
            return listArtboards(args);
        case "listLayers":
            return listLayers(args);
        case "exportDocument":
            return exportDocument(args);
        default:
            return aiError("Unknown command: " + command);
    }
}

function aiAttachResultMetadata(resultString, command, requestId) {
    var resultObj = null;
    try {
        resultObj = JSON.parse(resultString);
    } catch (_parseErr) {
        resultObj = {
            status: "success",
            result: resultString
        };
    }
    resultObj._responseTimestamp = aiIsoDate(new Date());
    resultObj._commandExecuted = command;
    resultObj._requestId = requestId || null;
    resultObj._aiInstance = aiGetInstanceMetadata();
    return JSON.stringify(resultObj, null, 2);
}

function aiMcpBridgeCheck() {
    try {
        aiWriteHeartbeat();
        if (!aiMcpBridgeState.autoRun) {
            aiMcpBridgeState.lastStatus = "idle";
            aiMcpBridgeState.lastMessage = "Auto-run is off.";
            return aiMcpBridgeGetState();
        }

        var pending = aiFindPendingCommand();
        if (!pending) {
            if (aiMcpBridgeState.lastStatus !== "error") {
                aiMcpBridgeState.lastStatus = "waiting";
                aiMcpBridgeState.lastMessage = "Waiting for commands.";
                aiMcpBridgeState.lastError = null;
            }
            return aiMcpBridgeGetState();
        }

        var payload = pending.payload;
        var command = payload.command;
        var requestId = payload.requestId || "";
        var args = payload.args || {};
        var resultFile = pending.isInstance || requestId ? aiInstanceResultFile() : aiRootResultFile();

        aiMcpCurrentRequestId = requestId;
        aiMcpCurrentCommand = command;
        aiWriteHeartbeat();
        aiUpdateCommandStatus(pending.file, payload, "running");

        var rawResult = "";
        try {
            rawResult = aiExecuteCommand(command, args);
        } catch (runErr) {
            rawResult = aiError(runErr.toString());
        }

        var resultString = aiAttachResultMetadata(aiResultToString(rawResult), command, requestId);
        aiWriteResult(resultFile, resultString);

        var finalStatus = "completed";
        try {
            var parsed = JSON.parse(resultString);
            if (parsed && (parsed.status === "error" || parsed.success === false)) {
                finalStatus = "error";
            }
        } catch (_statusErr) {}

        aiUpdateCommandStatus(pending.file, payload, finalStatus);
        aiMcpBridgeState.lastStatus = finalStatus;
        aiMcpBridgeState.lastCommand = command;
        aiMcpBridgeState.lastMessage = "Executed command: " + command;
        aiMcpBridgeState.lastError = finalStatus === "error" ? "Command failed" : null;
        aiMcpBridgeState.lastRunAt = aiIsoDate(new Date());
        aiMcpCurrentRequestId = "";
        aiMcpCurrentCommand = "";
        aiWriteHeartbeat();
    } catch (err) {
        aiMcpBridgeState.lastStatus = "error";
        aiMcpBridgeState.lastError = err.toString();
        try {
            if (aiMcpCurrentCommand) {
                var errorResult = aiAttachResultMetadata(
                    aiError(err.toString()),
                    aiMcpCurrentCommand,
                    aiMcpCurrentRequestId
                );
                aiWriteResult(aiInstanceResultFile(), errorResult);
            }
        } catch (_writeErr) {}
        aiMcpCurrentRequestId = "";
        aiMcpCurrentCommand = "";
        try {
            aiWriteHeartbeat();
        } catch (_heartbeatErr) {}
    }

    return aiMcpBridgeGetState();
}

try {
    $.global.aiMcpBridgeGetState = aiMcpBridgeGetState;
    $.global.aiMcpBridgeSetAutoRun = aiMcpBridgeSetAutoRun;
    $.global.aiMcpBridgeCheck = aiMcpBridgeCheck;
} catch (_globalErr) {}

aiMcpBridgeGetState();
