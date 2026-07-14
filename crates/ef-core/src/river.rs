//! River 2/3 telemetry decoding.
//!
//! The heartbeat packs are fixed-width little-endian C structs (NOT protobuf), ported
//! field-by-field from `ha-ef-ble/eflib/model/*.py`. We decode only the subset needed
//! for the CloudUPS mapping and keep raw payloads for discovery on unfamiliar firmware.
//!
//! Routing (src, cmd_set, cmd_id) -> pack, from `devices/river2.py::data_parse`:
//!   (0x02, 0x20, 0x02) -> PD    (DirectPdHeartbeatPack)
//!   (0x03, 0x20, 0x02) -> EMS   (DirectEmsDeltaHeartbeatPack)
//!   (0x03, 0x20, 0x32) -> BMS   (DirectBmsMDeltaHeartbeatPack)
//!   (0x04, *,    0x02) -> INV   (DirectInvDeltaHeartbeatPack)
//!   (0x05, 0x20, 0x02) -> MPPT  (Mr330MpptHeart)

use crate::model::{status, DeviceSettings, Snapshot};

fn u8at(b: &[u8], o: usize) -> Option<u8> {
    b.get(o).copied()
}
fn u16at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn f32at(b: &[u8], o: usize) -> Option<f32> {
    b.get(o..o + 4)
        .map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[derive(Default, Debug, Clone)]
pub struct RiverState {
    pub serial: Option<String>,

    // PD
    pub pd_soc: Option<u8>,
    pub pd_watts_out: Option<u16>,
    pub pd_watts_in: Option<u16>,

    // BMS
    pub bms_soc_f32: Option<f32>,
    pub bms_temp: Option<u8>,

    // EMS
    pub ems_chg_remain_min: Option<u32>,
    pub ems_dsg_remain_min: Option<u32>,

    // INV
    pub inv_ac_in_vol_mv: Option<u32>,
    pub inv_out_vol_mv: Option<u32>,
    pub inv_ac_in_freq: Option<u8>,
    pub inv_out_freq: Option<u8>,
    pub inv_input_watts: Option<u16>,
    pub inv_output_watts: Option<u16>,

    // MPPT
    pub mppt_in_watts: Option<u16>,

    // Control settings read back from telemetry (merged across packs).
    pub settings: DeviceSettings,
}

/// Returns the pack name if the route is recognised, else None (caller dumps raw).
pub fn pack_name(src: u8, cmd_set: u8, cmd_id: u8) -> Option<&'static str> {
    match (src, cmd_set, cmd_id) {
        (0x02, 0x20, 0x02) => Some("PD"),
        (0x03, 0x20, 0x02) => Some("EMS"),
        (0x03, 0x20, 0x32) => Some("BMS"),
        (0x04, _, 0x02) => Some("INV"),
        (0x05, 0x20, 0x02) => Some("MPPT"),
        _ => None,
    }
}

