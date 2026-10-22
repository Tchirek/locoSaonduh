use tauri::State;

use crate::service::Service;
use crate::state::Snapshot;

pub struct AppState {
    pub service: Option<Service>,
    pub startup_error: Option<String>,
}

impl AppState {
    fn service(&self) -> Result<Service, String> {
        self.service
            .clone()
            .ok_or_else(|| {
                self.startup_error
                    .clone()
                    .unwrap_or_else(|| "音频服务未能启动".to_owned())
            })
    }
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
pub async fn start_broadcast(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.start_broadcast(source_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn stop_broadcast(state: State<'_, AppState>) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.stop_broadcast())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn subscribe(state: State<'_, AppState>, peer_id: String) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.subscribe(peer_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn unsubscribe(state: State<'_, AppState>, sender_id: String) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.unsubscribe(sender_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_alias(state: State<'_, AppState>, alias: String) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.set_alias(alias))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_source(state: State<'_, AppState>, source_id: String) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.set_source(source_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_output(
    state: State<'_, AppState>,
    output_id: Option<String>,
) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.set_output(output_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn refresh_audio(state: State<'_, AppState>) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.refresh_devices())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn probe_network(state: State<'_, AppState>) -> Result<(), String> {
    let service = state.service()?;
    tauri::async_runtime::spawn_blocking(move || service.probe())
        .await
        .map_err(|error| error.to_string())?
}
