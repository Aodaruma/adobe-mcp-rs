use anyhow::{anyhow, Context, Result};
use bridge_core::BridgeClient;
use mcp_core::{
    capabilities_value, effects_help_text, general_help_text, legacy_tool_replacement,
    prepare_script, prompt_messages, prompt_specs, tool_specs, validate_and_read_script_file,
    AppConfig, AFTER_EFFECTS_HOST,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tracing::{debug, error, info};

const MAX_JSX_BYTES: usize = 1_048_576;
const DIAGNOSTIC_CONFIG_FILE: &str = "ae_mcp_debug_config.json";
const DIAGNOSTIC_LOG_FILE: &str = "ae_mcp_debug.log";
const SCHEDULER_DIAGNOSTIC_FILE: &str = "ae_mcp_scheduler_diagnostic.json";
const DIAGNOSTIC_LOG_READ_BYTES: u64 = 1_048_576;
const DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT: usize = 10;

#[derive(Debug, Clone, Copy)]
enum MessageFormat {
    ContentLength,
    JsonLine,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

pub async fn run_stdio_server(cfg: AppConfig) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = stdout;
    let bridge = BridgeClient::new(cfg.clone())?;

    info!("MCP stdio server started");

    while let Some((message, format)) = read_jsonrpc_message(&mut reader).await? {
        debug!("received message: {}", message);
        let response = match serde_json::from_value::<JsonRpcRequest>(message.clone()) {
            Ok(req) => handle_request(&cfg, &bridge, req).await,
            Err(e) => Some(error_response(
                Value::Null,
                -32700,
                format!("invalid request payload: {e}"),
            )),
        };

        if let Some(payload) = response {
            write_jsonrpc_message(&mut writer, &payload, format).await?;
        }
    }

    info!("stdio stream closed");
    Ok(())
}

async fn handle_request(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    req: JsonRpcRequest,
) -> Option<Value> {
    let Some(id) = req.id.clone() else {
        if req.method.starts_with("notifications/") || req.method == "initialized" {
            return None;
        }
        return None;
    };

    let result = match req.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-06-18",
            "serverInfo": {
                "name": "AfterEffectsServer",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false, "subscribe": false },
                "prompts": { "listChanged": false }
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list_result()),
        "tools/call" => tools_call_result(cfg, bridge, &req.params),
        "resources/list" => Ok(resources_list_result()),
        "resources/read" => resources_read_result(cfg, bridge, &req.params),
        "prompts/list" => Ok(prompts_list_result()),
        "prompts/get" => prompts_get_result(&req.params),
        _ => Err(anyhow!("method not found: {}", req.method)),
    };

    Some(match result {
        Ok(value) => success_response(id, value),
        Err(e) => {
            error!("request handling failed: {e}");
            error_response(id, -32601, e.to_string())
        }
    })
}

fn tools_list_result() -> Value {
    let tools = tool_specs()
        .into_iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema
            })
        })
        .collect::<Vec<_>>();

    json!({ "tools": tools })
}

fn resources_list_result() -> Value {
    json!({
        "resources": [
            {
                "name": "compositions",
                "description": "List compositions in the current After Effects project",
                "uri": "aftereffects://compositions",
                "mimeType": "application/json"
            }
        ]
    })
}

fn prompts_list_result() -> Value {
    let prompts = prompt_specs()
        .into_iter()
        .map(|p| {
            json!({
                "name": p.name,
                "description": p.description,
                "arguments": p.arguments
            })
        })
        .collect::<Vec<_>>();

    json!({ "prompts": prompts })
}

fn prompts_get_result(params: &Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("prompts/get requires string parameter 'name'"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    prompt_messages(name, &args).ok_or_else(|| anyhow!("unknown prompt: {name}"))
}

fn resources_read_result(cfg: &AppConfig, _bridge: &BridgeClient, params: &Value) -> Result<Value> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("resources/read requires parameter 'uri'"))?;

    if uri != "aftereffects://compositions" {
        return Err(anyhow!("unknown resource URI: {uri}"));
    }

    let outcome = daemon_core::call_daemon(
        cfg,
        compositions_resource_daemon_request(cfg),
        cfg.result_timeout_ms + 1_000,
    )?;
    let text = serde_json::to_string_pretty(&outcome)
        .with_context(|| "failed to serialize resource result")?;

    Ok(json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": "application/json",
                "text": text
            }
        ]
    }))
}

fn compositions_resource_daemon_request(cfg: &AppConfig) -> Value {
    json!({
        "op": "runCommand",
        "command": "listCompositions",
        "args": {},
        "timeoutMs": cfg.result_timeout_ms + 1_000,
        "pollIntervalMs": cfg.poll_interval_ms,
        "retentionSeconds": cfg.result_retention_seconds
    })
}

