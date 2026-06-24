use ai_core::{general_help_text, prompt_messages, prompt_specs, tool_specs};
use anyhow::{anyhow, Context, Result};
use bridge_core::{BridgeClient, BridgeRunOptions, BridgeTarget};
use mcp_core::AppConfig;
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
                "name": "IllustratorServer",
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
                "name": "documents",
                "description": "List open Illustrator documents",
                "uri": "illustrator://documents",
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

fn resources_read_result(cfg: &AppConfig, bridge: &BridgeClient, params: &Value) -> Result<Value> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("resources/read requires parameter 'uri'"))?;

    if uri != "illustrator://documents" {
        return Err(anyhow!("unknown resource URI: {uri}"));
    }

    let value = run_bridge_command_value(
        cfg,
        bridge,
        "listDocuments",
        json!({}),
        &json!({}),
        cfg.result_timeout_ms + 1_000,
    )?;
    let text = serde_json::to_string_pretty(&value)
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
    let result = dispatch_tool_inner(cfg, bridge, name, args);
    match result {
        Ok(value) => value,
        Err(e) => tool_error(format!("Error: {e}")),
    }
}

fn dispatch_tool_inner(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    name: &str,
    args: Value,
) -> Result<Value> {
    match name {
        "run-jsx" => run_jsx_tool(cfg, bridge, args),
        "run-jsx-file" => run_jsx_file_tool(cfg, bridge, args),
        "get-jsx-result" => get_jsx_result_tool(bridge, args),
        "list-illustrator-instances" => list_illustrator_instances_tool(cfg, bridge),
        "run-script" => run_script_tool(cfg, bridge, args),
        "get-results" => {
            if let Some(request_id) = args.get("requestId").and_then(Value::as_str) {
                let record = bridge.get_request_record(request_id)?;
                Ok(tool_json(illustrator_record_value(record.to_value()))?)
            } else {
                let value = bridge
                    .latest_request_record()?
                    .map(|record| illustrator_record_value(record.to_value()))
                    .unwrap_or_else(
                        || json!({ "status": "empty", "message": "No retained request result." }),
                    );
                Ok(tool_json(value)?)
            }
        }
        "get-help" => Ok(tool_text(general_help_text().to_string())),
        "run-bridge-test" => {
            run_direct_bridge_call(cfg, bridge, "ping", json!({}), "Error running bridge test")
        }
        _ => Ok(tool_error(format!("Unknown tool: {name}"))),
    }
}

fn run_script_tool(cfg: &AppConfig, bridge: &BridgeClient, args: Value) -> Result<Value> {
    let script = required_non_empty_string(&args, "script")?;
    if !ai_core::is_allowed_script(script) {
        return Ok(tool_error(format!(
            "Script \"{script}\" is not allowed. Allowed scripts are: {}",
            ai_core::ALLOWED_SCRIPTS.join(", ")
        )));
    }

    let parameters = args.get("parameters").cloned().unwrap_or_else(|| json!({}));
    if !parameters.is_object() {
        anyhow::bail!("'parameters' must be an object when provided");
    }

    let timeout_ms = timeout_ms_from_args(cfg, &args);
    run_bridge_command(
        cfg,
        bridge,
        script,
        parameters,
        &args,
        timeout_ms,
        "Error running Illustrator script",
    )
}

fn run_jsx_tool(cfg: &AppConfig, bridge: &BridgeClient, args: Value) -> Result<Value> {
    let code = required_non_empty_string(&args, "code")?;
    validate_unsafe_mode(&args)?;
    let description = required_non_empty_string(&args, "description")?;
    validate_jsx_size(code)?;

    let payload = json!({
        "code": code,
        "args": args.get("args").cloned().unwrap_or_else(|| json!({})),
        "mode": "unsafe",
        "description": description,
    });
    let timeout_ms = timeout_ms_from_args(cfg, &args);

    run_bridge_command(
        cfg,
        bridge,
        "executeJsx",
        payload,
        &args,
        timeout_ms,
        "Error running Illustrator JSX",
    )
}

fn run_jsx_file_tool(cfg: &AppConfig, bridge: &BridgeClient, args: Value) -> Result<Value> {
    let path = required_non_empty_string(&args, "path")?;
    validate_unsafe_mode(&args)?;
    let description = required_non_empty_string(&args, "description")?;
    let code = fs::read_to_string(path)
        .with_context(|| format!("failed to read Illustrator JSX file: {path}"))?;
    validate_jsx_size(&code)?;

    let payload = json!({
        "code": code,
        "args": args.get("args").cloned().unwrap_or_else(|| json!({})),
        "mode": "unsafe",
        "description": description,
        "sourcePath": path,
    });
    let timeout_ms = timeout_ms_from_args(cfg, &args);

    run_bridge_command(
        cfg,
        bridge,
        "executeJsx",
        payload,
        &args,
        timeout_ms,
        "Error running Illustrator JSX file",
    )
}

