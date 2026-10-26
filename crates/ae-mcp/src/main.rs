use anyhow::Result;
use clap::{Parser, Subcommand};
use mcp_core::{AppConfig, AFTER_EFFECTS_HOST};
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing::info;

mod mcp_stdio;

#[derive(Debug, Parser)]
#[command(name = "ae-mcp", version, about = "After Effects MCP server (Rust)")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// MCP stdio server mode.
    ServeStdio {
        #[arg(long)]
        once: bool,
    },
    /// Daemon broker process (managed by autostart on Windows or launchd on macOS).
    ServeDaemon {
        #[arg(long)]
        once: bool,
    },
    /// macOS launchd service management.
    #[cfg(target_os = "macos")]
    Service {
        #[arg(long, default_value = "AfterEffectsMcpDaemon")]
        service_name: String,
        #[arg(long, default_value = "After Effects MCP Daemon")]
        display_name: String,
        #[command(subcommand)]
        command: ServiceCommands,
    },
    /// Windows current-user autostart management (HKCU Run key and daemon PID).
    #[cfg(target_os = "windows")]
    Autostart {
        #[arg(long, default_value = "AfterEffectsMcp")]
        entry_name: String,
        #[command(subcommand)]
        command: AutostartCommands,
    },
    /// Bridge operations through the host daemon broker.
    Bridge {
        #[command(subcommand)]
        command: BridgeCommands,
    },
    /// Print a health summary.
    Health,
}

#[derive(Debug, Subcommand)]
enum BridgeCommands {
    /// Run an allowlisted command through the daemon and print its retained request.
    RunScript {
        #[arg(long)]
        script: String,
        #[arg(long, default_value = "{}")]
        parameters: String,
        #[arg(long)]
        target_instance_id: Option<String>,
        #[arg(long)]
        target_version: Option<String>,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        timeout_ms: Option<u64>,
    },
    /// Read a retained request by ID, or the latest retained request.
    GetResults {
        #[arg(long)]
        request_id: Option<String>,
        /// Deprecated compatibility option; retained requests do not use file-age warnings.
        #[arg(long, default_value_t = 30)]
        stale_seconds: u64,
    },
}

