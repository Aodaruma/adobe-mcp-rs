use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};

pub const ALLOWED_SCRIPTS: &[&str] = &[
    "listCompositions",
    "getProjectInfo",
    "getLayerInfo",
    "createComposition",
    "createTextLayer",
    "createShapeLayer",
    "createSolidLayer",
    "setLayerProperties",
    "setLayerKeyframe",
    "setLayerExpression",
    "applyEffect",
    "applyEffectTemplate",
    "listSupportedEffects",
    "describeEffect",
    "saveFramePng",
    "renderQueueAdd",
    "renderQueueStatus",
    "renderQueueStart",
    "renderQueueIsRendering",
    "setCurrentTime",
    "getCurrentTime",
    "setWorkArea",
    "getWorkArea",
    "getCompositionMarkers",
    "cleanupPreviewFolder",
    "setSuppressDialogs",
    "getSuppressDialogs",
    "projectOpen",
    "projectClose",
    "projectSave",
    "projectSaveAs",
    "applicationQuit",
    "test-animation",
    "bridgeTestEffects",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgePaths {
    pub root_dir: PathBuf,
    pub command_file: PathBuf,
    pub result_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub bridge: BridgePaths,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_result_timeout_ms")]
    pub result_timeout_ms: u64,
    #[serde(default = "default_result_retention_seconds")]
    pub result_retention_seconds: u64,
    #[serde(default = "default_result_retention_max_seconds")]
    pub result_retention_max_seconds: u64,
    #[serde(default = "default_instance_heartbeat_stale_ms")]
    pub instance_heartbeat_stale_ms: u64,
    #[serde(default = "default_daemon_addr")]
    pub daemon_addr: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        let root_dir = default_bridge_root_dir();
        let command_file = root_dir.join("ae_command.json");
        let result_file = root_dir.join("ae_mcp_result.json");

        Self {
            bridge: BridgePaths {
                root_dir,
                command_file,
                result_file,
            },
            poll_interval_ms: default_poll_interval_ms(),
            result_timeout_ms: default_result_timeout_ms(),
            result_retention_seconds: default_result_retention_seconds(),
            result_retention_max_seconds: default_result_retention_max_seconds(),
            instance_heartbeat_stale_ms: default_instance_heartbeat_stale_ms(),
            daemon_addr: default_daemon_addr(),
            log_level: default_log_level(),
        }
    }
}

fn default_poll_interval_ms() -> u64 {
    250
}

fn default_result_timeout_ms() -> u64 {
    5_000
}

fn default_result_retention_seconds() -> u64 {
    3_600
}

fn default_result_retention_max_seconds() -> u64 {
    86_400
}

fn default_instance_heartbeat_stale_ms() -> u64 {
    10_000
}

fn default_daemon_addr() -> String {
    "127.0.0.1:47655".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl AppConfig {
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = config_path {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("failed to read config file: {}", path.display()))?;
            let cfg: AppConfig =
                toml::from_str(&raw).with_context(|| "failed to parse TOML config")?;
            Ok(cfg)
        } else {
            Ok(Self::default())
        }
    }

    pub fn load_with_bridge_paths(config_path: Option<&Path>, bridge: BridgePaths) -> Result<Self> {
        let mut cfg = Self::load(config_path)?;
        if config_path.is_none() {
            cfg.bridge = bridge;
        }
        Ok(cfg)
    }
}

pub fn default_bridge_root_dir() -> PathBuf {
    default_bridge_root_dir_named("ae-mcp-bridge")
}

pub fn default_bridge_root_dir_named(folder: &str) -> PathBuf {
    default_documents_dir().join(folder)
}

fn default_documents_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(path) = windows_documents_dir() {
        return path;
    }

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents")
}

#[cfg(target_os = "windows")]
fn windows_documents_dir() -> Option<PathBuf> {
    let mut raw_path = ptr::null_mut();
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_DOCUMENTS, 0, ptr::null_mut(), &mut raw_path) };
    if hr < 0 || raw_path.is_null() {
        return None;
    }

    let path = unsafe { path_buf_from_pwstr(raw_path) };
    unsafe { CoTaskMemFree(raw_path.cast()) };
    Some(path)
}