fn get_jsx_result_tool(bridge: &BridgeClient, args: Value) -> Result<Value> {
    let request_id = required_non_empty_string(&args, "requestId")?;
    let record = bridge.get_request_record(request_id)?;
    Ok(tool_json(illustrator_record_value(record.to_value()))?)
}

fn list_illustrator_instances_tool(cfg: &AppConfig, bridge: &BridgeClient) -> Result<Value> {
    let instances =
        bridge.list_active_instances(Duration::from_millis(cfg.instance_heartbeat_stale_ms))?;
    Ok(tool_json(json!({
        "instances": instances,
        "count": instances.len(),
        "staleThresholdMs": cfg.instance_heartbeat_stale_ms
    }))?)
}

fn run_direct_bridge_call(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    command: &str,
    args: Value,
    error_prefix: &str,
) -> Result<Value> {
    let timeout_ms = timeout_ms_from_args(cfg, &args);
    run_bridge_command(
        cfg,
        bridge,
        command,
        args.clone(),
        &args,
        timeout_ms,
        error_prefix,
    )
}

fn run_bridge_command(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    command: &str,
    command_args: Value,
    option_args: &Value,
    timeout_ms: u64,
    error_prefix: &str,
) -> Result<Value> {
    match run_bridge_command_value(cfg, bridge, command, command_args, option_args, timeout_ms) {
        Ok(value) => Ok(tool_json(value)?),
        Err(error) => Ok(tool_error(format!("{error_prefix}: {error}"))),
    }
}

fn run_bridge_command_value(
    cfg: &AppConfig,
    bridge: &BridgeClient,
    command: &str,
    command_args: Value,
    option_args: &Value,
    timeout_ms: u64,
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

    let options = BridgeRunOptions {
        target: BridgeTarget {
            instance_id: option_args
                .get("targetInstanceId")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            version: option_args
                .get("targetVersion")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        },
        timeout: Duration::from_millis(timeout_ms),
        poll_interval: Duration::from_millis(cfg.poll_interval_ms),
        retention_seconds,
    };

    let outcome = bridge.run_command_sync(command, command_args, options)?;
    Ok(illustrator_record_value(outcome.to_value()))
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
        anyhow::bail!("only mode='unsafe' is currently supported for Illustrator JSX execution");
    }
    Ok(())
}

fn validate_jsx_size(code: &str) -> Result<()> {
    if code.len() > MAX_JSX_BYTES {
        anyhow::bail!(
            "Illustrator JSX code is too large: {} bytes > {} bytes",
            code.len(),
            MAX_JSX_BYTES
        );
    }
    Ok(())
}

fn timeout_ms_from_args(cfg: &AppConfig, args: &Value) -> u64 {
    args.get("timeoutMs")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or(cfg.result_timeout_ms)
}

fn illustrator_record_value(mut value: Value) -> Value {
    normalize_illustrator_host_text(&mut value);
    if let Some(obj) = value.as_object_mut() {
        if let Some(instance) = obj.remove("aeInstance") {
            obj.insert("illustratorInstance".to_string(), instance);
        }
    }
    value
}

fn normalize_illustrator_host_text(value: &mut Value) {
    match value {
        Value::String(text) => {
            *text = text
                .replace("After Effects", "Illustrator")
                .replace(
                    "Open Window > mcp-bridge-auto.jsx and enable Auto-run commands.",
                    "Open Window > Extensions > Illustrator MCP Bridge and enable Auto-run commands.",
                )
                .replace("AE to", "Illustrator to")
                .replace("AE instance", "Illustrator instance")
                .replace("AE bridge", "Illustrator bridge")
                .replace("AE to return", "Illustrator to return");
        }
        Value::Array(items) => {
            for item in items {
                normalize_illustrator_host_text(item);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                normalize_illustrator_host_text(item);
            }
        }
        _ => {}
    }
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
    Ok(tool_text(
        serde_json::to_string_pretty(&value).with_context(|| "failed to serialize tool JSON")?,
    ))
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

    #[test]
    fn tools_list_contains_generic_execution_tools() {
        let result = tools_list_result();
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array");
        assert!(tools
            .iter()
            .any(|t| t.get("name").and_then(Value::as_str) == Some("run-jsx")));
        assert!(tools
            .iter()
            .any(|t| t.get("name").and_then(Value::as_str) == Some("run-jsx-file")));
        assert!(tools
            .iter()
            .any(|t| t.get("name").and_then(Value::as_str) == Some("run-script")));
        assert!(tools
            .iter()
            .all(|t| t.get("name").and_then(Value::as_str) != Some("list-documents")));
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
