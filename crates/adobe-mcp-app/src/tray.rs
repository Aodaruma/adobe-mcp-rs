use super::Application;
use anyhow::Result;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

struct HostMenu {
    state: MenuItem,
    detail: MenuItem,
    toggle: MenuItem,
    retry: MenuItem,
    folder: MenuItem,
}

fn open_path(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("explorer.exe");
        command.creation_flags(0x08000000);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    command.arg(path).spawn()?;
    Ok(())
}

fn icon(active: bool) -> Result<Icon> {
    let bytes: &[u8] = if active {
        include_bytes!("../../../assets/icons/tray-active-64.png")
    } else {
        include_bytes!("../../../assets/icons/tray-inactive-64.png")
    };
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    let mut rgba = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| anyhow::anyhow!("Invalid icon size"))?
    ];
    let info = reader.next_frame(&mut rgba)?;
    anyhow::ensure!(
        info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight,
        "Tray icons must be 8-bit RGBA PNGs"
    );
    rgba.truncate(info.buffer_size());
    Ok(Icon::from_rgba(rgba, info.width, info.height)?)
}

pub(super) fn run(mut app: Application) -> Result<()> {
    let event_loop = EventLoopBuilder::<MenuEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    let event_loop = {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        let mut event_loop = event_loop;
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop
    };
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(event);
    }));
    let menu = Menu::new();
    let title = MenuItem::new(
        format!("Adobe MCP {}", env!("CARGO_PKG_VERSION")),
        false,
        None,
    );
    menu.append(&title)?;
    let mut hosts = Vec::new();
    for host in &app.hosts {
        let submenu = Submenu::new(host.host.display_name, true);
        let state = MenuItem::new("Starting…", false, None);
        let detail = MenuItem::new(&host.config.daemon_addr, false, None);
        let toggle = MenuItem::new("Stop receiving", true, None);
        let retry = MenuItem::new("Retry start", false, None);
        let folder = MenuItem::new("Open bridge folder…", true, None);
        submenu.append_items(&[&state, &detail, &toggle, &retry, &folder])?;
        menu.append(&submenu)?;
        hosts.push(HostMenu {
            state,
            detail,
            toggle,
            retry,
            folder,
        });
    }
    let diagnostics = MenuItem::new("Open diagnostics…", true, None);
    let settings = MenuItem::new("Open app settings folder…", true, None);
    let auto_launch = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("AdobeMcpDesktop")
        .set_app_path(&std::env::current_exe()?.to_string_lossy())
        .set_args(&[
            "--config",
            &std::path::absolute(&app.settings_path)?.to_string_lossy(),
            "--data-dir",
            &app.data_dir.to_string_lossy(),
        ])
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        .build()?;
    let login = CheckMenuItem::new(
        "Start at login",
        true,
        auto_launch.is_enabled().unwrap_or(false),
        None,
    );
    let quit = MenuItem::new("Quit Adobe MCP", true, None);
    menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &diagnostics,
        &settings,
        &login,
        &quit,
    ])?;
    let active_icon = icon(true)?;
    let inactive_icon = icon(false)?;
    let mut tray: Option<TrayIcon> = None;
    let mut last_indicator = None;
    let mut next_refresh = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_refresh);
        if let Event::NewEvents(StartCause::Init) = event {
            // macOS requires constructing the icon after the main event loop starts.
            let builder = TrayIconBuilder::new()
                .with_menu(Box::new(menu.clone()))
                .with_tooltip("Adobe MCP — Starting")
                // Keep color on macOS too: template images discard status colors.
                .with_icon(inactive_icon.clone());
            match builder.build() {
                Ok(value) => {
                    tray = Some(value);
                    app.start();
                }
                Err(error) => {
                    tracing::error!("Tray initialization failed: {error}");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
        }
        if let Event::UserEvent(event) = event {
            let result = if event.id == quit.id() {
                app.quit();
                quit.set_text("Waiting for pending Adobe work…");
                quit.set_enabled(false);
                Ok(())
            } else if event.id == diagnostics.id() {
                app.diagnostics().and_then(|path| open_path(&path))
            } else if event.id == settings.id() {
                open_path(app.settings_path.parent().unwrap_or(Path::new(".")))
            } else if event.id == login.id() {
                let result = if login.is_checked() {
                    auto_launch.enable()
                } else {
                    auto_launch.disable()
                };
                login.set_checked(auto_launch.is_enabled().unwrap_or(false));
                result.map_err(anyhow::Error::from)
            } else {
                let mut result = Ok(());
                for (index, host_menu) in hosts.iter().enumerate() {
                    if event.id == host_menu.toggle.id() && !app.quitting {
                        result = app.toggle(index);
                    }
                    if event.id == host_menu.retry.id() && !app.quitting && app.hosts[index].enabled
                    {
                        app.hosts[index].start();
                    }
                    if event.id == host_menu.folder.id() {
                        result = open_path(&app.hosts[index].config.bridge.root_dir);
                    }
                }
                result
            };
            if let Err(error) = result {
                title.set_text(format!("Adobe MCP: {error}"));
            }
            next_refresh = Instant::now();
        }
        if Instant::now() >= next_refresh {
            for (host, host_menu) in app.hosts.iter_mut().zip(&hosts) {
                host.reap();
                host_menu.state.set_text(host.label());
                host_menu
                    .detail
                    .set_text(host.error.as_deref().unwrap_or(&host.config.daemon_addr));
                let draining = host
                    .runtime
                    .as_ref()
                    .and_then(|runtime| runtime.snapshot().ok())
                    .is_some_and(|s| !s.accepting);
                host_menu.toggle.set_enabled(!app.quitting && !draining);
                host_menu
                    .retry
                    .set_enabled(!app.quitting && host.enabled && host.runtime.is_none());
                host_menu.toggle.set_text(if host.enabled {
                    "Stop receiving"
                } else {
                    "Start receiving"
                });
            }
            let indicator = app.indicator();
            if last_indicator != Some(indicator) {
                if let Some(tray) = &tray {
                    let image = if indicator.0 {
                        &active_icon
                    } else {
                        &inactive_icon
                    };
                    match tray
                        .set_icon(Some(image.clone()))
                        .and_then(|()| tray.set_tooltip(Some(indicator.1)))
                    {
                        Ok(()) => last_indicator = Some(indicator),
                        Err(error) => tracing::warn!("Could not update tray status: {error}"),
                    }
                }
            }
            next_refresh = Instant::now() + Duration::from_secs(1);
            *control_flow = ControlFlow::WaitUntil(next_refresh);
        }
        if app.quitting && app.drained() {
            tray.take();
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_status_assets_load_as_native_icons() {
        for active in [true, false] {
            super::icon(active).unwrap();
        }
    }
}