fn tools_call_result(cfg: &AppConfig, bridge: &BridgeClient, params: &Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("tools/call requires 'name'"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    Ok(dispatch_tool(cfg, bridge, name, args))
}

fn dispatch_tool(cfg: &AppConfig, bridge: &BridgeClient, name: &str, args: Value) -> Value {
    let canonical_run_script = name == "run-script" && args.get("code").is_some();
    let result = dispatch_tool_inner(cfg, bridge, name, args);
    let value = match result {
        Ok(value) => value,
        Err(e) => tool_error(format!("Error: {e}")),
    };

    match if canonical_run_script {
        None
    } else {
        legacy_tool_replacement(name)
    } {
        Some(replacement) => with_legacy_tool_notice(value, name, replacement),
        None => value,
    }
}

fn with_legacy_tool_notice(mut value: Value, name: &str, replacement: &str) -> Value {
    let notice = format!(
        "Deprecated compatibility tool `{name}` is not advertised by `tools/list`. This call was accepted for backward compatibility; migrate to the public `{replacement}` tool."
    );

    if let Some(text) = value
        .get_mut("content")
        .and_then(Value::as_array_mut)
        .and_then(|content| content.first_mut())
        .and_then(|item| item.get_mut("text"))
        .and_then(|text| text.as_str())
        .map(str::to_string)
    {
        value["content"][0]["text"] = Value::String(format!("{notice}\n\n{text}"));
        return value;
    }

    if let Some(object) = value.as_object_mut() {
        object.insert(
            "content".to_string(),
            json!([{ "type": "text", "text": notice }]),
        );
    }
    value
}

fn dispatch_tool_inner(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    name: &str,
    args: Value,
) -> Result<Value> {
    match name {
        "run-jsx" => run_jsx_tool(cfg, bridge, args, true),
        "run-jsx-file" | "run-script-file" => run_jsx_file_tool(cfg, bridge, args),
        "get-jsx-result" | "get-script-result" => get_jsx_result_tool(cfg, args),
        "get-capabilities" => get_capabilities_tool(cfg),
        "cancel-script-request" => cancel_script_request_tool(cfg, args),
        "list-ae-instances" => list_ae_instances_tool(cfg),
        "configure-bridge-diagnostics" => configure_bridge_diagnostics_tool(cfg, args),
        "get-bridge-diagnostics" => get_bridge_diagnostics_tool(cfg, args),
        "run-script" if args.get("code").is_some() => run_jsx_tool(cfg, bridge, args, false),
        "run-script" => {
            let script = args
                .get("script")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("'script' is required"))?;
            if !mcp_core::is_allowed_script(script) {
                return Ok(tool_error(format!(
                    "Script \"{script}\" is not allowed. Allowed scripts are: {}",
                    mcp_core::ALLOWED_SCRIPTS.join(", ")
                )));
            }
            let parameters = args.get("parameters").cloned().unwrap_or_else(|| json!({}));
            run_daemon_command(
                cfg,
                script,
                parameters,
                &args,
                timeout_ms_from_args(cfg, &args),
                false,
                None,
                "Error running compatibility script",
            )
        }
        "get-results" => {
            if let Some(request_id) = args.get("requestId").and_then(Value::as_str) {
                let value = daemon_core::call_daemon(
                    cfg,
                    json!({
                        "op": "getResult",
                        "requestId": request_id
                    }),
                    cfg.result_timeout_ms,
                )?;
                Ok(tool_json(value)?)
            } else {
                let value = daemon_core::call_daemon(
                    cfg,
                    json!({
                        "op": "latestResult"
                    }),
                    cfg.result_timeout_ms,
                )?;
                Ok(tool_json(value)?)
            }
        }
        "get-help" => Ok(tool_text(general_help_text().to_string())),
        "create-composition" => run_direct_bridge_call(
            cfg,
            bridge,
            "createComposition",
            args,
            "Error creating composition",
        ),
        "setLayerKeyframe" => run_direct_bridge_call(
            cfg,
            bridge,
            "setLayerKeyframe",
            args,
            "Error setting layer keyframe",
        ),
        "setLayerExpression" => run_direct_bridge_call(
            cfg,
            bridge,
            "setLayerExpression",
            args,
            "Error setting layer expression",
        ),
        "test-animation" => create_test_animation_script(args),
        "apply-effect" => {
            run_direct_bridge_call(cfg, bridge, "applyEffect", args, "Error applying effect")
        }
        "apply-effect-template" => run_direct_bridge_call(
            cfg,
            bridge,
            "applyEffectTemplate",
            args,
            "Error applying effect template",
        ),
        "list-supported-effects" => run_direct_bridge_call(
            cfg,
            bridge,
            "listSupportedEffects",
            args,
            "Error listing supported effects",
        ),
        "describe-effect" => {
            let has_effect_name = args
                .get("effectName")
                .and_then(Value::as_str)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            let has_effect_match_name = args
                .get("effectMatchName")
                .and_then(Value::as_str)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if !has_effect_name && !has_effect_match_name {
                return Ok(tool_error(
                    "Either 'effectName' or 'effectMatchName' is required.".to_string(),
                ));
            }

            run_direct_bridge_call(
                cfg,
                bridge,
                "describeEffect",
                args,
                "Error describing effect",
            )
        }
        "save-frame-png" => {
            let timeout_ms = timeout_ms_from_args(cfg, &args);
            run_direct_bridge_call_with_timeout(
                cfg,
                bridge,
                "saveFramePng",
                args,
                "Error saving PNG frame",
                timeout_ms,
            )
        }
        "render-queue-add" => run_direct_bridge_call(
            cfg,
            bridge,
            "renderQueueAdd",
            args,
            "Error adding render queue item",
        ),
        "render-queue-status" => run_direct_bridge_call(
            cfg,
            bridge,
            "renderQueueStatus",
            args,
            "Error reading render queue status",
        ),
        "render-queue-start" => {
            let wait_timeout_seconds = args
                .get("waitTimeoutSeconds")
                .and_then(Value::as_u64)
                .unwrap_or(7_200);
            let requested_timeout_ms = wait_timeout_seconds
                .checked_mul(1_000)
                .ok_or_else(|| anyhow!("waitTimeoutSeconds is too large"))?;
            if requested_timeout_ms == 0 {
                anyhow::bail!("waitTimeoutSeconds must be greater than 0");
            }
            // Only the client wait is bounded. The broker keeps the request ID
            // and instance reservation until the render actually returns.
            let timeout_ms = requested_timeout_ms.min(cfg.script_contract.max_timeout_ms);
            run_direct_bridge_call_with_timeout(
                cfg,
                bridge,
                "renderQueueStart",
                args,
                "Error starting render queue",
                timeout_ms,
            )
        }
        "render-queue-is-rendering" => run_direct_bridge_call(
            cfg,
            bridge,
            "renderQueueIsRendering",
            args,
            "Error checking render state",
        ),
        "set-current-time" => run_direct_bridge_call(
            cfg,
            bridge,
            "setCurrentTime",
            args,
            "Error setting current time",
        ),
        "get-current-time" => run_direct_bridge_call(
            cfg,
            bridge,
            "getCurrentTime",
            args,
            "Error reading current time",
        ),
        "set-work-area" => {
            run_direct_bridge_call(cfg, bridge, "setWorkArea", args, "Error setting work area")
        }
        "get-work-area" => {
            run_direct_bridge_call(cfg, bridge, "getWorkArea", args, "Error reading work area")
        }
        "get-composition-markers" => run_direct_bridge_call(
            cfg,
            bridge,
            "getCompositionMarkers",
            args,
            "Error reading composition markers",
        ),
        "cleanup-preview-folder" => {
            let timeout_ms = timeout_ms_from_args(cfg, &args);
            run_daemon_command(
                cfg,
                "cleanupPreviewFolder",
                args.clone(),
                &args,
                timeout_ms,
                true,
                None,
                "Error cleaning up preview folder",
            )
        }
        "set-suppress-dialogs" => run_direct_bridge_call(
            cfg,
            bridge,
            "setSuppressDialogs",
            args,
            "Error setting dialog suppression",
        ),
        "get-suppress-dialogs" => run_direct_bridge_call(
            cfg,
            bridge,
            "getSuppressDialogs",
            args,
            "Error reading dialog suppression",
        ),
        "project-open" => {
            run_direct_bridge_call(cfg, bridge, "projectOpen", args, "Error opening project")
        }
        "project-close" => {
            run_direct_bridge_call(cfg, bridge, "projectClose", args, "Error closing project")
        }
        "project-save" => {
            run_direct_bridge_call(cfg, bridge, "projectSave", args, "Error saving project")
        }
        "project-save-as" => run_direct_bridge_call(
            cfg,
            bridge,
            "projectSaveAs",
            args,
            "Error saving project as",
        ),
        "application-quit" => run_direct_bridge_call(
            cfg,
            bridge,
            "applicationQuit",
            args,
            "Error quitting After Effects",
        ),
        "mcp_aftereffects_applyEffect" => {
            run_direct_bridge_call(cfg, bridge, "applyEffect", args, "Error applying effect")
        }
        "mcp_aftereffects_applyEffectTemplate" => run_direct_bridge_call(
            cfg,
            bridge,
            "applyEffectTemplate",
            args,
            "Error applying effect template",
        ),
        "mcp_aftereffects_listSupportedEffects" => run_direct_bridge_call(
            cfg,
            bridge,
            "listSupportedEffects",
            args,
            "Error listing supported effects",
        ),
        "mcp_aftereffects_describeEffect" => {
            let has_effect_name = args
                .get("effectName")
                .and_then(Value::as_str)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            let has_effect_match_name = args
                .get("effectMatchName")
                .and_then(Value::as_str)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if !has_effect_name && !has_effect_match_name {
                return Ok(tool_error(
                    "Either 'effectName' or 'effectMatchName' is required.".to_string(),
                ));
            }
            run_direct_bridge_call(
                cfg,
                bridge,
                "describeEffect",
                args,
                "Error describing effect",
            )
        }
        "mcp_aftereffects_get_effects_help" => Ok(tool_text(effects_help_text().to_string())),
        "run-bridge-test" => run_direct_bridge_call(
            cfg,
            bridge,
            "bridgeTestEffects",
            args,
            "Error running bridge test",
        ),
        _ => Ok(tool_error(format!("Unknown tool: {name}"))),
    }
}

