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

## File I/O compatibility

The UXP `fs` module is not a complete Node.js implementation. Bridge writes
use the supported asynchronous `writeFile`, `rename`, and `unlink` APIs.
Temporary-file publication is serialized. The compatibility result and final
command status are saved before publishing the request-specific result, which
acknowledges completion to the Rust client. The old command is never written
after that acknowledgement, so the next pending request is preserved. Failed writes retain the
previous destination and remove their temporary file when possible.

Raw UXP code must await `bridge.writeTextFile(...)` and
`bridge.writeJsonFile(...)`. Reads remain synchronous.

Run the packaged runtime's file I/O regression tests with Node.js 24 or newer:

```sh
node --test scripts/tests/premiere-uxp-bridge.test.cjs
```

See the [Adobe UXP fs reference](https://developer.adobe.com/photoshop/uxp/2022/uxp-api/reference-js/modules/fs/fs)
for the supported API surface. Actual host acceptance requires a fresh heartbeat
and a successful `list-premiere-instances` / `run-bridge-test` round trip.
