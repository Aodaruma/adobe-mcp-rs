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
    Err(anyhow!("service install is supported only on macOS"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn uninstall(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service uninstall is supported only on macOS"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn start(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service start is supported only on macOS"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn stop(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service stop is supported only on macOS"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn status(_cfg: &ServiceConfig) -> Result<String> {
    Err(anyhow!("service status is supported only on macOS"))
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
    if read_autostart_command(cfg)?.is_none() {
        return Ok(format!(
            "autostart already removed for current user: {}",
            cfg.app_name
        ));
    }

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

    if !output.status.success() {
        return Err(anyhow!(render_output("reg delete", output)));
    }

    Ok(format!(
        "autostart removed for current user: {}",
        cfg.app_name
    ))
}

#[cfg(target_os = "windows")]
fn autostart_start(cfg: &AutostartConfig) -> Result<String> {
    match daemon_state(cfg)? {
        DaemonState::NotRunning => cleanup_stale_pid_file(cfg)?,
        DaemonState::Running {
            pid,
            executable_matches: true,
            ..
        } => return Ok(format!("daemon already running (pid={pid})")),
        DaemonState::Running {
            pid,
            recorded_executable,
            executable_matches: false,
        } => {
            return Err(anyhow!(
                "daemon pid={pid} is still running from `{}`. Stop it before starting `{}`",
                recorded_executable.display(),
                cfg.binary_path.display()
            ));
        }
    }

    let mut child = spawn_detached(cfg)?;
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        match daemon_state(cfg)? {
            DaemonState::Running {
                pid,
                executable_matches: true,
                ..
            } => return Ok(format!("daemon started (pid={pid})")),
            DaemonState::Running {
                pid,
                recorded_executable,
                executable_matches: false,
            } => {
                terminate_spawned_child(&mut child);
                return Err(anyhow!(
                    "daemon pid={pid} published an unexpected executable path `{}`",
                    recorded_executable.display()
                ));
            }
            DaemonState::NotRunning => {}
        }
        if let Some(status) = child
            .try_wait()
            .with_context(|| "failed to query spawned daemon process")?
        {
            return Err(anyhow!(
                "daemon process exited before publishing a valid pid file (status={status})"
            ));
        }
    }

    terminate_spawned_child(&mut child);
    Err(anyhow!(
        "daemon process did not publish a valid pid file within 5 seconds; it may have failed to bind or exited during startup"
    ))
}

#[cfg(target_os = "windows")]
fn autostart_stop(cfg: &AutostartConfig) -> Result<String> {
    let pid = match daemon_state(cfg)? {
        DaemonState::NotRunning => {
            cleanup_stale_pid_file(cfg)?;
            return Ok("daemon is not running".to_string());
        }
        DaemonState::Running { pid, .. } => pid,
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
        if matches!(daemon_state(cfg)?, DaemonState::NotRunning) {
            cleanup_stale_pid_file(cfg)?;
            return Ok(format!("daemon stopped (pid={pid})"));
        }
    }

    Err(anyhow!(
        "daemon pid={pid} is still running after taskkill completed"
    ))
}

#[cfg(target_os = "windows")]
fn autostart_status(cfg: &AutostartConfig) -> Result<String> {
    let expected_command = build_windows_command_line(&cfg.binary_path, &cfg.args);
    let registered_command = read_autostart_command(cfg)?;
    let install_state = match registered_command {
        Some(ref command) if command == &expected_command => "installed".to_string(),
        Some(command) => format!(
            "outdated (registered=`{command}`, expected=`{expected_command}`); run `autostart install`"
        ),
        None => "not installed".to_string(),
    };
    let running_state = match daemon_state(cfg)? {
        DaemonState::NotRunning => "not running".to_string(),
        DaemonState::Running {
            pid,
            executable_matches: true,
            ..
        } => format!("running (pid={pid})"),
        DaemonState::Running {
            pid,
            recorded_executable,
            executable_matches: false,
        } => format!(
            "running from a different executable (pid={pid}, executable={})",
            recorded_executable.display()
        ),
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
fn read_autostart_command(cfg: &AutostartConfig) -> Result<Option<String>> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            &cfg.entry_name,
        ])
        .output()
        .with_context(|| "failed to execute 'reg query'")?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_reg_sz_value(&stdout))
}

#[cfg(target_os = "windows")]
fn parse_reg_sz_value(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("REG_SZ")?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(target_os = "windows")]
fn spawn_detached(cfg: &AutostartConfig) -> Result<std::process::Child> {
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
        .with_context(|| "failed to spawn daemon process")
}

#[cfg(target_os = "windows")]
fn terminate_spawned_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonState {
    NotRunning,
    Running {
        pid: u32,
        recorded_executable: PathBuf,
        executable_matches: bool,
    },
}

#[cfg(target_os = "windows")]
fn daemon_state(cfg: &AutostartConfig) -> Result<DaemonState> {
    let Some((pid, recorded_executable)) = read_pid_file(&cfg.pid_file)? else {
        return Ok(DaemonState::NotRunning);
    };

    if !is_process_running(pid, &recorded_executable)? {
        return Ok(DaemonState::NotRunning);
    }

    Ok(DaemonState::Running {
        pid,
        executable_matches: paths_match(&recorded_executable, &cfg.binary_path),
        recorded_executable,
    })
}

