# Photoshop MCP Bridge UXP

This is the UXP implementation of the Photoshop MCP Bridge.

## Development Load

1. Open Photoshop 23.3 or newer.
2. Open Adobe UXP Developer Tool.
3. Add this plugin by selecting `manifest.json` in this folder.
4. Click `Load` or `Load & Watch`.
5. Open the `Photoshop MCP Bridge` panel from the Photoshop Plugins menu.
6. Keep `Auto-run commands` enabled.

Start `adobe-mcp-app` (or this branch's `ps-mcp serve-daemon`) before or after loading
the plugin. Reception starts automatically and continues while the panel is hidden.
The plugin connects over an authenticated loopback WebSocket using bootstrap
settings in `~/Documents/ps-mcp-bridge/connection.json`. Commands, results and
heartbeat do not use files. The application keeps the durable request registry.

To use the legacy file transport, set `transport` to `file` in `connection.json`
and reload the plugin before sending commands. A dropped WebSocket never switches
to files or automatically re-executes an uncertain command. Results are retained
in UXP memory until the application acknowledges them; plugin/host termination
before acknowledgement can lose that result. See `docs/desktop-app.md` for recovery.
