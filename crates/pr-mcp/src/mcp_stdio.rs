use anyhow::{anyhow, Context, Result};
use bridge_core::BridgeClient;
use mcp_core::AppConfig;
use pr_core::{general_help_text, prompt_messages, prompt_specs, tool_specs};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tracing::{debug, error, info};

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
                "name": "PremiereServer",
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
                "name": "sequences",
                "description": "List sequences in the current Premiere Pro project",
                "uri": "premiere://sequences",
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

    if uri != "premiere://sequences" {
        return Err(anyhow!("unknown resource URI: {uri}"));
    }

    bridge.clear_results_file()?;
    bridge.write_command_file("listSequences", json!({}))?;
    let text = bridge.wait_for_bridge_result(
        Some("listSequences"),
        Duration::from_millis(cfg.result_timeout_ms + 1_000),
        Duration::from_millis(cfg.poll_interval_ms),
    )?;

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
    _cfg: &AppConfig,
    bridge: &BridgeClient,
    name: &str,
    args: Value,
) -> Result<Value> {
    match name {
        "run-script" => {
            let script = args
                .get("script")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("'script' is required"))?;
            if !pr_core::is_allowed_script(script) {
                return Ok(tool_error(format!(
                    "Script \"{script}\" is not allowed. Allowed scripts are: {}",
                    pr_core::ALLOWED_SCRIPTS.join(", ")
                )));
            }
            let parameters = args.get("parameters").cloned().unwrap_or_else(|| json!({}));
            bridge.clear_results_file()?;
            bridge.write_command_file(script, parameters)?;
            Ok(tool_text(format!(
                "Command to run \"{script}\" has been queued.\nPlease ensure the \"Premiere MCP Bridge\" UXP panel is open in Premiere Pro.\nUse the \"get-results\" tool after a few seconds to check for results."
            )))
        }
        "get-results" => {
            let text = bridge.read_results_with_stale_warning(Duration::from_secs(30))?;
            Ok(tool_text(text))
        }
        "get-help" => Ok(tool_text(general_help_text().to_string())),
        "list-sequences" => {
            bridge.write_command_file("listSequences", args)?;
            Ok(tool_text(
                "Command to list sequences has been queued.\nUse the \"get-results\" tool after a few seconds to check for results."
                    .to_string(),
            ))
        }
        "get-active-sequence" => {
            bridge.write_command_file("getActiveSequence", args)?;
            Ok(tool_text(
                "Command to get the active sequence has been queued.\nUse the \"get-results\" tool after a few seconds to check for results."
                    .to_string(),
            ))
        }
        "set-playhead-time" => {
            bridge.write_command_file("setPlayheadTime", args)?;
            Ok(tool_text(
                "Command to set playhead time has been queued.\nUse the \"get-results\" tool after a few seconds to check for results."
                    .to_string(),
            ))
        }
        "export-sequence" => {
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            bridge.write_command_file("exportSequence", args)?;
            Ok(tool_text(format!(
                "Command to export sequence has been queued.\nOutput: {output_path}\nUse the \"get-results\" tool after a few seconds to check for results."
            )))
        }
        _ => Ok(tool_error(format!("Unknown tool: {name}"))),
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
    fn tools_list_contains_run_script() {
        let result = tools_list_result();
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array");
        assert!(tools
            .iter()
            .any(|t| t.get("name").and_then(Value::as_str) == Some("run-script")));
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
