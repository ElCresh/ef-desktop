//! Minimal local persistence for the resolved EcoFlow User ID and email. A JSON file in the
//! app config directory. The password is never stored. M2 will replace this with SQLite.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedCreds {
    pub user_id: Option<String>,
    pub email: Option<String>,
    #[serde(default = "default_region")]
    pub region: String,
}

impl Default for SavedCreds {
    fn default() -> Self {
        SavedCreds {
            user_id: None,
            email: None,
            region: default_region(),
        }
    }
}

fn default_region() -> String {
    "Eu".to_string()
}

fn creds_path(config_dir: &Path) -> PathBuf {
    config_dir.join("credentials.json")
}

pub fn load(config_dir: &Path) -> SavedCreds {
    let path = creds_path(config_dir);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(config_dir: &Path, creds: &SavedCreds) -> Result<(), String> {
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(creds).map_err(|e| e.to_string())?;
    std::fs::write(creds_path(config_dir), json).map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct KnownDevice {
    pub id: String,
    pub label: String,
    pub serial: Option<String>,
    pub address: Option<String>,
}

fn devices_path(config_dir: &Path) -> PathBuf {
    config_dir.join("devices.json")
}

pub fn load_devices(config_dir: &Path) -> Vec<KnownDevice> {
    std::fs::read_to_string(devices_path(config_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_devices(config_dir: &Path, devices: &[KnownDevice]) -> Result<(), String> {
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(devices).map_err(|e| e.to_string())?;
    std::fs::write(devices_path(config_dir), json).map_err(|e| e.to_string())
}

pub fn upsert_device(config_dir: &Path, device: KnownDevice) -> Result<(), String> {
    let mut devices = load_devices(config_dir);
    match devices.iter_mut().find(|d| d.id == device.id) {
        Some(existing) => *existing = device,
        None => devices.push(device),
    }
    save_devices(config_dir, &devices)
}

pub fn rename_device(config_dir: &Path, id: &str, label: &str) -> Result<(), String> {
    let mut devices = load_devices(config_dir);
    if let Some(d) = devices.iter_mut().find(|d| d.id == id) {
        d.label = label.to_string();
    }
    save_devices(config_dir, &devices)
}

pub fn remove_device(config_dir: &Path, id: &str) -> Result<(), String> {
    let mut devices = load_devices(config_dir);
    devices.retain(|d| d.id != id);
    save_devices(config_dir, &devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_region_is_eu() {
        assert_eq!(SavedCreds::default().region, "Eu");
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ecoflow_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn dev(id: &str, label: &str) -> KnownDevice {
        KnownDevice {
            id: id.to_string(),
            label: label.to_string(),
            serial: Some(id.to_string()),
            address: Some("AA:BB".to_string()),
        }
    }

    #[test]
    fn devices_missing_file_is_empty() {
        let dir = temp_dir("missing");
        assert!(load_devices(&dir).is_empty());
    }

    #[test]
    fn upsert_inserts_then_updates_in_place() {
        let dir = temp_dir("upsert");
        upsert_device(&dir, dev("R611", "Camper")).unwrap();
        upsert_device(&dir, dev("R601", "Home")).unwrap();
        // Update the label of an existing id — must not duplicate.
        let mut d = dev("R611", "Van");
        d.address = Some("CC:DD".to_string());
        upsert_device(&dir, d).unwrap();

        let list = load_devices(&dir);
        assert_eq!(list.len(), 2);
        let r611 = list.iter().find(|d| d.id == "R611").unwrap();
        assert_eq!(r611.label, "Van");
        assert_eq!(r611.address.as_deref(), Some("CC:DD"));
    }

    #[test]
    fn rename_and_remove() {
        let dir = temp_dir("rename_remove");
        upsert_device(&dir, dev("R611", "Camper")).unwrap();
        rename_device(&dir, "R611", "Camper 2").unwrap();
        assert_eq!(load_devices(&dir)[0].label, "Camper 2");
        remove_device(&dir, "R611").unwrap();
        assert!(load_devices(&dir).is_empty());
    }
}
