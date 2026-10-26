// Run with node, or cscript //nologo on Windows (ES3, no extra dependency).
(function () {
    var source, nativeSource, print;
    if (typeof require === "function") {
        source = require("fs").readFileSync(require("path").join(__dirname, "../../src/scripts/mcp-bridge-auto.jsx"), "utf8");
        nativeSource = require("fs").readFileSync(require("path").join(__dirname, "../../crates/daemon-core/src/ae_recovery.jsx"), "utf8");
        print = console.log;
    } else {
        var fs = new ActiveXObject("Scripting.FileSystemObject");
        var root = fs.GetParentFolderName(fs.GetParentFolderName(fs.GetParentFolderName(WScript.ScriptFullName)));
        var stream = fs.OpenTextFile(root + "\\src\\scripts\\mcp-bridge-auto.jsx", 1);
        source = stream.ReadAll(); stream.Close();
        stream = fs.OpenTextFile(root + "\\crates\\daemon-core\\src\\ae_recovery.jsx", 1);
        nativeSource = stream.ReadAll(); stream.Close();
        print = function (text) { WScript.Echo(text); };
    }
    function assert(value, message) { if (!value) throw new Error(message); }
    function section(start, end) {
        var from = source.indexOf(start), to = source.indexOf(end, from);
        assert(from >= 0 && to > from, "source section exists: " + start);
        return source.substring(from, to);
    }
    var scheduler = section("// Check interval (ms)", "function isControlValid") +
        section("function setCommandCheckerTaskId", "applyDebugLogConfig(readDebugLogConfig());");
    function fixture(failFirstCommand) {
        var host = { tasks: {}, nextId: 0, checks: 0, events: 0, failSchedule: false, failFirstCommand: failFirstCommand, onCheck: null, onHeartbeat: null };
        host.app = {
            scheduleTask: function (code, delay, repeat) {
                if (host.onSchedule) host.onSchedule();
                if (host.failSchedule) throw new Error("schedule failure");
                if (host.failFirstCommand && code.indexOf("CommandTick") >= 0) { host.failFirstCommand = false; throw new Error("initial command schedule failure"); }
                var id = ++host.nextId;
                host.tasks[id] = { code: code, repeat: repeat };
                return id;
            },
            cancelTask: function (id) { delete host.tasks[id]; }
        };
        var create = new Function("host", "var app=host.app, $={global:{}}, AE_MCP_BRIDGE_VERSION='test', aeMcpPreviousInstanceId='', aeMcpLifecycleMode='startup-headless', aeMcpBootstrapConfig={};\n" +
            "function fxDateToIsoString(d){return String(d.getTime());} function logToPanel(){} function setStatus(){} function refreshPermissionState(){} function getBridgeInstanceId(){return 'test';} function isRenderQueueActive(){return false;} function removeInstanceHeartbeat(){}\n" +
            "function writeInstanceHeartbeat(){lastHeartbeatWriteAtMs=new Date().getTime();heartbeatSequence++;return true;} function heartbeatTick(){if(host.onHeartbeat)host.onHeartbeat();writeInstanceHeartbeat();} function checkForCommands(){host.checks++;lastCommandPollAtMs=new Date().getTime();if(host.onCheck)host.onCheck();}\n" + scheduler +
            "\nwriteSchedulerDiagnosticEvent=function(){host.events++;}; removeInstanceHeartbeat=function(){}; bridgeRuntime={}; $.global.__adobeMcpBridgeRuntime=bridgeRuntime;\n" +
            "return {start:startBridgeRuntime,stop:stopBridgeRuntime,restart:restartBridgeRuntime,ensure:ensureBridgeRuntimeHealthy,state:getBridgeRuntimeState,globals:$.global,owner:bridgeRuntime,age:function(){lastWatchdogTickAtMs=new Date().getTime();lastHeartbeatWriteAtMs=new Date().getTime()-20000;lastCommandPollAtMs=lastHeartbeatWriteAtMs;lastCommandTickCompletedAtMs=lastHeartbeatWriteAtMs;bridgeStartedAtMs=lastHeartbeatWriteAtMs;},ageCommand:function(){lastWatchdogTickAtMs=new Date().getTime();lastCommandPollAtMs=new Date().getTime()-20000;lastCommandTickCompletedAtMs=lastCommandPollAtMs;bridgeStartedAtMs=lastCommandPollAtMs;},busy:function(v){isChecking=v;currentRequestId=v?'nonce':'';}};");
        host.runtime = create(host);
        try { host.runtime.start(); } catch (error) { if (!failFirstCommand) throw error; host.startError = error; }
        host.tick = function (kind, generation) {
            var state = host.runtime.state(), key = kind === "Command" ? "commandTaskId" : kind === "Heartbeat" ? "heartbeatTaskId" : "watchdogTaskId";
            if (kind !== "Watchdog") delete host.tasks[state[key]];
            host.runtime.globals["__adobeMcpBridge" + kind + "Tick"](generation || state.runtimeId, function(args,mcp,code){return eval(code);});
        };
        return host;
    }
    var tests = 0;
    function test(name, body) { body(); tests++; print("PASS " + name); }
    test("in-flight callback: stale heartbeat, refresh/restart/stop/reentrant tick cannot replace or dispatch", function () {
        var f = fixture(), generation = f.runtime.state().runtimeId;
        f.onCheck = function () {
            f.runtime.age();
            assert(f.runtime.state().callbackActive, "callback active");
            assert(!f.runtime.state().healthy && f.runtime.state().healthReason === 'callback-in-flight', "busy is not proof of healthy idle progress");
            f.runtime.ensure(); f.runtime.restart(); f.runtime.stop(); f.tick("Watchdog"); f.tick("Command");
            assert(f.runtime.state().runtimeId === generation, "generation preserved");
            assert(f.checks === 1 && f.events === 0, "no duplicate or recovery");
        };
        f.tick("Command");
        assert(!f.runtime.state().callbackActive && f.runtime.state().commandTaskId, "finally re-arms");
        assert(f.runtime.state().healthReason !== "command-poll-stale", "slow completion counts as progress");
        f.tick("Heartbeat"); f.tick("Watchdog");
        assert(f.events === 0, "successful slow callback does not trigger recovery after returning");
    });
    test("heartbeat callback reentrancy is deferred", function () {
        var f = fixture();
        f.onHeartbeat = function () { f.runtime.age(); f.runtime.ensure(); f.runtime.restart(); assert(f.events === 0, "no recovery in heartbeat callback"); };
        f.tick("Heartbeat"); assert(f.runtime.state().heartbeatTaskId, "rearmed");
    });
    test("callback remains active throughout scheduleTask re-arm", function () {
        var f = fixture(), generation = f.runtime.state().runtimeId;
        f.onSchedule = function () {
            assert(f.runtime.state().callbackActive, "scheduleTask remains inside callback");
            f.runtime.ensure(); f.runtime.restart();
            assert(f.runtime.state().runtimeId === generation && f.events === 0, "re-arm reentrancy deferred");
        };
        f.tick("Command"); f.tick("Heartbeat");
        assert(!f.runtime.state().callbackActive, "flags clear after re-arm");
    });
    test("manual request reservation prevents recovery", function () {
        var f = fixture(); f.runtime.busy(true); f.runtime.age(); f.runtime.ensure(); f.runtime.restart(); f.tick("Watchdog");
        assert(f.events === 0 && f.checks === 0, "busy request never redispatched");
        f.runtime.busy(false); f.tick("Watchdog"); assert(f.events === 1, "idle recovery resumes");
    });
    test("idle loss of both one-shots recovers without panel interaction", function () {
        var f = fixture(), before = f.runtime.state();
        delete f.tasks[before.commandTaskId]; delete f.tasks[before.heartbeatTaskId]; f.runtime.age(); f.tick("Watchdog");
        assert(f.runtime.state().runtimeId !== before.runtimeId && f.events === 1, "watchdog recovers");
        assert(f.checks === 0, "recovery only schedules, never dispatches");
        f.tick("Command", before.runtimeId); assert(f.checks === 0, "old generation ignored");
        f.tick("Command"); assert(f.checks === 1, "new generation dispatches once");
    });
    test("command stall is detected even when heartbeat stays fresh", function () {
        var f = fixture(); f.runtime.ageCommand();
        assert(f.runtime.state().healthReason === "command-poll-stale", "command freshness checked");
        f.tick("Watchdog"); assert(f.events === 1, "recover command loop");
    });
    test("ordinary command exception re-arms once", function () {
        var f = fixture(); f.onCheck = function () { throw new Error("script failure"); }; f.tick("Command");
        assert(f.runtime.state().commandTaskId && !f.runtime.state().callbackActive, "exception finally");
        assert(f.runtime.state().lastSchedulerError.indexOf("command-tick:") === 0, "diagnostic retained");
    });
    test("failed re-arm recovers via surviving watchdog", function () {
        var f = fixture(); f.failSchedule = true; f.tick("Command"); f.failSchedule = false;
        assert(f.runtime.state().healthReason === "task-id-missing", "failed task detected");
        f.tick("Watchdog"); assert(f.events === 1 && f.runtime.state().commandTaskId, "watchdog restores task");
    });
    test("partial startup failure retains watchdog and recovers automatically", function () {
        var f = fixture(true);
        assert(f.startError && f.runtime.state().watchdogTaskId && !f.runtime.state().commandTaskId, "watchdog armed first");
        f.tick("Watchdog"); assert(f.events === 1 && f.runtime.state().healthy && f.checks === 0, "partial startup recovered without dispatch");
    });
    test("callback ownership mismatch is detected and repaired", function () {
        var f = fixture(); f.runtime.globals.__adobeMcpBridgeCommandTick = function () {};
        assert(f.runtime.state().healthReason === "callbacks-missing", "identity matters");
        f.tick("Watchdog"); assert(f.events === 1 && f.runtime.state().healthy, "callback restored");
    });
    test("superseded runtime callback cannot dispatch", function () {
        var f = fixture(), state = f.runtime.state(), originalCallback = f.runtime.globals.__adobeMcpBridgeCommandTick;
        f.runtime.globals.__adobeMcpBridgeRuntime = {}; f.tick("Command");
        f.runtime.start(); f.runtime.stop(); f.runtime.restart(); f.runtime.ensure();
        assert(f.checks === 0, "runtime ownership guard");
        assert(f.runtime.state().runtimeId === state.runtimeId && f.events === 0 &&
            f.runtime.globals.__adobeMcpBridgeCommandTick === originalCallback, "stale lifecycle cannot reclaim ownership");
    });
    test("headless reload during an in-flight request preserves the old runtime", function () {
        var stopped = 0, owner = { getState: function () { return { running: true, callbackActive: true }; }, stop: function () { stopped++; } };
        var globals = { __adobeMcpBridgeRuntime: owner, __adobeMcpBridgeBootstrapConfig: { headless: true } };
        var reload = section("var AE_MCP_BRIDGE_VERSION", "// --- createComposition");
        new Function("$", "thisObj", reload)({ global: globals }, {});
        assert(stopped === 0 && globals.__adobeMcpBridgeRuntime === owner, "in-flight reload deferred");
    });
    test("stop cancels schedules and late callbacks stay inert", function () {
        var f = fixture(), old = f.runtime.state().runtimeId; f.runtime.stop();
        for (var id in f.tasks) throw new Error("uncancelled task " + id);
        f.tick("Command", old); f.tick("Heartbeat", old); f.tick("Watchdog", old);
        assert(f.checks === 0 && !f.runtime.state().running, "stopped runtime inert");
    });
    // Exercise the real attachment code with minimal ScriptUI controls.
    var panelCode = section("function attachDiagnosticsPanel", "if (aeMcpAttachExistingRuntime)");
    test("docked and closed diagnostics panel resolves replacement runtime", function () {
        var controls = [], oldCalls = 0, newCalls = 0, resumed = false;
        function Panel() { this.layout = { layout: function () {}, resize: function () {} }; }
        Panel.prototype.add = function () { var control = { preferredSize: {} }; controls.push(control); return control; };
        var panel = new Panel(), globals = {};
        function runtime(counter) { return { getState: function () { return { running: true, healthy: true }; }, ensureHealthy: function () { throw new Error('A status refresh must not mutate the runtime'); }, restart: counter }; }
        var old = runtime(function () { oldCalls++; }), next = runtime(function () { newCalls++; }); globals.__adobeMcpBridgeRuntime = old;
        next.getAutoRunEnabled = function () { return resumed; };
        next.setAutoRunEnabled = function (enabled) { resumed = enabled; };
        new Function("thisObj", "Panel", "$", "runtime", "var AE_MCP_BRIDGE_VERSION='test';" + panelCode + "attachDiagnosticsPanel(runtime);")(panel, Panel, { global: globals }, old);
        assert(oldCalls === 0, "initial attachment only observes"); globals.__adobeMcpBridgeRuntime = next;
        panel.onActivate(); controls[3].onClick(); panel.onClose();
        assert(oldCalls === 0 && newCalls === 1, "only explicit restart mutates the replacement runtime");
        assert(controls[4].enabled && !controls[4].value, "reattached panel exposes the paused state");
        controls[4].value = true; controls[4].onClick();
        assert(resumed && controls[4].value && oldCalls === 0, "reattached panel resumes the current runtime");
    });
    test("fallback panel close keeps process runtime alive", function () {
        var closing = section("panel.onClose = function () {", "panel.onResizing = panel.onResize");
        var f = fixture(), panel = {};
        new Function("panel", "stopBridgeRuntime", "isAutoRunEnabled", closing)(panel, f.runtime.stop, function () { return true; });
        panel.onClose(); assert(f.runtime.state().running, "panel lifetime independent");
    });
    test("fallback panel pause survives destroyed controls and a temporary permission lock", function () {
        var control = { valid: true, enabled: true, value: false };
        var read = new Function("control", "var autoRunEnabled=true,autoRunCheckbox=control;function isControlValid(c){return c.valid;}" +
            section("function isAutoRunEnabled", "function safePanelUpdate") + "return isAutoRunEnabled;")(control);
        assert(read() === false, "explicit pause captured before panel closes");
        control.valid = false;
        assert(read() === false, "closed controls do not resume commands");
        control.valid = true; control.value = true;
        assert(read() === true, "explicit resume captured");
        control.enabled = false; control.value = false;
        assert(read() === false, "permission lock pauses execution");
        control.valid = false;
        assert(read() === true, "permission lock does not overwrite user choice");
    });
    test("reattached controls can change pause after the original panel is destroyed", function () {
        var control = { valid: true, enabled: true, value: false };
        var choice = new Function("control", "var autoRunEnabled=true,autoRunValueBeforePermissionLock=true,autoRunCheckbox=control;function isControlValid(c){return c.valid;}" +
            section("function isAutoRunEnabled", "function safePanelUpdate") + "return {read:isAutoRunEnabled,set:setAutoRunEnabled,restore:function(){control.valid=true;control.enabled=true;control.value=autoRunValueBeforePermissionLock;}};")(control);
        assert(!choice.read(), "original panel paused");
        control.valid = false;
        choice.set(true); assert(choice.read(), "explicit resume works without old controls");
        control.valid = true; control.enabled = false; control.value = false;
        choice.set(true); assert(!choice.read(), "permission lock remains effective");
        choice.restore(); assert(choice.read(), "new choice restored when permission returns");
        choice.set(false); assert(!control.value && !choice.read(), "live original controls also follow the new choice");
    });
    test("native recovery wrapper parses in ES3 without executing host code", function () {
        new Function(nativeSource.replace("/*CONFIG*/", "{}"));
    });
    print("PASS " + tests + " scheduler/ScriptUI regression tests");
})();
