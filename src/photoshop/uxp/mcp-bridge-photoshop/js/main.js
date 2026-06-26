(function () {
  var BRIDGE_VERSION = "0.4.4";
  var fs = safeRequire("fs");
  var os = safeRequire("os");
  var uxp = safeRequire("uxp");
  var photoshop = safeRequire("photoshop");
  var app = photoshop && photoshop.app ? photoshop.app : null;
  var action = photoshop && photoshop.action ? photoshop.action : null;
  var core = photoshop && photoshop.core ? photoshop.core : null;

  var autoRunCheckbox = null;
  var statusEl = null;
  var bridgeRootEl = null;
  var commandFileEl = null;
  var resultFileEl = null;
  var instanceIdEl = null;
  var logEl = null;

  var autoRun = true;
  var initialized = false;
  var pollInFlight = false;
  var pollTimer = null;
  var heartbeatTimer = null;
  var currentRequestId = null;
  var state = {
    autoRun: true,
    lastStatus: "idle",
    lastCommand: null,
    lastMessage: null,
    lastError: null,
    lastRunAt: null
  };

  function safeRequire(name) {
    try {
      if (typeof require === "function") {
        return require(name);
      }
      if (typeof window !== "undefined" && typeof window.require === "function") {
        return window.require(name);
      }
    } catch (_e) {}
    return null;
  }

  function setText(el, text) {
    if (el) {
      el.textContent = text;
    }
  }

  function setLog(text) {
    if (logEl) {
      logEl.textContent = text;
    }
  }

  function errorText(err) {
    if (!err) {
      return "Unknown error";
    }
    return err.stack || err.message || String(err);
  }

  function nowIso() {
    return new Date().toISOString();
  }

  function joinPath() {
    var sep = os && os.platform && os.platform() === "win32" ? "\\" : "/";
    var parts = [];
    for (var i = 0; i < arguments.length; i++) {
      var part = String(arguments[i] || "");
      if (!part) {
        continue;
      }
      part = part.replace(/[\\\/]+$/g, "");
      if (parts.length > 0) {
        part = part.replace(/^[\\\/]+/g, "");
      }
      parts.push(part);
    }
    return parts.join(sep);
  }

  function getHomeDir() {
    if (os && os.homedir) {
      return os.homedir();
    }
    return "";
  }

  function getInstanceId() {
    var key = "photoshopMcpBridgeInstanceId";
    try {
      var stored = window.localStorage && window.localStorage.getItem(key);
      if (stored) {
        return stored;
      }
    } catch (_e) {}

    var random = Math.random().toString(36).slice(2, 10);
    var id = "ps-uxp-" + Date.now().toString(36) + "-" + random;
    try {
      if (window.localStorage) {
        window.localStorage.setItem(key, id);
      }
    } catch (_e2) {}
    return id;
  }

  var instanceId = getInstanceId();

  function getBridgePaths() {
    var root = joinPath(getHomeDir(), "Documents", "ps-mcp-bridge");
    var instanceRoot = joinPath(root, "instances", instanceId);
    return {
      root: root,
      commandFile: joinPath(root, "ps_command.json"),
      resultFile: joinPath(root, "ps_mcp_result.json"),
      instancesRoot: joinPath(root, "instances"),
      instanceRoot: instanceRoot,
      instanceCommandFile: joinPath(instanceRoot, "ps_command.json"),
      instanceResultFile: joinPath(instanceRoot, "ps_mcp_result.json"),
      heartbeatFile: joinPath(instanceRoot, "heartbeat.json")
    };
  }

  function pathExists(path) {
    if (!fs || !path) {
      return false;
    }
    try {
      fs.lstatSync(path);
      return true;
    } catch (_e) {
      return false;
    }
  }

  async function ensureDir(path) {
    if (!fs || !path || pathExists(path)) {
      return;
    }
    await fs.mkdir(path, { recursive: true });
  }

  async function ensureBridgeDirs() {
    var paths = getBridgePaths();
    await ensureDir(paths.root);
    await ensureDir(paths.instancesRoot);
    await ensureDir(paths.instanceRoot);
  }

  function readTextFile(path) {
    if (!fs || !pathExists(path)) {
      return null;
    }
    return fs.readFileSync(path, { encoding: "utf-8" });
  }

  function writeTextFile(path, text) {
    if (!fs) {
      throw new Error("UXP fs module is not available.");
    }
    fs.writeFileSync(path, text, { encoding: "utf-8" });
  }

  function readJsonFile(path) {
    var raw = readTextFile(path);
    if (!raw || !raw.trim()) {
      return null;
    }
    return JSON.parse(raw);
  }

  function writeJsonFile(path, value) {
    writeTextFile(path, JSON.stringify(value, null, 2));
  }

  function safeGet(target, key) {
    if (!target) {
      return null;
    }
    try {
      var value = target[key];
      return value === undefined ? null : value;
    } catch (_e) {
      return null;
    }
  }

  function primitiveOrString(value) {
    if (value === null || value === undefined) {
      return null;
    }
    var valueType = typeof value;
    if (valueType === "string" || valueType === "number" || valueType === "boolean") {
      return value;
    }
    try {
      if (value.value !== undefined && value.unit !== undefined) {
        return {
          value: value.value,
          unit: String(value.unit)
        };
      }
    } catch (_e) {}
    try {
      if (value.toString && value.toString !== Object.prototype.toString) {
        var text = value.toString();
        if (text && text !== "[object Object]") {
          return text;
        }
      }
    } catch (_e2) {}
    return toSerializable(value, 0, []);
  }

  function collectionToArray(collection) {
    var items = [];
    if (!collection) {
      return items;
    }
    if (Array.isArray(collection)) {
      return collection.slice(0);
    }
    if (typeof collection.forEach === "function") {
      try {
        collection.forEach(function (item) {
          items.push(item);
        });
        return items;
      } catch (_e) {}
    }
    var length = collectionLength(collection);
    for (var i = 0; i < length; i++) {
      try {
        items.push(collection[i]);
      } catch (_e2) {}
    }
    return items;
  }

  function collectionLength(collection) {
    if (!collection) {
      return 0;
    }
    var keys = ["length", "count", "numItems"];
    for (var i = 0; i < keys.length; i++) {
      var value = safeGet(collection, keys[i]);
      if (typeof value === "number" && !isNaN(value)) {
        return value;
      }
    }
    if (Array.isArray(collection)) {
      return collection.length;
    }
    return 0;
  }

  function getDocumentsArray() {
    if (!app) {
      return [];
    }
    return collectionToArray(safeGet(app, "documents"));
  }

  function getActiveDocumentOrNull() {
    if (!app) {
      return null;
    }
    try {
      return app.activeDocument || null;
    } catch (_e) {
      return null;
    }
  }

  function summarizeDocument(doc, index) {
    if (!doc) {
      return null;
    }
    var title = safeGet(doc, "title") || safeGet(doc, "name") || "";
    return {
      index: index,
      id: safeGet(doc, "id"),
      title: title,
      name: safeGet(doc, "name") || title,
      path: safeGet(doc, "path"),
      width: primitiveOrString(safeGet(doc, "width")),
      height: primitiveOrString(safeGet(doc, "height")),
      resolution: primitiveOrString(safeGet(doc, "resolution")),
      mode: primitiveOrString(safeGet(doc, "mode")),
      saved: safeGet(doc, "saved"),
      layerCount: collectionLength(safeGet(doc, "layers"))
    };
  }

  function resolveDocument(args) {
    args = args || {};
    var documents = getDocumentsArray();
    if (args.documentId !== undefined && args.documentId !== null) {
      var targetId = String(args.documentId);
      for (var idIndex = 0; idIndex < documents.length; idIndex++) {
        if (String(safeGet(documents[idIndex], "id")) === targetId) {
          return documents[idIndex];
        }
      }
      return null;
    }

    if (args.documentTitle || args.documentName) {
      var needle = String(args.documentTitle || args.documentName).toLowerCase();
      for (var nameIndex = 0; nameIndex < documents.length; nameIndex++) {
        var title = String(safeGet(documents[nameIndex], "title") || safeGet(documents[nameIndex], "name") || "").toLowerCase();
        if (title === needle) {
          return documents[nameIndex];
        }
      }
      return null;
    }

    if (args.documentIndex !== undefined && args.documentIndex !== null) {
      var idxNumber = Number(args.documentIndex);
      if (!isNaN(idxNumber)) {
        var oneBased = idxNumber - 1;
        if (oneBased >= 0 && oneBased < documents.length) {
          return documents[oneBased];
        }
        if (idxNumber >= 0 && idxNumber < documents.length) {
          return documents[idxNumber];
        }
      }
      return null;
    }

    return getActiveDocumentOrNull();
  }

  async function writeHeartbeat(status) {
    var paths = getBridgePaths();
    var host = uxp && uxp.host ? uxp.host : {};
    var appVersion = host.version || safeGet(app, "version") || "";
    var activeDocument = getActiveDocumentOrNull();
    var payload = {
      protocolVersion: 1,
      instanceId: instanceId,
      hostId: "photoshop",
      appName: host.name || "Photoshop",
      appVersion: appVersion,
      displayName: appVersion ? "Photoshop " + appVersion : "Photoshop UXP",
      projectPath: activeDocument ? safeGet(activeDocument, "path") : null,
      bridgeRuntime: "uxp",
      bridgeVersion: BRIDGE_VERSION,
      capabilities: ["run-jsx", "documents.list", "layers.list", "batchPlay"],
      status: status || state.lastStatus || "idle",
      currentRequestId: currentRequestId,
      bridgeRoot: paths.root,
      commandFile: paths.instanceCommandFile,
      resultFile: paths.instanceResultFile,
      lastHeartbeatAt: nowIso(),
      updatedAt: nowIso(),
      heartbeatPath: paths.heartbeatFile
    };
    writeJsonFile(paths.heartbeatFile, payload);
  }

  function updateUi() {
    var paths = getBridgePaths();
    setText(statusEl, state.lastStatus || "idle");
    setText(bridgeRootEl, paths.root || "-");
    setText(commandFileEl, paths.commandFile || "-");
    setText(resultFileEl, paths.resultFile || "-");
    setText(instanceIdEl, instanceId || "-");

    var logLines = [];
    if (state.lastCommand) {
      logLines.push("Last command: " + state.lastCommand);
    }
    if (state.lastMessage) {
      logLines.push(state.lastMessage);
    }
    if (state.lastError) {
      logLines.push("Error: " + state.lastError);
    }
    if (state.lastRunAt) {
      logLines.push("Last run: " + state.lastRunAt);
    }
    setLog(logLines.length ? logLines.join("\n") : "Waiting for commands...");
  }

  function setState(next) {
    for (var key in next) {
      if (Object.prototype.hasOwnProperty.call(next, key)) {
        state[key] = next[key];
      }
    }
    state.autoRun = autoRun;
    updateUi();
  }

  function findPendingCommand() {
    var paths = getBridgePaths();
    var candidates = [
      { commandFile: paths.instanceCommandFile, resultFile: paths.instanceResultFile, scope: "instance" },
      { commandFile: paths.commandFile, resultFile: paths.resultFile, scope: "global" }
    ];

    for (var i = 0; i < candidates.length; i++) {
      var payload = null;
      try {
        payload = readJsonFile(candidates[i].commandFile);
      } catch (_e) {
        continue;
      }
      if (!payload || !payload.command) {
        continue;
      }
      var status = String(payload.status || "").toLowerCase();
      if (status === "pending") {
        candidates[i].payload = payload;
        return candidates[i];
      }
    }
    return null;
  }

  function updateCommandStatus(commandFile, payload, status) {
    var next = {};
    for (var key in payload) {
      if (Object.prototype.hasOwnProperty.call(payload, key)) {
        next[key] = payload[key];
      }
    }
    next.status = status;
    writeJsonFile(commandFile, next);
  }

  function normalizeResult(command, requestId, rawResult) {
    var resultObj = rawResult;
    if (typeof rawResult === "string") {
      try {
        resultObj = JSON.parse(rawResult);
      } catch (_e) {
        resultObj = {
          status: "success",
          message: rawResult
        };
      }
    }
    if (!resultObj || typeof resultObj !== "object") {
      resultObj = {
        status: "success",
        result: resultObj
      };
    }
    if (!resultObj.status) {
      resultObj.status = "success";
    }
    resultObj._commandExecuted = command;
    resultObj._responseTimestamp = nowIso();
    if (requestId) {
      resultObj._requestId = requestId;
    }
    return resultObj;
  }

  function writeResult(context, resultObj) {
    var paths = getBridgePaths();
    writeJsonFile(context.resultFile, resultObj);
    if (context.resultFile !== paths.resultFile) {
      writeJsonFile(paths.resultFile, resultObj);
    }
  }

  async function executePendingCommand(context) {
    var payload = context.payload;
    var command = payload.command;
    var args = payload.args || {};
    var requestId = payload.requestId || payload.request_id || null;
    currentRequestId = requestId;

    setState({
      lastStatus: "running",
      lastCommand: command,
      lastMessage: "Executing command: " + command,
      lastError: null
    });
    await writeHeartbeat("running");

    updateCommandStatus(context.commandFile, payload, "running");

    var resultObj = null;
    try {
      var rawResult = await dispatchCommand(command, args);
      resultObj = normalizeResult(command, requestId, rawResult);
    } catch (err) {
      resultObj = normalizeResult(command, requestId, {
        status: "error",
        message: errorText(err)
      });
    }

    writeResult(context, resultObj);

    var finalStatus = resultObj.status === "error" ? "error" : "completed";
    updateCommandStatus(context.commandFile, payload, finalStatus);
    currentRequestId = null;
    setState({
      lastStatus: finalStatus,
      lastCommand: command,
      lastMessage: "Executed command: " + command,
      lastError: finalStatus === "error" ? resultObj.message || "Command failed" : null,
      lastRunAt: nowIso()
    });
    await writeHeartbeat(finalStatus);
  }

  async function pollOnce() {
    if (pollInFlight) {
      return;
    }
    pollInFlight = true;
    try {
      if (!fs || !os || !photoshop || !app) {
        throw new Error("Required Photoshop UXP modules are not available.");
      }
      await ensureBridgeDirs();
      await writeHeartbeat(state.lastStatus || "idle");
      if (!autoRun) {
        setState({
          lastStatus: "idle",
          lastMessage: "Auto-run commands is off."
        });
        return;
      }

      var context = findPendingCommand();
      if (!context) {
        if (state.lastStatus === "initializing" || state.lastStatus === "running") {
          setState({ lastStatus: "waiting" });
        }
        return;
      }
      await executePendingCommand(context);
    } catch (err) {
      setState({
        lastStatus: "error",
        lastError: errorText(err)
      });
    } finally {
      pollInFlight = false;
    }
  }

  function getAppInfo() {
    var host = uxp && uxp.host ? uxp.host : {};
    var documents = getDocumentsArray();
    var activeDocument = getActiveDocumentOrNull();
    return {
      status: "success",
      app: {
        name: host.name || "Photoshop",
        version: host.version || safeGet(app, "version") || null,
        locale: safeGet(app, "locale"),
        documentCount: documents.length,
        activeDocument: activeDocument ? summarizeDocument(activeDocument, null) : null
      }
    };
  }

  function listDocuments() {
    var documents = getDocumentsArray();
    var activeDocument = getActiveDocumentOrNull();
    var activeId = activeDocument ? safeGet(activeDocument, "id") : null;
    var items = [];
    for (var i = 0; i < documents.length; i++) {
      var summary = summarizeDocument(documents[i], i + 1);
      if (summary) {
        summary.active = activeId !== null && String(summary.id) === String(activeId);
        items.push(summary);
      }
    }
    return {
      status: "success",
      total: items.length,
      documents: items
    };
  }

  function getActiveDocument() {
    var doc = getActiveDocumentOrNull();
    if (!doc) {
      return {
        status: "error",
        message: "No active Photoshop document."
      };
    }
    return {
      status: "success",
      document: summarizeDocument(doc, null)
    };
  }

  function summarizeLayer(layer, index, depth, recursive, maxDepth) {
    if (!layer) {
      return null;
    }
    var childCollection = safeGet(layer, "layers");
    var children = [];
    if (recursive && depth < maxDepth) {
      var childLayers = collectionToArray(childCollection);
      for (var i = 0; i < childLayers.length; i++) {
        var child = summarizeLayer(childLayers[i], i + 1, depth + 1, recursive, maxDepth);
        if (child) {
          children.push(child);
        }
      }
    }

    var item = {
      index: index,
      id: safeGet(layer, "id"),
      name: safeGet(layer, "name") || "",
      kind: primitiveOrString(safeGet(layer, "kind")),
      visible: safeGet(layer, "visible"),
      opacity: primitiveOrString(safeGet(layer, "opacity")),
      blendMode: primitiveOrString(safeGet(layer, "blendMode")),
      locked: safeGet(layer, "locked"),
      isBackgroundLayer: safeGet(layer, "isBackgroundLayer"),
      childCount: collectionLength(childCollection)
    };
    if (children.length) {
      item.layers = children;
    }
    return item;
  }

  function listLayers(args) {
    args = args || {};
    var doc = resolveDocument(args);
    if (!doc) {
      return {
        status: "error",
        message: "Photoshop document not found."
      };
    }

    var recursive = args.recursive !== undefined ? !!args.recursive : true;
    var maxDepth = args.maxDepth !== undefined && args.maxDepth !== null ? Number(args.maxDepth) : 10;
    if (isNaN(maxDepth) || maxDepth < 0) {
      maxDepth = 10;
    }

    var layers = collectionToArray(safeGet(doc, "layers"));
    var items = [];
    for (var i = 0; i < layers.length; i++) {
      var summary = summarizeLayer(layers[i], i + 1, 0, recursive, maxDepth);
      if (summary) {
        items.push(summary);
      }
    }

    return {
      status: "success",
      document: summarizeDocument(doc, null),
      total: items.length,
      recursive: recursive,
      layers: items
    };
  }

  function toSerializable(value, depth, seen) {
    if (value === null || value === undefined) {
      return value === undefined ? null : value;
    }
    var valueType = typeof value;
    if (valueType === "string" || valueType === "number" || valueType === "boolean") {
      return value;
    }
    if (valueType === "function") {
      return "[Function]";
    }
    if (valueType !== "object") {
      return String(value);
    }
    if (depth > 6) {
      return "[MaxDepth]";
    }

    seen = seen || [];
    for (var i = 0; i < seen.length; i++) {
      if (seen[i] === value) {
        return "[Circular]";
      }
    }
    seen.push(value);

    if (Object.prototype.toString.call(value) === "[object Date]") {
      seen.pop();
      return value.toISOString ? value.toISOString() : String(value);
    }
    if (Array.isArray(value)) {
      var items = [];
      for (var a = 0; a < value.length; a++) {
        items.push(toSerializable(value[a], depth + 1, seen));
      }
      seen.pop();
      return items;
    }

    var result = {};
    for (var key in value) {
      if (!Object.prototype.hasOwnProperty.call(value, key)) {
        continue;
      }
      try {
        result[key] = toSerializable(value[key], depth + 1, seen);
      } catch (err) {
        result[key] = "[Unserializable: " + errorText(err) + "]";
      }
    }
    seen.pop();
    return result;
  }

  function createExecutionBridge() {
    return {
      getAppInfo: getAppInfo,
      listDocuments: listDocuments,
      getActiveDocument: getActiveDocument,
      listLayers: listLayers,
      getDocumentsArray: getDocumentsArray,
      getActiveDocumentOrNull: getActiveDocumentOrNull,
      summarizeDocument: summarizeDocument,
      summarizeLayer: summarizeLayer,
      readJsonFile: readJsonFile,
      writeJsonFile: writeJsonFile,
      readTextFile: readTextFile,
      writeTextFile: writeTextFile,
      joinPath: joinPath,
      getBridgePaths: getBridgePaths,
      toSerializable: toSerializable,
      batchPlay: function (commands, options) {
        if (!action || !action.batchPlay) {
          throw new Error("Photoshop action.batchPlay API is not available.");
        }
        return action.batchPlay(commands, options || {});
      },
      executeAsModal: function (fn, options) {
        if (core && core.executeAsModal) {
          return core.executeAsModal(fn, options || { commandName: "Photoshop MCP command" });
        }
        return fn();
      },
      require: safeRequire
    };
  }

  async function executeJsx(args) {
    args = args || {};
    if (args.mode !== "unsafe") {
      throw new Error("executeJsx requires mode='unsafe'");
    }
    var description = String(args.description || "").trim();
    if (!description) {
      throw new Error("executeJsx requires a non-empty description");
    }
    var code = args.code;
    if (typeof code !== "string" || !code.trim()) {
      throw new Error("executeJsx requires non-empty string code");
    }

    var userArgs = args.args || {};
    var bridge = createExecutionBridge();
    var sourcePath = args.sourcePath || null;
    setState({
      lastMessage: "Running UXP code: " + description + (sourcePath ? " (" + sourcePath + ")" : "")
    });

    var fn = new Function(
      "args",
      "photoshop",
      "app",
      "action",
      "core",
      "batchPlay",
      "uxp",
      "fs",
      "os",
      "bridge",
      "\"use strict\";\nreturn (async function () {\n" + code + "\n}).call(bridge);"
    );
    var result = await fn(
      userArgs,
      photoshop,
      app,
      action,
      core,
      bridge.batchPlay,
      uxp,
      fs,
      os,
      bridge
    );
    return {
      status: "success",
      description: description,
      sourcePath: sourcePath,
      result: toSerializable(result, 0, [])
    };
  }

  async function executeJsxFile(args) {
    args = args || {};
    var filePath = args.path || args.sourcePath;
    if (!filePath) {
      throw new Error("executeJsxFile requires path");
    }
    if (!fs) {
      throw new Error("UXP fs module is not available.");
    }
    args.code = fs.readFileSync(filePath, { encoding: "utf-8" });
    args.sourcePath = filePath;
    return await executeJsx(args);
  }

  async function dispatchCommand(command, args) {
    switch (command) {
      case "executeJsx":
        return await executeJsx(args);
      case "executeJsxFile":
        return await executeJsxFile(args);
      case "ping":
        return ping();
      case "getAppInfo":
        return getAppInfo();
      case "listDocuments":
        return listDocuments();
      case "getActiveDocument":
        return getActiveDocument();
      case "listLayers":
        return listLayers(args);
      default:
        return {
          status: "error",
          message: "Unknown command: " + command
        };
    }
  }

  function ping() {
    return {
      status: "success",
      message: "ok"
    };
  }

  function initElements() {
    autoRunCheckbox = document.getElementById("autoRun");
    statusEl = document.getElementById("status");
    bridgeRootEl = document.getElementById("bridgeRoot");
    commandFileEl = document.getElementById("commandFile");
    resultFileEl = document.getElementById("resultFile");
    instanceIdEl = document.getElementById("instanceId");
    logEl = document.getElementById("log");

    if (autoRunCheckbox) {
      autoRunCheckbox.checked = autoRun;
      autoRunCheckbox.addEventListener("change", function () {
        autoRun = autoRunCheckbox.checked;
        setState({
          lastStatus: autoRun ? "waiting" : "idle",
          lastMessage: autoRun ? "Auto-run commands is on." : "Auto-run commands is off.",
          lastError: null
        });
      });
    }
    initialized = true;
    setState({
      lastStatus: "initializing",
      lastMessage: "Panel loaded. Waiting for bridge state.",
      lastError: null
    });
  }

  async function startBridge() {
    if (!initialized) {
      initElements();
    }
    await pollOnce();
    if (!pollTimer) {
      pollTimer = window.setInterval(pollOnce, 1000);
    }
    if (!heartbeatTimer) {
      heartbeatTimer = window.setInterval(function () {
        ensureBridgeDirs()
          .then(function () {
            return writeHeartbeat(state.lastStatus || "idle");
          })
          .catch(function (err) {
            setState({
              lastStatus: "error",
              lastError: errorText(err)
            });
          });
      }, 3000);
    }
  }

  function stopBridge() {
    if (pollTimer) {
      window.clearInterval(pollTimer);
      pollTimer = null;
    }
    if (heartbeatTimer) {
      window.clearInterval(heartbeatTimer);
      heartbeatTimer = null;
    }
  }

  function setupEntrypoints() {
    try {
      if (uxp && uxp.entrypoints && uxp.entrypoints.setup) {
        uxp.entrypoints.setup({
          panels: {
            mcpBridgePanel: {
              create: function () {
                startBridge();
              },
              show: function () {
                startBridge();
              },
              hide: function () {
                startBridge();
              },
              destroy: function () {
                stopBridge();
              }
            }
          }
        });
      }
    } catch (err) {
      setState({
        lastStatus: "error",
        lastError: errorText(err)
      });
    }
  }

  window.addEventListener("error", function (event) {
    setState({
      lastStatus: "panel error",
      lastError: event.message || "JavaScript error"
    });
  });

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      initElements();
      startBridge();
    });
  } else {
    initElements();
    startBridge();
  }

  setupEntrypoints();
})();
