use mcp_core::{PromptArgument, PromptSpec, ToolSpec};
use serde_json::{json, Value};

pub const ALLOWED_SCRIPTS: &[&str] = &[
    "ping",
    "listSequences",
    "getActiveSequence",
    "setPlayheadTime",
    "exportSequence",
];

pub fn is_allowed_script(script: &str) -> bool {
    ALLOWED_SCRIPTS.contains(&script)
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run-script",
            description: "Run a predefined script in Premiere Pro (read/write depending on script)",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string" },
                    "parameters": { "type": "object" }
                },
                "required": ["script"]
            }),
        },
        ToolSpec {
            name: "get-results",
            description: "Get results from the last script executed in Premiere Pro",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "get-help",
            description: "Get help on using the Premiere Pro MCP integration",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "list-sequences",
            description: "List sequences in the current Premiere Pro project",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "get-active-sequence",
            description: "Get the active sequence in Premiere Pro",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "set-playhead-time",
            description: "Set the playhead time (in seconds) for a sequence",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "timeSeconds": { "type": "number", "minimum": 0 },
                    "timeTicks": { "type": "number", "minimum": 0 },
                    "sequenceName": { "type": "string" },
                    "sequenceIndex": { "type": "integer", "minimum": 0 }
                },
                "required": ["timeSeconds"]
            }),
        },
        ToolSpec {
            name: "export-sequence",
            description: "Export a sequence using an Adobe Media Encoder preset",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "outputPath": { "type": "string" },
                    "presetPath": { "type": "string" },
                    "sequenceName": { "type": "string" },
                    "sequenceIndex": { "type": "integer", "minimum": 0 },
                    "workAreaType": { "type": "integer", "minimum": 0, "maximum": 1 }
                },
                "required": ["outputPath", "presetPath"]
            }),
        },
    ]
}

pub fn prompt_specs() -> Vec<PromptSpec> {
    vec![
        PromptSpec {
            name: "list-sequences",
            description: "List sequences in the current Premiere Pro project",
            arguments: vec![],
        },
        PromptSpec {
            name: "set-playhead-time",
            description: "Move the playhead to a specific time in a sequence",
            arguments: vec![
                PromptArgument {
                    name: "timeSeconds",
                    description: "Time in seconds for the playhead",
                    required: true,
                },
                PromptArgument {
                    name: "sequenceName",
                    description: "Sequence name (optional if active sequence)",
                    required: false,
                },
            ],
        },
        PromptSpec {
            name: "export-sequence",
            description: "Export a sequence to a file",
            arguments: vec![
                PromptArgument {
                    name: "sequenceName",
                    description: "Sequence name (optional if active sequence)",
                    required: false,
                },
                PromptArgument {
                    name: "outputPath",
                    description: "Absolute output file path",
                    required: true,
                },
                PromptArgument {
                    name: "presetPath",
                    description: "Absolute path to the Adobe Media Encoder preset (.epr)",
                    required: true,
                },
                PromptArgument {
                    name: "workAreaType",
                    description: "0 = entire sequence, 1 = work area only",
                    required: false,
                },
            ],
        },
    ]
}

pub fn prompt_messages(name: &str, args: &Value) -> Option<Value> {
    let msg = match name {
        "list-sequences" => {
            "Please list all sequences in the current Premiere Pro project.".to_string()
        }
        "set-playhead-time" => {
            let time_seconds = args
                .get("timeSeconds")
                .and_then(Value::as_f64)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "0".to_string());
            let sequence_name = args
                .get("sequenceName")
                .and_then(Value::as_str)
                .unwrap_or("Active Sequence");
            format!(
                "Please move the playhead using the set-playhead-time tool.\nSequence: {sequence_name}\nTime (seconds): {time_seconds}\nAfter queueing, call get-results."
            )
        }
        "export-sequence" => {
            let sequence_name = args
                .get("sequenceName")
                .and_then(Value::as_str)
                .unwrap_or("Active Sequence");
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("<ABSOLUTE_OUTPUT_PATH>");
            let preset_path = args
                .get("presetPath")
                .and_then(Value::as_str)
                .unwrap_or("<ABSOLUTE_PRESET_PATH>");
            let work_area = args
                .get("workAreaType")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            format!(
                "Please export a sequence using the export-sequence tool.\nSequence: {sequence_name}\nOutput path: {output_path}\nPreset path: {preset_path}\nWork area type: {work_area}\nAfter queueing, call get-results."
            )
        }
        _ => return None,
    };

    Some(json!({
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": msg
                }
            }
        ]
    }))
}

pub fn general_help_text() -> &'static str {
    r#"# Premiere Pro MCP Integration Help

To use this integration with Premiere Pro, follow these steps:

1. Load the Premiere MCP Bridge UXP plugin with Adobe UXP Developer Tool
2. Open Adobe Premiere Pro
3. Open Window > UXP Plugins > Premiere MCP Bridge
4. Enable "Auto-run commands" in the panel
5. Use tools from your MCP client and read back results

UXP bridge source:
- src/premiere/uxp/mcp-bridge-premiere/manifest.json

Legacy CEP bridge is kept as a fallback only.

Best practices:
- Prefer sequenceName/sequenceIndex to target the right sequence
- Call get-results after queueing any tool command
- outputPath and presetPath should be absolute paths
- workAreaType: 0 = full sequence, 1 = work area only

Available scripts:
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_contains_core_scripts() {
        assert!(is_allowed_script("listSequences"));
        assert!(!is_allowed_script("unknown"));
    }
}