#[cfg(target_os = "windows")]
unsafe fn path_buf_from_pwstr(raw_path: *const u16) -> PathBuf {
    let mut len = 0usize;
    while *raw_path.add(len) != 0 {
        len += 1;
    }
    PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(
        raw_path, len,
    )))
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[cfg(target_os = "windows")]
const FOLDERID_DOCUMENTS: Guid = Guid {
    data1: 0xFDD3_9AD0,
    data2: 0x238F,
    data3: 0x46AF,
    data4: [0xAD, 0xB4, 0x6C, 0x85, 0x48, 0x03, 0x69, 0xC7],
};

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
extern "system" {
    fn SHGetKnownFolderPath(
        rfid: *const Guid,
        dwflags: u32,
        htoken: *mut std::ffi::c_void,
        ppszpath: *mut *mut u16,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "ole32")]
extern "system" {
    fn CoTaskMemFree(pv: *mut std::ffi::c_void);
}

pub fn is_allowed_script(script: &str) -> bool {
    ALLOWED_SCRIPTS.contains(&script)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PromptSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub arguments: Vec<PromptArgument>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PromptArgument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run-jsx",
            description: "Run unsafe JSX code in After Effects and wait for a result",
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
            description: "Run an unsafe local JSX file in After Effects and wait for a result",
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
            name: "get-jsx-result",
            description: "Get a retained JSX/request result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                },
                "required": ["requestId"]
            }),
        },
        ToolSpec {
            name: "list-ae-instances",
            description: "List active After Effects bridge panel instances and versions",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "get-results",
            description:
                "Get the latest retained request result, or a specific result by requestId",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string", "minLength": 1 }
                }
            }),
        },
        ToolSpec {
            name: "get-help",
            description: "Get help on using the After Effects MCP integration",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "save-frame-png",
            description:
                "Save a single frame from a composition as PNG without using the render queue",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "compId": { "type": "integer", "minimum": 1 },
                    "compIndex": { "type": "integer", "minimum": 1 },
                    "compName": { "type": "string" },
                    "timeSeconds": { "type": "number", "minimum": 0 },
                    "outputPath": { "type": "string" },
                    "overwrite": { "type": "boolean" },
                    "suppressDialogs": { "type": "boolean" },
                    "timeoutMs": { "type": "integer", "minimum": 1 },
                    "resultRetentionSeconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
                    "targetInstanceId": { "type": "string", "minLength": 1 },
                    "targetVersion": { "type": "string", "minLength": 1 }
                },
                "required": ["outputPath"]
            }),
        },
        ToolSpec {
            name: "cleanup-preview-folder",
            description: "Delete preview PNG files from a folder",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folderPath": { "type": "string" },
                    "extension": { "type": "string" },
                    "prefix": { "type": "string" },
                    "maxAgeSeconds": { "type": "number", "minimum": 0 },
                    "timeoutMs": { "type": "integer", "minimum": 1 },
                    "resultRetentionSeconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
                    "targetInstanceId": { "type": "string", "minLength": 1 },
                    "targetVersion": { "type": "string", "minLength": 1 }
                },
                "required": ["folderPath"]
            }),
        },
        ToolSpec {
            name: "run-bridge-test",
            description: "Run the bridge test effects script to verify communication",
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
            name: "list-compositions",
            description: "List compositions in the current After Effects project",
            arguments: vec![],
        },
        PromptSpec {
            name: "analyze-composition",
            description: "Analyze a composition by name",
            arguments: vec![PromptArgument {
                name: "compositionName",
                description: "Name of the composition to analyze",
                required: true,
            }],
        },
        PromptSpec {
            name: "create-composition",
            description: "Create a new composition with custom settings",
            arguments: vec![],
        },
        PromptSpec {
            name: "save-preview-png",
            description: "Save a single-frame PNG preview from a composition",
            arguments: vec![
                PromptArgument {
                    name: "compositionName",
                    description: "Name of the composition to preview (optional if active comp)",
                    required: false,
                },
                PromptArgument {
                    name: "timeSeconds",
                    description: "Time in seconds for the preview frame (optional)",
                    required: false,
                },
                PromptArgument {
                    name: "outputPath",
                    description: "Absolute path for the PNG file to write",
                    required: true,
                },
            ],
        },
        PromptSpec {
            name: "render-queue-setup",
            description: "Add a composition to the render queue without starting the render",
            arguments: vec![
                PromptArgument {
                    name: "compositionName",
                    description: "Name of the composition to render (optional if active comp)",
                    required: false,
                },
                PromptArgument {
                    name: "outputPath",
                    description: "Absolute path for the output file",
                    required: true,
                },
                PromptArgument {
                    name: "renderSettingsTemplate",
                    description: "Render settings template name (optional)",
                    required: false,
                },
                PromptArgument {
                    name: "outputModuleTemplate",
                    description: "Output module template name (optional)",
                    required: false,
                },
            ],
        },
        PromptSpec {
            name: "cleanup-preview-folder",
            description: "Delete preview PNG files in a folder",
            arguments: vec![
                PromptArgument {
                    name: "folderPath",
                    description: "Absolute path to the preview folder",
                    required: true,
                },
                PromptArgument {
                    name: "extension",
                    description: "File extension to target (default: png)",
                    required: false,
                },
                PromptArgument {
                    name: "prefix",
                    description: "Filename prefix to filter (optional)",
                    required: false,
                },
                PromptArgument {
                    name: "maxAgeSeconds",
                    description: "Only delete files older than this many seconds (optional)",
                    required: false,
                },
            ],
        },
    ]
}