fn run_direct_bridge_call(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    command: &str,
    args: Value,
    error_prefix: &str,
) -> Result<Value> {
    let timeout_ms = timeout_ms_from_args(cfg, &args);
    run_direct_bridge_call_with_timeout(cfg, bridge, command, args, error_prefix, timeout_ms)
}

fn run_direct_bridge_call_with_timeout(
    cfg: &AppConfig,
    _bridge: &BridgeClient,
    command: &str,
    args: Value,
    error_prefix: &str,
    timeout_ms: u64,
) -> Result<Value> {
    run_daemon_command(
        cfg,
        command,
        args.clone(),
        &args,
        timeout_ms,
        false,
        None,
        error_prefix,
    )
}

fn run_jsx_tool(
    cfg: &AppConfig,
    _bridge: &BridgeClient,
    args: Value,
    legacy: bool,
) -> Result<Value> {
    let code = required_non_empty_string(&args, "code")?.to_string();
    validate_unsafe_mode(&args)?;
    let undo_group = requested_undo_group(&args)?;
    let limit = if legacy {
        MAX_JSX_BYTES as u64
    } else {
        cfg.script_contract.max_inline_bytes
    };
    let prepared = prepare_script(cfg, AFTER_EFFECTS_HOST, &args, code, "unsafe", None, limit)?;
    if prepared.preflight_only {
        return tool_json(prepared.preflight_envelope(AFTER_EFFECTS_HOST));
    }

    let payload = json!({
        "code": prepared.code,
        "args": prepared.input,
        "mode": "unsafe",
        "description": prepared.description,
        "undoGroup": undo_group,
    });
    let timeout_ms = prepared.timeout_ms;
    let audit = serde_json::to_value(&prepared.audit)?;

    run_daemon_command(
        cfg,
        "executeJsx",
        payload,
        &args,
        timeout_ms,
        false,
        Some(audit),
        "Error running JSX",
    )
}

fn run_jsx_file_tool(cfg: &AppConfig, _bridge: &BridgeClient, args: Value) -> Result<Value> {
    let path = required_non_empty_string(&args, "path")?;
    let mode = required_non_empty_string(&args, "mode")?;
    let undo_group = requested_undo_group(&args)?;
    let validated = validate_and_read_script_file(cfg, path, mode)?;
    let prepared = prepare_script(
        cfg,
        AFTER_EFFECTS_HOST,
        &args,
        validated.code,
        mode,
        Some(validated.audit.source_path),
        cfg.script_files.max_bytes,
    )?;
    if prepared.preflight_only {
        return tool_json(prepared.preflight_envelope(AFTER_EFFECTS_HOST));
    }
    let audit = serde_json::to_value(&prepared.audit)?;

    let payload = json!({
        "code": prepared.code,
        "args": prepared.input,
        "mode": mode,
        "description": prepared.description,
        "undoGroup": undo_group,
        "sourcePath": prepared.audit.source_path,
        "sourceSha256": prepared.audit.source_sha256,
        "sourceSizeBytes": prepared.audit.source_size_bytes,
    });
    let timeout_ms = prepared.timeout_ms;

    run_daemon_command(
        cfg,
        "executeJsx",
        payload,
        &args,
        timeout_ms,
        false,
        Some(audit),
        "Error running JSX file",
    )
}

