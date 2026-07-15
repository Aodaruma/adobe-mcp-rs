use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
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

/// After Effects tools advertised through MCP `tools/list`.
///
/// Keep this list in the same order as [`tool_specs`]. Hidden legacy dispatch
/// names are intentionally excluded from the public contract.
pub const PUBLIC_TOOL_NAMES: &[&str] = &[
    "run-jsx",
    "run-jsx-file",
    "get-jsx-result",
    "list-ae-instances",
    "get-results",
    "get-help",
    "save-frame-png",
    "cleanup-preview-folder",
    "run-bridge-test",
];

/// Names accepted only so older MCP clients receive a migration path.
pub const LEGACY_TOOL_NAMES: &[&str] = &[
    "run-script",
    "create-composition",
    "setLayerKeyframe",
    "setLayerExpression",
    "test-animation",
    "apply-effect",
    "apply-effect-template",
    "list-supported-effects",
    "describe-effect",
    "render-queue-add",
    "render-queue-status",
    "render-queue-start",
    "render-queue-is-rendering",
    "set-current-time",
    "get-current-time",
    "set-work-area",
    "get-work-area",
    "get-composition-markers",
    "set-suppress-dialogs",
    "get-suppress-dialogs",
    "project-open",
    "project-close",
    "project-save",
    "project-save-as",
    "application-quit",
    "mcp_aftereffects_applyEffect",
    "mcp_aftereffects_applyEffectTemplate",
    "mcp_aftereffects_listSupportedEffects",
    "mcp_aftereffects_describeEffect",
    "mcp_aftereffects_get_effects_help",
];

pub fn legacy_tool_replacement(name: &str) -> Option<&'static str> {
    if !LEGACY_TOOL_NAMES.contains(&name) {
        return None;
    }

    Some(match name {
        "mcp_aftereffects_get_effects_help" => "get-help",
        _ => "run-jsx",
    })
}

/// Static metadata that defines one Adobe host integration.
///
/// Adding a host starts with declaring one of these values; bridge paths and
/// host-aware diagnostics are then derived from the same source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub binary_name: &'static str,
    pub bridge_root_name: &'static str,
    pub command_file_name: &'static str,
    pub result_file_name: &'static str,
    pub instance_tool_name: &'static str,
    pub bridge_runtime: &'static str,
    pub bridge_setup_hint: &'static str,
    pub daemon_port: u16,
}

impl HostSpec {
    pub fn bridge_paths(self) -> BridgePaths {
        let root_dir = default_bridge_root_dir_named(self.bridge_root_name);
        BridgePaths {
            command_file: root_dir.join(self.command_file_name),
            result_file: root_dir.join(self.result_file_name),
            root_dir,
        }
    }

    pub fn daemon_addr(self) -> String {
        format!("127.0.0.1:{}", self.daemon_port)
    }
}

pub const AFTER_EFFECTS_HOST: HostSpec = HostSpec {
    id: "aftereffects",
    display_name: "After Effects",
    binary_name: "ae-mcp",
    bridge_root_name: "ae-mcp-bridge",
    command_file_name: "ae_command.json",
    result_file_name: "ae_mcp_result.json",
    instance_tool_name: "list-ae-instances",
    bridge_runtime: "extendscript-startup",
    bridge_setup_hint:
        "Install the AE Startup bridge, enable file/network access, and restart After Effects.",
    daemon_port: 47655,
};

pub const PREMIERE_PRO_HOST: HostSpec = HostSpec {
    id: "premiere",
    display_name: "Premiere Pro",
    binary_name: "pr-mcp",
    bridge_root_name: "pr-mcp-bridge",
    command_file_name: "pr_command.json",
    result_file_name: "pr_mcp_result.json",
    instance_tool_name: "list-premiere-instances",
    bridge_runtime: "uxp",
    bridge_setup_hint:
        "Open Window > UXP Plugins > Premiere MCP Bridge and enable Auto-run commands.",
    daemon_port: 47656,
};

