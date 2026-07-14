//! Device profiles: map a device family to its telemetry decoder, capabilities and
//! validation status. In Milestone 1 profiles are read-only (no command encoding).

use crate::model::Snapshot;
use crate::river::RiverState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ValidationStatus {
    Validated,
    Beta,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Capabilities {
    /// Command identifiers this profile can encode. Empty until Milestone 3.
    pub commands: Vec<&'static str>,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("operation not supported by this profile")]
    Unsupported,
}

/// A stateful decoder for one live connection: heartbeats are fed in, a snapshot read out.
pub trait TelemetryDecoder {
    fn ingest(&mut self, src: u8, cmd_set: u8, cmd_id: u8, payload: &[u8]);
    fn snapshot(&self) -> Snapshot;
}

/// A device family.
pub trait Profile: Send + Sync {
    fn id(&self) -> &'static str;
    fn validation(&self) -> ValidationStatus;
    fn capabilities(&self) -> Capabilities;
    fn new_decoder(&self, serial: Option<String>) -> Box<dyn TelemetryDecoder + Send>;
}

struct RiverDecoder(RiverState);

impl TelemetryDecoder for RiverDecoder {
    fn ingest(&mut self, src: u8, cmd_set: u8, cmd_id: u8, payload: &[u8]) {
        self.0.update(src, cmd_set, cmd_id, payload);
    }
    fn snapshot(&self) -> Snapshot {
        self.0.to_snapshot()
    }
}

struct RiverProfile;

impl Profile for RiverProfile {
    fn id(&self) -> &'static str {
        "river"
    }
    fn validation(&self) -> ValidationStatus {
        ValidationStatus::Validated
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn new_decoder(&self, serial: Option<String>) -> Box<dyn TelemetryDecoder + Send> {
        Box::new(RiverDecoder(RiverState {
            serial,
            ..Default::default()
        }))
    }
}

/// Generic beta profile: reuses the River decoder but is flagged unvalidated. Lets an
/// unknown device still surface whatever fields happen to line up, clearly marked.
struct BetaProfile;

impl Profile for BetaProfile {
    fn id(&self) -> &'static str {
        "generic-beta"
    }
    fn validation(&self) -> ValidationStatus {
        ValidationStatus::Beta
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn new_decoder(&self, serial: Option<String>) -> Box<dyn TelemetryDecoder + Send> {
        Box::new(RiverDecoder(RiverState {
            serial,
            ..Default::default()
        }))
    }
}

/// Resolve a device to its profile by serial prefix. River serials start with `R6`.
pub fn profile_for(serial: Option<&str>, _product_type: u8) -> Box<dyn Profile> {
    match serial {
        Some(s) if s.starts_with("R6") => Box::new(RiverProfile),
        _ => Box::new(BetaProfile),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn river_serial_resolves_to_validated_profile() {
        let p = profile_for(Some("R601ZEB0000000A"), 0);
        assert_eq!(p.id(), "river");
        assert!(matches!(p.validation(), ValidationStatus::Validated));
    }

    #[test]
    fn river_decoder_accumulates_bms_soc() {
        let p = profile_for(Some("R601ZEB0000000A"), 0);
        let mut d = p.new_decoder(Some("R601ZEB0000000A".into()));
        let mut buf = vec![0u8; 69];
        buf[53..57].copy_from_slice(&84.5f32.to_le_bytes()); // BMS f32ShowSoc
        d.ingest(0x03, 0x20, 0x32, &buf);
        assert_eq!(d.snapshot().battery_charge, Some(84.5));
    }

    #[test]
    fn unknown_serial_is_beta() {
        let p = profile_for(Some("XX99"), 0);
        assert!(matches!(p.validation(), ValidationStatus::Beta));
    }
}
