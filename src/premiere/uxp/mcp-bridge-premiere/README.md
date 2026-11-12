# Premiere MCP Bridge UXP

This is the UXP implementation of the Premiere MCP Bridge.

## Development Load

1. Open Premiere Pro 25.6 or newer.
2. Enable developer mode in Premiere Pro settings, then restart Premiere Pro.
3. Open Adobe UXP Developer Tool.
4. Add this plugin by selecting `manifest.json` in this folder.
5. Click `Load` or `Load & Watch`.
6. Open `Window > UXP Plugins > Premiere MCP Bridge`.

Start `adobe-mcp-app` (or this branch's `pr-mcp serve-daemon`) before or after loading
the plugin. Reception starts automatically and continues while the panel is hidden.
The plugin connects over an authenticated loopback WebSocket using bootstrap
settings in `~/Documents/pr-mcp-bridge/connection.json`. Commands, results and
heartbeat do not use files. The application keeps the durable request registry.

To use the legacy file transport, set `transport` to `file` in `connection.json`
and reload the plugin before sending commands. A dropped WebSocket never switches
to files or automatically re-executes an uncertain command. Results are retained
in UXP memory until the application acknowledges them; plugin/host termination
before acknowledgement can lose that result. See `docs/desktop-app.md` for recovery.
