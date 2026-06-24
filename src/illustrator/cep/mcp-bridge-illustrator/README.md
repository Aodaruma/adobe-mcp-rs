# Illustrator MCP Bridge

CEP/ExtendScript bridge panel for `ai-mcp`.

Install the `mcp-bridge-illustrator` folder into an Adobe CEP extensions directory, then open Illustrator and choose `Window > Extensions > Illustrator MCP Bridge`.

Bridge files are written under:

```text
~/Documents/ai-mcp-bridge/
  ai_command.json
  ai_mcp_result.json
  instances/<instanceId>/
    heartbeat.json
    ai_command.json
    ai_mcp_result.json
```

For local unsigned CEP development, Illustrator may require CEP debug mode to be enabled.