fn requested_undo_group(args: &Value) -> Result<bool> {
    match args.get("undoGroup") {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(anyhow!("undoGroup must be a boolean")),
    }
}

fn get_capabilities_tool(cfg: &AppConfig) -> Result<Value> {
    let instances =
        daemon_core::call_daemon(cfg, json!({ "op": "listInstances" }), cfg.result_timeout_ms)
            .unwrap_or_else(|error| json!({ "status": "offline", "error": error.to_string() }));
    tool_json(capabilities_value(cfg, AFTER_EFFECTS_HOST, instances))
}

fn cancel_script_request_tool(cfg: &AppConfig, args: Value) -> Result<Value> {
    let request_id = required_non_empty_string(&args, "requestId")?;
    tool_json(daemon_core::call_daemon(
        cfg,
        json!({ "op": "cancelRequest", "requestId": request_id }),
        cfg.result_timeout_ms,
    )?)
}

fn get_jsx_result_tool(cfg: &AppConfig, args: Value) -> Result<Value> {
    let request_id = required_non_empty_string(&args, "requestId")?;
    let value = daemon_core::call_daemon(
        cfg,
        json!({
            "op": "getResult",
            "requestId": request_id
        }),
        cfg.result_timeout_ms,
    )?;
    Ok(tool_json(value)?)
}

fn list_ae_instances_tool(cfg: &AppConfig) -> Result<Value> {
    let value = daemon_core::call_daemon(
        cfg,
        json!({
            "op": "listInstances"
        }),
        cfg.result_timeout_ms,
    )?;
    Ok(tool_json(value)?)
}

fn diagnostic_config_path(cfg: &AppConfig) -> PathBuf {
    cfg.bridge.root_dir.join(DIAGNOSTIC_CONFIG_FILE)
}

fn diagnostic_log_path(cfg: &AppConfig) -> PathBuf {
    cfg.bridge.root_dir.join(DIAGNOSTIC_LOG_FILE)
}

fn scheduler_diagnostic_path(cfg: &AppConfig) -> PathBuf {
    cfg.bridge.root_dir.join(SCHEDULER_DIAGNOSTIC_FILE)
}

fn default_diagnostic_config(cfg: &AppConfig) -> Value {
    json!({
        "enabled": false,
        "logPath": diagnostic_log_path(cfg).display().to_string()
    })
}

