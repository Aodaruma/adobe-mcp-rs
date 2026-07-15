use super::{sha256_hex, AppConfig, HostSpec, ScriptFileAudit, ToolSpec};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_INLINE_SCRIPT_MAX_BYTES: u64 = 262_144;
pub const DEFAULT_STRUCTURED_VALUE_MAX_BYTES: u64 = 1_048_576;
pub const DEFAULT_SCRIPT_TIMEOUT_MAX_MS: u64 = 600_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ScriptContractConfig {
    pub max_inline_bytes: u64,
    pub max_input_bytes: u64,
    pub max_result_bytes: u64,
    pub max_timeout_ms: u64,
    pub block_destructive_enabled: bool,
    pub confirm_destructive_enabled: bool,
    /// Environment variable containing the external approver's shared HMAC secret.
    /// The server validates tokens but deliberately exposes no token-issuing tool.
    pub approval_hmac_secret_env: Option<String>,
}

impl Default for ScriptContractConfig {
    fn default() -> Self {
        Self {
            max_inline_bytes: DEFAULT_INLINE_SCRIPT_MAX_BYTES,
            max_input_bytes: DEFAULT_STRUCTURED_VALUE_MAX_BYTES,
            max_result_bytes: DEFAULT_STRUCTURED_VALUE_MAX_BYTES,
            max_timeout_ms: DEFAULT_SCRIPT_TIMEOUT_MAX_MS,
            block_destructive_enabled: false,
            confirm_destructive_enabled: false,
            approval_hmac_secret_env: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RiskPolicy {
    Raw,
    Analyze,
    BlockDestructive,
    ConfirmDestructive,
}

impl RiskPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Analyze => "analyze",
            Self::BlockDestructive => "block-destructive",
            Self::ConfirmDestructive => "confirm-destructive",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RiskFinding {
    pub level: String,
    pub id: String,
    pub offset: usize,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RiskReport {
    pub analyzed: bool,
    pub level: String,
    pub detected: Vec<RiskFinding>,
    pub warnings: Vec<String>,
}

impl RiskReport {
    pub fn is_destructive(&self) -> bool {
        self.detected.iter().any(|item| item.level == "destructive")
    }
}

#[derive(Debug, Clone)]
pub struct PreparedScript {
    pub code: String,
    pub input: Value,
    pub runtime: String,
    pub description: String,
    pub preflight_only: bool,
    pub timeout_ms: u64,
    pub retention_seconds: u64,
    pub audit: ScriptFileAudit,
}

/// Produce a deliberately conservative, best-effort report. This scanner is an
/// accident-prevention aid, not a parser, sandbox, or security boundary.
pub fn scan_script_risk(source: &str) -> RiskReport {
    const PATTERNS: &[(&str, &str, &str)] = &[
        ("destructive", "script.delete", "delete "),
        ("destructive", "object.remove", ".remove("),
        ("destructive", "filesystem.unlink", "unlink("),
        ("destructive", "filesystem.rm", "rm("),
        ("destructive", "uxp.delete-entry", "deleteentry("),
        ("destructive", "photoshop.delete", "\"_obj\":\"delete\""),
        ("destructive", "application.quit", ".quit("),
        ("persistent-write", "document.save", ".save("),
        ("persistent-write", "document.export", ".export"),
        ("persistent-write", "render", ".render("),
        ("external", "network.fetch", "fetch("),
        ("external", "network.xhr", "xmlhttprequest"),
        ("external", "shell", "callsystem("),
        ("opaque", "dynamic.eval", "eval("),
        ("opaque", "dynamic.function", "new function"),
        ("opaque", "dynamic.do-script", ".doscript("),
        ("opaque", "dynamic.command-id", "executecommand("),
    ];
    let normalized: String = source
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    let mut detected = Vec::new();
    for (level, id, pattern) in PATTERNS {
        let needle: String = pattern
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect();
        let mut remaining = normalized.as_str();
        let mut base = 0;
        while let Some(offset) = remaining.find(&needle) {
            detected.push(RiskFinding {
                level: (*level).to_string(),
                id: (*id).to_string(),
                offset: base + offset,
                evidence: (*pattern).trim().to_string(),
            });
            let next = offset + needle.len();
            base += next;
            remaining = &remaining[next..];
        }
    }
    let level = [
        "destructive",
        "external",
        "opaque",
        "persistent-write",
        "reversible-write",
    ]
    .into_iter()
    .find(|level| detected.iter().any(|item| item.level == *level))
    .unwrap_or("read")
    .to_string();
    RiskReport {
        analyzed: true,
        level,
        detected,
        warnings: vec![
            "Best-effort lexical analysis can produce false positives and false negatives; it is not a sandbox or security boundary.".to_string(),
            "Dynamic property access, aliases, generated code, command IDs, and indirect modules may evade detection.".to_string(),
        ],
    }
}

pub fn prepare_script(
    cfg: &AppConfig,
    host: HostSpec,
    args: &Value,
    code: String,
    mode: &str,
    source_path: Option<String>,
    source_limit: u64,
) -> Result<PreparedScript> {
    if mode != "unsafe" && mode != "trusted" {
        bail!("mode must be 'unsafe' or 'trusted'");
    }
    if code.is_empty() {
        bail!("'code' is required and must be a non-empty string");
    }
    if code.len() as u64 > source_limit {
        bail!(
            "script source is too large: {} bytes > {} bytes",
            code.len(),
            source_limit
        );
    }
    let description = required_string(args, "description")?.to_string();
    let input = args
        .get("input")
        .or_else(|| args.get("args"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    validate_json_size(
        "structured input",
        &input,
        cfg.script_contract.max_input_bytes,
    )?;
    let runtime = match args.get("runtime") {
        None => host.bridge_runtime,
        Some(Value::String(value)) if value == "auto" => host.bridge_runtime,
        Some(Value::String(value)) if value == host.bridge_runtime => host.bridge_runtime,
        Some(Value::String(value)) => bail!(
            "unsupported runtime '{value}' for {}; expected 'auto' or '{}'",
            host.id,
            host.bridge_runtime
        ),
        Some(_) => bail!("runtime must be a string"),
    }
    .to_string();
    let risk_policy: RiskPolicy = match args.get("riskPolicy").and_then(Value::as_str) {
        None | Some("analyze") => RiskPolicy::Analyze,
        Some("raw") => RiskPolicy::Raw,
        Some("block-destructive") => RiskPolicy::BlockDestructive,
        Some("confirm-destructive") => RiskPolicy::ConfirmDestructive,
        Some(other) => bail!("unsupported riskPolicy: {other}"),
    };
    let risk = if risk_policy == RiskPolicy::Raw {
        RiskReport {
            analyzed: false,
            level: "opaque".to_string(),
            detected: Vec::new(),
            warnings: vec![
                "Static analysis was skipped by riskPolicy=raw; execution is not sandboxed."
                    .to_string(),
            ],
        }
    } else {
        scan_script_risk(&code)
    };
    enforce_risk_policy(cfg, host, args, &code, risk_policy, &risk)?;
    let timeout_ms = validated_timeout_ms(cfg, args)?;
    let retention_seconds = validated_retention_seconds(cfg, args)?;
    let source_sha256 = sha256_hex(code.as_bytes());
    let declared_effects = match args.get("declaredEffects") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                let effect = value
                    .as_str()
                    .ok_or_else(|| anyhow!("declaredEffects entries must be strings"))?;
                if !matches!(
                    effect,
                    "read"
                        | "reversible-write"
                        | "persistent-write"
                        | "destructive"
                        | "external"
                        | "opaque"
                ) {
                    bail!("unsupported declared effect: {effect}");
                }
                Ok(effect.to_string())
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => bail!("declaredEffects must be an array"),
    };
    Ok(PreparedScript {
        code,
        input,
        runtime: runtime.clone(),
        description,
        preflight_only: args
            .get("preflightOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        timeout_ms,
        retention_seconds,
        audit: ScriptFileAudit {
            host_id: host.id.to_string(),
            mode: mode.to_string(),
            runtime,
            risk_policy: risk_policy.as_str().to_string(),
            risk: Some(risk),
            declared_effects,
            source_path: source_path.unwrap_or_default(),
            source_sha256,
            source_size_bytes: 0, // overwritten below
        },
    }
    .with_source_size())
}

impl PreparedScript {
    fn with_source_size(mut self) -> Self {
        self.audit.source_size_bytes = self.code.len() as u64;
        self
    }

    pub fn preflight_envelope(&self, host: HostSpec) -> Value {
        json!({
            "requestId": Value::Null,
            "hostId": host.id,
            "instanceId": Value::Null,
            "runtime": self.runtime,
            "state": "preflight",
            "risk": self.audit.risk,
            "result": Value::Null,
            "audit": self.audit,
            "executed": false
        })
    }
}

fn enforce_risk_policy(
    cfg: &AppConfig,
    host: HostSpec,
    args: &Value,
    code: &str,
    policy: RiskPolicy,
    risk: &RiskReport,
) -> Result<()> {
    if !risk.is_destructive() || args.get("preflightOnly").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    match policy {
        RiskPolicy::Raw | RiskPolicy::Analyze => Ok(()),
        RiskPolicy::BlockDestructive => {
            if !cfg.script_contract.block_destructive_enabled {
                bail!("riskPolicy=block-destructive is disabled by deployment policy");
            }
            bail!("best-effort scanner detected destructive code; execution was blocked")
        }
        RiskPolicy::ConfirmDestructive => {
            if !cfg.script_contract.confirm_destructive_enabled {
                bail!("riskPolicy=confirm-destructive is disabled by deployment policy");
            }
            let instance = required_string(args, "targetInstanceId")?;
            let token = required_string(args, "confirmationToken")?;
            verify_and_consume_approval_token(
                cfg,
                token,
                host.id,
                instance,
                &sha256_hex(code.as_bytes()),
            )
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ApprovalClaims {
    version: u32,
    host_id: String,
    target_instance_id: String,
    source_sha256: String,
    risk: String,
    expires_at_unix: u64,
    nonce: String,
}

fn verify_and_consume_approval_token(
    cfg: &AppConfig,
    token: &str,
    host_id: &str,
    target_instance_id: &str,
    source_sha256: &str,
) -> Result<()> {
    let env_name = cfg
        .script_contract
        .approval_hmac_secret_env
        .as_deref()
        .ok_or_else(|| anyhow!("approval_hmac_secret_env is not configured"))?;
    let secret = std::env::var(env_name).with_context(|| {
        format!("approval secret environment variable {env_name} is unavailable")
    })?;
    if secret.len() < 32 {
        bail!("approval HMAC secret must contain at least 32 bytes");
    }
    let mut parts = token.split('.');
    if parts.next() != Some("v1") {
        bail!("unsupported confirmation token version");
    }
    let payload_b64 = parts
        .next()
        .ok_or_else(|| anyhow!("invalid confirmation token"))?;
    let signature = hex_decode(
        parts
            .next()
            .ok_or_else(|| anyhow!("invalid confirmation token"))?,
    )?;
    if parts.next().is_some() {
        bail!("invalid confirmation token");
    }
    let payload = base64url_decode(payload_b64)?;
    let expected = hmac_sha256(secret.as_bytes(), format!("v1.{payload_b64}").as_bytes());
    if !constant_time_eq(&signature, &expected) {
        bail!("confirmation token signature is invalid");
    }
    let claims: ApprovalClaims = serde_json::from_slice(&payload)
        .with_context(|| "confirmation token claims are invalid")?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if claims.version != 1
        || claims.host_id != host_id
        || claims.target_instance_id != target_instance_id
        || !claims.source_sha256.eq_ignore_ascii_case(source_sha256)
        || claims.risk != "destructive"
        || claims.expires_at_unix < now
        || claims.expires_at_unix > now + 600
        || claims.nonce.len() < 16
    {
        bail!("confirmation token claims do not match source, host instance, risk, or TTL");
    }
    let replay_dir = cfg.bridge.root_dir.join("approval-replay");
    fs::create_dir_all(&replay_dir)?;
    let marker = replay_dir.join(sha256_hex(token.as_bytes()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .with_context(|| "confirmation token was already consumed")?;
    writeln!(file, "{}", claims.expires_at_unix)?;
    Ok(())
}

pub fn validated_timeout_ms(cfg: &AppConfig, args: &Value) -> Result<u64> {
    let value = args
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(cfg.result_timeout_ms);
    if value == 0 || value > cfg.script_contract.max_timeout_ms {
        bail!(
            "timeoutMs must be between 1 and {}",
            cfg.script_contract.max_timeout_ms
        );
    }
    Ok(value)
}

pub fn validated_retention_seconds(cfg: &AppConfig, args: &Value) -> Result<u64> {
    let value = args
        .get("resultRetentionSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(cfg.result_retention_seconds);
    if value == 0 || value > cfg.result_retention_max_seconds {
        bail!(
            "resultRetentionSeconds must be between 1 and {}",
            cfg.result_retention_max_seconds
        );
    }
    Ok(value)
}

pub fn validate_json_size(label: &str, value: &Value, max: u64) -> Result<()> {
    let size = serde_json::to_vec(value)?.len() as u64;
    if size > max {
        bail!("{label} is too large: {size} bytes > {max} bytes");
    }
    Ok(())
}

pub fn canonical_script_tool_specs(host: HostSpec) -> Vec<ToolSpec> {
    let common = json!({
        "code": { "type": "string", "minLength": 1, "maxLength": DEFAULT_INLINE_SCRIPT_MAX_BYTES },
        "runtime": { "type": "string", "enum": ["auto", host.bridge_runtime] },
        "input": {},
        "mode": { "type": "string", "enum": ["unsafe"] },
        "description": { "type": "string", "minLength": 1 },
        "declaredEffects": { "type": "array", "items": { "type": "string", "enum": ["read", "reversible-write", "persistent-write", "destructive", "external", "opaque"] } },
        "riskPolicy": { "type": "string", "enum": ["raw", "analyze", "block-destructive", "confirm-destructive"], "default": "analyze" },
        "preflightOnly": { "type": "boolean", "default": false },
        "targetInstanceId": { "type": "string", "minLength": 1 },
        "targetVersion": { "type": "string", "minLength": 1 },
        "timeoutMs": { "type": "integer", "minimum": 1, "maximum": DEFAULT_SCRIPT_TIMEOUT_MAX_MS },
        "resultRetentionSeconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
        "confirmationToken": { "type": "string", "minLength": 1 }
    });
    let mut file = common.clone();
    let object = file.as_object_mut().expect("properties object");
    object.remove("code");
    object.insert(
        "path".to_string(),
        json!({ "type": "string", "minLength": 1 }),
    );
    object.insert(
        "mode".to_string(),
        json!({ "type": "string", "enum": ["unsafe", "trusted"] }),
    );
    vec![
        ToolSpec {
            name: "run-script",
            description: "Run raw host script with structured input and best-effort risk reporting; unsafe is not sandboxed",
            input_schema: json!({ "type": "object", "properties": common, "required": ["code", "mode", "description"] }),
        },
        ToolSpec {
            name: "run-script-file",
            description: if host.id == "indesign" {
                "Run a validated synchronous UXP function body; this is not a general top-level .idjs file, and path/hash/risk checks are not a sandbox"
            } else {
                "Run a validated host script file; path/hash checks and risk analysis are not a sandbox"
            },
            input_schema: json!({ "type": "object", "properties": file, "required": ["path", "mode", "description"] }),
        },
        ToolSpec {
            name: "get-script-result",
            description: "Get a retained script result by requestId",
            input_schema: json!({ "type": "object", "properties": { "requestId": { "type": "string", "minLength": 1 } }, "required": ["requestId"] }),
        },
        ToolSpec {
            name: "get-capabilities",
            description: "Get the common script runtime, permission, payload, guard, timeout, cancellation, and retention contract",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "cancel-script-request",
            description: "Request cancellation; queued work can stop, but running host code is cooperative and may still complete",
            input_schema: json!({ "type": "object", "properties": { "requestId": { "type": "string", "minLength": 1 } }, "required": ["requestId"] }),
        },
    ]
}

/// Merge the canonical surface without removing a previously public allowlist
/// `run-script` payload. Runtime dispatch disambiguates `code` and `script`.
pub fn merge_canonical_script_tools(tools: &mut Vec<ToolSpec>, host: HostSpec) {
    for mut canonical in canonical_script_tool_specs(host) {
        if let Some(index) = tools.iter().position(|tool| tool.name == canonical.name) {
            let legacy = &tools[index];
            if canonical.name == "run-script"
                && legacy.input_schema["properties"].get("script").is_some()
            {
                if let (Some(properties), Some(legacy_properties)) = (
                    canonical.input_schema["properties"].as_object_mut(),
                    legacy.input_schema["properties"].as_object(),
                ) {
                    for (name, schema) in legacy_properties {
                        properties
                            .entry(name.clone())
                            .or_insert_with(|| schema.clone());
                    }
                }
                canonical.input_schema["required"] = json!([]);
                canonical.input_schema["anyOf"] = json!([
                    { "required": ["code", "mode", "description"] },
                    { "required": ["script"] }
                ]);
                canonical.description = "Run raw host code (canonical), or accept the legacy allowlisted template payload with a script field during migration";
            }
            tools[index] = canonical;
        } else {
            tools.push(canonical);
        }
    }
}

pub fn capabilities_value(cfg: &AppConfig, host: HostSpec, instances: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "hostId": host.id,
        "hostDisplayName": host.display_name,
        "capabilities": ["script.execute.inline", "script.execute.file", "script.input.structured", "script.result.retained", "script.guard.preflight", "host.instances"],
        "runtime": { "default": host.bridge_runtime, "allowed": [host.bridge_runtime] },
        "tools": {
            "canonical": ["run-script", "run-script-file", "get-script-result", "get-capabilities", "cancel-script-request"],
            "compatibilityAliases": if host.id == "indesign" { json!([]) } else { json!(["run-jsx", "run-jsx-file", "get-jsx-result"]) }
        },
        "bridge": { "runtime": host.bridge_runtime, "instances": instances },
        "permissions": { "executesWithHostAuthority": true, "sandboxed": false, "setupHint": host.bridge_setup_hint },
        "payload": {
            "inlineSourceMaxBytes": cfg.script_contract.max_inline_bytes,
            "fileSourceMaxBytes": cfg.script_files.max_bytes,
            "structuredInputMaxBytes": cfg.script_contract.max_input_bytes,
            "jsonResultMaxBytes": cfg.script_contract.max_result_bytes,
            "oversizedResult": "return an artifact path, size, SHA-256, and MIME type instead of embedding binary data"
        },
        "guard": {
            "defaultRiskPolicy": "analyze",
            "supportedRiskPolicies": ["raw", "analyze", "block-destructive", "confirm-destructive"],
            "bestEffortOnly": true,
            "securityBoundary": false,
            "blockDestructiveEnabled": cfg.script_contract.block_destructive_enabled,
            "confirmDestructiveEnabled": cfg.script_contract.confirm_destructive_enabled,
            "externalApproval": { "format": "v1.<base64url-json-claims>.<hex-hmac-sha256>", "maxTtlSeconds": 600, "singleUse": true, "boundTo": ["sourceSha256", "hostId", "targetInstanceId", "risk", "expiry", "nonce"] }
        },
        "execution": {
            "timeoutMaxMs": cfg.script_contract.max_timeout_ms,
            "timeoutStopsHostCode": false,
            "cancellation": "queued-or-cooperative",
            "resultRetentionDefaultSeconds": cfg.result_retention_seconds,
            "resultRetentionMaxSeconds": cfg.result_retention_max_seconds,
            "auditLog": "request registry retains source identity, risk report, instance, state, and expiry; source text is not logged"
        }
    })
}

fn required_string<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("'{name}' is required and must be a non-empty string"))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36u8; 64];
    let mut outer_pad = [0x5cu8; 64];
    for index in 0..64 {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("confirmation token signature must be 64 hex characters");
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(Into::into))
        .collect()
}

fn base64url_decode(value: &str) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => bail!("invalid base64url confirmation token payload"),
        } as u32;
        accumulator = (accumulator << 6) | digit;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base64url_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut output = String::new();
        let mut accumulator = 0u32;
        let mut bits = 0u8;
        for byte in bytes {
            accumulator = (accumulator << 8) | *byte as u32;
            bits += 8;
            while bits >= 6 {
                bits -= 6;
                output.push(TABLE[((accumulator >> bits) & 63) as usize] as char);
            }
        }
        if bits > 0 {
            output.push(TABLE[((accumulator << (6 - bits)) & 63) as usize] as char);
        }
        output
    }

    fn approval_token(secret: &str, claims: &ApprovalClaims) -> String {
        let payload = serde_json::to_vec(claims).unwrap();
        let encoded = base64url_encode(&payload);
        let signature = hmac_sha256(secret.as_bytes(), format!("v1.{encoded}").as_bytes());
        format!(
            "v1.{encoded}.{}",
            signature
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    fn approval_config() -> (AppConfig, tempfile::TempDir, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.bridge.root_dir = temp.path().to_path_buf();
        cfg.script_contract.confirm_destructive_enabled = true;
        let env_name = format!("ADOBE_MCP_TEST_APPROVAL_{}", std::process::id());
        let secret = "0123456789abcdef0123456789abcdef".to_string();
        std::env::set_var(&env_name, &secret);
        cfg.script_contract.approval_hmac_secret_env = Some(env_name.clone());
        (cfg, temp, env_name, secret)
    }

    #[test]
    fn scanner_reports_destructive_and_opaque_patterns_without_claiming_safety() {
        let report = scan_script_risk("app.project.item(1).remove(); eval(code);");
        assert_eq!(report.level, "destructive");
        assert!(report.is_destructive());
        assert!(report.detected.iter().any(|item| item.level == "opaque"));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("not a sandbox")));
    }

    #[test]
    fn canonical_schema_has_common_guard_and_recovery_fields() {
        let tools = canonical_script_tool_specs(super::super::AFTER_EFFECTS_HOST);
        let inline = tools.iter().find(|tool| tool.name == "run-script").unwrap();
        assert_eq!(
            inline.input_schema["properties"]["riskPolicy"]["default"],
            "analyze"
        );
        assert!(tools.iter().any(|tool| tool.name == "get-capabilities"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "cancel-script-request"));
    }

    #[test]
    fn raw_policy_is_reported_as_unanalyzed() {
        let mut cfg = AppConfig::default();
        cfg.host_id = "aftereffects".to_string();
        let prepared = prepare_script(
            &cfg,
            super::super::AFTER_EFFECTS_HOST,
            &json!({"mode":"unsafe","description":"test","riskPolicy":"raw"}),
            "app.project.item(1).remove();".to_string(),
            "unsafe",
            None,
            cfg.script_contract.max_inline_bytes,
        )
        .unwrap();
        assert!(!prepared.audit.risk.unwrap().analyzed);
    }

    #[test]
    fn runtime_auto_is_normalized_and_unknown_contract_values_are_rejected() {
        let cfg = AppConfig::default();
        let prepared = prepare_script(
            &cfg,
            super::super::AFTER_EFFECTS_HOST,
            &json!({
                "mode": "unsafe",
                "description": "runtime",
                "runtime": "auto",
                "declaredEffects": ["read", "reversible-write"]
            }),
            "return 1;".to_string(),
            "unsafe",
            None,
            cfg.script_contract.max_inline_bytes,
        )
        .unwrap();
        assert_eq!(prepared.runtime, "extendscript-startup");
        assert_eq!(prepared.audit.runtime, "extendscript-startup");
        assert_eq!(
            prepared.audit.declared_effects,
            vec!["read", "reversible-write"]
        );

        for args in [
            json!({ "mode": "unsafe", "description": "runtime", "runtime": "uxp" }),
            json!({ "mode": "unsafe", "description": "effects", "declaredEffects": ["unknown"] }),
        ] {
            assert!(prepare_script(
                &cfg,
                super::super::AFTER_EFFECTS_HOST,
                &args,
                "return 1;".to_string(),
                "unsafe",
                None,
                cfg.script_contract.max_inline_bytes,
            )
            .is_err());
        }
    }

    #[test]
    fn confirmation_token_validates_binding_expiry_integrity_and_replay() {
        let (cfg, _temp, env_name, secret) = approval_config();
        let source = "app.project.item(1).remove();";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let base = ApprovalClaims {
            version: 1,
            host_id: "aftereffects".to_string(),
            target_instance_id: "ae-1".to_string(),
            source_sha256: sha256_hex(source.as_bytes()),
            risk: "destructive".to_string(),
            expires_at_unix: now + 60,
            nonce: "nonce-0000000001".to_string(),
        };
        let token = approval_token(&secret, &base);
        verify_and_consume_approval_token(
            &cfg,
            &token,
            "aftereffects",
            "ae-1",
            &base.source_sha256,
        )
        .unwrap();
        assert!(verify_and_consume_approval_token(
            &cfg,
            &token,
            "aftereffects",
            "ae-1",
            &base.source_sha256
        )
        .unwrap_err()
        .to_string()
        .contains("already consumed"));

        let mut tampered = approval_token(
            &secret,
            &ApprovalClaims {
                nonce: "nonce-0000000002".to_string(),
                ..base.clone()
            },
        );
        tampered.pop();
        tampered.push('0');
        assert!(verify_and_consume_approval_token(
            &cfg,
            &tampered,
            "aftereffects",
            "ae-1",
            &base.source_sha256
        )
        .is_err());

        let expired = approval_token(
            &secret,
            &ApprovalClaims {
                expires_at_unix: now - 1,
                nonce: "nonce-0000000003".to_string(),
                ..base.clone()
            },
        );
        assert!(verify_and_consume_approval_token(
            &cfg,
            &expired,
            "aftereffects",
            "ae-1",
            &base.source_sha256
        )
        .is_err());

        let source_mismatch = approval_token(
            &secret,
            &ApprovalClaims {
                nonce: "nonce-0000000004".to_string(),
                ..base.clone()
            },
        );
        assert!(verify_and_consume_approval_token(
            &cfg,
            &source_mismatch,
            "aftereffects",
            "ae-1",
            &"0".repeat(64)
        )
        .is_err());
        assert!(verify_and_consume_approval_token(
            &cfg,
            &source_mismatch,
            "aftereffects",
            "ae-2",
            &base.source_sha256
        )
        .is_err());
        std::env::remove_var(env_name);
    }

    #[test]
    fn common_payload_and_timeout_limits_are_enforced() {
        let mut cfg = AppConfig::default();
        cfg.script_contract.max_inline_bytes = 8;
        cfg.script_contract.max_input_bytes = 16;
        cfg.script_contract.max_timeout_ms = 100;
        let base = json!({ "mode": "unsafe", "description": "limits" });
        assert!(prepare_script(
            &cfg,
            super::super::AFTER_EFFECTS_HOST,
            &base,
            "123456789".to_string(),
            "unsafe",
            None,
            cfg.script_contract.max_inline_bytes,
        )
        .unwrap_err()
        .to_string()
        .contains("source is too large"));
        let oversized_input = json!({
            "mode": "unsafe", "description": "limits", "input": { "value": "01234567890123456789" }
        });
        assert!(prepare_script(
            &cfg,
            super::super::AFTER_EFFECTS_HOST,
            &oversized_input,
            "return 1".to_string(),
            "unsafe",
            None,
            100,
        )
        .unwrap_err()
        .to_string()
        .contains("structured input is too large"));
        assert!(validated_timeout_ms(&cfg, &json!({ "timeoutMs": 101 }))
            .unwrap_err()
            .to_string()
            .contains("between 1 and 100"));
    }
}
