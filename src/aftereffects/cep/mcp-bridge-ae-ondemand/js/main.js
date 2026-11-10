// AE MCP on-demand bridge: headless CEP companion for mcp-bridge-auto.jsx.
// Watches the instance command file from Node (zero After Effects cost) and
// asks the host to run exactly one bridge tick per command. No host timers.
(function () {
    var fs = require("fs");
    var path = require("path");
    var os = require("os");
    var cep = window.__adobe_cep__;

    var SAFETY_SCAN_MS = 5000;     // Node-only file read; never touches After Effects
    var HEARTBEAT_TOUCH_MS = 3000; // Node-only utimes; never touches After Effects
    var MAX_RETRIES = 2;
    // Stay out of After Effects' way while it is still starting (welcome screen,
    // plug-in and panel initialisation). Nothing needs the bridge this early.
    var STARTUP_DELAY_MS = 2000;
    var BOOTSTRAP_RETRY_MS = 5000;
    var BOOTSTRAP_MAX_ATTEMPTS = 6;

    var state = { instanceId: "", runtimeId: "", bridgeRoot: "", dir: "", commandFile: "", heartbeatFile: "" };
    var busy = false;
    var rerun = false;
    var watcher = null;

    function logFile() {
        var root = state.bridgeRoot;
        if (!root) {
            // Before bootstrap the host has not told us the bridge folder yet.
            var guess = path.join(os.homedir(), "Documents", "ae-mcp-bridge");
            root = fs.existsSync(guess) ? guess : os.tmpdir();
        }
        return path.join(root, "cep-on-demand.log");
    }

    function log(message) {
        try {
            fs.appendFileSync(logFile(), new Date().toISOString() + " " + message + "\n");
        } catch (e) {}
    }

    function evalScript(script, callback) {
        cep.evalScript(script, function (result) { callback(String(result)); });
    }

    function readBootstrapTemplate() {
        var candidates = [
            path.join(__dirname, "jsx", "bootstrap.jsx"),
            path.join(__dirname, "..", "jsx", "bootstrap.jsx")
        ];
        var i;
        for (i = 0; i < candidates.length; i++) {
            try { return fs.readFileSync(candidates[i], "utf8"); } catch (e) {}
        }
        throw new Error("bootstrap.jsx not found; __dirname=" + __dirname);
    }

    function bootstrap(callback) {
        evalScript(readBootstrapTemplate(), function (result) {
            var parts = result.split("|");
            if (parts[0] !== "ok" || parts.length < 5) {
                log("bootstrap failed: " + result);
                callback(false, result);
                return;
            }
            state.instanceId = parts[1];
            state.runtimeId = parts[2];
            state.bridgeRoot = parts.slice(4).join("|");
            state.dir = path.join(state.bridgeRoot, "instances", state.instanceId);
            state.commandFile = path.join(state.dir, "ae_command.json");
            state.heartbeatFile = path.join(state.dir, "heartbeat.json");
            log("bootstrap ok instance=" + state.instanceId + " runtime=" + state.runtimeId + " engine=" + parts[3]);
            callback(true, result);
        });
    }

    function commandIsPending() {
        try {
            var data = JSON.parse(fs.readFileSync(state.commandFile, "utf8"));
            return data.status === "pending" || data.status === "running";
        } catch (e) {
            return false;
        }
    }

    function tickOnce(attempt) {
        var script = "$.global.__adobeMcpBridgeCommandTick(" + JSON.stringify(state.runtimeId) +
            ",function(args,mcp,code){return eval(code);})";
        evalScript(script, function () {
            if (commandIsPending() && attempt < MAX_RETRIES) {
                log("command still pending after tick; refreshing runtime id (attempt " + (attempt + 1) + ")");
                bootstrap(function (ok) {
                    if (ok) { tickOnce(attempt + 1); } else { finish(); }
                });
                return;
            }
            finish();
        });
    }

    function finish() {
        busy = false;
        if (rerun) { rerun = false; check(); }
    }

    function check() {
        if (!state.commandFile) { return; }
        if (busy) { rerun = true; return; }
        if (!commandIsPending()) { return; }
        busy = true;
        tickOnce(0);
    }

    function watch() {
        try { if (watcher) { watcher.close(); } } catch (e) {}
        try {
            watcher = fs.watch(state.dir, function () { check(); });
            watcher.on("error", function (err) { log("watch error: " + err); });
        } catch (e) {
            log("fs.watch failed: " + e);
        }
    }

    function touchHeartbeat() {
        if (!state.heartbeatFile) { return; }
        try {
            var now = new Date();
            fs.utimesSync(state.heartbeatFile, now, now);
        } catch (e) {}
    }

    // These failures cannot fix themselves, so retrying would only add host calls.
    function isPermanentFailure(result) {
        return result.indexOf("error|runtime-not-found") === 0 ||
            result.indexOf("error|timer-runtime-already-loaded") === 0 ||
            result.indexOf("error|runtime-api-missing") === 0;
    }

    // Diagnostics only: how long after launch the panel loads and the bridge comes up.
    var loadedAt = Date.now();

    function hostLaunchTime() {
        try {
            if (process.platform !== "darwin") { return ""; }
            var out = require("child_process").execSync(
                "ps -axo lstart=,command= | grep 'MacOS/After Effects' | grep -v grep | head -1",
                { encoding: "utf8", timeout: 2000 });
            var started = new Date(out.substring(0, 24).trim()).getTime();
            return isNaN(started) ? "" : String(Math.round((Date.now() - started) / 1000));
        } catch (e) { return ""; }
    }

    function start(attempt) {
        bootstrap(function (ok, result) {
            log("bootstrap attempt " + attempt + (ok ? " ok" : " failed") + " " + (Date.now() - loadedAt) + " ms after panel load");
            if (!ok) {
                if (!isPermanentFailure(result) && attempt < BOOTSTRAP_MAX_ATTEMPTS) {
                    setTimeout(function () { start(attempt + 1); }, BOOTSTRAP_RETRY_MS);
                }
                return;
            }
            watch();
            setInterval(check, SAFETY_SCAN_MS);
            setInterval(touchHeartbeat, HEARTBEAT_TOUCH_MS);
            check();
        });
    }

    window.onerror = function (message, source, line) { log("uncaught: " + message + " @" + line); };
    log("panel loaded; ae_uptime_s=" + hostLaunchTime());
    setTimeout(function () {
        try { start(1); } catch (e) { log("start failed: " + e); }
    }, STARTUP_DELAY_MS);
})();
