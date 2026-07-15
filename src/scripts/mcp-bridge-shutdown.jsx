// mcp-bridge-shutdown.jsx
// Removes the live heartbeat when After Effects performs a normal shutdown.
#target aftereffects
#targetengine "ae_mcp_bridge"

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
