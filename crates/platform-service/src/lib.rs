use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutostartConfig {
    pub app_name: String,
    pub entry_name: String,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub pid_file: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartAction {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

pub fn run(action: ServiceAction, cfg: &ServiceConfig) -> Result<String> {
    match action {
        ServiceAction::Install => install(cfg),
        ServiceAction::Uninstall => uninstall(cfg),
        ServiceAction::Start => start(cfg),
        ServiceAction::Stop => stop(cfg),
        ServiceAction::Status => status(cfg),
    }
}

#[cfg(target_os = "windows")]
fn install(cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(windows_service_unsupported_message(
        "install",
        &cfg.service_name
    )))
}

#[cfg(target_os = "windows")]
fn uninstall(cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(windows_service_unsupported_message(
        "uninstall",
        &cfg.service_name
    )))
}

#[cfg(target_os = "windows")]
fn start(cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(windows_service_unsupported_message(
        "start",
        &cfg.service_name
    )))
}

#[cfg(target_os = "windows")]
fn stop(cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(windows_service_unsupported_message(
        "stop",
        &cfg.service_name
    )))
}

#[cfg(target_os = "windows")]
fn status(cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(windows_service_unsupported_message(
        "status",
        &cfg.service_name
    )))
}

#[cfg(target_os = "macos")]
fn install(cfg: &ServiceConfig) -> Result<String> {
    let plist_path = plist_path(&cfg.service_name)?;
    let plist_body = build_launchd_plist(cfg)?;
    std::fs::write(&plist_path, plist_body)
        .with_context(|| format!("failed to write plist: {}", plist_path.display()))?;

    let output = Command::new("launchctl")
        .args(["load", "-w", plist_path.to_string_lossy().as_ref()])
        .output()
        .with_context(|| "failed to execute 'launchctl load'")?;
    if !output.status.success() {
        return Err(anyhow!(render_output("launchctl load", output)));
    }
    Ok(format!(
        "launch agent installed at {}",
        plist_path.display()
    ))
}

#[cfg(target_os = "macos")]
fn uninstall(cfg: &ServiceConfig) -> Result<String> {
    let plist_path = plist_path(&cfg.service_name)?;
    let _ = Command::new("launchctl")
        .args(["unload", "-w", plist_path.to_string_lossy().as_ref()])
        .output();
    if plist_path.exists() {
        std::fs::remove_file(&plist_path)
            .with_context(|| format!("failed to remove plist: {}", plist_path.display()))?;
    }
    Ok("launch agent uninstalled".to_string())
}

#[cfg(target_os = "macos")]
fn start(cfg: &ServiceConfig) -> Result<String> {
    let output = Command::new("launchctl")
        .args(["start", &cfg.service_name])
        .output()
        .with_context(|| "failed to execute 'launchctl start'")?;
    if !output.status.success() {
        return Err(anyhow!(render_output("launchctl start", output)));
    }
    Ok("launch agent started".to_string())
}

#[cfg(target_os = "macos")]
fn stop(cfg: &ServiceConfig) -> Result<String> {
    let output = Command::new("launchctl")
        .args(["stop", &cfg.service_name])
        .output()
        .with_context(|| "failed to execute 'launchctl stop'")?;
    if !output.status.success() {
        return Err(anyhow!(render_output("launchctl stop", output)));
    }
    Ok("launch agent stopped".to_string())
}

#[cfg(target_os = "macos")]
fn status(cfg: &ServiceConfig) -> Result<String> {
    let output = Command::new("launchctl")
        .args(["list"])
        .output()
        .with_context(|| "failed to execute 'launchctl list'")?;
    if !output.status.success() {
        return Err(anyhow!(render_output("launchctl list", output)));
    }
    let all = String::from_utf8_lossy(&output.stdout).to_string();
    let lines = all
        .lines()
        .filter(|line| line.contains(&cfg.service_name))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        Ok(format!("{}: not listed", cfg.service_name))
    } else {
        Ok(lines.join("\n"))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn install(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(
        "service install is supported only on Windows/macOS"
    ))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn uninstall(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!(
        "service uninstall is supported only on Windows/macOS"
    ))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn start(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service start is supported only on Windows/macOS"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn stop(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service stop is supported only on Windows/macOS"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn status(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service status is supported only on Windows/macOS"))
}

pub fn run_autostart(action: AutostartAction, cfg: &AutostartConfig) -> Result<String> {
    match action {
        AutostartAction::Install => autostart_install(cfg),
        AutostartAction::Uninstall => autostart_uninstall(cfg),
        AutostartAction::Start => autostart_start(cfg),
        AutostartAction::Stop => autostart_stop(cfg),
        AutostartAction::Status => autostart_status(cfg),
    }
}

#[cfg(target_os = "windows")]
fn autostart_install(cfg: &AutostartConfig) -> Result<String> {
    let command_line = build_windows_command_line(&cfg.binary_path, &cfg.args);
    let output = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            &cfg.entry_name,
            "/t",
            "REG_SZ",
            "/d",
            &command_line,
            "/f",
        ])
        .output()
        .with_context(|| "failed to execute 'reg add'")?;
    if !output.status.success() {
        return Err(anyhow!(render_output("reg add", output)));
    }
    Ok(format!(
        "autostart installed for current user: {}",
        cfg.app_name
    ))
}

