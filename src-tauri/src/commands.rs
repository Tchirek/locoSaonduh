use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::audio::{SourceKind, virtual_mic};
use crate::service::Service;
use crate::state::Snapshot;

pub struct AppState {
    pub service: Option<Service>,
    pub startup_error: Option<String>,
}

impl AppState {
    fn service(&self) -> Result<Service, String> {
        self.service.clone().ok_or_else(|| {
            self.startup_error
                .clone()
                .unwrap_or_else(|| "音频服务未能启动".to_owned())
        })
    }
}

async fn blocking<F>(service: Service, task: F) -> Result<(), String>
where
    F: FnOnce(Service) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || task(service))
        .await
        .map_err(|error| error.to_string())?
}

fn kind(value: &str) -> Result<SourceKind, String> {
    SourceKind::parse(value).ok_or_else(|| "未知的音源".to_owned())
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state.service()?.snapshot())
}

#[tauri::command]
pub async fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state.service()?.snapshot())
}

#[tauri::command]
pub async fn start_stream(state: State<'_, AppState>, kind: String) -> Result<(), String> {
    let kind = self::kind(&kind)?;
    blocking(state.service()?, move |service| service.start_stream(kind)).await
}

#[tauri::command]
pub async fn stop_stream(state: State<'_, AppState>, kind: String) -> Result<(), String> {
    let kind = self::kind(&kind)?;
    blocking(state.service()?, move |service| service.stop_stream(kind)).await
}

#[tauri::command]
pub async fn subscribe(state: State<'_, AppState>, key: String) -> Result<(), String> {
    blocking(state.service()?, move |service| service.subscribe(key)).await
}

#[tauri::command]
pub async fn unsubscribe(state: State<'_, AppState>, key: String) -> Result<(), String> {
    blocking(state.service()?, move |service| service.unsubscribe(key)).await
}

#[tauri::command]
pub async fn set_alias(state: State<'_, AppState>, alias: String) -> Result<(), String> {
    blocking(state.service()?, move |service| service.set_alias(alias)).await
}

#[tauri::command]
pub async fn set_mic(state: State<'_, AppState>, source_id: String) -> Result<(), String> {
    blocking(state.service()?, move |service| service.set_mic(source_id)).await
}

#[tauri::command]
pub async fn set_output(
    state: State<'_, AppState>,
    output_id: Option<String>,
) -> Result<(), String> {
    blocking(state.service()?, move |service| {
        service.set_output(output_id)
    })
    .await
}

#[tauri::command]
pub async fn set_prefs(
    state: State<'_, AppState>,
    restore: Option<bool>,
    background: Option<bool>,
) -> Result<(), String> {
    blocking(state.service()?, move |service| {
        service.set_prefs(restore, background)
    })
    .await
}

#[tauri::command]
pub async fn set_virtual_mic(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    blocking(state.service()?, move |service| {
        service.set_virtual_mic(enabled)
    })
    .await
}

#[tauri::command]
pub async fn refresh_audio(state: State<'_, AppState>) -> Result<(), String> {
    blocking(state.service()?, |service| service.refresh_devices()).await
}

#[tauri::command]
pub async fn probe_network(state: State<'_, AppState>) -> Result<(), String> {
    blocking(state.service()?, |service| service.probe()).await
}

#[tauri::command]
pub async fn probe_address(state: State<'_, AppState>, address: String) -> Result<(), String> {
    blocking(state.service()?, move |service| {
        service.probe_address(address)
    })
    .await
}

#[tauri::command]
pub fn autostart_status(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_vendor_page() -> Result<(), String> {
    let link = virtual_mic::LINK.ok_or("没有可打开的页面")?;
    #[cfg(target_os = "windows")]
    let opener = "explorer.exe";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(link)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
