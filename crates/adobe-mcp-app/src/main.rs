#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::{Context, Result};
use clap::Parser;
use daemon_core::{start_embedded_server, ManagedDaemon};
use fs2::FileExt;
use mcp_core::{AppConfig, HostSpec, HOST_SPECS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod tray;

#[derive(Parser)]
#[command(version, about = "Adobe MCP desktop app with in-process host brokers")]
struct Args {
    /// App TOML; host config paths are relative to this file.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override the app profile directory (logs, settings, and instance lock).
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(default)]
    hosts: BTreeMap<String, HostSettings>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostSettings {
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    config: Option<PathBuf>,
}

fn enabled_by_default() -> bool {
    true
}

struct HostRuntime {
    host: HostSpec,
    enabled: bool,
    config: AppConfig,
    runtime: Option<ManagedDaemon>,
    error: Option<String>,
}

impl HostRuntime {
    fn start(&mut self) {
        if self.runtime.is_some() {
            return;
        }
        match start_embedded_server(self.config.clone()) {
            Ok(runtime) => {
                self.runtime = Some(runtime);
                self.error = None;
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    fn stop(&self) {
        if let Some(runtime) = &self.runtime {
            runtime.begin_shutdown();
        }
    }

    fn reap(&mut self) {
        if self
            .runtime
            .as_ref()
            .is_some_and(ManagedDaemon::is_finished)
        {
            let stopped = self
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.snapshot().ok())
                .is_some_and(|s| !s.accepting);
            self.runtime = None;
            if self.enabled && !stopped {
                self.error = Some("Runtime stopped unexpectedly; retry from this menu.".into());
            }
        }
    }

    fn label(&self) -> String {
        if let Some(runtime) = &self.runtime {
            match runtime.snapshot() {
                Ok(status) if !status.accepting => {
                    format!("Stopping: {} pending", status.pending_jobs)
                }
                Ok(status) if status.pending_jobs > 0 => {
                    format!("Processing: {} pending", status.pending_jobs)
                }
                Ok(status) if status.connected_instances > 0 => {
                    format!("Connected: {} instance(s)", status.connected_instances)
                }
                Ok(_) => "Waiting for Adobe bridge".into(),
                Err(error) => format!("Needs attention: {error}"),
            }
        } else if self.error.is_some() {
            "Needs attention: server did not start".into()
        } else {
            "Stopped".into()
        }
    }
}

struct Application {
    _instance_lock: File,
    data_dir: PathBuf,
    settings_path: PathBuf,
    settings: Settings,
    hosts: Vec<HostRuntime>,
    quitting: bool,
}

impl Application {
    fn load(settings_path: PathBuf, instance_lock: File) -> Result<Self> {
        let settings: Settings = if settings_path.exists() {
            toml::from_str(&fs::read_to_string(&settings_path)?)?
        } else {
            Settings::default()
        };
        for id in settings.hosts.keys() {
            anyhow::ensure!(
                HOST_SPECS.iter().any(|host| host.id == id),
                "Unknown host: {id}"
            );
        }
        let mut hosts = Vec::new();
        for host in HOST_SPECS {
            let entry = settings.hosts.get(host.id);
            let path = entry
                .and_then(|entry| entry.config.as_ref())
                .map(|path| settings_path.parent().unwrap_or(Path::new(".")).join(path));
            hosts.push(HostRuntime {
                host: *host,
                enabled: entry.is_none_or(|entry| entry.enabled),
                config: AppConfig::load_for_host(path.as_deref(), *host)?,
                runtime: None,
                error: None,
            });
        }
        Ok(Self {
            _instance_lock: instance_lock,
            data_dir: settings_path
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf(),
            settings_path,
            settings,
            hosts,
            quitting: false,
        })
    }

    fn start(&mut self) {
        for host in &mut self.hosts {
            if host.enabled {
                host.start();
            }
        }
    }

    fn toggle(&mut self, index: usize) -> Result<()> {
        let host = &mut self.hosts[index];
        let enabled = !host.enabled;
        let mut settings = self.settings.clone();
        settings
            .hosts
            .entry(host.host.id.into())
            .or_insert(HostSettings {
                enabled: host.enabled,
                config: None,
            })
            .enabled = enabled;
        // Save before changing a live runtime. Failed writes keep the runtime as-is.
        bridge_core::write_atomic_text_file(
            &self.settings_path,
            toml::to_string_pretty(&settings)?.as_bytes(),
        )?;
        self.settings = settings;
        host.enabled = enabled;
        if enabled {
            host.start();
        } else {
            host.stop();
            host.error = None;
        }
        Ok(())
    }

    fn quit(&mut self) {
        self.quitting = true;
        for host in &mut self.hosts {
            host.stop();
        }
    }

    fn drained(&self) -> bool {
        self.hosts
            .iter()
            .all(|host| host.runtime.as_ref().is_none_or(ManagedDaemon::is_finished))
    }

    fn diagnostics(&self) -> Result<PathBuf> {
        let hosts: Vec<_> = self
            .hosts
            .iter()
            .map(|host| {
                serde_json::json!({
                    "host": host.host.id, "status": host.label(), "error": host.error,
                    "address": host.config.daemon_addr, "bridgeRoot": host.config.bridge.root_dir,
                    "runtime": host.runtime.as_ref().and_then(|runtime| runtime.snapshot().ok()),
                })
            })
            .collect();
        let path = self.settings_path.with_file_name("last-status.json");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"), "processId": std::process::id(), "hosts": hosts,
            }))?,
        )?;
        Ok(path)
    }
}

