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
use std::time::Duration;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tracing::{debug, error, info};

const MAX_JSX_BYTES: usize = 1_048_576;

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
            bridge.clear_results_file()?;
            bridge.write_command_file(script, parameters)?;
            Ok(tool_text(format!(
                "Command to run \"{script}\" has been queued.\nPlease ensure the \"MCP Bridge Auto\" panel is open in After Effects.\nUse the \"get-results\" tool after a few seconds to check for results."
            )))
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
        "create-composition" => {
            bridge.write_command_file("createComposition", args)?;
            Ok(tool_text(
                "Command to create composition has been queued.\nPlease ensure the \"MCP Bridge Auto\" panel is open in After Effects.\nUse the \"get-results\" tool after a few seconds to check for results.".to_string(),
            ))
        }
        "setLayerKeyframe" => {
            let comp_index = args.get("compIndex").and_then(Value::as_i64).unwrap_or(0);
            let layer_index = args.get("layerIndex").and_then(Value::as_i64).unwrap_or(0);
            let property_name = args
                .get("propertyName")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("setLayerKeyframe", args)?;
            Ok(tool_text(format!(
                "Command to set keyframe for \"{property_name}\" on layer {layer_index} in comp {comp_index} has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "setLayerExpression" => {
            let comp_index = args.get("compIndex").and_then(Value::as_i64).unwrap_or(0);
            let layer_index = args.get("layerIndex").and_then(Value::as_i64).unwrap_or(0);
            let property_name = args
                .get("propertyName")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("setLayerExpression", args)?;
            Ok(tool_text(format!(
                "Command to set expression for \"{property_name}\" on layer {layer_index} in comp {comp_index} has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "test-animation" => create_test_animation_script(args),
        "apply-effect" => {
            let comp_target = format_comp_target(&args);
            let layer_target = format_layer_target(&args);
            bridge.write_command_file("applyEffect", args)?;
            Ok(tool_text(format!(
                "Command to apply effect to {layer_target} in {comp_target} has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "apply-effect-template" => {
            let comp_target = format_comp_target(&args);
            let layer_target = format_layer_target(&args);
            let template_name = args
                .get("templateName")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("applyEffectTemplate", args)?;
            Ok(tool_text(format!(
                "Command to apply effect template '{template_name}' to {layer_target} in {comp_target} has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "list-supported-effects" => {
            bridge.write_command_file("listSupportedEffects", args)?;
            Ok(tool_text(
                "Command to list supported effects has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
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

            bridge.write_command_file("describeEffect", args)?;
            Ok(tool_text(
                "Command to describe effect has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
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
        "render-queue-add" => {
            let comp_target = format_comp_target(&args);
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("renderQueueAdd", args)?;
            Ok(tool_text(format!(
                "Command to add {comp_target} to the render queue has been queued.\nOutput: {output_path}\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "render-queue-status" => {
            bridge.write_command_file("renderQueueStatus", args)?;
            Ok(tool_text(
                "Command to get render queue status has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "render-queue-start" => {
            let wait_timeout_seconds = args
                .get("waitTimeoutSeconds")
                .and_then(Value::as_u64)
                .unwrap_or(7_200);
            bridge.clear_results_file()?;
            bridge.write_command_file("renderQueueStart", args)?;
            let text = bridge.wait_for_bridge_result(
                Some("renderQueueStart"),
                Duration::from_secs(wait_timeout_seconds),
                Duration::from_millis(cfg.poll_interval_ms),
            )?;
            Ok(tool_text(text))
        }
        "render-queue-is-rendering" => run_direct_bridge_call(
            cfg,
            bridge,
            "renderQueueIsRendering",
            args,
            "Error checking render state",
        ),
        "set-current-time" => {
            bridge.write_command_file("setCurrentTime", args)?;
            Ok(tool_text(
                "Command to set current time has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "get-current-time" => {
            bridge.write_command_file("getCurrentTime", args)?;
            Ok(tool_text(
                "Command to get current time has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "set-work-area" => {
            bridge.write_command_file("setWorkArea", args)?;
            Ok(tool_text(
                "Command to set work area has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "get-work-area" => {
            bridge.write_command_file("getWorkArea", args)?;
            Ok(tool_text(
                "Command to get work area has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "get-composition-markers" => {
            bridge.write_command_file("getCompositionMarkers", args)?;
            Ok(tool_text(
                "Command to get composition markers has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
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
        "set-suppress-dialogs" => {
            bridge.write_command_file("setSuppressDialogs", args)?;
            Ok(tool_text(
                "Command to set dialog suppression has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "get-suppress-dialogs" => {
            bridge.write_command_file("getSuppressDialogs", args)?;
            Ok(tool_text(
                "Command to get dialog suppression state has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "project-open" => {
            let file_path = args
                .get("filePath")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("projectOpen", args)?;
            Ok(tool_text(format!(
                "Command to open project has been queued.\nFile: {file_path}\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "project-close" => {
            bridge.write_command_file("projectClose", args)?;
            Ok(tool_text(
                "Command to close current project has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
        "project-save" => {
            let save_as_path = args
                .get("saveAsPath")
                .or_else(|| args.get("filePath"))
                .or_else(|| args.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            bridge.write_command_file("projectSave", args)?;
            if save_as_path.is_empty() {
                Ok(tool_text(
                    "Command to save current project has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                        .to_string(),
                ))
            } else {
                Ok(tool_text(format!(
                    "Command to save project has been queued.\nSave As: {save_as_path}\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                )))
            }
        }
        "project-save-as" => {
            let file_path = args
                .get("filePath")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("projectSaveAs", args)?;
            Ok(tool_text(format!(
                "Command to save project as has been queued.\nFile: {file_path}\nUse the \"get-results\" tool after a few seconds to check for confirmation."
            )))
        }
        "application-quit" => {
            bridge.write_command_file("applicationQuit", args)?;
            Ok(tool_text(
                "Command to gracefully quit After Effects has been queued.\nUse the \"get-results\" tool after a few seconds to check for confirmation."
                    .to_string(),
            ))
        }
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
            json!({}),
            "Error running bridge test",
        ),
        _ => Ok(tool_error(format!("Unknown tool: {name}"))),
    }
}

fn format_comp_target(args: &Value) -> String {
    if let Some(id) = args.get("compId").and_then(Value::as_i64) {
        format!("composition id {id}")
    } else if let Some(name) = args
        .get("compName")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
    {
        format!("composition '{name}'")
    } else if let Some(index) = args.get("compIndex").and_then(Value::as_i64) {
        format!("composition {index}")
    } else {
        "resolved composition".to_string()
    }
}

fn format_layer_target(args: &Value) -> String {
    if let Some(id) = args.get("layerId").and_then(Value::as_i64) {
        format!("layer id {id}")
    } else if let Some(name) = args
        .get("layerName")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
    {
        format!("layer '{name}'")
    } else if let Some(index) = args.get("layerIndex").and_then(Value::as_i64) {
        format!("layer {index}")
    } else {
        "resolved layer".to_string()
    }
}

fn run_direct_bridge_call(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    command: &str,
    args: Value,
    error_prefix: &str,
) -> Result<Value> {
    run_direct_bridge_call_with_timeout(
        cfg,
        bridge,
        command,
        args,
        error_prefix,
        cfg.result_timeout_ms,
    )
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
