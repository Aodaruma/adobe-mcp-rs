// mcp-bridge-shutdown.jsx
// Removes the live heartbeat when After Effects performs a normal shutdown.
#target aftereffects
// Resolve the runtime in AE's shared engine, just as Startup and panel scripts do.

(function () {
    try {
        var runtime = $.global.__adobeMcpBridgeRuntime;
        if (runtime && runtime.stop) {
            runtime.stop({
                removeHeartbeat: true,
                reason: "after-effects-shutdown"
            });
        }
    } catch (_shutdownErr) {}
})();
