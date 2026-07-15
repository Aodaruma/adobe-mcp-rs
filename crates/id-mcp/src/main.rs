use anyhow::Result;
use clap::{Parser, Subcommand};
use mcp_core::{AppConfig, INDESIGN_HOST};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

mod mcp_stdio;

#[derive(Debug, Parser)]
#[command(name = "id-mcp", version, about = "InDesign MCP server (Rust)")]
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
        #[arg(long, default_value = "InDesignMcpDaemon")]
        service_name: String,
        #[arg(long, default_value = "InDesign MCP Daemon")]
        display_name: String,
        #[command(subcommand)]
        command: ServiceCommands,
    },
    /// Windows current-user autostart management (HKCU Run key and daemon PID).
    #[cfg(target_os = "windows")]
    Autostart {
        #[arg(long, default_value = "InDesignMcp")]
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
    /// Queue an allowlisted template command for InDesign.
    RunTemplate {
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

    let cfg = AppConfig::load_for_host(cli.config.as_deref(), INDESIGN_HOST)?;

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
        description: "InDesign MCP daemon service".to_string(),
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
        app_name: "InDesign MCP".to_string(),
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
        BridgeCommands::RunTemplate { script, parameters } => {
            if !id_core::is_allowed_template(&script) {
                anyhow::bail!(
                    "script '{}' is not allowed. Allowed scripts: {}",
                    script,
                    id_core::ALLOWED_TEMPLATES.join(", ")
                );
            }
            let value: serde_json::Value = serde_json::from_str(&parameters)?;
            bridge.clear_results_file()?;
            bridge.write_command_file(&script, value)?;
            println!(
                "queued command='{}' and cleared previous result. The InDesign startup bridge will execute it when the host is running.",
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

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::CommandFactory;

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
