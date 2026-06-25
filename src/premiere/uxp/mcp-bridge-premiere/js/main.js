(function () {
  var BRIDGE_VERSION = "0.4.2";
  var fs = safeRequire("fs");
  var os = safeRequire("os");
  var uxp = safeRequire("uxp");
  var ppro = safeRequire("premierepro");

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
    } catch (_e) {}
    return null;
  }

  function refreshRequiredModules() {
    fs = fs || safeRequire("fs");
    os = os || safeRequire("os");
    uxp = uxp || safeRequire("uxp");
    ppro = ppro || safeRequire("premierepro");
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
    refreshRequiredModules();
    if (os && os.homedir) {
      return os.homedir();
    }
    return "";
  }

  function isValidInstanceId(value) {
    return typeof value === "string" &&
      /^pr-uxp-[A-Za-z0-9_.-]+$/.test(value) &&
      value.length <= 128;
  }

  function createInstanceId() {
    var random = Math.random().toString(36).slice(2, 10);
    return "pr-uxp-" + Date.now().toString(36) + "-" + random;
  }

  function getInstanceId() {
    var key = "premiereMcpBridgeInstanceId";
    try {
      var stored = window.localStorage && window.localStorage.getItem(key);
      if (isValidInstanceId(stored)) {
        return stored;
      }
    } catch (_e) {}

    var id = createInstanceId();
    try {
      if (window.localStorage) {
        window.localStorage.setItem(key, id);
      }
    } catch (_e2) {}
    return id;
  }

  var instanceId = getInstanceId();

  function ensureInstanceId() {
    if (!isValidInstanceId(instanceId)) {
      instanceId = getInstanceId();
    }
    return instanceId;
  }

  function getBridgePaths() {
    ensureInstanceId();
    var root = joinPath(getHomeDir(), "Documents", "pr-mcp-bridge");
    var instanceRoot = joinPath(root, "instances", instanceId);
    return {
      root: root,
      commandFile: joinPath(root, "pr_command.json"),
      resultFile: joinPath(root, "pr_mcp_result.json"),
      instancesRoot: joinPath(root, "instances"),
      instanceRoot: instanceRoot,
      instanceCommandFile: joinPath(instanceRoot, "pr_command.json"),
      instanceResultFile: joinPath(instanceRoot, "pr_mcp_result.json"),
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

  async function getProjectPath() {
    try {
      var project = await getActiveProject();
      return project && project.path ? project.path : null;
    } catch (_e) {
      return null;
    }
  }

  async function writeHeartbeat(status) {
    var paths = getBridgePaths();
    var host = uxp && uxp.host ? uxp.host : {};
    var appVersion = host.version || "";
    var projectPath = await getProjectPath();
    var payload = {
      protocolVersion: 1,
      instanceId: instanceId,
      hostId: "premiere",
      appName: host.name || "Premiere Pro",
      appVersion: appVersion,
      displayName: appVersion ? "Premiere Pro " + appVersion : "Premiere Pro UXP",
      projectPath: projectPath,
      bridgeRuntime: "uxp",
      bridgeVersion: BRIDGE_VERSION,
      status: status || state.lastStatus || "idle",
      currentRequestId: currentRequestId,
      bridgeRoot: paths.root,
      commandFile: paths.instanceCommandFile,
      resultFile: paths.instanceResultFile,
      lastHeartbeatAt: nowIso(),
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
      refreshRequiredModules();
      if (!fs || !os) {
        throw new Error("Required UXP file system modules are not available.");
      }
      await ensureBridgeDirs();
      await writeHeartbeat(state.lastStatus || "idle");
      if (!ppro) {
        setState({
          lastStatus: "error",
          lastError: "Premiere UXP module is not available yet."
        });
        return;
      }
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

  async function getActiveProject() {
    if (!ppro || !ppro.Project || !ppro.Project.getActiveProject) {
      throw new Error("Premiere UXP Project API is not available.");
    }
    return await ppro.Project.getActiveProject();
  }

  function stringifyIdentifier(value) {
    if (value === undefined || value === null) {
      return null;
    }
    if (typeof value === "string") {
      return value || null;
    }
    if (typeof value === "number" || typeof value === "boolean") {
      return String(value);
    }
    try {
      if (value.toString && value.toString !== Object.prototype.toString) {
        var text = value.toString();
        if (text && text !== "[object Object]") {
          return text;
        }
      }
    } catch (_e) {}
    try {
      if (value.valueOf && value.valueOf !== Object.prototype.valueOf) {
        var primitive = value.valueOf();
        if (primitive !== value) {
          return stringifyIdentifier(primitive);
        }
      }
    } catch (_e2) {}
    return null;
  }

  function getSequenceId(sequence) {
    if (!sequence) {
      return null;
    }
    var candidates = [sequence.guid, sequence.sequenceID, sequence.id];
    for (var i = 0; i < candidates.length; i++) {
      var id = stringifyIdentifier(candidates[i]);
      if (id) {
        return id;
      }
    }
    return null;
  }

  async function getProjectSequences(project) {
    if (!project || !project.getSequences) {
      return [];
    }
    var sequences = await project.getSequences();
    return sequences || [];
  }

  async function findSequenceIndex(project, target) {
    var sequences = await getProjectSequences(project);
    var targetId = getSequenceId(target);
    for (var i = 0; i < sequences.length; i++) {
      var seq = sequences[i];
      if (seq === target) {
        return i + 1;
      }
      if (targetId && getSequenceId(seq) === targetId) {
        return i + 1;
      }
    }
    return null;
  }

  async function resolveSequence(args) {
    args = args || {};
    var project = await getActiveProject();
    if (!project) {
      return { project: null, sequence: null };
    }

    var sequences = await getProjectSequences(project);
    if (args.sequenceName) {
      var needle = String(args.sequenceName).toLowerCase();
      for (var i = 0; i < sequences.length; i++) {
        if (sequences[i] && String(sequences[i].name || "").toLowerCase() === needle) {
          return { project: project, sequence: sequences[i] };
        }
      }
    }

    if (args.sequenceIndex !== undefined && args.sequenceIndex !== null) {
      var idxNumber = Number(args.sequenceIndex);
      if (!isNaN(idxNumber)) {
        var oneBased = idxNumber - 1;
        if (oneBased >= 0 && oneBased < sequences.length) {
          return { project: project, sequence: sequences[oneBased] };
        }
        if (idxNumber >= 0 && idxNumber < sequences.length) {
          return { project: project, sequence: sequences[idxNumber] };
        }
      }
    }

    var active = project.getActiveSequence ? await project.getActiveSequence() : null;
    return { project: project, sequence: active };
  }

  function collectionLength(collection) {
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

  async function getSequencePlayhead(sequence) {
    if (!sequence || !sequence.getPlayerPosition) {
      return null;
    }
    try {
      var position = await sequence.getPlayerPosition();
      return {
        seconds: position && position.seconds !== undefined ? position.seconds : null,
        ticks: position && position.ticks !== undefined ? String(position.ticks) : null
      };
    } catch (_e) {
      return null;
    }
  }

  async function summarizeSequence(project, sequence) {
    if (!sequence) {
      return null;
    }
    return {
      name: sequence.name || "",
      index: project ? await findSequenceIndex(project, sequence) : null,
      id: getSequenceId(sequence),
      videoTrackCount: collectionLength(sequence.videoTracks),
      audioTrackCount: collectionLength(sequence.audioTracks),
      playhead: await getSequencePlayhead(sequence)
    };
  }

  async function getProjectInfo() {
    var project = await getActiveProject();
    if (!project) {
      return {
        status: "error",
        message: "Premiere project is not available."
      };
    }

    var sequences = await getProjectSequences(project);
    var active = project.getActiveSequence ? await project.getActiveSequence() : null;
    return {
      status: "success",
      project: {
        name: project.name || "",
        path: project.path || null,
        sequenceCount: sequences.length || 0,
        activeSequence: await summarizeSequence(project, active)
      }
    };
  }

  async function getSequenceInfo(args) {
    var resolved = await resolveSequence(args || {});
    if (!resolved.project) {
      return {
        status: "error",
        message: "Premiere project is not available."
      };
    }
    if (!resolved.sequence) {
      return {
        status: "error",
        message: "Sequence not found."
      };
    }

    return {
      status: "success",
      sequence: await summarizeSequence(resolved.project, resolved.sequence)
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
      getActiveProject: getActiveProject,
      getProjectSequences: getProjectSequences,
      getProjectInfo: getProjectInfo,
      listSequences: listSequences,
      getActiveSequence: getActiveSequence,
      getSequenceInfo: getSequenceInfo,
      setPlayheadTime: setPlayheadTime,
      exportSequence: exportSequence,
      readJsonFile: readJsonFile,
      writeJsonFile: writeJsonFile,
      readTextFile: readTextFile,
      writeTextFile: writeTextFile,
      joinPath: joinPath,
      getBridgePaths: getBridgePaths,
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
      "ppro",
      "premierepro",
      "uxp",
      "fs",
      "os",
      "bridge",
      "\"use strict\";\nreturn (async function () {\n" + code + "\n}).call(bridge);"
    );
    var result = await fn(userArgs, ppro, ppro, uxp, fs, os, bridge);
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
      case "getProjectInfo":
        return await getProjectInfo();
      case "listSequences":
        return await listSequences();
      case "getActiveSequence":
        return await getActiveSequence();
      case "getSequenceInfo":
        return await getSequenceInfo(args);
      case "setPlayheadTime":
        return await setPlayheadTime(args);
      case "exportSequence":
        return await exportSequence(args);
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

  async function listSequences() {
    var project = await getActiveProject();
    if (!project) {
      return {
        status: "error",
        message: "Premiere project is not available."
      };
    }

    var sequences = await getProjectSequences(project);
    if (!sequences || sequences.length === 0) {
      return {
        status: "error",
        message: "No sequences found in the current project."
      };
    }

    var items = [];
    for (var i = 0; i < sequences.length; i++) {
      var seq = sequences[i];
      if (!seq) {
        continue;
      }
      items.push({
        index: i + 1,
        name: seq.name || "",
        id: getSequenceId(seq)
      });
    }

    return {
      status: "success",
      total: items.length,
      sequences: items
    };
  }

  async function getActiveSequence() {
    var project = await getActiveProject();
    if (!project) {
      return {
        status: "error",
        message: "Premiere project is not available."
      };
    }

    var seq = project.getActiveSequence ? await project.getActiveSequence() : null;
    if (!seq) {
      return {
        status: "error",
        message: "No active sequence."
      };
    }

    return {
      status: "success",
      sequence: {
        name: seq.name || "",
        index: await findSequenceIndex(project, seq),
        id: getSequenceId(seq)
      }
    };
  }

  function makeTickTime(args) {
    if (!ppro || !ppro.TickTime) {
      throw new Error("Premiere UXP TickTime API is not available.");
    }
    if (args.timeTicks !== undefined && args.timeTicks !== null) {
      return ppro.TickTime.createWithTicks(String(args.timeTicks));
    }
    if (args.timeSeconds !== undefined && args.timeSeconds !== null) {
      var seconds = Number(args.timeSeconds);
      if (isNaN(seconds)) {
        throw new Error("timeSeconds must be a number.");
      }
      return ppro.TickTime.createWithSeconds(seconds);
    }
    throw new Error("timeSeconds or timeTicks is required.");
  }

  async function setPlayheadTime(args) {
    args = args || {};
    var resolved = await resolveSequence(args);
    var seq = resolved.sequence;
    if (!seq) {
      return {
        status: "error",
        message: "Sequence not found."
      };
    }

    var tickTime = makeTickTime(args);
    var result = await seq.setPlayerPosition(tickTime);
    return {
      status: "success",
      result: result,
      sequenceName: seq.name || "",
      timeSeconds: tickTime.seconds,
      timeTicks: tickTime.ticks
    };
  }

  async function exportSequence(args) {
    args = args || {};
    var resolved = await resolveSequence(args);
    var seq = resolved.sequence;
    if (!seq) {
      return {
        status: "error",
        message: "Sequence not found."
      };
    }

    var outputPath = args.outputPath;
    if (!outputPath) {
      return {
        status: "error",
        message: "outputPath is required."
      };
    }

    var presetPath = args.presetPath;
    if (!presetPath) {
      return {
        status: "error",
        message: "presetPath is required."
      };
    }

    if (!ppro || !ppro.EncoderManager || !ppro.EncoderManager.getManager) {
      throw new Error("Premiere UXP EncoderManager API is not available.");
    }

    var constants = ppro.Constants || ppro.constants;
    if (!constants || !constants.ExportType || !constants.ExportType.IMMEDIATELY) {
      throw new Error("Premiere UXP ExportType constants are not available.");
    }

    var workAreaType = args.workAreaType !== undefined && args.workAreaType !== null
      ? Number(args.workAreaType)
      : 0;
    var exportFull = args.exportFull !== undefined ? !!args.exportFull : workAreaType !== 1;

    var encoder = await ppro.EncoderManager.getManager();
    var result = await encoder.exportSequence(
      seq,
      constants.ExportType.IMMEDIATELY,
      outputPath,
      presetPath,
      exportFull
    );

    return {
      status: "success",
      result: result,
      sequenceName: seq.name || "",
      outputPath: outputPath,
      presetPath: presetPath,
      workAreaType: workAreaType,
      exportFull: exportFull
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

  function startHeartbeatTimer() {
    refreshRequiredModules();
    if (!heartbeatTimer) {
      heartbeatTimer = window.setInterval(function () {
        refreshRequiredModules();
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
  }

  async function startBridge() {
    refreshRequiredModules();
    if (!initialized) {
      initElements();
    }
    startHeartbeatTimer();
    await pollOnce();
    if (!pollTimer) {
      pollTimer = window.setInterval(pollOnce, 1000);
    }
  }

  function stopBridge() {
    if (pollTimer) {
      window.clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function stopHeartbeat() {
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
              show: function () {
                startBridge();
              },
              hide: function () {
                setState({
                  lastStatus: "hidden",
                  lastMessage: "Panel hidden; heartbeat remains active.",
                  lastError: null
                });
                startHeartbeatTimer();
                if (pollTimer) {
                  window.clearInterval(pollTimer);
                  pollTimer = null;
                }
              },
              destroy: function () {
                stopBridge();
                stopHeartbeat();
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
