use anyhow::Result;
use clap::{Parser, Subcommand};
use mcp_core::{default_bridge_root_dir_named, AppConfig, BridgePaths};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

mod mcp_stdio;

#[derive(Debug, Parser)]
#[command(name = "ps-mcp", version, about = "Photoshop MCP server (Rust)")]
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
    /// Daemon mode intended for OS service execution.
    ServeDaemon {
        #[arg(long)]
        once: bool,
    },
    /// Service management (Windows Service / macOS launchd).
    Service {
        #[arg(long, default_value = "PhotoshopMcpDaemon")]
        service_name: String,
        #[arg(long, default_value = "Photoshop MCP Daemon")]
        display_name: String,
        #[command(subcommand)]
        command: ServiceCommands,
    },
    /// Windows autostart management for the daemon.
    Autostart {
        #[arg(long, default_value = "PhotoshopMcp")]
        entry_name: String,
        #[command(subcommand)]
        command: AutostartCommands,
    },
    /// Direct bridge operations for validation.
    Bridge {
        #[command(subcommand)]
        command: BridgeCommands,
    },
    /// Print a health summary.
    Health,
}

#[derive(Debug, Subcommand)]
enum BridgeCommands {
    /// Queue a script command for Photoshop.
    RunScript {
        #[arg(long)]
        script: String,
        #[arg(long, default_value = "{}")]
        parameters: String,
    },
    /// Read the latest result payload.
    GetResults {
        #[arg(long, default_value_t = 30)]
        stale_seconds: u64,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommands {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

#[derive(Debug, Subcommand)]
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
    let cli_config = cli.config.clone();

    let bridge_root = default_bridge_root_dir_named("ps-mcp-bridge");
    let bridge_paths = BridgePaths {
        root_dir: bridge_root.clone(),
        command_file: bridge_root.join("ps_command.json"),
        result_file: bridge_root.join("ps_mcp_result.json"),
    };
    let cfg = AppConfig::load_with_bridge_paths(cli.config.as_deref(), bridge_paths)?;

    init_tracing(&cfg.log_level);
    bridge_core::ensure_bridge_dir(&cfg)?;

    match cli.command {
        Commands::ServeStdio { once } => serve_stdio(cfg, once).await,
        Commands::ServeDaemon { once } => serve_daemon(once).await,
        Commands::Service {
            service_name,
            display_name,
            command,
        } => run_service_command(cli_config, service_name, display_name, command),
        Commands::Autostart {
            entry_name,
            command,
        } => run_autostart_command(cli_config, cfg, entry_name, command),
        Commands::Bridge { command } => run_bridge_command(cfg, command),
        Commands::Health => {
            println!("status=ok");
            println!("bridge_root={}", cfg.bridge.root_dir.display());
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

async fn serve_daemon(once: bool) -> Result<()> {
    info!("serve-daemon started");
    if once {
        return Ok(());
    }
    let bridge_root = default_bridge_root_dir_named("ps-mcp-bridge");
    let _pid_file = DaemonPidFile::create(bridge_root.join("daemon.pid"))?;
    loop {
        info!("serve-daemon heartbeat");
        sleep(Duration::from_secs(60)).await;
    }
}

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
        description: "Photoshop MCP daemon service".to_string(),
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
        app_name: "Photoshop MCP".to_string(),
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
    let bridge = bridge_core::BridgeClient::new(cfg)?;
    match command {
        BridgeCommands::RunScript { script, parameters } => {
            if !ps_core::is_allowed_script(&script) {
                anyhow::bail!(
                    "script '{}' is not allowed. Allowed scripts: {}",
                    script,
                    ps_core::ALLOWED_SCRIPTS.join(", ")
                );
            }
            let value: serde_json::Value = serde_json::from_str(&parameters)?;
            bridge.clear_results_file()?;
            bridge.write_command_file(&script, value)?;
            println!(
                "queued command='{}' and cleared previous result. open Photoshop MCP Bridge panel and execute.",
                script
            );
            Ok(())
        }
        BridgeCommands::GetResults { stale_seconds } => {
            let raw = bridge.read_results_with_stale_warning(Duration::from_secs(stale_seconds))?;
            println!("{raw}");
            Ok(())
        }
    }
}

struct DaemonPidFile {
    path: PathBuf,
    pid: u32,
}

impl DaemonPidFile {
    fn create(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let pid = std::process::id();
        let exe = std::env::current_exe()?;
        fs::write(&path, format!("{pid}\n{}\n", exe.display()))?;
        Ok(Self { path, pid })
    }
}

impl Drop for DaemonPidFile {
    fn drop(&mut self) {
        let Ok(raw) = fs::read_to_string(&self.path) else {
            return;
        };
        let Some(pid_line) = raw.lines().next() else {
            return;
        };
        if pid_line.trim() == self.pid.to_string() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