#[cfg(target_os = "windows")]
fn autostart_uninstall(cfg: &AutostartConfig) -> Result<String> {
    let output = Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            &cfg.entry_name,
            "/f",
        ])
        .output()
        .with_context(|| "failed to execute 'reg delete'")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    if !output.status.success() && !combined.contains("unable to find") {
        return Err(anyhow!(render_output("reg delete", output)));
    }

    Ok(format!(
        "autostart removed for current user: {}",
        cfg.app_name
    ))
}

#[cfg(target_os = "windows")]
fn autostart_start(cfg: &AutostartConfig) -> Result<String> {
    if let Some(pid) = running_pid_for_config(cfg)? {
        return Ok(format!("daemon already running (pid={pid})"));
    }

    spawn_detached(cfg)?;
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if let Some(pid) = running_pid_for_config(cfg)? {
            return Ok(format!("daemon started (pid={pid})"));
        }
    }

    Ok("daemon start requested; process did not publish a pid file yet".to_string())
}

#[cfg(target_os = "windows")]
fn autostart_stop(cfg: &AutostartConfig) -> Result<String> {
    let Some(pid) = running_pid_for_config(cfg)? else {
        cleanup_stale_pid_file(cfg)?;
        return Ok("daemon is not running".to_string());
    };

    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output()
        .with_context(|| "failed to execute 'taskkill'")?;
    if !output.status.success() {
        return Err(anyhow!(render_output("taskkill", output)));
    }

    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if running_pid_for_config(cfg)?.is_none() {
            cleanup_stale_pid_file(cfg)?;
            return Ok(format!("daemon stopped (pid={pid})"));
        }
    }

    cleanup_stale_pid_file(cfg)?;
    Ok(format!("stop requested for daemon pid={pid}"))
}

#[cfg(target_os = "windows")]
fn autostart_status(cfg: &AutostartConfig) -> Result<String> {
    let installed = is_autostart_installed(cfg)?;
    let running = running_pid_for_config(cfg)?;
    let install_state = if installed {
        "installed"
    } else {
        "not installed"
    };
    let running_state = match running {
        Some(pid) => format!("running (pid={pid})"),
        None => "not running".to_string(),
    };
    Ok(format!(
        "autostart: {install_state}\ndaemon: {running_state}\npid_file={}",
        cfg.pid_file.display()
    ))
}

#[cfg(not(target_os = "windows"))]
fn autostart_install(_cfg: &AutostartConfig) -> Result<String> {
    Err(anyhow!("autostart install is supported only on Windows"))
}

#[cfg(not(target_os = "windows"))]
fn autostart_uninstall(_cfg: &AutostartConfig) -> Result<String> {
    Err(anyhow!("autostart uninstall is supported only on Windows"))
}

#[cfg(not(target_os = "windows"))]
fn autostart_start(_cfg: &AutostartConfig) -> Result<String> {
    Err(anyhow!("autostart start is supported only on Windows"))
}

#[cfg(not(target_os = "windows"))]
fn autostart_stop(_cfg: &AutostartConfig) -> Result<String> {
    Err(anyhow!("autostart stop is supported only on Windows"))
}

#[cfg(not(target_os = "windows"))]
fn autostart_status(_cfg: &AutostartConfig) -> Result<String> {
    Err(anyhow!("autostart status is supported only on Windows"))
}

