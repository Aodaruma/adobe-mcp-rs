use mcp_core::{PromptSpec, ToolSpec};
use serde_json::{json, Value};

pub const ALLOWED_TEMPLATES: &[&str] = &[
    "ping",
    "getAppInfo",
    "listDocuments",
    "getActiveDocument",
    "listPages",
    "listStories",
];

pub fn is_allowed_template(script: &str) -> bool {
    ALLOWED_TEMPLATES.contains(&script)
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run-script",
            description: "Run unsafe UXP script code in InDesign UXP and wait for a result",
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
            name: "run-script-file",
            description:
                "Run a validated local UXP script file in InDesign UXP and wait for a result",
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
            name: "run-template",
            description: "Run an allowlisted InDesign template operation and wait for a result",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "enum": ALLOWED_TEMPLATES
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
            name: "get-script-result",
            description: "Get a retained InDesign UXP request result by requestId",
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
            description:
                "Get the latest retained InDesign request result, or a specific result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                }
            }),
        },
        ToolSpec {
            name: "get-help",
            description: "Get help on using the InDesign MCP integration",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "list-indesign-instances",
            description: "List active InDesign UXP startup bridge instances and versions",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "run-bridge-test",
            description: "Run an InDesign bridge test command to verify communication",
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
    r#"# InDesign MCP Integration Help

To use this experimental integration with InDesign, follow these steps:

1. Install `mcp-bridge-indesign.idjs` in the InDesign `Scripts/Startup Scripts` folder
2. Start `id-mcp serve-daemon`
3. Open Adobe InDesign; no panel or Auto-run toggle is required
4. Use `list-indesign-instances`, then `run-bridge-test`
5. Use tools from your MCP client and read back retained results

UXP bridge source:
- src/indesign/uxp/mcp-bridge-indesign.idjs

Best practices:
- Prefer run-script or run-script-file for general automation. Raw code is passed to InDesign `app.doScript` as `ScriptLanguage.UXPSCRIPT`; it does not use eval/Function.
- Pass mode="unsafe" and a short description for custom code so the call is explicit.
- run-script-file mode="unsafe" is limited to configured allowed roots; mode="trusted" requires an exact configured path/SHA-256 entry.
- "unsafe" is not a sandbox; code runs with the Adobe host's authority. InDesign UXP scripts have fixed full filesystem/network permissions.
- Use run-template for the allowlisted operations listed below.
- Use get-script-result with requestId when a command times out or needs later inspection.
- Use list-indesign-instances when multiple InDesign versions are open.
- Specify targetInstanceId or targetVersion when more than one InDesign instance is active.

This is a non-host-tested PoC. Adobe documents `Application.doScript` with String input, but raw string execution from a long-running UXP Startup Script still requires live InDesign verification.

Available templates:
- ping
- getAppInfo
- listDocuments
- getActiveDocument
- listPages
- listStories
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_contains_initial_templates() {
        assert!(is_allowed_template("ping"));
        assert!(is_allowed_template("getAppInfo"));
        assert!(is_allowed_template("listStories"));
        assert!(!is_allowed_template("unknown"));
    }

    #[test]
    fn public_tools_use_raw_first_indesign_surface() {
        let specs = tool_specs();
        let file_tool = specs
            .iter()
            .find(|tool| tool.name == "run-script-file")
            .unwrap();
        assert!(file_tool.input_schema["properties"]["mode"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("trusted")));
        let names = specs.into_iter().map(|tool| tool.name).collect::<Vec<_>>();
        assert!(names.contains(&"run-script"));
        assert!(names.contains(&"run-script-file"));
        assert!(names.contains(&"run-template"));
        assert!(names.contains(&"get-script-result"));
        assert!(names.contains(&"get-results"));
        assert!(names.contains(&"get-help"));
        assert!(names.contains(&"list-indesign-instances"));
        assert!(names.contains(&"run-bridge-test"));
        assert_eq!(names.len(), 8);
    }
}
