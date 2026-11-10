# After Effects on-demand bridge (opt-in)

Status: experimental, opt-in. The Startup loader (`mcp-bridge-startup.jsx`) remains the
default and is unchanged.

## Problem

`mcp-bridge-auto.jsx` keeps three `app.scheduleTask` loops alive for the lifetime of
the AE session (command check every 2 s, heartbeat every 3 s, repeating watchdog every
3 s). On macOS (AE 26.5, Apple silicon) each scheduled callback resets the host
cursor: hovering anything that should show a non-arrow cursor (resize handles,
pointer, I-beam, custom CEP cursors) shows it briefly and then reverts to the arrow
with the mouse perfectly still. It affects Adobe's native elements too.

Isolation performed:

| Configuration | Cursor reverts? |
| --- | --- |
| AE safe mode (no plugins/scripts/extensions) | No |
| Plugins disabled, scripts and extensions enabled | Yes |
| All CEP extension processes killed | Yes |
| Startup bridge script removed, everything else enabled | No |
| Bridge `checkInterval` and `heartbeatInterval` raised to 30 s | Holds between ticks |
| On-demand mode (this document) | No |

A 30 s polling interval is not workable for agentic use, so the polling is removed
instead.

## Design

`aeMcpBootstrapConfig.onDemand === true` makes `mcp-bridge-auto.jsx`:

- skip `app.scheduleTask` for the command check, heartbeat, watchdog and recovery probe,
- report `schedulerMode: "on-demand"`,
- treat scheduler health as satisfied (the companion supplies liveness).

The CEP companion in `src/aftereffects/cep/mcp-bridge-ae-ondemand` loads the runtime
with that flag, watches the command file from Node and calls
`__adobeMcpBridgeCommandTick` once per pending command. See that folder's README for
install and verification steps.

## Measured (macOS, AE 26.5x89)

- Idle: zero scheduled callbacks (`heartbeatTickSequence` and `watchdogTickSequence`
  stay 0).
- Command latency (read-only `executeJsx`, end to end through the MCP server): about
  0.7 s.
- Write with `undoGroup: true`: one command, one Undo step restores the project.
- Custom cursors hold indefinitely.

## Install

macOS: `scripts/install-bridge.sh --on-demand` installs `mcp-bridge-auto.jsx`, the CEP
companion and the shutdown script, enables CEP `PlayerDebugMode` (CSXS 9 to 12) and moves
any existing `Scripts/Startup/mcp-bridge-startup.jsx` to `Scripts/Startup_backup/`
(otherwise the extension reports `timer-runtime-already-loaded`).

Windows (manual): copy `src/aftereffects/cep/mcp-bridge-ae-ondemand` to
`%APPDATA%\Adobe\CEP\extensions\`, set `HKCU\Software\Adobe\CSXS.<9..12>\PlayerDebugMode`
to `1` (string), and remove the Startup loader.

## Startup behaviour

The extension waits 8 seconds after After Effects launches before its first host call and
retries transient bootstrap failures (up to 6 times, 5 seconds apart). This keeps it out of
the way of the welcome screen and plug-in initialisation. The bridge is therefore not
available for the first few seconds after launch.

## Verified (macOS, AE 26.5x89)

- Read-only and write commands, one command tick per command, no heartbeat or watchdog ticks.
- Long-running script (a 15 s blocking `$.sleep`): completed successfully.
- Render queue (`renderQueue.render()` of a small comp, run inside a command): finished with
  status DONE and wrote its output file.

## Not covered yet

- Windows (host-side path discovery uses `Folder.appPackage` and its parent; the manual
  install steps above are unverified; `install-bridge.ps1` has no `-OnDemand` switch yet).
- Behaviour while After Effects is blocked on a modal dialog (the Node-side heartbeat keeps
  the instance looking alive).
