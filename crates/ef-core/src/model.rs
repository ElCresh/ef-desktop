//! Normalised telemetry snapshot, shaped like the CloudUPS `device_state` row.
//!
//! This is the whole point of the PoC: prove that River BLE telemetry maps cleanly
//! onto the same fields the USB/SNMP sources already feed into CloudUPS.

/// StatusFlag bitmask, matching `docs/design/architettura.md` §6.4.
pub mod status {
    pub const ONLINE: u16 = 1;
    pub const ON_BATTERY: u16 = 2;
    pub const LOW_BATTERY: u16 = 4;
    pub const REPLACE_BATTERY: u16 = 8;
    pub const CHARGING: u16 = 16;
    pub const DISCHARGING: u16 = 32;
    pub const OVERLOAD: u16 = 64;
    pub const BYPASS: u16 = 128;
    pub const OFFLINE: u16 = 256;
    pub const FAULT: u16 = 512;

    pub fn labels(flags: u16) -> Vec<&'static str> {
        let mut v = Vec::new();
        for (bit, name) in [
            (ONLINE, "online"),
            (ON_BATTERY, "on_battery"),
            (LOW_BATTERY, "low_battery"),
            (REPLACE_BATTERY, "replace_battery"),
            (CHARGING, "charging"),
            (DISCHARGING, "discharging"),
            (OVERLOAD, "overload"),
            (BYPASS, "bypass"),
            (OFFLINE, "offline"),
            (FAULT, "fault"),
        ] {
            if flags & bit != 0 {
                v.push(name);
            }
        }
        v
    }
}

/// Current device settings read back from the telemetry stream, so the control UI can
/// show real values instead of guesses. Offsets reverse-engineered from the HCI capture
/// (see captures/ANALYSIS.md): charge/discharge limits live in the EMS pack, the rest in
/// the src=0x05 config pack.
#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct DeviceSettings {
    pub ac_enabled: Option<bool>,
    pub xboost_enabled: Option<bool>,
    pub dc_enabled: Option<bool>,
    pub dc_mode: Option<u8>,
    pub ac_charge_watts: Option<u16>,
    pub car_input_ma: Option<u16>,
    pub charge_limit: Option<u8>,
    pub discharge_limit: Option<u8>,
    pub unit_timeout_min: Option<u16>,
    pub screen_timeout_sec: Option<u16>,
    pub ac_timeout_min: Option<u16>,
}

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct Snapshot {
    pub device_uid: Option<String>, // EcoFlow serial (stable identity)
    pub battery_charge: Option<f32>,
    pub input_voltage: Option<f32>,
    pub output_voltage: Option<f32>,
    pub load_pct: Option<f32>,
    pub battery_runtime_s: Option<u32>,
    pub input_power_w: Option<u32>,
    pub output_power_w: Option<u32>,
    pub input_frequency: Option<u8>,
    pub temperature: Option<i32>,
    pub status_flags: u16,
    pub settings: DeviceSettings,
}

fn fopt(v: Option<f32>, unit: &str) -> String {
    match v {
        Some(x) => format!("{x:.1}{unit}"),
        None => "—".to_string(),
    }
}
fn iopt<T: std::fmt::Display>(v: Option<T>, unit: &str) -> String {
    match v {
        Some(x) => format!("{x}{unit}"),
        None => "—".to_string(),
    }
}

impl Snapshot {
    /// Human-readable status flag labels for this snapshot.
    pub fn status_labels(&self) -> Vec<&'static str> {
        status::labels(self.status_flags)
    }

    /// Render as the CloudUPS device_state view (left) — the durable deliverable.
    pub fn render(&self) -> String {
        let labels = status::labels(self.status_flags);
        let flags = if labels.is_empty() {
            "—".to_string()
        } else {
            format!("{} (0x{:03X})", labels.join("|"), self.status_flags)
        };
        format!(
            "  device_uid        : {}\n\
             \x20 status_flags      : {}\n\
             \x20 battery_charge    : {}\n\
             \x20 battery_runtime_s : {}\n\
             \x20 load_pct          : {}\n\
             \x20 input_voltage     : {}\n\
             \x20 output_voltage    : {}\n\
             \x20 input_frequency   : {}\n\
             \x20 input_power       : {}\n\
             \x20 output_power      : {}\n\
             \x20 temperature       : {}",
            self.device_uid.as_deref().unwrap_or("—"),
            flags,
            fopt(self.battery_charge, "%"),
            iopt(self.battery_runtime_s, " s"),
            fopt(self.load_pct, "%"),
            fopt(self.input_voltage, " V"),
            fopt(self.output_voltage, " V"),
            iopt(self.input_frequency, " Hz"),
            iopt(self.input_power_w, " W"),
            iopt(self.output_power_w, " W"),
            iopt(self.temperature, " °C"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_serializes_to_json_with_status_labels() {
        let mut s = Snapshot::default();
        s.battery_charge = Some(84.5);
        s.status_flags = status::ONLINE | status::CHARGING;
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["battery_charge"], 84.5);
        assert_eq!(s.status_labels(), vec!["online", "charging"]);
    }
}