impl RiverState {
    /// Decode a heartbeat pack into state. Returns the labelled fields it extracted
    /// (name, value) for the raw/discovery view, or None if the route is unknown.
    pub fn update(
        &mut self,
        src: u8,
        cmd_set: u8,
        cmd_id: u8,
        p: &[u8],
    ) -> Option<Vec<(&'static str, String)>> {
        let name = pack_name(src, cmd_set, cmd_id)?;
        let mut fields: Vec<(&'static str, String)> = Vec::new();

        match name {
            "PD" => {
                self.pd_soc = u8at(p, 14);
                self.pd_watts_out = u16at(p, 15);
                self.pd_watts_in = u16at(p, 17);
                push(&mut fields, "soc", self.pd_soc);
                push(&mut fields, "wattsOutSum", self.pd_watts_out);
                push(&mut fields, "wattsInSum", self.pd_watts_in);
                push_i(&mut fields, "remainTime", i32at(p, 19));
            }
            "EMS" => {
                push(&mut fields, "chgState", u8at(p, 0));
                push(&mut fields, "maxChargeSoc", u8at(p, 12));
                push(&mut fields, "minDsgSoc", u8at(p, 43));
                push(&mut fields, "openUpsFlag", u8at(p, 15));
                self.ems_chg_remain_min = u32at(p, 17);
                self.ems_dsg_remain_min = u32at(p, 21);
                push(&mut fields, "chgRemainTime", self.ems_chg_remain_min);
                push(&mut fields, "dsgRemainTime", self.ems_dsg_remain_min);
                push_f(&mut fields, "f32LcdShowSoc", f32at(p, 26));
                // Charge/discharge SoC limits (offsets confirmed against the capture).
                self.settings.charge_limit = u8at(p, 12);
                self.settings.discharge_limit = u8at(p, 43);
            }
            "BMS" => {
                push(&mut fields, "soc", u8at(p, 11));
                push(&mut fields, "vol(mV)", u32at(p, 12));
                push(&mut fields, "temp(°C)", u8at(p, 20));
                self.bms_temp = u8at(p, 20);
                self.bms_soc_f32 = f32at(p, 53);
                push_f(&mut fields, "f32ShowSoc", self.bms_soc_f32);
                push(&mut fields, "inputWatts", u32at(p, 57));
                push(&mut fields, "outputWatts", u32at(p, 61));
                push(&mut fields, "remainTime", u32at(p, 65));
            }
            "INV" => {
                self.inv_input_watts = u16at(p, 9);
                self.inv_output_watts = u16at(p, 11);
                self.inv_out_vol_mv = u32at(p, 14);
                self.inv_out_freq = u8at(p, 22);
                self.inv_ac_in_vol_mv = u32at(p, 23);
                self.inv_ac_in_freq = u8at(p, 31);
                push(&mut fields, "inputWatts", self.inv_input_watts);
                push(&mut fields, "outputWatts", self.inv_output_watts);
                push(&mut fields, "invOutVol(mV)", self.inv_out_vol_mv);
                push(&mut fields, "invOutFreq", self.inv_out_freq);
                push(&mut fields, "acInVol(mV)", self.inv_ac_in_vol_mv);
                push(&mut fields, "acInFreq", self.inv_ac_in_freq);
                push(&mut fields, "cfgAcEnabled", u8at(p, 47));
            }
            "MPPT" => {
                self.mppt_in_watts = u16at(p, 16);
                push(&mut fields, "inWatts", self.mppt_in_watts);
                // This src=0x05 config pack carries most of the device settings. Offsets
                // reverse-engineered from the capture (see captures/ANALYSIS.md).
                self.settings.dc_mode = u8at(p, 31);
                self.settings.dc_enabled = u8at(p, 56).map(|v| v != 0);
                self.settings.car_input_ma = u16at(p, 61);
                self.settings.ac_enabled = u8at(p, 66).map(|v| v != 0);
                self.settings.xboost_enabled = u8at(p, 67).map(|v| v != 0);
                self.settings.ac_charge_watts = u16at(p, 73);
                self.settings.ac_timeout_min = u16at(p, 75);
                self.settings.unit_timeout_min = u16at(p, 80);
                self.settings.screen_timeout_sec = u16at(p, 82);
                push(&mut fields, "acEnabled", u8at(p, 66));
                push(&mut fields, "xboost", u8at(p, 67));
                push(&mut fields, "dcEnabled", u8at(p, 56));
                push(&mut fields, "dcMode", u8at(p, 31));
                push(&mut fields, "acChargeWatts", u16at(p, 73));
                push(&mut fields, "carInput(mA)", u16at(p, 61));
            }
            _ => {}
        }
        Some(fields)
    }

    /// Rated AC output (W) by serial prefix, for load_pct. None when unknown.
    fn rated_ac_watts(&self) -> Option<f32> {
        let sn = self.serial.as_deref()?;
        let prefix = sn.get(..4)?;
        Some(match prefix {
            "R601" | "R603" => 300.0,            // River 2
            "R611" | "R613" => 500.0,            // River 2 Max
            "R621" | "R623" => 800.0,            // River 2 Pro
            "R631" | "R633" => 300.0,            // River 3 (approx)
            "R651" | "R653" => 600.0,            // River 3 Plus (approx)
            _ => return None,
        })
    }

    /// Derive the normalised CloudUPS snapshot from accumulated state.
    pub fn to_snapshot(&self) -> Snapshot {
        let mut s = Snapshot {
            device_uid: self.serial.clone(),
            settings: self.settings.clone(),
            ..Default::default()
        };

        // Battery charge: prefer the precise BMS float SoC, fall back to PD integer.
        s.battery_charge = self.bms_soc_f32.or_else(|| self.pd_soc.map(|v| v as f32));

        // Voltages/frequency from the inverter pack (EcoFlow reports mV).
        s.input_voltage = self.inv_ac_in_vol_mv.map(|mv| mv as f32 / 1000.0);
        s.output_voltage = self.inv_out_vol_mv.map(|mv| mv as f32 / 1000.0);
        s.input_frequency = self.inv_ac_in_freq.filter(|&f| f > 0).or(self.inv_out_freq);

        // Total system power (PD sums across all ports).
        s.input_power_w = self.pd_watts_in.map(|w| w as u32);
        s.output_power_w = self.pd_watts_out.map(|w| w as u32);
        s.temperature = self.bms_temp.map(|t| t as i32);

        // Mains presence: AC input voltage present, or AC charging watts flowing.
        let mains = self.inv_ac_in_vol_mv.map(|v| v > 1000).unwrap_or(false)
            || self.inv_input_watts.map(|w| w > 0).unwrap_or(false)
            || self.pd_watts_in.map(|w| w > 0).unwrap_or(false);
        let output_active = self.pd_watts_out.map(|w| w > 0).unwrap_or(false)
            || self.inv_output_watts.map(|w| w > 0).unwrap_or(false);

        let mut flags = 0u16;
        if let Some(soc) = s.battery_charge {
            if soc < 20.0 {
                flags |= status::LOW_BATTERY;
            }
        }
        if mains {
            flags |= status::ONLINE;
            if self.pd_watts_in.map(|w| w > 0).unwrap_or(false) {
                flags |= status::CHARGING;
            }
            s.battery_runtime_s = self.ems_chg_remain_min.map(|m| m * 60);
        } else {
            flags |= status::ON_BATTERY;
            if output_active {
                flags |= status::DISCHARGING;
            }
            s.battery_runtime_s = self.ems_dsg_remain_min.map(|m| m * 60);
        }
        s.status_flags = flags;

        // load_pct from AC output watts against rated AC output.
        if let (Some(rated), Some(out)) = (self.rated_ac_watts(), self.inv_output_watts) {
            if rated > 0.0 {
                s.load_pct = Some((out as f32 / rated) * 100.0);
            }
        }

        s
    }
}

fn i32at(b: &[u8], o: usize) -> Option<i32> {
    b.get(o..o + 4)
        .map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn push<T: std::fmt::Display>(v: &mut Vec<(&'static str, String)>, name: &'static str, val: Option<T>) {
    if let Some(x) = val {
        v.push((name, x.to_string()));
    }
}
fn push_i(v: &mut Vec<(&'static str, String)>, name: &'static str, val: Option<i32>) {
    if let Some(x) = val {
        v.push((name, x.to_string()));
    }
}
fn push_f(v: &mut Vec<(&'static str, String)>, name: &'static str, val: Option<f32>) {
    if let Some(x) = val {
        v.push((name, format!("{x:.2}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_route_is_none() {
        let mut st = RiverState::default();
        assert!(st.update(0x09, 0x20, 0x02, &[0u8; 40]).is_none());
    }

    #[test]
    fn settings_decode_from_config_packs() {
        let mut st = RiverState::default();

        // EMS pack carries the SoC limits.
        let mut ems = vec![0u8; 46];
        ems[12] = 50; // charge limit %
        ems[43] = 30; // discharge limit %
        st.update(0x03, 0x20, 0x02, &ems);

        // The src=0x05 config pack carries the rest.
        let mut cfg = vec![0u8; 94];
        cfg[31] = 2; // dc mode
        cfg[56] = 1; // dc output on
        cfg[61..63].copy_from_slice(&6000u16.to_le_bytes()); // car input mA (6 A)
        cfg[66] = 1; // ac output on
        cfg[67] = 0; // x-boost off
        cfg[73..75].copy_from_slice(&500u16.to_le_bytes()); // ac charge W
        cfg[75..77].copy_from_slice(&240u16.to_le_bytes()); // ac timeout min
        cfg[80..82].copy_from_slice(&1440u16.to_le_bytes()); // unit timeout min
        cfg[82..84].copy_from_slice(&1800u16.to_le_bytes()); // screen timeout sec
        st.update(0x05, 0x20, 0x02, &cfg);

        let s = st.to_snapshot().settings;
        assert_eq!(s.charge_limit, Some(50));
        assert_eq!(s.discharge_limit, Some(30));
        assert_eq!(s.dc_mode, Some(2));
        assert_eq!(s.dc_enabled, Some(true));
        assert_eq!(s.car_input_ma, Some(6000));
        assert_eq!(s.ac_enabled, Some(true));
        assert_eq!(s.xboost_enabled, Some(false));
        assert_eq!(s.ac_charge_watts, Some(500));
        assert_eq!(s.ac_timeout_min, Some(240));
        assert_eq!(s.unit_timeout_min, Some(1440));
        assert_eq!(s.screen_timeout_sec, Some(1800));
    }

    #[test]
    fn bms_soc_decodes_and_maps() {
        let mut st = RiverState::default();
        let mut p = vec![0u8; 69];
        // f32_show_soc at offset 53 = 84.5%
        p[53..57].copy_from_slice(&84.5f32.to_le_bytes());
        p[20] = 25; // temp
        st.update(0x03, 0x20, 0x32, &p);
        let snap = st.to_snapshot();
        assert_eq!(snap.battery_charge, Some(84.5));
        assert_eq!(snap.temperature, Some(25));
    }
}
