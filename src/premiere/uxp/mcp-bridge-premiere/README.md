# Premiere MCP Bridge UXP

This is the UXP implementation of the Premiere MCP Bridge.

## Development Load

1. Open Premiere Pro 25.6 or newer.
2. Enable developer mode in Premiere Pro settings, then restart Premiere Pro.
3. Open Adobe UXP Developer Tool.
4. Add this plugin by selecting `manifest.json` in this folder.
5. Click `Load` or `Load & Watch`.
6. Open `Window > UXP Plugins > Premiere MCP Bridge`.

The bridge uses `~/Documents/pr-mcp-bridge` and mirrors command results to
`pr_mcp_result.json` for compatibility with the existing `pr-mcp` binary.
