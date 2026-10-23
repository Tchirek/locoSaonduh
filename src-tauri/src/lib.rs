pub mod adaptive;
pub mod audio;
pub mod commands;
pub mod net;
pub mod protocol;
pub mod service;
pub mod settings;
pub mod state;
pub mod tray;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use commands::AppState;
use service::Service;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

const PORT: u16 = protocol::DEFAULT_PORT;
pub const HIDDEN_ARG: &str = "--hidden";

fn keeps_running(app: &tauri::AppHandle) -> bool {
    app.try_state::<AppState>()
        .and_then(|state| state.service.as_ref().map(|service| service.snapshot()))
        .is_some_and(|snapshot| snapshot.prefs.background)
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let start_hidden = std::env::args().any(|arg| arg == HIDDEN_ARG);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .setup(move |app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let (service, startup_error) = match Service::start(config_dir, PORT) {
                Ok(service) => (Some(service), None),
                Err(error) => {
                    log::error!("could not start audio service: {error:#}");
                    (None, Some(format!("{error:#}")))
                }
            };
            let alive = Arc::new(AtomicBool::new(true));
            app.manage(AppState {
                service: service.clone(),
                startup_error,
            });
            app.manage(Arc::clone(&alive));

            let mut tray = tray::build(app.handle())?;
            if start_hidden && service.is_some() {
                tray::hide_main(app.handle());
            } else {
                tray::show_main(app.handle());
            }

            if let Some(service) = service {
                let handle = app.handle().clone();
                let alive = Arc::clone(&alive);
                std::thread::Builder::new()
                    .name("loco-ui-events".to_owned())
                    .spawn(move || {
                        while alive.load(Ordering::Acquire) {
                            let snapshot = service.snapshot();
                            tray.sync(&snapshot);
                            let _ = handle.emit("loco://state", snapshot);
                            std::thread::sleep(Duration::from_millis(180));
                        }
                    })?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && keeps_running(window.app_handle())
            {
                api.prevent_close();
                tray::hide_main(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::snapshot,
            commands::start_stream,
            commands::stop_stream,
            commands::subscribe,
            commands::unsubscribe,
            commands::set_alias,
            commands::set_mic,
            commands::set_output,
            commands::set_prefs,
            commands::set_virtual_mic,
            commands::refresh_audio,
            commands::probe_network,
            commands::probe_address,
            commands::autostart_status,
            commands::set_autostart,
            commands::open_vendor_page,
        ])
        .build(tauri::generate_context!())
        .expect("error while building locoSaonduh");

    app.run(|app_handle, event| match event {
        tauri::RunEvent::Exit => {
            if let Some(alive) = app_handle.try_state::<Arc<AtomicBool>>() {
                alive.store(false, Ordering::Release);
            }
            if let Some(state) = app_handle.try_state::<AppState>()
                && let Some(service) = &state.service
            {
                service.shutdown();
            }
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => tray::show_main(app_handle),
        _ => {}
    });
}