#[cfg(target_os = "windows")]
fn cleanup_stale_pid_file(cfg: &AutostartConfig) -> Result<()> {
    if matches!(daemon_state(cfg)?, DaemonState::NotRunning) && cfg.pid_file.exists() {
        std::fs::remove_file(&cfg.pid_file).with_context(|| {
            format!(
                "failed to remove stale pid file: {}",
                cfg.pid_file.display()
            )
        })?;
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
    let Some(pid) = lines
        .next()
        .and_then(|line| line.trim().parse::<u32>().ok())
    else {
        return Ok(None);
    };
    let Some(exe_line) = lines.next().map(str::trim).filter(|line| !line.is_empty()) else {
        return Ok(None);
    };

    Ok(Some((pid, PathBuf::from(exe_line))))
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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
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
    #[test]
    fn windows_service_actions_are_explicitly_unsupported() {
        let cfg = ServiceConfig {
            service_name: "AfterEffectsMcpDaemon".to_string(),
            display_name: "After Effects MCP Daemon".to_string(),
            description: "test".to_string(),
            binary_path: std::env::current_exe().expect("current executable"),
            args: vec!["serve-daemon".to_string()],
        };

        let error = run(ServiceAction::Status, &cfg).expect_err("Windows service must fail");
        let message = error.to_string();
        assert!(message.contains("not supported on Windows"));
        assert!(message.contains("autostart"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parses_reg_sz_command_value() {
        let output = concat!(
            "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\n",
            "    AfterEffectsMcp    REG_SZ    \"C:\\Program Files\\AfterEffectsMcp\\ae-mcp.exe\" serve-daemon\n"
        );
        assert_eq!(
            parse_reg_sz_value(output).as_deref(),
            Some(r#""C:\Program Files\AfterEffectsMcp\ae-mcp.exe" serve-daemon"#)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn stale_pid_file_is_removed() {
        let cfg = test_autostart_config(std::env::current_exe().expect("current executable"));
        std::fs::write(
            &cfg.pid_file,
            format!("{}\n{}\n", u32::MAX, cfg.binary_path.display()),
        )
        .expect("write stale pid file");

        assert_eq!(
            daemon_state(&cfg).expect("daemon state"),
            DaemonState::NotRunning
        );
        cleanup_stale_pid_file(&cfg).expect("remove stale pid file");
        assert!(!cfg.pid_file.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn moved_executable_is_reported_without_losing_live_pid() {
        let running_executable = std::env::current_exe().expect("current executable");
        let moved_executable = running_executable.with_file_name("moved-ae-mcp.exe");
        let cfg = test_autostart_config(moved_executable);
        std::fs::write(
            &cfg.pid_file,
            format!("{}\n{}\n", std::process::id(), running_executable.display()),
        )
        .expect("write pid file");

        let state = daemon_state(&cfg).expect("daemon state");
        assert!(matches!(
            state,
            DaemonState::Running {
                executable_matches: false,
                ..
            }
        ));
        cleanup_stale_pid_file(&cfg).expect("live pid file must be retained");
        assert!(cfg.pid_file.exists());
        let error = autostart_start(&cfg).expect_err("old executable prevents duplicate start");
        assert!(error.to_string().contains("Stop it before starting"));

        std::fs::remove_file(&cfg.pid_file).expect("remove test pid file");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn running_daemon_prevents_duplicate_start() {
        let executable = std::env::current_exe().expect("current executable");
        let cfg = test_autostart_config(executable.clone());
        std::fs::write(
            &cfg.pid_file,
            format!("{}\n{}\n", std::process::id(), executable.display()),
        )
        .expect("write pid file");

        let result = autostart_start(&cfg).expect("existing process is not an error");
        assert!(result.contains("already running"));

        std::fs::remove_file(&cfg.pid_file).expect("remove test pid file");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn malformed_pid_file_is_cleaned_before_startup_checks() {
        let cfg = test_autostart_config(std::env::current_exe().expect("current executable"));
        std::fs::write(&cfg.pid_file, "not-a-pid\n").expect("write malformed pid file");

        cleanup_stale_pid_file(&cfg).expect("remove malformed pid file");
        assert!(!cfg.pid_file.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn child_exit_before_pid_publication_is_an_error() {
        let command = std::env::var_os("ComSpec").expect("ComSpec");
        let mut cfg = test_autostart_config(PathBuf::from(command));
        cfg.args = vec!["/C".to_string(), "exit".to_string(), "7".to_string()];

        let error = autostart_start(&cfg).expect_err("early child exit must fail");
        assert!(error.to_string().contains("exited before publishing"));
    }

    #[cfg(target_os = "windows")]
    fn test_autostart_config(binary_path: PathBuf) -> AutostartConfig {
        let unique = format!(
            "adobe-mcp-platform-service-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
        AutostartConfig {
            app_name: "test".to_string(),
            entry_name: "test".to_string(),
            binary_path,
            args: vec!["serve-daemon".to_string()],
            pid_file: std::env::temp_dir().join(format!("{unique}.pid")),
        }
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
