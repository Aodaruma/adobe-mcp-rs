# Photoshop MCP Bridge UXP

This is the UXP implementation of the Photoshop MCP Bridge.

## Development Load

1. Open Photoshop 23.3 or newer.
2. Open Adobe UXP Developer Tool.
3. Add this plugin by selecting `manifest.json` in this folder.
4. Click `Load` or `Load & Watch`.
5. Open the `Photoshop MCP Bridge` panel from the Photoshop Plugins menu.
6. Keep `Auto-run commands` enabled.

The bridge uses `~/Documents/ps-mcp-bridge` and mirrors command results to
`ps_mcp_result.json` for compatibility with the `ps-mcp` binary.