#[cfg(target_os = "macos")]
fn plist_path(service_name: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME").with_context(|| "HOME is not set")?;
    Ok(std::path::Path::new(&home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{service_name}.plist")))
}

#[cfg(target_os = "macos")]
fn build_launchd_plist(cfg: &ServiceConfig) -> Result<String> {
    let args = std::iter::once(cfg.binary_path.to_string_lossy().to_string())
        .chain(cfg.args.iter().cloned())
        .map(|x| format!("<string>{x}</string>"))
        .collect::<Vec<_>>()
        .join("\n        ");

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        {args}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#,
        label = cfg.service_name,
        args = args
    ))
}

fn render_output(command: &str, output: std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!(
        "{command} failed with status={}: stdout=`{}` stderr=`{}`",
        output.status,
        stdout.trim(),
        stderr.trim()
    )
}

#[cfg(target_os = "windows")]
fn windows_service_unsupported_message(action: &str, service_name: &str) -> String {
    format!(
        "service {action} is not supported on Windows for {service_name}. \
Use the `autostart` subcommand instead."
    )
}

#[cfg(target_os = "windows")]
fn build_windows_command_line(binary_path: &std::path::Path, args: &[String]) -> String {
    std::iter::once(binary_path.as_os_str().to_string_lossy().to_string())
        .chain(args.iter().cloned())
        .map(|part| quote_windows_arg(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn quote_windows_arg(arg: &str) -> String {
    if !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    format!("\"{}\"", arg.replace('"', "\\\""))
}

#[cfg(target_os = "windows")]
fn is_autostart_installed(cfg: &AutostartConfig) -> Result<bool> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            &cfg.entry_name,
        ])
        .output()
        .with_context(|| "failed to execute 'reg query'")?;
    Ok(output.status.success())
}

#[cfg(target_os = "windows")]
fn spawn_detached(cfg: &AutostartConfig) -> Result<()> {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new(&cfg.binary_path)
        .args(&cfg.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .with_context(|| "failed to spawn daemon process")?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn running_pid_for_config(cfg: &AutostartConfig) -> Result<Option<u32>> {
    let Some((pid, exe_path)) = read_pid_file(&cfg.pid_file)? else {
        return Ok(None);
    };

    if !paths_match(&exe_path, &cfg.binary_path) {
        return Ok(None);
    }

    if is_process_running(pid, &cfg.binary_path)? {
        Ok(Some(pid))
    } else {
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
fn cleanup_stale_pid_file(cfg: &AutostartConfig) -> Result<()> {
    if running_pid_for_config(cfg)?.is_none() && cfg.pid_file.exists() {
        let _ = std::fs::remove_file(&cfg.pid_file);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn read_pid_file(path: &std::path::Path) -> Result<Option<(u32, PathBuf)>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read pid file: {}", path.display()))?;
    let mut lines = raw.lines();
    let Some(pid_line) = lines.next() else {
        return Ok(None);
    };
    let Some(exe_line) = lines.next() else {
        return Ok(None);
    };

    let pid = pid_line
        .trim()
        .parse::<u32>()
        .with_context(|| format!("invalid pid file contents: {}", path.display()))?;
    Ok(Some((pid, PathBuf::from(exe_line.trim()))))
}

#[cfg(target_os = "windows")]
fn paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(target_os = "windows")]
fn is_process_running(pid: u32, expected_path: &std::path::Path) -> Result<bool> {
    let expected = expected_path.to_string_lossy().replace('\'', "''");
    let command = format!(
        "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; \
if ($null -eq $p) {{ exit 1 }}; \
if ($p.Path -and $p.Path -ieq '{expected}') {{ exit 0 }} else {{ exit 2 }}"
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &command])
        .status()
        .with_context(|| "failed to execute process probe")?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_output_formats_status() {
        let output = std::process::Output {
            status: success_status(),
            stdout: b"ok".to_vec(),
            stderr: vec![],
        };
        let rendered = render_output("dummy", output);
        assert!(rendered.contains("dummy failed"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn build_windows_command_line_quotes_spaces() {
        let command = build_windows_command_line(
            std::path::Path::new(r"C:\Program Files\AfterEffectsMcp\ae-mcp.exe"),
            &[
                r"--config".to_string(),
                r"C:\Users\foo bar\ae-mcp.toml".to_string(),
            ],
        );
        assert!(command.contains(r#""C:\Program Files\AfterEffectsMcp\ae-mcp.exe""#));
        assert!(command.contains(r#""C:\Users\foo bar\ae-mcp.toml""#));
    }

    #[cfg(target_os = "windows")]
    fn success_status() -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(not(target_os = "windows"))]
    fn success_status() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
}
