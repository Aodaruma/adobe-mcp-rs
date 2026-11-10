# AE MCP on-demand bridge (CEP companion)

Optional headless CEP extension that lets `mcp-bridge-auto.jsx` run **without any
`app.scheduleTask` timers**.

The default After Effects bridge polls for commands and writes a heartbeat from
`app.scheduleTask` callbacks every 2–3 seconds for as long as AE is open. On macOS
each scheduled callback resets the host cursor, so custom/resize/pointer cursors
revert to the arrow within a few seconds of hovering (in every panel, including
Adobe's own). This extension removes that idle work:

- A Node `fs.watch` on `instances/<instanceId>/ae_command.json` detects a new
  command. Node-side work costs After Effects nothing.
- When a command is `pending`, the extension calls the existing
  `__adobeMcpBridgeCommandTick` **once** through `CSInterface.evalScript`. All
  protocol handling (results, request IDs, undo grouping, permissions) is the
  unchanged bridge code.
- `heartbeat.json` freshness is kept by touching the file's mtime from Node (the
  server only checks the mtime). The host rewrites it with real state on every
  command.
- A 5-second Node-only scan backs up a missed file event.

## Install

On macOS run `scripts/install-bridge.sh --on-demand` to do all of this automatically. Manually:

1. Copy this folder into the CEP extensions directory
   (`~/Library/Application Support/Adobe/CEP/extensions/` on macOS,
   `%APPDATA%\Adobe\CEP\extensions\` on Windows).
2. Enable unsigned extensions if you have not already (`PlayerDebugMode=1` for your
   CSXS version).
3. **Remove `mcp-bridge-startup.jsx` from `Scripts/Startup`.** Otherwise the timer-mode
   runtime loads first and this extension reports `timer-runtime-already-loaded`.
   `mcp-bridge-auto.jsx` must stay in `Scripts/ScriptUI Panels`.
4. Restart After Effects. The extension starts automatically (no panel).

## Verify

- `~/Documents/ae-mcp-bridge/cep-on-demand.log` contains `bootstrap ok ... engine=main`.
- `instances/<id>/heartbeat.json` reports `"schedulerMode": "on-demand"`,
  `"schedulerReady": true` and `heartbeatTickSequence` / `watchdogTickSequence` stay 0.
- Run any read-only command; `commandTickSequence` increments once per command.

## Notes and limits

- If After Effects hangs on a modal dialog, the Node-side heartbeat keeps the
  instance looking alive; the pending command times out instead of the instance
  going stale.
- The default Startup-loader mode is unchanged and remains the documented setup.