fn main() -> Result<()> {
    let result = run();
    if let Err(error) = &result {
        tracing::error!("Desktop app failed: {error:#}");
    }
    result
}

fn run() -> Result<()> {
    let args = Args::parse();
    let app_dir = std::path::absolute(
        args.data_dir
            .unwrap_or_else(|| mcp_core::default_bridge_root_dir_named("adobe-mcp")),
    )?;
    fs::create_dir_all(&app_dir)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(app_dir.join("application.log"))?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(log)
        .init();
    // One lock per app profile, independent of the chosen config file. Kept open for
    // the event loop lifetime; a crash releases the OS lock automatically.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(app_dir.join("application.lock"))?;
    if let Err(error) = lock.try_lock_exclusive() {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(());
        }
        return Err(error.into());
    }
    let settings_path = args
        .config
        .unwrap_or_else(|| app_dir.join("application.toml"));
    let mut app =
        Application::load(settings_path, lock).context("failed to load desktop app settings")?;
    app.data_dir = app_dir;
    if !app.settings_path.exists() {
        bridge_core::write_atomic_text_file(&app.settings_path, b"# Host IDs: aftereffects, premiere, photoshop, illustrator, indesign\n# Example:\n# [hosts.photoshop]\n# enabled = true\n# config = 'photoshop.toml'\n")?;
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    return tray::run(app);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        anyhow::bail!("The desktop app supports Windows and macOS.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn occupied_host_is_not_taken_over_and_can_be_disabled_persistently() {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut cfg = AppConfig::default();
        cfg.daemon_addr = listener.local_addr().unwrap().to_string();
        cfg.bridge.root_dir = dir.path().join("bridge");
        cfg.bridge.command_file = cfg.bridge.root_dir.join("ae_command.json");
        cfg.bridge.result_file = cfg.bridge.root_dir.join("ae_mcp_result.json");
        fs::write(dir.path().join("ae.toml"), toml::to_string(&cfg).unwrap()).unwrap();
        let mut settings = Settings::default();
        for host in HOST_SPECS {
            settings.hosts.insert(
                host.id.into(),
                HostSettings {
                    enabled: host.id == "aftereffects",
                    config: (host.id == "aftereffects").then(|| PathBuf::from("ae.toml")),
                },
            );
        }
        let path = dir.path().join("application.toml");
        fs::write(&path, toml::to_string(&settings).unwrap()).unwrap();
        let lock = File::create(dir.path().join("application.lock")).unwrap();
        let mut app = Application::load(path.clone(), lock).unwrap();
        app.start();
        assert!(app.hosts.iter().all(|host| host.runtime.is_none()));
        assert!(app.hosts[0]
            .error
            .as_deref()
            .unwrap()
            .contains("failed to bind"));
        assert!(!cfg.bridge.root_dir.join("app-runtime.pid").exists());
        assert_eq!(listener.local_addr().unwrap().to_string(), cfg.daemon_addr);
        app.toggle(0).unwrap();
        assert!(!app.hosts[0].enabled);
        assert!(app.hosts[0].error.is_none());
        let saved: Settings = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert!(!saved.hosts["aftereffects"].enabled);
        assert_eq!(
            saved.hosts["aftereffects"].config,
            Some(PathBuf::from("ae.toml"))
        );
        app.quit();
        assert!(app.drained());
    }
}