pub fn prompt_messages(name: &str, args: &Value) -> Option<Value> {
    let msg = match name {
        "list-compositions" => {
            "Please list all compositions in the current After Effects project.".to_string()
        }
        "analyze-composition" => {
            let target = args
                .get("compositionName")
                .and_then(Value::as_str)
                .unwrap_or("Unknown");
            format!(
                "Please analyze the composition named \"{target}\" in the current After Effects project. Provide details about its duration, frame rate, resolution, and layers."
            )
        }
        "create-composition" => {
            "Please create a new composition with custom settings. You can specify parameters like name, width, height, frame rate, etc.".to_string()
        }
        "save-preview-png" => {
            let composition_name = args
                .get("compositionName")
                .and_then(Value::as_str)
                .unwrap_or("Active Composition");
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("<ABSOLUTE_OUTPUT_PATH>");
            let time_seconds = args
                .get("timeSeconds")
                .and_then(Value::as_f64)
                .map(|v| v.to_string())
                .unwrap_or("current time".to_string());
            format!(
                "Please save a single-frame PNG preview using the save-frame-png tool.\nComposition: {composition_name}\nTime: {time_seconds}\nOutput path: {output_path}\nRemember: outputPath is required, and after queueing call get-results."
            )
        }
        "render-queue-setup" => {
            let composition_name = args
                .get("compositionName")
                .and_then(Value::as_str)
                .unwrap_or("Active Composition");
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("<ABSOLUTE_OUTPUT_PATH>");
            let render_template = args
                .get("renderSettingsTemplate")
                .and_then(Value::as_str)
                .unwrap_or("(default)");
            let output_template = args
                .get("outputModuleTemplate")
                .and_then(Value::as_str)
                .unwrap_or("(default)");
            format!(
                "Please add a render queue item using the render-queue-add tool.\nComposition: {composition_name}\nOutput path: {output_path}\nRender settings template: {render_template}\nOutput module template: {output_template}\nDo not start rendering automatically. After queueing, call get-results."
            )
        }
        "cleanup-preview-folder" => {
            let folder_path = args
                .get("folderPath")
                .and_then(Value::as_str)
                .unwrap_or("<ABSOLUTE_FOLDER_PATH>");
            let extension = args
                .get("extension")
                .and_then(Value::as_str)
                .unwrap_or("png");
            let prefix = args
                .get("prefix")
                .and_then(Value::as_str)
                .unwrap_or("(none)");
            let max_age = args
                .get("maxAgeSeconds")
                .and_then(Value::as_f64)
                .map(|v| v.to_string())
                .unwrap_or("(none)".to_string());
            format!(
                "Please clean up preview files using the cleanup-preview-folder tool.\nFolder: {folder_path}\nExtension: {extension}\nPrefix: {prefix}\nMax age (seconds): {max_age}\nAfter queueing, call get-results."
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
    r#"# After Effects MCP Integration Help

To use this integration with After Effects, follow these steps:

1. Install bridge panel script with the installer command
2. Start the broker with `ae-mcp serve-daemon`; on Windows you can also use `ae-mcp autostart install` and `ae-mcp autostart start`
3. Open Adobe After Effects
4. Open Window > mcp-bridge-auto.jsx
5. Enable "Auto-run commands"
6. Use tools from MCP client and read back results

Best practices:
- Use list-ae-instances when multiple After Effects versions are open
- Specify targetInstanceId or targetVersion when more than one AE instance is active
- Prefer compId/layerId when available to avoid index drift
- Use get-jsx-result with requestId after a timeout
- save-frame-png is optimized for fast previews (single PNG only)
- Use render-queue-start when you want MCP to wait until render completion
- Use suppressDialogs (default true) to avoid blocking dialogs
- Ensure outputPath points to a writable location
- Default mode is non-interactive. For LLM automation, keep interactive=false and pass explicit paths.
- For user handoff, set interactive=true (dialogs allowed; suppressDialogs is treated as false).
- In non-interactive close/open/quit flows, avoid prompt-based close options and provide saveAsPath when needed.

Available scripts:
- getProjectInfo
- listCompositions
- getLayerInfo
- createComposition
- createTextLayer
- createShapeLayer
- createSolidLayer
- setLayerProperties
- setLayerKeyframe
- setLayerExpression
- applyEffect
- applyEffectTemplate
- listSupportedEffects
- describeEffect
- saveFramePng
- renderQueueAdd
- renderQueueStatus
- renderQueueStart
- renderQueueIsRendering
- setCurrentTime
- getCurrentTime
- setWorkArea
- getWorkArea
- getCompositionMarkers
- cleanupPreviewFolder
- setSuppressDialogs
- getSuppressDialogs
- projectOpen
- projectClose
- projectSave
- projectSaveAs
- applicationQuit
"#
}

pub fn effects_help_text() -> &'static str {
    r#"# After Effects Effects Help

Common Effect Match Names:
- Gaussian Blur: ADBE Gaussian Blur 2
- Directional Blur: ADBE Directional Blur
- Brightness & Contrast: ADBE Brightness & Contrast 2
- Color Balance (HLS): ADBE Color Balance (HLS)
- Curves: ADBE CurvesCustom
- Glow: ADBE Glow
- Drop Shadow: ADBE Drop Shadow
- Vibrance: ADBE Vibrance

Templates:
- gaussian-blur
- directional-blur
- color-balance
- brightness-contrast
- curves
- glow
- drop-shadow
- smooth-gradient
- cinematic-look
- text-pop

Additional tools:
- list-supported-effects
- describe-effect
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_files() {
        let cfg = AppConfig::default();
        assert!(cfg.bridge.command_file.ends_with("ae_command.json"));
        assert!(cfg.bridge.result_file.ends_with("ae_mcp_result.json"));
        assert_eq!(cfg.poll_interval_ms, 250);
    }

    #[test]
    fn script_allowlist_contains_core_entries() {
        assert!(is_allowed_script("listCompositions"));
        assert!(is_allowed_script("applyEffectTemplate"));
        assert!(!is_allowed_script("unknownScript"));
    }
}