fn read_diagnostic_config(cfg: &AppConfig) -> Result<Value> {
    let path = diagnostic_config_path(cfg);
    if !path.exists() {
        return Ok(default_diagnostic_config(cfg));
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read diagnostic config: {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse diagnostic config: {}", path.display()))
}

fn configure_bridge_diagnostics_tool(cfg: &AppConfig, args: Value) -> Result<Value> {
    let enabled = args
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("'enabled' must be a boolean"))?;
    fs::create_dir_all(&cfg.bridge.root_dir).with_context(|| {
        format!(
            "failed to create bridge root: {}",
            cfg.bridge.root_dir.display()
        )
    })?;
    let config_path = diagnostic_config_path(cfg);
    let log_path = diagnostic_log_path(cfg);
    let config = json!({
        "enabled": enabled,
        "logPath": log_path.display().to_string()
    });
    let encoded = serde_json::to_string_pretty(&config)?;
    fs::write(&config_path, encoded).with_context(|| {
        format!(
            "failed to write diagnostic config: {}",
            config_path.display()
        )
    })?;

    tool_json(json!({
        "enabled": enabled,
        "defaultEnabled": false,
        "configPath": config_path,
        "logPath": log_path,
        "schedulerDiagnosticPath": scheduler_diagnostic_path(cfg),
        "applies": "The running bridge reloads this setting on its next scheduler tick; a stale bridge applies it immediately after restart."
    }))
}

fn read_log_tail(path: &Path, tail_lines: usize) -> Result<(Vec<String>, u64, bool)> {
    if tail_lines == 0 || !path.exists() {
        return Ok((Vec::new(), 0, false));
    }

    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open diagnostic log: {}", path.display()))?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(DIAGNOSTIC_LOG_READ_BYTES);
    if start > 0 {
        file.seek(SeekFrom::Start(start))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut raw = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        if let Some(first_newline) = raw.find('\n') {
            raw = raw[first_newline + 1..].to_string();
        }
    }
    let lines = raw.lines().map(str::to_owned).collect::<Vec<_>>();
    let keep_from = lines.len().saturating_sub(tail_lines);
    Ok((lines[keep_from..].to_vec(), size, start > 0))
}

fn is_readable_bridge_local_path(root: &Path, path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(canonical_root) = root.canonicalize() else {
        return false;
    };
    let Ok(canonical_path) = path.canonicalize() else {
        return false;
    };
    canonical_path.starts_with(canonical_root)
}

fn compact_instance_diagnostics(mut instances: Value) -> Value {
    let Some(instance_object) = instances.as_object_mut() else {
        return instances;
    };
    let Some(inactive) = instance_object
        .get_mut("inactiveInstances")
        .and_then(Value::as_array_mut)
    else {
        return instances;
    };

    let inactive_count = inactive.len();
    if inactive_count > DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT {
        let keep_from = inactive_count - DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT;
        inactive.drain(0..keep_from);
    }
    instance_object.insert("inactiveInstanceCount".to_string(), json!(inactive_count));
    instance_object.insert(
        "inactiveInstancesTruncated".to_string(),
        json!(inactive_count > DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT),
    );
    instances
}

fn get_bridge_diagnostics_tool(cfg: &AppConfig, args: Value) -> Result<Value> {
    let tail_lines = args.get("tailLines").and_then(Value::as_u64).unwrap_or(40);
    if tail_lines > 200 {
        return Err(anyhow!("'tailLines' must be between 0 and 200"));
    }

    let config_path = diagnostic_config_path(cfg);
    let config = read_diagnostic_config(cfg)?;
    let enabled = config
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let configured_log_path = config
        .get("logPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| diagnostic_log_path(cfg));
    let log_is_bridge_local =
        is_readable_bridge_local_path(&cfg.bridge.root_dir, &configured_log_path);
    let (log_tail, log_size_bytes, log_tail_truncated) = if log_is_bridge_local {
        read_log_tail(&configured_log_path, tail_lines as usize)?
    } else {
        (Vec::new(), 0, false)
    };

    let scheduler_path = scheduler_diagnostic_path(cfg);
    let scheduler_diagnostic = if scheduler_path.exists() {
        let raw = fs::read_to_string(&scheduler_path).with_context(|| {
            format!(
                "failed to read scheduler diagnostic: {}",
                scheduler_path.display()
            )
        })?;
        serde_json::from_str::<Value>(&raw).with_context(|| {
            format!(
                "failed to parse scheduler diagnostic: {}",
                scheduler_path.display()
            )
        })?
    } else {
        Value::Null
    };
    let instances = compact_instance_diagnostics(
        daemon_core::call_daemon(cfg, json!({ "op": "listInstances" }), cfg.result_timeout_ms)
            .unwrap_or_else(|error| {
                json!({
                    "status": "offline",
                    "error": error.to_string()
                })
            }),
    );

    tool_json(json!({
        "enabled": enabled,
        "defaultEnabled": false,
        "configPath": config_path,
        "configExists": config_path.exists(),
        "logPath": configured_log_path,
        "logPathReadableByTool": log_is_bridge_local,
        "logExists": configured_log_path.exists(),
        "logSizeBytes": log_size_bytes,
        "logTailTruncatedToLastMiB": log_tail_truncated,
        "logTail": log_tail,
        "schedulerDiagnosticPath": scheduler_path,
        "schedulerDiagnostic": scheduler_diagnostic,
        "instances": instances
    }))
}

fn run_daemon_command(
    cfg: &AppConfig,
    command: &str,
    command_args: Value,
    option_args: &Value,
    timeout_ms: u64,
    global_exclusive: bool,
    audit: Option<Value>,
    error_prefix: &str,
) -> Result<Value> {
    let retention_seconds = option_args
        .get("resultRetentionSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(cfg.result_retention_seconds);
    if retention_seconds == 0 {
        anyhow::bail!("resultRetentionSeconds must be greater than 0");
    }
    if retention_seconds > cfg.result_retention_max_seconds {
        anyhow::bail!(
            "resultRetentionSeconds exceeds the configured maximum: {} > {}",
            retention_seconds,
            cfg.result_retention_max_seconds
        );
    }

    let daemon_value = daemon_core::call_daemon(
        cfg,
        json!({
            "op": "runCommand",
            "command": command,
            "args": command_args,
            "targetInstanceId": option_args.get("targetInstanceId").cloned().unwrap_or(Value::Null),
            "targetVersion": option_args.get("targetVersion").cloned().unwrap_or(Value::Null),
            "timeoutMs": timeout_ms,
            "pollIntervalMs": cfg.poll_interval_ms,
            "retentionSeconds": retention_seconds,
            "globalExclusive": global_exclusive,
            "audit": audit
        }),
        timeout_ms,
    );

    match daemon_value {
        Ok(value) => Ok(tool_json(value)?),
        Err(error) => Ok(tool_error(format!("{error_prefix}: {error}"))),
    }
}

fn required_non_empty_string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("'{key}' is required and must be a non-empty string"))
}

fn validate_unsafe_mode(args: &Value) -> Result<()> {
    let mode = required_non_empty_string(args, "mode")?;
    if mode != "unsafe" {
        anyhow::bail!("only mode='unsafe' is currently supported for JSX execution");
    }
    Ok(())
}

fn timeout_ms_from_args(cfg: &AppConfig, args: &Value) -> u64 {
    args.get("timeoutMs")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or(cfg.result_timeout_ms)
}

fn create_test_animation_script(args: Value) -> Result<Value> {
    let operation = args
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("'operation' is required"))?;
    let comp_index = args.get("compIndex").and_then(Value::as_i64).unwrap_or(1);
    let layer_index = args.get("layerIndex").and_then(Value::as_i64).unwrap_or(1);

    if operation != "keyframe" && operation != "expression" {
        return Ok(tool_error(
            "operation must be one of: keyframe, expression".to_string(),
        ));
    }

    let file_name = format!(
        "ae_test_{}_{}.jsx",
        operation,
        chrono::Utc::now().timestamp_millis()
    );
    let script_path = std::env::temp_dir().join(file_name);
    let content = if operation == "keyframe" {
        format!(
            r#"
try {{
  var comp = app.project.items[{comp_index}];
  var layer = comp.layers[{layer_index}];
  var prop = layer.property("Transform").property("Opacity");
  prop.setValueAtTime(1, 25);
  alert("Test successful: Added opacity keyframe at 1s with value 25%");
}} catch (e) {{
  alert("Test failed: " + e.toString());
}}
"#
        )
    } else {
        format!(
            r#"
try {{
  var comp = app.project.items[{comp_index}];
  var layer = comp.layers[{layer_index}];
  var prop = layer.property("Transform").property("Position");
  prop.expression = "wiggle(3, 30)";
  alert("Test successful: Added position expression: wiggle(3, 30)");
}} catch (e) {{
  alert("Test failed: " + e.toString());
}}
"#
        )
    };

    fs::write(&script_path, content).with_context(|| {
        format!(
            "failed to write test script file: {}",
            script_path.display()
        )
    })?;

    Ok(tool_text(format!(
        "I've created a direct test script for the {operation} operation.\n\nPlease run this script manually in After Effects:\n1. In After Effects, go to File > Scripts > Run Script File...\n2. Navigate to: {}\n3. You should see an alert confirming the result.\n\nThis bypasses the MCP Bridge Auto panel and will directly modify the specified layer.",
        script_path.display()
    )))
}

