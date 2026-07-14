// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use ef_core::ble::Found;
use ef_core::cloud::{fetch_user_id as cloud_fetch_user_id, Region};
use ef_core::command::Command;
use ef_core::manager::DeviceManager;
use serde::Serialize;
use tauri::{Emitter, Manager, State};

mod store;

struct AppState {
    manager: Arc<DeviceManager>,
}

fn parse_region(s: &str) -> Region {
    match s {
        "Us" => Region::Us,
        _ => Region::Eu,
    }
}

#[derive(Serialize)]
struct ScanResult {
    address: String,
    name: String,
    serial: Option<String>,
    encrypt_type: u8,
    product_type: u8,
}

impl From<Found> for ScanResult {
    fn from(f: Found) -> Self {
        ScanResult {
            address: f.address,
            name: f.name,
            serial: f.serial,
            encrypt_type: f.encrypt_type,
            product_type: f.product_type,
        }
    }
}

#[tauri::command]
async fn scan(secs: u64, state: State<'_, AppState>) -> Result<Vec<ScanResult>, String> {
    let found = state.manager.scan(secs).await.map_err(|e| e.to_string())?;
    Ok(found.into_iter().map(ScanResult::from).collect())
}

#[tauri::command]
async fn connect(
    address: String,
    serial: Option<String>,
    user_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.manager.connect(address, serial, user_id).await;
    Ok(())
}

#[tauri::command]
async fn disconnect(uid: String, state: State<'_, AppState>) -> Result<(), String> {
    state.manager.disconnect(&uid).await;
    Ok(())
}

#[tauri::command]
async fn send_command(
    uid: String,
    command: Command,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .manager
        .send_command(&uid, command)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_user_id(
    email: String,
    password: String,
    region: String,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let user_id = cloud_fetch_user_id(&email, &password, parse_region(&region))
        .await
        .map_err(|e| e.to_string())?;
    // Persistence is best-effort: a save failure must not discard a successfully fetched
    // user_id, so we ignore the error here rather than propagating it.
    if let Ok(config_dir) = app.path().app_config_dir() {
        let _ = store::save(
            &config_dir,
            &store::SavedCreds {
                user_id: Some(user_id.clone()),
                email: Some(email),
                region,
            },
        );
    }
    Ok(user_id)
}

#[tauri::command]
async fn saved_credentials(app: tauri::AppHandle) -> Result<store::SavedCreds, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(store::load(&config_dir))
}

#[tauri::command]
async fn known_devices(app: tauri::AppHandle) -> Result<Vec<store::KnownDevice>, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(store::load_devices(&dir))
}

#[tauri::command]
async fn save_known_device(
    device: store::KnownDevice,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    store::upsert_device(&dir, device)
}

#[tauri::command]
async fn rename_known_device(
    id: String,
    label: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    store::rename_device(&dir, &id, &label)
}

#[tauri::command]
async fn forget_device(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    store::remove_device(&dir, &id)
}

fn main() {
    let (manager, mut rx) = DeviceManager::new();
    let manager = Arc::new(manager);

    tauri::Builder::default()
        .manage(AppState {
            manager: manager.clone(),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            let _ = handle.emit("device-event", &ev);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break, // Closed
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan,
            connect,
            disconnect,
            send_command,
            fetch_user_id,
            saved_credentials,
            known_devices,
            save_known_device,
            rename_known_device,
            forget_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running EcoFlow Desktop");
}
