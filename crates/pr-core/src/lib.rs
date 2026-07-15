use mcp_core::{PromptArgument, PromptSpec, ToolSpec};
use serde_json::{json, Value};

pub const ALLOWED_SCRIPTS: &[&str] = &[
    "ping",
    "getProjectInfo",
    "listSequences",
    "getActiveSequence",
    "getSequenceInfo",
    "setPlayheadTime",
    "exportSequence",
];

pub fn is_allowed_script(script: &str) -> bool {
    ALLOWED_SCRIPTS.contains(&script)
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run-jsx",
            description: "Run unsafe JavaScript/JSX-style code in Premiere Pro UXP and wait for a result",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "minLength": 1 },
                    "args": { "type": "object" },
                    "mode": { "type": "string", "enum": ["unsafe"] },
                    "description": { "type": "string", "minLength": 1 },
                    "timeoutMs": { "type": "integer", "minimum": 1 },
                    "resultRetentionSeconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
                    "targetInstanceId": { "type": "string", "minLength": 1 },
                    "targetVersion": { "type": "string", "minLength": 1 }
                },
                "required": ["code", "mode", "description"]
            }),
        },
        ToolSpec {
            name: "run-jsx-file",
            description: "Run a validated local JavaScript/JSX-style file in Premiere Pro UXP and wait for a result",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "minLength": 1 },
                    "args": { "type": "object" },
                    "mode": { "type": "string", "enum": ["unsafe", "trusted"] },
                    "description": { "type": "string", "minLength": 1 },
                    "timeoutMs": { "type": "integer", "minimum": 1 },
                    "resultRetentionSeconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
                    "targetInstanceId": { "type": "string", "minLength": 1 },
                    "targetVersion": { "type": "string", "minLength": 1 }
                },
                "required": ["path", "mode", "description"]
            }),
        },
        ToolSpec {
            name: "run-script",
            description: "Run an allowlisted Premiere Pro template operation and wait for a result",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "enum": ALLOWED_SCRIPTS
                    },
                    "parameters": { "type": "object" },
                    "timeoutMs": { "type": "integer", "minimum": 1 },
                    "resultRetentionSeconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
                    "targetInstanceId": { "type": "string", "minLength": 1 },
                    "targetVersion": { "type": "string", "minLength": 1 }
                },
                "required": ["script"]
            }),
        },
        ToolSpec {
            name: "get-jsx-result",
            description: "Get a retained Premiere Pro UXP request result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                },
                "required": ["requestId"]
            }),
        },
        ToolSpec {
            name: "get-results",
            description: "Get the latest retained Premiere Pro request result, or a specific result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                }
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
            name: "list-premiere-instances",
            description: "List active Premiere Pro UXP bridge panel instances and versions",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "run-bridge-test",
            description: "Run a Premiere Pro bridge test command to verify communication",
            input_schema: json!({
                "type": "object",
                "properties": {}
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
            "Please list all sequences in the current Premiere Pro project using run-script with script=\"listSequences\".".to_string()
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
                "Please move the playhead using run-script with script=\"setPlayheadTime\".\nSequence: {sequence_name}\nTime (seconds): {time_seconds}\nPass sequenceName and timeSeconds in parameters."
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
                "Please export a sequence using run-script with script=\"exportSequence\".\nSequence: {sequence_name}\nOutput path: {output_path}\nPreset path: {preset_path}\nWork area type: {work_area}\nPass sequenceName, outputPath, presetPath, and workAreaType in parameters."
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
5. Start `pr-mcp serve-daemon`
6. Use tools from your MCP client and read back results

UXP bridge source:
- src/premiere/uxp/mcp-bridge-premiere/manifest.json

Legacy CEP bridge is kept as a fallback only.

Best practices:
- Prefer run-jsx or run-jsx-file for general automation. The code runs inside the Premiere UXP panel.
- Pass mode="unsafe" and a short description for custom code so the call is explicit.
- run-jsx-file mode="unsafe" is limited to configured allowed roots; mode="trusted" requires an exact configured path/SHA-256 entry.
- "unsafe" is not a sandbox; code runs with the Adobe host's authority.
- Use run-script only for allowlisted template operations listed below.
- Use get-jsx-result with requestId when a command times out or needs later inspection.
- Prefer sequenceName/sequenceIndex to target the right sequence
- outputPath and presetPath should be absolute paths
- workAreaType: 0 = full sequence, 1 = work area only

Available scripts:
- ping
- getProjectInfo
- listSequences
- getActiveSequence
- getSequenceInfo
- setPlayheadTime
- exportSequence
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_contains_core_scripts() {
        assert!(is_allowed_script("listSequences"));
        assert!(is_allowed_script("getProjectInfo"));
        assert!(is_allowed_script("getSequenceInfo"));
        assert!(!is_allowed_script("unknown"));
    }

    #[test]
    fn public_tools_use_generic_execution_surface() {
        let specs = tool_specs();
        let file_tool = specs
            .iter()
            .find(|tool| tool.name == "run-jsx-file")
            .unwrap();
        assert!(file_tool.input_schema["properties"]["mode"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("trusted")));
        let names = specs.into_iter().map(|tool| tool.name).collect::<Vec<_>>();
        assert!(names.contains(&"run-jsx"));
        assert!(names.contains(&"run-jsx-file"));
        assert!(names.contains(&"get-jsx-result"));
        assert!(names.contains(&"run-script"));
        assert!(names.contains(&"list-premiere-instances"));
        assert!(!names.contains(&"list-sequences"));
    }
}
