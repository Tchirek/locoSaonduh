pub mod adaptive;
pub mod audio;
pub mod commands;
pub mod net;
pub mod protocol;
pub mod service;
pub mod settings;
pub mod state;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use commands::AppState;
use service::Service;
use tauri::Emitter;
use tauri::Manager;

const PORT: u16 = protocol::DEFAULT_PORT;

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let app = tauri::Builder::default()
        .setup(|app| {
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

            if let Some(service) = service {
                let handle = app.handle().clone();
                let alive = Arc::clone(&alive);
                std::thread::Builder::new()
                    .name("loco-ui-events".to_owned())
                    .spawn(move || {
                        while alive.load(Ordering::Acquire) {
                            let snapshot = service.snapshot();
                            let _ = handle.emit("loco://state", snapshot);
                            std::thread::sleep(Duration::from_millis(180));
                        }
                    })?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::snapshot,
            commands::start_broadcast,
            commands::stop_broadcast,
            commands::subscribe,
            commands::unsubscribe,
            commands::set_alias,
            commands::set_source,
            commands::set_output,
            commands::refresh_audio,
            commands::probe_network,
        ])
        .build(tauri::generate_context!())
        .expect("error while building locoSaonduh");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(alive) = app_handle.try_state::<Arc<AtomicBool>>() {
                alive.store(false, Ordering::Release);
            }
            if let Some(state) = app_handle.try_state::<AppState>()
                && let Some(service) = &state.service
            {
                service.shutdown();
            }
        }
    });
}