fn tool_text(text: String) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

fn tool_json(value: Value) -> Result<Value> {
    let text =
        serde_json::to_string_pretty(&value).with_context(|| "failed to serialize tool JSON")?;
    Ok(tool_text(text))
}

fn tool_error(text: String) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "isError": true
    })
}

async fn read_jsonrpc_message<R>(reader: &mut R) -> Result<Option<(Value, MessageFormat)>>
where
    R: AsyncBufRead + Unpin,
{
    loop {
        let buf = reader
            .fill_buf()
            .await
            .with_context(|| "failed to inspect MCP input buffer")?;
        let Some(first) = buf.first().copied() else {
            return Ok(None);
        };
        if first == b'\r' || first == b'\n' {
            reader.consume(1);
            continue;
        }
        if first == b'{' || first == b'[' {
            let mut line = String::new();
            let n = reader
                .read_line(&mut line)
                .await
                .with_context(|| "failed to read JSON-line MCP message")?;
            if n == 0 {
                return Ok(None);
            }
            let value = serde_json::from_str::<Value>(line.trim_end_matches(['\r', '\n']))
                .with_context(|| "failed to parse JSON-line MCP request")?;
            return Ok(Some((value, MessageFormat::JsonLine)));
        }
        break;
    }

    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .await
            .with_context(|| "failed to read message header line")?;
        if n == 0 {
            if content_length.is_none() {
                return Ok(None);
            }
            return Err(anyhow!("unexpected EOF while reading MCP headers"));
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, rest)) = trimmed.split_once(':') {
            if !name.eq_ignore_ascii_case("content-length") {
                continue;
            }
            let len = rest
                .trim()
                .parse::<usize>()
                .with_context(|| format!("invalid Content-Length header: {trimmed}"))?;
            content_length = Some(len);
        }
    }

    let len = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut buf = vec![0_u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .with_context(|| "failed to read message body")?;
    let value = serde_json::from_slice::<Value>(&buf)
        .with_context(|| "failed to parse JSON-RPC request")?;
    Ok(Some((value, MessageFormat::ContentLength)))
}