pub const PHOTOSHOP_HOST: HostSpec = HostSpec {
    id: "photoshop",
    display_name: "Photoshop",
    binary_name: "ps-mcp",
    bridge_root_name: "ps-mcp-bridge",
    command_file_name: "ps_command.json",
    result_file_name: "ps_mcp_result.json",
    instance_tool_name: "list-photoshop-instances",
    bridge_runtime: "uxp",
    bridge_setup_hint: "Open the Photoshop MCP Bridge panel and enable Auto-run commands.",
    daemon_port: 47657,
};

pub const ILLUSTRATOR_HOST: HostSpec = HostSpec {
    id: "illustrator",
    display_name: "Illustrator",
    binary_name: "ai-mcp",
    bridge_root_name: "ai-mcp-bridge",
    command_file_name: "ai_command.json",
    result_file_name: "ai_mcp_result.json",
    instance_tool_name: "list-illustrator-instances",
    bridge_runtime: "cep-extendscript",
    bridge_setup_hint:
        "Open Window > Extensions > Illustrator MCP Bridge and enable Auto-run commands.",
    daemon_port: 47658,
};

pub const INDESIGN_HOST: HostSpec = HostSpec {
    id: "indesign",
    display_name: "InDesign",
    binary_name: "id-mcp",
    bridge_root_name: "id-mcp-bridge",
    command_file_name: "id_command.json",
    result_file_name: "id_mcp_result.json",
    instance_tool_name: "list-indesign-instances",
    bridge_runtime: "uxp-startup-script",
    bridge_setup_hint: "Install mcp-bridge-indesign.idjs into the InDesign Startup Scripts folder.",
    daemon_port: 47659,
};

pub const HOST_SPECS: &[HostSpec] = &[
    AFTER_EFFECTS_HOST,
    PREMIERE_PRO_HOST,
    PHOTOSHOP_HOST,
    ILLUSTRATOR_HOST,
    INDESIGN_HOST,
];

