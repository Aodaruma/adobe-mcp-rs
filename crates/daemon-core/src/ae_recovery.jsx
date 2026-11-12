// Fixed native recovery payload. It never executes or replays a user command.
(function (config) {
    var outcome = "refused";
    var before = null;
    var after = null;
    try {
        if (new Date().getTime() > config.expiresAtMs) return;
        if (String(app.version) !== config.appVersion) return;
        var runtime = $.global.__adobeMcpBridgeRuntime;
        if (!runtime || !runtime.getState || !runtime.ensureHealthy) return;
        before = runtime.getState();
        if (before.version !== "0.5.5" || before.instanceId !== config.instanceId ||
            before.runtimeId !== config.runtimeId || !before.running ||
            before.callbackActive || before.currentRequestId || before.renderQueueActive) return;
        var reservationFile = new File(config.reservationFile);
        var pendingId = config.pendingRequestId || "";
        if (!pendingId && reservationFile.exists) return;
        var command = new File(config.commandFile);
        if (command.exists) {
            if (!command.open("r")) return;
            var raw = command.read();
            command.close();
            var pending = JSON.parse(raw);
            if (pendingId) {
                if (!/^[A-Za-z0-9_-]{1,160}$/.test(pendingId) || pending.status !== "pending" || pending.requestId !== pendingId) return;
                if (!reservationFile.open("r")) return;
                var reservationRaw = reservationFile.read();
                reservationFile.close();
                var reservation = JSON.parse(reservationRaw);
                if (reservation.requestId !== pendingId || reservation.command !== pending.command || reservation.status !== "running") return;
            } else if (pending.status !== "completed" && pending.status !== "error") return;
        } else if (pendingId) {
            return;
        }
        // File reads must not extend a queued wrapper's admission deadline.
        if (new Date().getTime() > config.expiresAtMs) return;
        after = runtime.ensureHealthy({ reason: "native-idle-recovery" });
        outcome = after.runtimeId !== before.runtimeId ? "rearmed" :
            (after.recoveryProbeTaskId ? "deferred" : "unchanged");
    } catch (error) {
        outcome = "error: " + String(error);
    } finally {
        try {
            var result = new File(config.resultFile);
            result.encoding = "UTF-8";
            if (result.open("w")) {
                result.write(JSON.stringify({ nonce: config.nonce, outcome: outcome,
                    before: before, after: after }));
                result.close();
            }
        } catch (_resultError) {}
    }
})(/*CONFIG*/);