async fn write_jsonrpc_message<W>(
    writer: &mut W,
    value: &Value,
    format: MessageFormat,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(value).with_context(|| "failed to serialize response JSON")?;
    if matches!(format, MessageFormat::ContentLength) {
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        writer
            .write_all(header.as_bytes())
            .await
            .with_context(|| "failed to write response header")?;
    }
    writer
        .write_all(&payload)
        .await
        .with_context(|| "failed to write response body")?;
    if matches!(format, MessageFormat::JsonLine) {
        writer
            .write_all(b"\n")
            .await
            .with_context(|| "failed to write JSON-line response terminator")?;
    }
    writer
        .flush()
        .await
        .with_context(|| "failed to flush response")
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn isolated_bridge_config(label: &str) -> (AppConfig, BridgeClient) {
        let root = std::env::temp_dir().join(format!(
            "ae-mcp-{label}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut cfg = AppConfig::default();
        cfg.bridge.root_dir = root.clone();
        cfg.bridge.command_file = root.join("ae_command.json");
        cfg.bridge.result_file = root.join("ae_mcp_result.json");
        let bridge = BridgeClient::new(cfg.clone()).expect("isolated bridge");
        (cfg, bridge)
    }

    fn fake_daemon(cfg: &mut AppConfig, count: usize) -> std::thread::JoinHandle<Vec<Value>> {
        use std::io::{BufRead, BufReader as StdBufReader, Write};
        use std::net::TcpListener;
        use std::time::{Duration, Instant};

        let listener = TcpListener::bind("127.0.0.1:0").expect("isolated listener");
        cfg.daemon_addr = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut requests = Vec::new();
            while requests.len() < count {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing broker request");
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut line = String::new();
                StdBufReader::new(&mut stream).read_line(&mut line).unwrap();
                requests.push(serde_json::from_str::<Value>(&line).unwrap());
                let response = json!({
                    "ok": true,
                    "value": {
                        "requestId": format!("req-compat-{}", requests.len()),
                        "state": "completed",
                        "result": {"status": "success"}
                    }
                });
                writeln!(stream, "{response}").unwrap();
            }
            requests
        })
    }

    #[test]
    fn compatibility_tools_use_broker_routing_and_return_request_ids_without_touching_mailboxes() {
        let routes = [
            ("create-composition", "createComposition"),
            ("setLayerKeyframe", "setLayerKeyframe"),
            ("setLayerExpression", "setLayerExpression"),
            ("apply-effect", "applyEffect"),
            ("apply-effect-template", "applyEffectTemplate"),
            ("list-supported-effects", "listSupportedEffects"),
            ("describe-effect", "describeEffect"),
            ("save-frame-png", "saveFramePng"),
            ("render-queue-add", "renderQueueAdd"),
            ("render-queue-status", "renderQueueStatus"),
            ("render-queue-start", "renderQueueStart"),
            ("render-queue-is-rendering", "renderQueueIsRendering"),
            ("set-current-time", "setCurrentTime"),
            ("get-current-time", "getCurrentTime"),
            ("set-work-area", "setWorkArea"),
            ("get-work-area", "getWorkArea"),
            ("get-composition-markers", "getCompositionMarkers"),
            ("cleanup-preview-folder", "cleanupPreviewFolder"),
            ("set-suppress-dialogs", "setSuppressDialogs"),
            ("get-suppress-dialogs", "getSuppressDialogs"),
            ("project-open", "projectOpen"),
            ("project-close", "projectClose"),
            ("project-save", "projectSave"),
            ("project-save-as", "projectSaveAs"),
            ("application-quit", "applicationQuit"),
            ("mcp_aftereffects_applyEffect", "applyEffect"),
            (
                "mcp_aftereffects_applyEffectTemplate",
                "applyEffectTemplate",
            ),
            (
                "mcp_aftereffects_listSupportedEffects",
                "listSupportedEffects",
            ),
            ("mcp_aftereffects_describeEffect", "describeEffect"),
            ("run-bridge-test", "bridgeTestEffects"),
            ("run-script", "listCompositions"),
        ];
        let (mut cfg, bridge) = isolated_bridge_config("compat-broker");
        fs::write(&cfg.bridge.command_file, "existing command").unwrap();
        fs::write(&cfg.bridge.result_file, "existing result").unwrap();
        let server = fake_daemon(&mut cfg, routes.len());
        for (index, (tool, _)) in routes.iter().enumerate() {
            let args = json!({
                "targetInstanceId": "ae-test-instance",
                "targetVersion": "25.6",
                "resultRetentionSeconds": 123,
                "timeoutMs": 1234,
                "waitTimeoutSeconds": 2,
                "effectName": "Test Effect",
                "marker": index,
                "script": "listCompositions",
                "parameters": {"payloadMarker": index}
            });
            let result = dispatch_tool_inner(&cfg, &bridge, tool, args).unwrap();
            assert_ne!(result["isError"], true, "{tool}: {result}");
            let response: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().expect("JSON result"))
                    .unwrap();
            assert_eq!(response["requestId"], format!("req-compat-{}", index + 1));
            assert_eq!(response["state"], "completed");
        }
        let requests = server.join().expect("fake daemon completed");
        for (index, ((tool, command), request)) in routes.iter().zip(requests).enumerate() {
            assert_eq!(request["op"], "runCommand", "{tool}");
            assert_eq!(request["command"], *command, "{tool}");
            assert_eq!(request["targetInstanceId"], "ae-test-instance", "{tool}");
            assert_eq!(request["targetVersion"], "25.6", "{tool}");
            assert_eq!(request["retentionSeconds"], 123, "{tool}");
            assert_eq!(
                request["globalExclusive"],
                *tool == "cleanup-preview-folder",
                "{tool}"
            );
            assert_eq!(
                request["timeoutMs"],
                if *tool == "render-queue-start" {
                    2000
                } else {
                    1234
                }
            );
            if *tool == "run-script" {
                assert_eq!(request["args"], json!({"payloadMarker":index}));
            } else {
                assert_eq!(request["args"]["marker"], index);
            }
        }
        assert_eq!(
            fs::read_to_string(&cfg.bridge.command_file).unwrap(),
            "existing command"
        );
        assert_eq!(
            fs::read_to_string(&cfg.bridge.result_file).unwrap(),
            "existing result"
        );
        fs::remove_dir_all(&cfg.bridge.root_dir).unwrap();
    }

    #[test]
    fn compatibility_dispatch_fails_without_a_broker_and_preserves_existing_mailboxes() {
        let (mut cfg, bridge) = isolated_bridge_config("compat-offline");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        cfg.daemon_addr = listener.local_addr().unwrap().to_string();
        drop(listener);
        fs::write(&cfg.bridge.command_file, "active request").unwrap();
        fs::write(&cfg.bridge.result_file, "retained response").unwrap();
        let result = dispatch_tool_inner(
            &cfg,
            &bridge,
            "run-script",
            json!({"script":"listCompositions"}),
        )
        .unwrap();
        assert_eq!(result["isError"], true);
        assert_eq!(
            fs::read_to_string(&cfg.bridge.command_file).unwrap(),
            "active request"
        );
        assert_eq!(
            fs::read_to_string(&cfg.bridge.result_file).unwrap(),
            "retained response"
        );
        fs::remove_dir_all(&cfg.bridge.root_dir).unwrap();
    }

    #[test]
    fn compatibility_render_wait_is_bounded_and_invalid_durations_never_dispatch() {
        let (mut cfg, bridge) = isolated_bridge_config("compat-render");
        cfg.script_contract.max_timeout_ms = 100;
        let server = fake_daemon(&mut cfg, 1);
        let result = dispatch_tool_inner(&cfg, &bridge, "render-queue-start", json!({})).unwrap();
        assert_ne!(result["isError"], true);
        assert_eq!(server.join().unwrap()[0]["timeoutMs"], 100);
        for seconds in [0, u64::MAX] {
            let result = dispatch_tool_inner(
                &cfg,
                &bridge,
                "render-queue-start",
                json!({"waitTimeoutSeconds":seconds}),
            );
            assert!(result.is_err());
        }
        assert!(!cfg.bridge.command_file.exists());
        fs::remove_dir_all(&cfg.bridge.root_dir).unwrap();
    }

    #[test]
    fn tools_list_contains_run_jsx() {
        let result = tools_list_result();
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array");
        assert!(tools
            .iter()
            .any(|t| t.get("name").and_then(Value::as_str) == Some("run-jsx")));
    }

    #[test]
    fn undo_group_defaults_to_enabled_and_accepts_explicit_opt_out() {
        assert!(requested_undo_group(&json!({})).unwrap());
        assert!(!requested_undo_group(&json!({ "undoGroup": false })).unwrap());
        assert!(requested_undo_group(&json!({ "undoGroup": "false" })).is_err());
    }

    #[test]
    fn bridge_diagnostics_are_off_by_default_and_mcp_can_enable_them() {
        let root = std::env::temp_dir().join(format!(
            "ae-mcp-diagnostics-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut cfg = AppConfig::default();
        cfg.bridge.root_dir = root.clone();
        cfg.bridge.command_file = root.join("ae_command.json");
        cfg.bridge.result_file = root.join("ae_mcp_result.json");

        let defaults = read_diagnostic_config(&cfg).expect("default diagnostic config");
        assert_eq!(defaults["enabled"], false);
        assert_eq!(
            defaults["logPath"],
            diagnostic_log_path(&cfg).display().to_string()
        );
        assert!(!diagnostic_config_path(&cfg).exists());

        configure_bridge_diagnostics_tool(&cfg, json!({ "enabled": true }))
            .expect("enable diagnostics");
        let enabled = read_diagnostic_config(&cfg).expect("enabled diagnostic config");
        assert_eq!(enabled["enabled"], true);
        assert_eq!(
            enabled["logPath"],
            diagnostic_log_path(&cfg).display().to_string()
        );

        fs::write(diagnostic_log_path(&cfg), "one\ntwo\nthree\n").expect("write diagnostic log");
        let (tail, size, truncated) =
            read_log_tail(&diagnostic_log_path(&cfg), 2).expect("read diagnostic tail");
        assert_eq!(tail, vec!["two", "three"]);
        assert!(size > 0);
        assert!(!truncated);
        assert!(is_readable_bridge_local_path(
            &root,
            &diagnostic_log_path(&cfg)
        ));
        let outside_log = root.with_extension("outside.log");
        fs::write(&outside_log, "outside").expect("write outside diagnostic test file");
        assert!(!is_readable_bridge_local_path(
            &root,
            &root
                .join("..")
                .join(outside_log.file_name().expect("outside file name"))
        ));

        configure_bridge_diagnostics_tool(&cfg, json!({ "enabled": false }))
            .expect("disable diagnostics");
        let disabled = read_diagnostic_config(&cfg).expect("disabled diagnostic config");
        assert_eq!(disabled["enabled"], false);

        let _ = fs::remove_file(outside_log);
        let _ = fs::remove_dir_all(PathBuf::from(root));
    }

    #[test]
    fn bridge_diagnostics_limit_inactive_instance_history() {
        let inactive = (0..(DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT + 3))
            .map(|index| json!({ "instanceId": format!("inactive-{index}") }))
            .collect::<Vec<_>>();
        let compacted = compact_instance_diagnostics(json!({
            "count": 1,
            "instances": [{ "instanceId": "active" }],
            "inactiveInstances": inactive
        }));

        assert_eq!(
            compacted["inactiveInstanceCount"],
            DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT + 3
        );
        assert_eq!(compacted["inactiveInstancesTruncated"], true);
        assert_eq!(
            compacted["inactiveInstances"]
                .as_array()
                .expect("inactive instances")
                .len(),
            DIAGNOSTIC_INACTIVE_INSTANCE_LIMIT
        );
        assert_eq!(
            compacted["inactiveInstances"][0]["instanceId"],
            "inactive-3"
        );
    }

    #[test]
    fn tools_list_is_exactly_the_documented_public_surface() {
        let result = tools_list_result();
        let names = result
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(names, mcp_core::PUBLIC_TOOL_NAMES);
        assert!(names.contains(&"run-script"));
        assert!(!names.contains(&"render-queue-add"));
    }

    #[test]
    fn deprecated_dispatch_returns_public_replacement_notice() {
        let root = std::env::temp_dir().join(format!(
            "ae-mcp-legacy-dispatch-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut cfg = AppConfig::default();
        cfg.bridge.root_dir = root.clone();
        cfg.bridge.command_file = root.join("ae_command.json");
        cfg.bridge.result_file = root.join("ae_mcp_result.json");
        let bridge = BridgeClient::new(cfg.clone()).expect("temporary bridge");

        let result = dispatch_tool(
            &cfg,
            &bridge,
            "mcp_aftereffects_get_effects_help",
            json!({}),
        );
        let text = result["content"][0]["text"].as_str().expect("text result");
        assert!(text.contains("Deprecated compatibility tool"));
        assert!(text.contains("public `get-help` tool"));
        assert!(!result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false));

        let _ = fs::remove_dir_all(PathBuf::from(root));
    }

    #[test]
    fn canonical_run_script_does_not_break_legacy_allowlist_dispatch() {
        let root = std::env::temp_dir().join(format!("ae-mcp-run-script-{}", std::process::id()));
        let mut cfg = AppConfig::default();
        cfg.bridge.root_dir = root.clone();
        let bridge = BridgeClient::new(cfg.clone()).unwrap();
        let canonical = dispatch_tool(
            &cfg,
            &bridge,
            "run-script",
            json!({
                "code": "return 1;",
                "mode": "unsafe",
                "description": "preflight",
                "preflightOnly": true
            }),
        );
        let text = canonical["content"][0]["text"].as_str().unwrap();
        assert!(!text.contains("Deprecated compatibility tool"));
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap()["state"],
            "preflight"
        );

        let legacy = dispatch_tool(
            &cfg,
            &bridge,
            "run-script",
            json!({ "script": "notAllowlisted" }),
        );
        let legacy_text = legacy["content"][0]["text"].as_str().unwrap();
        assert!(legacy_text.contains("Deprecated compatibility tool"));
        assert!(legacy_text.contains("is not allowed"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compositions_resource_builds_a_daemon_broker_request() {
        let cfg = AppConfig::default();
        let request = compositions_resource_daemon_request(&cfg);
        assert_eq!(request["op"], "runCommand");
        assert_eq!(request["command"], "listCompositions");
        assert_eq!(request["timeoutMs"], cfg.result_timeout_ms + 1_000);
        assert_eq!(request["pollIntervalMs"], cfg.poll_interval_ms);
        assert_eq!(request["retentionSeconds"], cfg.result_retention_seconds);
    }

    #[test]
    fn prompts_get_rejects_unknown_prompt() {
        let params = json!({ "name": "unknown", "arguments": {} });
        let err = prompts_get_result(&params).expect_err("must fail");
        assert!(err.to_string().contains("unknown prompt"));
    }

    #[test]
    fn tool_error_marks_is_error() {
        let v = tool_error("x".to_string());
        assert_eq!(v.get("isError").and_then(Value::as_bool), Some(true));
    }
}