pub fn host_spec_by_id(id: &str) -> Option<HostSpec> {
    HOST_SPECS.iter().copied().find(|spec| spec.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgePaths {
    pub root_dir: PathBuf,
    pub command_file: PathBuf,
    pub result_file: PathBuf,
}

pub const DEFAULT_SCRIPT_FILE_MAX_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrustedScriptFile {
    /// Absolute path of a bundled/deployment-managed script.
    pub path: PathBuf,
    /// Lower- or upper-case SHA-256 hex is accepted; comparisons are case-insensitive.
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScriptFilePolicyConfig {
    /// Canonical roots from which user-provided `mode = "unsafe"` files may run.
    #[serde(default)]
    pub allowed_roots: Vec<PathBuf>,
    /// Exact path and digest allowlist for deployment-managed `mode = "trusted"` files.
    #[serde(default)]
    pub trusted_scripts: Vec<TrustedScriptFile>,
    #[serde(default = "default_script_file_max_bytes")]
    pub max_bytes: u64,
}

impl Default for ScriptFilePolicyConfig {
    fn default() -> Self {
        // Migration default for configurations created before this policy existed. It keeps
        // workspace-local file execution working, while no longer allowing arbitrary paths.
        let allowed_roots = std::env::current_dir().into_iter().collect();
        Self {
            allowed_roots,
            trusted_scripts: Vec::new(),
            max_bytes: DEFAULT_SCRIPT_FILE_MAX_BYTES,
        }
    }
}

fn default_script_file_max_bytes() -> u64 {
    DEFAULT_SCRIPT_FILE_MAX_BYTES
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFileAudit {
    pub host_id: String,
    pub mode: String,
    pub source_path: String,
    pub source_sha256: String,
    pub source_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedScriptFile {
    pub code: String,
    pub audit: ScriptFileAudit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default = "default_host_id")]
    pub host_id: String,
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
    #[serde(default)]
    pub script_files: ScriptFilePolicyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let bridge = AFTER_EFFECTS_HOST.bridge_paths();

        Self {
            host_id: AFTER_EFFECTS_HOST.id.to_string(),
            bridge,
            poll_interval_ms: default_poll_interval_ms(),
            result_timeout_ms: default_result_timeout_ms(),
            result_retention_seconds: default_result_retention_seconds(),
            result_retention_max_seconds: default_result_retention_max_seconds(),
            instance_heartbeat_stale_ms: default_instance_heartbeat_stale_ms(),
            daemon_addr: default_daemon_addr(),
            log_level: default_log_level(),
            script_files: ScriptFilePolicyConfig::default(),
        }
    }
}

/// Validate and read a host-side script file before it is sent to an Adobe host.
///
/// This is a path/trust guard and audit mechanism. In particular, `mode = "unsafe"` does
/// **not** sandbox the script; it still executes with the Adobe host's authority.
pub fn validate_and_read_script_file(
    cfg: &AppConfig,
    requested_path: &str,
    mode: &str,
) -> Result<ValidatedScriptFile> {
    if mode != "unsafe" && mode != "trusted" {
        anyhow::bail!("mode must be either 'unsafe' or 'trusted' for script file execution");
    }
    if cfg.script_files.max_bytes == 0 {
        anyhow::bail!("script_files.max_bytes must be greater than 0");
    }

    let path = Path::new(requested_path);
    if !path.is_absolute() {
        anyhow::bail!("script file path must be absolute: {requested_path}");
    }
    let canonical_path = fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize script file: {requested_path}"))?;
    let metadata = fs::metadata(&canonical_path).with_context(|| {
        format!(
            "failed to read script file metadata: {}",
            canonical_path.display()
        )
    })?;
    if !metadata.is_file() {
        anyhow::bail!(
            "script path is not a regular file: {}",
            canonical_path.display()
        );
    }

    validate_script_extension(&cfg.host_id, &canonical_path)?;
    if metadata.len() > cfg.script_files.max_bytes {
        anyhow::bail!(
            "script file is too large: {} bytes > {} bytes",
            metadata.len(),
            cfg.script_files.max_bytes
        );
    }

    let trusted_sha256 = match mode {
        "unsafe" => {
            validate_unsafe_script_root(cfg, &canonical_path)?;
            None
        }
        "trusted" => Some(trusted_script_expected_sha256(cfg, &canonical_path)?),
        _ => unreachable!("mode was validated above"),
    };

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(&canonical_path)
        .with_context(|| format!("failed to open script file: {}", canonical_path.display()))?
        .take(cfg.script_files.max_bytes + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read script file: {}", canonical_path.display()))?;
    if bytes.len() as u64 > cfg.script_files.max_bytes {
        anyhow::bail!(
            "script file is too large: {} bytes > {} bytes",
            bytes.len(),
            cfg.script_files.max_bytes
        );
    }
    let code = String::from_utf8(bytes).with_context(|| {
        format!(
            "script file is not valid UTF-8: {}",
            canonical_path.display()
        )
    })?;
    let source_sha256 = sha256_hex(code.as_bytes());
    if let Some(expected) = trusted_sha256 {
        if !expected.eq_ignore_ascii_case(&source_sha256) {
            anyhow::bail!(
                "trusted script SHA-256 mismatch for {}",
                canonical_path.display()
            );
        }
    }

    Ok(ValidatedScriptFile {
        audit: ScriptFileAudit {
            host_id: cfg.host_id.clone(),
            mode: mode.to_string(),
            source_path: canonical_path.display().to_string(),
            source_sha256,
            source_size_bytes: code.len() as u64,
        },
        code,
    })
}

fn validate_script_extension(host_id: &str, path: &Path) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| anyhow::anyhow!("script file must have a supported extension"))?;
    let allowed = match host_id {
        "aftereffects" | "illustrator" => &["jsx"][..],
        "premiere" | "photoshop" => &["js", "jsx"][..],
        "indesign" => &["idjs"][..],
        other => anyhow::bail!("unsupported hostId for script file policy: {other}"),
    };
    if !allowed.contains(&extension.as_str()) {
        anyhow::bail!(
            "unsupported script extension '.{}' for {} (allowed: {})",
            extension,
            host_id,
            allowed
                .iter()
                .map(|value| format!(".{value}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

fn validate_unsafe_script_root(cfg: &AppConfig, canonical_path: &Path) -> Result<()> {
    let mut invalid_roots = Vec::new();
    for root in &cfg.script_files.allowed_roots {
        if !root.is_absolute() {
            invalid_roots.push(format!("{} (not absolute)", root.display()));
            continue;
        }
        match fs::canonicalize(root) {
            Ok(canonical_root)
                if canonical_root.is_dir() && canonical_path.starts_with(&canonical_root) =>
            {
                return Ok(())
            }
            Ok(canonical_root) if !canonical_root.is_dir() => {
                invalid_roots.push(format!("{} (not a directory)", root.display()))
            }
            Ok(_) => {}
            Err(error) => invalid_roots.push(format!("{} ({error})", root.display())),
        }
    }
    let suffix = if invalid_roots.is_empty() {
        String::new()
    } else {
        format!(" Invalid configured roots: {}.", invalid_roots.join("; "))
    };
    anyhow::bail!(
        "unsafe script file is outside configured script_files.allowed_roots: {}.{}",
        canonical_path.display(),
        suffix
    )
}

fn trusted_script_expected_sha256(cfg: &AppConfig, canonical_path: &Path) -> Result<String> {
    for trusted in &cfg.script_files.trusted_scripts {
        if !trusted.path.is_absolute() {
            continue;
        }
        let Ok(trusted_path) = fs::canonicalize(&trusted.path) else {
            continue;
        };
        if trusted_path == canonical_path {
            if !is_sha256_hex(&trusted.sha256) {
                anyhow::bail!(
                    "trusted script has an invalid configured SHA-256 digest: {}",
                    trusted.path.display()
                );
            }
            return Ok(trusted.sha256.clone());
        }
    }
    anyhow::bail!(
        "trusted script is not present in script_files.trusted_scripts: {}",
        canonical_path.display()
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn default_host_id() -> String {
    AFTER_EFFECTS_HOST.id.to_string()
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

    pub fn load_for_host(config_path: Option<&Path>, host: HostSpec) -> Result<Self> {
        let mut cfg = Self::load(config_path)?;
        cfg.host_id = host.id.to_string();
        if config_path.is_none() {
            cfg.bridge = host.bridge_paths();
        }
        if config_path.is_none() || !config_declares_daemon_addr(config_path)? {
            cfg.daemon_addr = host.daemon_addr();
        }
        Ok(cfg)
    }
}

fn config_declares_daemon_addr(config_path: Option<&Path>) -> Result<bool> {
    let Some(path) = config_path else {
        return Ok(false);
    };
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    let value: toml::Value = toml::from_str(&raw).with_context(|| "failed to parse TOML config")?;
    Ok(value.get("daemon_addr").is_some())
}

pub fn default_bridge_root_dir() -> PathBuf {
    default_bridge_root_dir_named(AFTER_EFFECTS_HOST.bridge_root_name)
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
            description: "Run a validated local JSX file in After Effects and wait for a result",
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
        "list-compositions" => "Read the public `aftereffects://compositions` resource and list all compositions in the current After Effects project. The resource read is routed through the daemon broker. If custom inspection is needed, use only the public `run-jsx` tool with `mode: \"unsafe\"` and a clear description.".to_string(),
        "analyze-composition" => {
            let target = args
                .get("compositionName")
                .and_then(Value::as_str)
                .unwrap_or("Unknown");
            format!(
                "Analyze the composition named \"{target}\" in the current After Effects project. Use only the public `run-jsx` tool with `mode: \"unsafe\"`, a clear description, and JSX that returns structured JSON. Provide its duration, frame rate, resolution, and layers."
            )
        }
        "create-composition" => "Create a new composition with custom settings by using only the public `run-jsx` tool with `mode: \"unsafe\"` and a clear description. Ask for any missing name, width, height, duration, or frame-rate values before executing JSX.".to_string(),
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
                "Save a single-frame PNG preview using the public `save-frame-png` tool.\nComposition: {composition_name}\nTime: {time_seconds}\nOutput path: {output_path}\n`outputPath` is required. The tool waits through the daemon broker and returns its request result; use public `get-results` only if a retained result must be recovered."
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
                "Add a render queue item by using only the public `run-jsx` tool with `mode: \"unsafe\"` and a clear description. Write JSX that resolves the composition, adds it to `app.project.renderQueue`, applies the requested templates when present, sets the output file, and returns structured JSON.\nComposition: {composition_name}\nOutput path: {output_path}\nRender settings template: {render_template}\nOutput module template: {output_template}\nDo not start rendering automatically."
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
                "Clean up preview files using the public `cleanup-preview-folder` tool.\nFolder: {folder_path}\nExtension: {extension}\nPrefix: {prefix}\nMax age (seconds): {max_age}\nThe tool executes through the daemon broker and returns its request result."
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

1. Install the bridge runtime and Startup bootstrap with the installer command
2. Start the broker with `ae-mcp serve-daemon`; on Windows you can also use `ae-mcp autostart install` and `ae-mcp autostart start`
3. Open Adobe After Effects
4. Enable "Allow Scripts to Write Files and Access Network" and restart After Effects
5. Call `list-ae-instances`, then `run-bridge-test` for the shortest end-to-end check

Public tools (the exact `tools/list` contract):
- run-jsx
- run-jsx-file
- get-jsx-result
- list-ae-instances
- get-results
- get-help
- save-frame-png
- cleanup-preview-folder
- run-bridge-test

Best practices:
- Use list-ae-instances when multiple After Effects versions are open
- Specify targetInstanceId or targetVersion when more than one AE instance is active
- Prefer compId/layerId when available to avoid index drift
- Use get-jsx-result with requestId after a timeout
- save-frame-png is optimized for fast previews (single PNG only)
- Use run-jsx for render-queue and other host-specific operations that do not have a public intent tool
- run-jsx-file requires an absolute path. Use mode="unsafe" only inside configured allowed roots, or mode="trusted" for an exact configured path/SHA-256 entry.
- "unsafe" is explicit host-authority execution, not a sandbox.
- For save-frame-png, keep suppressDialogs at its default true to avoid blocking dialogs
- Ensure outputPath points to a writable location
- Public JSX execution has no interactive flag. Write non-interactive JSX, pass explicit paths in code/args, and avoid prompt-based project lifecycle operations.
- If a workflow deliberately needs user dialogs, treat it as an explicit unsafe run-jsx handoff rather than relying on a hidden compatibility-tool argument.

Compatibility boundary:
- Historical host-specific tool names and `run-script` are accepted only as hidden legacy dispatch entries and are not advertised by `tools/list`.
- A legacy call returns a deprecation notice naming the public replacement. New prompts and setup instructions never depend on those names.
- `run-script` remains hidden because its allowlist is useful for compatibility, but its historical asynchronous direct-file semantics do not match the synchronous daemon-backed public contract. The trusted path/SHA-256 policy applies to public run-jsx-file and does not change that legacy transport.
- MCP resources that query After Effects use the daemon broker. MCP prompts only return instructions; every operation named by those instructions uses a public daemon-backed tool or resource.

Bridge script allowlist (compatibility/diagnostics, not public MCP tools):
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

Public access:
- Use `run-jsx` with `mode: "unsafe"` to inspect or apply effects.
- `list-supported-effects` and `describe-effect` are deprecated compatibility dispatch names and are not public MCP tools.
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script_test_config(host: HostSpec, allowed_root: &Path) -> AppConfig {
        let mut cfg = AppConfig::load_for_host(None, host).unwrap();
        cfg.script_files.allowed_roots = vec![allowed_root.to_path_buf()];
        cfg.script_files.trusted_scripts.clear();
        cfg
    }

    #[test]
    fn default_config_has_expected_files() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.host_id, AFTER_EFFECTS_HOST.id);
        assert!(cfg.bridge.command_file.ends_with("ae_command.json"));
        assert!(cfg.bridge.result_file.ends_with("ae_mcp_result.json"));
        assert_eq!(cfg.poll_interval_ms, 250);
        assert!(!cfg.script_files.allowed_roots.is_empty());
    }

    #[test]
    fn unsafe_script_records_canonical_audit_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        fs::create_dir(&allowed).unwrap();
        let script = allowed.join("hello.jsx");
        fs::write(&script, "return 'hello';\n").unwrap();
        let cfg = script_test_config(AFTER_EFFECTS_HOST, &allowed);

        let validated =
            validate_and_read_script_file(&cfg, script.to_str().unwrap(), "unsafe").unwrap();
        assert_eq!(validated.code, "return 'hello';\n");
        assert_eq!(validated.audit.mode, "unsafe");
        assert_eq!(validated.audit.host_id, "aftereffects");
        assert_eq!(validated.audit.source_size_bytes, 16);
        assert_eq!(validated.audit.source_sha256.len(), 64);
        assert_eq!(
            PathBuf::from(validated.audit.source_path),
            fs::canonicalize(script).unwrap()
        );
    }

    #[test]
    fn relative_and_canonical_parent_bypass_paths_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        fs::create_dir(&allowed).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("escape.jsx"), "return 1;").unwrap();
        let cfg = script_test_config(AFTER_EFFECTS_HOST, &allowed);

        let relative = validate_and_read_script_file(&cfg, "escape.jsx", "unsafe")
            .unwrap_err()
            .to_string();
        assert!(relative.contains("must be absolute"));

        let bypass = allowed.join("..").join("outside").join("escape.jsx");
        let error = validate_and_read_script_file(&cfg, bypass.to_str().unwrap(), "unsafe")
            .unwrap_err()
            .to_string();
        assert!(error.contains("outside configured"));
    }

    #[test]
    fn host_specific_extension_and_utf8_rules_are_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let js = dir.path().join("script.js");
        let idjs = dir.path().join("script.idjs");
        let binary = dir.path().join("binary.jsx");
        fs::write(&js, "return 1;").unwrap();
        fs::write(&idjs, "return 1;").unwrap();
        fs::write(&binary, [0xff, 0xfe]).unwrap();

        let ae = script_test_config(AFTER_EFFECTS_HOST, dir.path());
        let error = validate_and_read_script_file(&ae, js.to_str().unwrap(), "unsafe")
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported script extension"));

        let premiere = script_test_config(PREMIERE_PRO_HOST, dir.path());
        assert!(validate_and_read_script_file(&premiere, js.to_str().unwrap(), "unsafe").is_ok());
        let indesign = script_test_config(INDESIGN_HOST, dir.path());
        assert!(validate_and_read_script_file(&indesign, idjs.to_str().unwrap(), "unsafe").is_ok());
        assert!(validate_and_read_script_file(&indesign, js.to_str().unwrap(), "unsafe").is_err());
        let error = validate_and_read_script_file(&ae, binary.to_str().unwrap(), "unsafe")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not valid UTF-8"));
    }

    #[test]
    fn script_file_size_limit_is_enforced_before_execution() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("large.jsx");
        fs::write(&script, "12345").unwrap();
        let mut cfg = script_test_config(AFTER_EFFECTS_HOST, dir.path());
        cfg.script_files.max_bytes = 4;

        let error = validate_and_read_script_file(&cfg, script.to_str().unwrap(), "unsafe")
            .unwrap_err()
            .to_string();
        assert!(error.contains("too large"));
    }

    #[test]
    fn trusted_scripts_require_exact_canonical_path_and_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bundled.jsx");
        fs::write(&script, "return 42;").unwrap();
        let mut cfg = script_test_config(AFTER_EFFECTS_HOST, dir.path());
        cfg.script_files.allowed_roots.clear();
        cfg.script_files.trusted_scripts = vec![TrustedScriptFile {
            path: script.clone(),
            sha256: sha256_hex(b"return 42;"),
        }];

        assert!(validate_and_read_script_file(&cfg, script.to_str().unwrap(), "trusted").is_ok());
        fs::write(&script, "return 43;").unwrap();
        let error = validate_and_read_script_file(&cfg, script.to_str().unwrap(), "trusted")
            .unwrap_err()
            .to_string();
        assert!(error.contains("SHA-256 mismatch"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_file_outside_allowed_root_is_rejected() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside.jsx");
        fs::create_dir(&allowed).unwrap();
        fs::write(&outside, "return 1;").unwrap();
        let link = allowed.join("link.jsx");
        symlink(&outside, &link).unwrap();
        let cfg = script_test_config(AFTER_EFFECTS_HOST, &allowed);

        let error = validate_and_read_script_file(&cfg, link.to_str().unwrap(), "unsafe")
            .unwrap_err()
            .to_string();
        assert!(error.contains("outside configured"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_symlink_or_junction_target_outside_allowed_root_is_rejected() {
        use std::os::windows::fs::symlink_dir;

        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        fs::create_dir(&allowed).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("escape.jsx"), "return 1;").unwrap();
        let link = allowed.join("link");
        if symlink_dir(&outside, &link).is_err() {
            // Creating a directory link needs Developer Mode or elevation on older Windows.
            return;
        }
        let cfg = script_test_config(AFTER_EFFECTS_HOST, &allowed);
        let linked_script = link.join("escape.jsx");
        let error = validate_and_read_script_file(&cfg, linked_script.to_str().unwrap(), "unsafe")
            .unwrap_err()
            .to_string();
        assert!(error.contains("outside configured"));
    }

    #[test]
    fn all_supported_hosts_derive_distinct_bridge_paths() {
        assert_eq!(HOST_SPECS.len(), 5);
        for host in HOST_SPECS {
            let paths = host.bridge_paths();
            assert!(paths.root_dir.ends_with(host.bridge_root_name));
            assert!(paths.command_file.ends_with(host.command_file_name));
            assert!(paths.result_file.ends_with(host.result_file_name));
            assert_eq!(host_spec_by_id(host.id), Some(*host));

            let cfg = AppConfig::load_for_host(None, *host).expect("host config");
            assert_eq!(cfg.host_id, host.id);
            assert_eq!(cfg.bridge, paths);
            assert_eq!(cfg.daemon_addr, host.daemon_addr());
        }
        let ports = HOST_SPECS
            .iter()
            .map(|host| host.daemon_port)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ports.len(), HOST_SPECS.len());
    }

    #[test]
    fn host_default_port_applies_when_config_omits_daemon_addr() {
        let path = std::env::temp_dir().join(format!(
            "adobe-mcp-config-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"
host_id = "ignored"
poll_interval_ms = 250
result_timeout_ms = 5000
result_retention_seconds = 3600
result_retention_max_seconds = 86400
instance_heartbeat_stale_ms = 10000
log_level = "info"

[bridge]
root_dir = "bridge"
command_file = "bridge/command.json"
result_file = "bridge/result.json"
"#,
        )
        .unwrap();
        let cfg = AppConfig::load_for_host(Some(&path), PREMIERE_PRO_HOST).unwrap();
        assert_eq!(cfg.daemon_addr, PREMIERE_PRO_HOST.daemon_addr());
        assert!(!cfg.script_files.allowed_roots.is_empty());

        let raw = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("daemon_addr = \"127.0.0.1:49000\"\n{raw}")).unwrap();
        let overridden = AppConfig::load_for_host(Some(&path), PREMIERE_PRO_HOST).unwrap();
        let _ = fs::remove_file(path);
        assert_eq!(overridden.daemon_addr, "127.0.0.1:49000");
    }

    #[test]
    fn supported_hosts_have_stable_default_daemon_ports() {
        assert_eq!(AFTER_EFFECTS_HOST.daemon_addr(), "127.0.0.1:47655");
        assert_eq!(PREMIERE_PRO_HOST.daemon_addr(), "127.0.0.1:47656");
        assert_eq!(PHOTOSHOP_HOST.daemon_addr(), "127.0.0.1:47657");
        assert_eq!(ILLUSTRATOR_HOST.daemon_addr(), "127.0.0.1:47658");
        assert_eq!(INDESIGN_HOST.daemon_addr(), "127.0.0.1:47659");
    }

    #[test]
    fn bridge_heartbeats_declare_protocol_v1_fields() {
        let sources = [
            include_str!("../../../src/scripts/mcp-bridge-auto.jsx"),
            include_str!("../../../src/premiere/uxp/mcp-bridge-premiere/js/main.js"),
            include_str!("../../../src/premiere/cep/mcp-bridge-premiere/jsx/bridge.jsx"),
            include_str!("../../../src/photoshop/uxp/mcp-bridge-photoshop/js/main.js"),
            include_str!("../../../src/illustrator/cep/mcp-bridge-illustrator/jsx/bridge.jsx"),
            include_str!("../../../src/indesign/uxp/mcp-bridge-indesign.idjs"),
        ];
        for source in sources {
            for field in [
                "protocolVersion",
                "hostId",
                "bridgeRuntime",
                "capabilities",
                "updatedAt",
            ] {
                assert!(
                    source.contains(field),
                    "heartbeat source is missing {field}"
                );
            }
        }
    }

    #[test]
    fn script_allowlist_contains_core_entries() {
        assert!(is_allowed_script("listCompositions"));
        assert!(is_allowed_script("applyEffectTemplate"));
        assert!(!is_allowed_script("unknownScript"));
    }

    #[test]
    fn public_tool_names_match_advertised_specs() {
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
        assert_eq!(names, PUBLIC_TOOL_NAMES);
        assert!(!names.contains(&"run-script"));
        assert!(!names.contains(&"render-queue-add"));
    }

    #[test]
    fn every_prompt_uses_only_public_execution_paths() {
        let cases = [
            ("list-compositions", json!({})),
            (
                "analyze-composition",
                json!({ "compositionName": "Comp 1" }),
            ),
            ("create-composition", json!({})),
            (
                "save-preview-png",
                json!({ "outputPath": "C:/preview.png" }),
            ),
            (
                "render-queue-setup",
                json!({ "outputPath": "C:/render.mov" }),
            ),
            (
                "cleanup-preview-folder",
                json!({ "folderPath": "C:/preview" }),
            ),
        ];

        let prompt_names = prompt_specs()
            .into_iter()
            .map(|prompt| prompt.name)
            .collect::<Vec<_>>();
        let covered_names = cases.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        assert_eq!(
            prompt_names, covered_names,
            "add new prompts to this contract test"
        );

        for (name, args) in cases {
            let message = prompt_messages(name, &args)
                .expect("known prompt")
                .to_string();
            assert!(
                message.contains("run-jsx")
                    || message.contains("save-frame-png")
                    || message.contains("cleanup-preview-folder"),
                "prompt {name} does not name a public execution path"
            );
            for legacy_name in LEGACY_TOOL_NAMES {
                assert!(
                    !message.contains(legacy_name),
                    "prompt {name} refers to legacy tool {legacy_name}"
                );
            }
        }
    }

    #[test]
    fn help_lists_every_public_tool_and_explains_legacy_boundary() {
        let help = general_help_text();
        for tool in PUBLIC_TOOL_NAMES {
            assert!(
                help.contains(&format!("- {tool}")),
                "help is missing {tool}"
            );
        }
        assert!(help.contains("hidden legacy dispatch"));
        assert!(help.contains("`run-script` remains hidden"));
        assert!(!help.contains("Use render-queue-start"));
    }

    #[test]
    fn setup_smoke_test_uses_only_public_tools() {
        let setup = include_str!("../../../docs/setup-codex-mcp.md");
        let smoke = setup
            .split("## 7. 動作確認（最短）")
            .nth(1)
            .and_then(|tail| tail.split("### 7.1").next())
            .expect("setup smoke-test section");

        assert!(smoke.contains("`list-ae-instances`"));
        assert!(smoke.contains("`run-bridge-test`"));
        for legacy_name in LEGACY_TOOL_NAMES {
            assert!(
                !smoke.contains(legacy_name),
                "setup smoke test refers to legacy tool {legacy_name}"
            );
        }
    }
}