#[derive(Debug, Subcommand)]
#[cfg(target_os = "macos")]
enum ServiceCommands {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

#[derive(Debug, Subcommand)]
#[cfg(target_os = "windows")]
enum AutostartCommands {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

fn init_tracing(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_new(level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let cli_config = cli.config.clone();
    let cfg = AppConfig::load_for_host(cli.config.as_deref(), AFTER_EFFECTS_HOST)?;
    init_tracing(&cfg.log_level);
    bridge_core::ensure_bridge_dir(&cfg)?;

    match cli.command {
        Commands::ServeStdio { once } => serve_stdio(cfg, once).await,
        Commands::ServeDaemon { once } => serve_daemon(cfg, once).await,
        #[cfg(target_os = "macos")]
        Commands::Service {
            service_name,
            display_name,
            command,
        } => run_service_command(cli_config, service_name, display_name, command),
        #[cfg(target_os = "windows")]
        Commands::Autostart {
            entry_name,
            command,
        } => run_autostart_command(cli_config, cfg, entry_name, command),
        Commands::Bridge { command } => run_bridge_command(cfg, command),
        Commands::Health => {
            println!("status=ok");
            println!("bridge_root={}", cfg.bridge.root_dir.display());
            println!("daemon_addr={}", cfg.daemon_addr);
            Ok(())
        }
    }
}

async fn serve_stdio(cfg: AppConfig, once: bool) -> Result<()> {
    info!("serve-stdio started");
    if once {
        return Ok(());
    }
    mcp_stdio::run_stdio_server(cfg).await
}

async fn serve_daemon(cfg: AppConfig, once: bool) -> Result<()> {
    info!("serve-daemon started");
    if once {
        return Ok(());
    }
    daemon_core::run_daemon_server(cfg)
}

#[cfg(target_os = "macos")]
fn run_service_command(
    cli_config: Option<PathBuf>,
    service_name: String,
    display_name: String,
    command: ServiceCommands,
) -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let mut args = Vec::new();
    if let Some(path) = cli_config {
        args.push("--config".to_string());
        args.push(path.to_string_lossy().to_string());
    }
    args.push("serve-daemon".to_string());

    let service_cfg = platform_service::ServiceConfig {
        service_name,
        display_name,
        description: "After Effects MCP daemon service".to_string(),
        binary_path: current_exe,
        args,
    };

    let action = match command {
        ServiceCommands::Install => platform_service::ServiceAction::Install,
        ServiceCommands::Uninstall => platform_service::ServiceAction::Uninstall,
        ServiceCommands::Start => platform_service::ServiceAction::Start,
        ServiceCommands::Stop => platform_service::ServiceAction::Stop,
        ServiceCommands::Status => platform_service::ServiceAction::Status,
    };
    let output = platform_service::run(action, &service_cfg)?;
    println!("{output}");
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_autostart_command(
    cli_config: Option<PathBuf>,
    cfg: AppConfig,
    entry_name: String,
    command: AutostartCommands,
) -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let mut args = Vec::new();
    if let Some(path) = cli_config {
        args.push("--config".to_string());
        args.push(path.to_string_lossy().to_string());
    }
    args.push("serve-daemon".to_string());

    let autostart_cfg = platform_service::AutostartConfig {
        app_name: "After Effects MCP".to_string(),
        entry_name,
        binary_path: current_exe,
        args,
        pid_file: cfg.bridge.root_dir.join("daemon.pid"),
    };

    let action = match command {
        AutostartCommands::Install => platform_service::AutostartAction::Install,
        AutostartCommands::Uninstall => platform_service::AutostartAction::Uninstall,
        AutostartCommands::Start => platform_service::AutostartAction::Start,
        AutostartCommands::Stop => platform_service::AutostartAction::Stop,
        AutostartCommands::Status => platform_service::AutostartAction::Status,
    };
    let output = platform_service::run_autostart(action, &autostart_cfg)?;
    println!("{output}");
    Ok(())
}

fn run_bridge_command(cfg: AppConfig, command: BridgeCommands) -> Result<()> {
    let value = bridge_command_result(&cfg, command)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn bridge_command_result(cfg: &AppConfig, command: BridgeCommands) -> Result<Value> {
    match command {
        BridgeCommands::RunScript {
            script,
            parameters,
            target_instance_id,
            target_version,
            timeout_ms,
        } => {
            if !mcp_core::is_allowed_script(&script) {
                anyhow::bail!(
                    "script '{}' is not allowed. Allowed scripts: {}",
                    script,
                    mcp_core::ALLOWED_SCRIPTS.join(", ")
                );
            }
            let value: Value = serde_json::from_str(&parameters)?;
            let timeout_ms = timeout_ms.unwrap_or(cfg.result_timeout_ms);
            daemon_core::call_daemon(
                cfg,
                json!({
                    "op": "runCommand",
                    "command": script,
                    "args": value,
                    "targetInstanceId": target_instance_id,
                    "targetVersion": target_version,
                    "timeoutMs": timeout_ms,
                    "pollIntervalMs": cfg.poll_interval_ms,
                    "retentionSeconds": cfg.result_retention_seconds,
                    "globalExclusive": false
                }),
                timeout_ms,
            )
        }
        BridgeCommands::GetResults {
            request_id,
            stale_seconds: _,
        } => daemon_core::call_daemon(
            cfg,
            match request_id {
                Some(request_id) => json!({"op":"getResult", "requestId":request_id}),
                None => json!({"op":"latestResult"}),
            },
            cfg.result_timeout_ms,
        ),
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::CommandFactory;

    fn parse_bridge_command(arguments: &[&str]) -> BridgeCommands {
        let cli = Cli::try_parse_from(arguments).expect("compatible CLI arguments");
        match cli.command {
            Commands::Bridge { command } => command,
            _ => panic!("expected bridge command"),
        }
    }

    fn isolated_config(label: &str) -> AppConfig {
        let root = std::env::temp_dir().join(format!(
            "ae-mcp-cli-{label}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut cfg = AppConfig::default();
        cfg.bridge.root_dir = root.clone();
        cfg.bridge.command_file = root.join("ae_command.json");
        cfg.bridge.result_file = root.join("ae_mcp_result.json");
        bridge_core::ensure_bridge_dir(&cfg).unwrap();
        cfg
    }

    #[test]
    fn bridge_cli_uses_broker_and_tracks_a_timed_out_request_without_resubmission() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::time::{Duration, Instant};

        let mut cfg = isolated_config("broker");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        cfg.daemon_addr = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        std::fs::write(&cfg.bridge.command_file, "existing command").unwrap();
        std::fs::write(&cfg.bridge.result_file, "existing result").unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut requests = Vec::new();
            while requests.len() < 4 {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing broker request");
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream).read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let request_id = request["requestId"]
                    .as_str()
                    .unwrap_or("req-cli-test")
                    .to_string();
                requests.push(request);
                writeln!(
                    stream,
                    "{}",
                    json!({
                        "ok":true,
                        "value": {
                            "requestId":request_id,
                            "state":if requests.len() == 1 { "timeout" } else { "completed" }
                        }
                    })
                )
                .unwrap();
            }
            requests
        });

        let legacy = bridge_command_result(
            &cfg,
            parse_bridge_command(&[
                "ae-mcp",
                "bridge",
                "run-script",
                "--script",
                "listCompositions",
                "--parameters",
                "{\"marker\":42}",
            ]),
        )
        .unwrap();
        assert_eq!(legacy["state"], "timeout");
        assert_eq!(legacy["requestId"], "req-cli-test");
        let recovered = bridge_command_result(
            &cfg,
            parse_bridge_command(&[
                "ae-mcp",
                "bridge",
                "get-results",
                "--request-id",
                "req-cli-test",
            ]),
        )
        .unwrap();
        assert_eq!(recovered["state"], "completed");
        assert_eq!(recovered["requestId"], legacy["requestId"]);
        bridge_command_result(
            &cfg,
            parse_bridge_command(&[
                "ae-mcp",
                "bridge",
                "run-script",
                "--script",
                "listCompositions",
                "--target-instance-id",
                "ae-test",
                "--target-version",
                "25.6",
                "--timeout-ms",
                "123",
            ]),
        )
        .unwrap();
        bridge_command_result(
            &cfg,
            parse_bridge_command(&["ae-mcp", "bridge", "get-results", "--stale-seconds", "30"]),
        )
        .unwrap();

        let requests = server.join().unwrap();
        assert_eq!(requests[0]["op"], "runCommand");
        assert_eq!(requests[0]["command"], "listCompositions");
        assert_eq!(requests[0]["args"], json!({"marker":42}));
        assert_eq!(requests[0]["timeoutMs"], cfg.result_timeout_ms);
        assert_eq!(
            requests[0]["retentionSeconds"],
            cfg.result_retention_seconds
        );
        assert_eq!(requests[0]["targetInstanceId"], Value::Null);
        assert_eq!(
            requests[1],
            json!({"op":"getResult", "requestId":"req-cli-test"})
        );
        assert_eq!(requests[2]["op"], "runCommand");
        assert_eq!(requests[2]["args"], json!({}));
        assert_eq!(requests[2]["targetInstanceId"], "ae-test");
        assert_eq!(requests[2]["targetVersion"], "25.6");
        assert_eq!(requests[2]["timeoutMs"], 123);
        assert_eq!(requests[3], json!({"op":"latestResult"}));
        assert_eq!(
            std::fs::read_to_string(&cfg.bridge.command_file).unwrap(),
            "existing command"
        );
        assert_eq!(
            std::fs::read_to_string(&cfg.bridge.result_file).unwrap(),
            "existing result"
        );
        std::fs::remove_dir_all(&cfg.bridge.root_dir).unwrap();
    }

    #[test]
    fn bridge_cli_requires_a_daemon_and_never_falls_back_to_local_mailboxes() {
        let mut cfg = isolated_config("offline");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        cfg.daemon_addr = listener.local_addr().unwrap().to_string();
        drop(listener);
        std::fs::write(&cfg.bridge.command_file, "active request").unwrap();
        std::fs::write(&cfg.bridge.result_file, "retained response").unwrap();
        for command in [
            parse_bridge_command(&[
                "ae-mcp",
                "bridge",
                "run-script",
                "--script",
                "listCompositions",
            ]),
            parse_bridge_command(&["ae-mcp", "bridge", "get-results"]),
        ] {
            let error = bridge_command_result(&cfg, command)
                .unwrap_err()
                .to_string();
            assert!(error.contains("failed to connect"), "{error}");
            assert!(error.contains("serve-daemon"), "{error}");
        }
        for parameters in ["not JSON", "{}"] {
            let command = BridgeCommands::RunScript {
                script: if parameters == "{}" {
                    "notAllowlisted"
                } else {
                    "listCompositions"
                }
                .into(),
                parameters: parameters.into(),
                target_instance_id: None,
                target_version: None,
                timeout_ms: None,
            };
            assert!(bridge_command_result(&cfg, command).is_err());
        }
        assert!(Cli::try_parse_from([
            "ae-mcp",
            "bridge",
            "run-script",
            "--script",
            "listCompositions",
            "--timeout-ms",
            "0",
        ])
        .is_err());
        assert_eq!(
            std::fs::read_to_string(&cfg.bridge.command_file).unwrap(),
            "active request"
        );
        assert_eq!(
            std::fs::read_to_string(&cfg.bridge.result_file).unwrap(),
            "retained response"
        );
        std::fs::remove_dir_all(&cfg.bridge.root_dir).unwrap();
    }

    #[test]
    fn platform_management_commands_match_the_target_os() {
        let command = Cli::command();
        let names = command
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect::<Vec<_>>();

        assert_eq!(names.contains(&"service"), cfg!(target_os = "macos"));
        assert_eq!(names.contains(&"autostart"), cfg!(target_os = "windows"));
    }
}
