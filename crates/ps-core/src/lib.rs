use mcp_core::{PromptSpec, ToolSpec};
use serde_json::{json, Value};

pub const ALLOWED_SCRIPTS: &[&str] = &[
    "ping",
    "getAppInfo",
    "listDocuments",
    "getActiveDocument",
    "listLayers",
];

pub fn is_allowed_script(script: &str) -> bool {
    ALLOWED_SCRIPTS.contains(&script)
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run-jsx",
            description: "Run unsafe JavaScript/JSX-style code in Photoshop UXP and wait for a result",
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
            description: "Run an unsafe local JavaScript/JSX-style file in Photoshop UXP and wait for a result",
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
            description: "Run an allowlisted Photoshop template operation and wait for a result",
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
            description: "Get a retained Photoshop UXP request result by requestId",
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
            description: "Get the latest retained Photoshop request result, or a specific result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                }
            }),
        },
        ToolSpec {
            name: "get-help",
            description: "Get help on using the Photoshop MCP integration",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "list-photoshop-instances",
            description: "List active Photoshop UXP bridge panel instances and versions",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "run-bridge-test",
            description: "Run a Photoshop bridge test command to verify communication",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

pub fn prompt_specs() -> Vec<PromptSpec> {
    Vec::new()
}

pub fn prompt_messages(_name: &str, _args: &Value) -> Option<Value> {
    None
}

pub fn general_help_text() -> &'static str {
    r#"# Photoshop MCP Integration Help

To use this integration with Photoshop, follow these steps:

1. Load the Photoshop MCP Bridge UXP plugin with Adobe UXP Developer Tool
2. Open Adobe Photoshop
3. Open the Photoshop MCP Bridge panel from the Plugins menu
4. Enable "Auto-run commands" in the panel
5. Use tools from your MCP client and read back retained results

UXP bridge source:
- src/photoshop/uxp/mcp-bridge-photoshop/manifest.json

Best practices:
- Prefer run-jsx or run-jsx-file for general automation. The code runs inside the Photoshop UXP panel.
- Pass mode="unsafe" and a short description for custom code so the call is explicit.
- Use run-script only for allowlisted template operations listed below.
- Use get-jsx-result with requestId when a command times out or needs later inspection.
- Use list-photoshop-instances when multiple Photoshop versions or bridge panels are open.
- Specify targetInstanceId or targetVersion when more than one Photoshop instance is active.

Available scripts:
- ping
- getAppInfo
- listDocuments
- getActiveDocument
- listLayers
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_contains_initial_scripts() {
        assert!(is_allowed_script("ping"));
        assert!(is_allowed_script("getAppInfo"));
        assert!(is_allowed_script("listLayers"));
        assert!(!is_allowed_script("unknown"));
    }

    #[test]
    fn public_tools_use_small_photoshop_surface() {
        let names = tool_specs()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"run-jsx"));
        assert!(names.contains(&"run-jsx-file"));
        assert!(names.contains(&"run-script"));
        assert!(names.contains(&"get-jsx-result"));
        assert!(names.contains(&"get-results"));
        assert!(names.contains(&"get-help"));
        assert!(names.contains(&"list-photoshop-instances"));
        assert!(names.contains(&"run-bridge-test"));
        assert_eq!(names.len(), 8);
    }
}
