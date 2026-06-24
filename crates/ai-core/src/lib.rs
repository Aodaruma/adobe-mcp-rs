use mcp_core::{PromptArgument, PromptSpec, ToolSpec};
use serde_json::{json, Value};

pub const ALLOWED_SCRIPTS: &[&str] = &[
    "ping",
    "getAppInfo",
    "listDocuments",
    "getActiveDocument",
    "listArtboards",
    "listLayers",
    "exportDocument",
];

pub fn is_allowed_script(script: &str) -> bool {
    ALLOWED_SCRIPTS.contains(&script)
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run-jsx",
            description: "Run unsafe JSX/ExtendScript code in Illustrator and wait for a result",
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
            description: "Run an unsafe local JSX/ExtendScript file in Illustrator and wait for a result",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "minLength": 1 },
                    "args": { "type": "object" },
                    "mode": { "type": "string", "enum": ["unsafe"] },
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
            description: "Run an allowlisted Illustrator template operation and wait for a result",
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
            description: "Get a retained Illustrator request result by requestId",
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
            description: "Get the latest retained Illustrator request result, or a specific result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                }
            }),
        },
        ToolSpec {
            name: "get-help",
            description: "Get help on using the Illustrator MCP integration",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "list-illustrator-instances",
            description: "List active Illustrator CEP bridge panel instances and versions",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "run-bridge-test",
            description: "Run an Illustrator bridge test command to verify communication",
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
            name: "list-documents",
            description: "List open Illustrator documents",
            arguments: vec![],
        },
        PromptSpec {
            name: "list-artboards",
            description: "List artboards in the active or selected Illustrator document",
            arguments: vec![PromptArgument {
                name: "documentName",
                description:
                    "Document name to inspect (optional; active document is used by default)",
                required: false,
            }],
        },
        PromptSpec {
            name: "export-document",
            description: "Export the active or selected Illustrator document",
            arguments: vec![
                PromptArgument {
                    name: "outputPath",
                    description: "Absolute output file path",
                    required: true,
                },
                PromptArgument {
                    name: "format",
                    description: "Export format: png24, png8, jpg, svg, or pdf",
                    required: false,
                },
                PromptArgument {
                    name: "documentName",
                    description:
                        "Document name to export (optional; active document is used by default)",
                    required: false,
                },
            ],
        },
    ]
}

pub fn prompt_messages(name: &str, args: &Value) -> Option<Value> {
    let msg = match name {
        "list-documents" => {
            "Please list open Illustrator documents using run-script with script=\"listDocuments\"."
                .to_string()
        }
        "list-artboards" => {
            let document_name = args
                .get("documentName")
                .and_then(Value::as_str)
                .unwrap_or("Active Document");
            format!(
                "Please list Illustrator artboards using run-script with script=\"listArtboards\".\nDocument: {document_name}\nPass documentName only when a non-active document should be targeted."
            )
        }
        "export-document" => {
            let document_name = args
                .get("documentName")
                .and_then(Value::as_str)
                .unwrap_or("Active Document");
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("<ABSOLUTE_OUTPUT_PATH>");
            let format = args
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("png24");
            format!(
                "Please export an Illustrator document using run-script with script=\"exportDocument\".\nDocument: {document_name}\nOutput path: {output_path}\nFormat: {format}\nPass outputPath, format, and optionally documentName/documentIndex in parameters."
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
    r#"# Illustrator MCP Integration Help

To use this integration with Illustrator, follow these steps:

1. Install or copy the CEP bridge panel from `src/illustrator/cep/mcp-bridge-illustrator`
2. Open Adobe Illustrator
3. Open Window > Extensions > Illustrator MCP Bridge
4. Enable "Auto-run commands" in the panel
5. Use tools from your MCP client and read back results

Bridge files are stored under `~/Documents/ai-mcp-bridge`.

Best practices:
- Prefer run-jsx or run-jsx-file for general automation. The code runs inside the Illustrator ExtendScript context.
- Pass mode="unsafe" and a short description for custom code so the call is explicit.
- Use run-script only for allowlisted template operations listed below.
- Use get-jsx-result with requestId when a command times out or needs later inspection.
- Use documentName or documentIndex when more than one Illustrator document is open.
- outputPath should be an absolute path and point to a writable location.

Available scripts:
- ping
- getAppInfo
- listDocuments
- getActiveDocument
- listArtboards
- listLayers
- exportDocument
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_contains_core_scripts() {
        assert!(is_allowed_script("ping"));
        assert!(is_allowed_script("listDocuments"));
        assert!(is_allowed_script("exportDocument"));
        assert!(!is_allowed_script("unknown"));
    }

    #[test]
    fn public_tools_use_generic_execution_surface() {
        let names = tool_specs()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"run-jsx"));
        assert!(names.contains(&"run-jsx-file"));
        assert!(names.contains(&"get-jsx-result"));
        assert!(names.contains(&"run-script"));
        assert!(names.contains(&"list-illustrator-instances"));
        assert!(!names.contains(&"list-documents"));
    }
}
