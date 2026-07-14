//! Encoding of control commands.
//!
//! River 2 accepts control over the plaintext AA protocol (reverse-engineered from an
//! Android HCI snoop of the official app; see `captures/ANALYSIS.md`). Every command is
//! a V2 packet from the host (`0x21`) with `cmd_set 0x20`; the module address (`dst`)
//! selects the PD/inverter (`0x05`) or the BMS (`0x03`). The app also logs in first with
//! a plaintext auth packet carrying the account User ID — replayed by `build_plaintext_auth`.
//!
//! The encrypted `build_command_frame` path below is retained for future devices that
//! speak the 0x5A5A protocol (River 3 / Delta); River 2 does not use it.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::Type7;
use crate::encpacket::{encode_frame, FRAME_TYPE_PROTOCOL};
use crate::packet::Packet;

/// Host address used as the source of every app-originated packet.
const HOST_SRC: u8 = 0x21;
/// Control command set for River 2.
const CMD_SET: u8 = 0x20;
/// PD / inverter module (AC, DC, charging, timeouts).
const DST_PD: u8 = 0x05;
/// Battery management system (state-of-charge limits).
const DST_BMS: u8 = 0x03;

/// A River 2 control command. Each variant encodes to a `(dst, cmd_id, payload)` triple
/// and then to a plaintext AA frame via [`Command::to_frame`]. Deserialised from the
/// frontend as `{ "kind": "<Variant>", "value": <data> }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Command {
    /// AC inverter output on/off.
    AcOutput(bool),
    /// X-Boost on/off.
    XBoost(bool),
    /// DC (12V) output on/off.
    DcOutput(bool),
    /// DC output mode (0, 1 or 2).
    DcMode(u8),
    /// AC charging speed, in watts.
    AcChargeWatts(u16),
    /// Car / DC input current, in milliamps.
    CarInputMilliamps(u32),
    /// Charge limit (maximum state of charge), percent.
    ChargeLimit(u8),
    /// Discharge limit (minimum state of charge), percent.
    DischargeLimit(u8),
    /// Unit standby timeout, in minutes (0 disables).
    UnitTimeoutMinutes(u16),
    /// Screen (LCD) timeout, in seconds (0 disables).
    ScreenTimeoutSeconds(u16),
    /// AC standby timeout, in minutes (0 disables).
    AcTimeoutMinutes(u16),
}

impl Command {
    /// Resolve to the module address, command id and payload bytes. `0xff` in the AC
    /// slots means "leave that setting unchanged" — so AC and X-Boost share cmd_id 0x42.
    fn encode(self) -> (u8, u8, Vec<u8>) {
        match self {
            Command::AcOutput(on) => {
                (DST_PD, 0x42, vec![on as u8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])
            }
            Command::XBoost(on) => {
                (DST_PD, 0x42, vec![0xff, on as u8, 0xff, 0xff, 0xff, 0xff, 0xff])
            }
            Command::DcOutput(on) => (DST_PD, 0x51, vec![on as u8]),
            Command::DcMode(mode) => (DST_PD, 0x52, vec![mode]),
            Command::AcChargeWatts(watts) => {
                let mut p = watts.to_le_bytes().to_vec();
                p.push(0xff);
                (DST_PD, 0x45, p)
            }
            Command::CarInputMilliamps(ma) => (DST_PD, 0x47, ma.to_le_bytes().to_vec()),
            Command::ChargeLimit(pct) => (DST_BMS, 0x31, vec![pct]),
            Command::DischargeLimit(pct) => (DST_BMS, 0x33, vec![pct]),
            Command::UnitTimeoutMinutes(m) => (DST_PD, 0x21, m.to_le_bytes().to_vec()),
            Command::ScreenTimeoutSeconds(s) => {
                let mut p = s.to_le_bytes().to_vec();
                p.push(0xff);
                (DST_PD, 0x27, p)
            }
            Command::AcTimeoutMinutes(m) => (DST_PD, 0x99, m.to_le_bytes().to_vec()),
        }
    }

    /// Build the plaintext AA frame to write for this command.
    pub fn to_frame(self) -> Vec<u8> {
        let (dst, cmd_id, payload) = self.encode();
        Packet::new(HOST_SRC, dst, CMD_SET, cmd_id, payload, 2).to_bytes()
    }
}

/// Build the plaintext auth frame that mirrors the official app's River 2 login: a V3
/// packet (host -> 0x35, cmd_set 0x35, cmd_id 0xA8) whose payload is
/// `01 | user_id (ASCII, right-padded to 64 bytes with zeros) | unix_ts (u32 LE)`.
pub fn build_plaintext_auth(user_id: &str, unix_ts: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(69);
    payload.push(0x01);
    let mut id = [0u8; 64];
    let bytes = user_id.as_bytes();
    let n = bytes.len().min(64);
    id[..n].copy_from_slice(&bytes[..n]);
    payload.extend_from_slice(&id);
    payload.extend_from_slice(&unix_ts.to_le_bytes());
    Packet::new(HOST_SRC, 0x35, 0x35, 0xA8, payload, 3).to_bytes()
}

/// Current unix time in seconds, truncated to `u32` for the auth timestamp field.
pub fn now_unix() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

/// Build the encrypted outer frame to write for a control command. Src/dst mirror the auth
/// packets: host (0x21) -> device (`auth_header_dst`), cmd_set/cmd_id/payload as given.
pub fn build_command_frame(
    session: &Type7,
    cmd_set: u8,
    cmd_id: u8,
    payload: Vec<u8>,
    packet_version: u8,
    auth_header_dst: u8,
) -> Vec<u8> {
    let pkt = Packet::new(0x21, auth_header_dst, cmd_set, cmd_id, payload, packet_version);
    encode_frame(FRAME_TYPE_PROTOCOL, &pkt.to_bytes(), Some(session))
}

/// A reply acknowledges a command when it carries the same cmd_set/cmd_id.
pub fn is_ack_for(pkt: &Packet, cmd_set: u8, cmd_id: u8) -> bool {
    pkt.cmd_set == cmd_set && pkt.cmd_id == cmd_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Type7;
    use crate::encpacket::FrameReader;
    use crate::packet::Packet;

    fn test_session() -> Type7 {
        Type7::new([0x11; 16], [0x22; 16])
    }

    #[test]
    fn command_frame_roundtrips_through_session() {
        let session = test_session();
        let frame = build_command_frame(&session, 0x20, 0x66, vec![1, 2, 3], 2, 0x35);
        // Reassemble the outer 0x5A5A frame, decrypt, and parse the inner packet back.
        let mut reader = FrameReader::new();
        let frames = reader.push(&frame);
        assert_eq!(frames.len(), 1);
        let inner = session.decrypt(&frames[0]);
        let pkt = Packet::from_bytes(&inner).expect("inner packet parses");
        assert_eq!(pkt.cmd_set, 0x20);
        assert_eq!(pkt.cmd_id, 0x66);
        assert_eq!(pkt.payload, vec![1, 2, 3]);
    }

    #[test]
    fn ack_matches_same_cmd_ids() {
        let pkt = Packet::new(0x35, 0x21, 0x20, 0x66, vec![], 2);
        assert!(is_ack_for(&pkt, 0x20, 0x66));
        assert!(!is_ack_for(&pkt, 0x20, 0x02));
    }

    // Golden frames captured from the official app (see captures/ANALYSIS.md). Matching
    // them byte-for-byte proves the layout, CRC8 and CRC16 all line up with the device.

    #[test]
    fn ac_output_on_matches_captured_frame() {
        assert_eq!(
            hex::encode(Command::AcOutput(true).to_frame()),
            "aa020700de0d0000000000002105204201ffffffffffff6b11"
        );
    }

    #[test]
    fn ac_output_off_matches_captured_frame() {
        assert_eq!(
            hex::encode(Command::AcOutput(false).to_frame()),
            "aa020700de0d0000000000002105204200ffffffffffff7bd1"
        );
    }

    #[test]
    fn plaintext_auth_matches_captured_frame() {
        // ts 0x6a53e9eb serialises little-endian to the captured `ebe9536a` tail.
        let frame = build_plaintext_auth("2075587123161088002", 0x6a53_e9eb);
        assert_eq!(
            hex::encode(frame),
            "aa034500c40d0000000000002135010135a80132303735353837313233313631303838303032000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ebe9536a334a"
        );
    }

    #[test]
    fn commands_round_trip_through_packet_parser() {
        // Each command must parse back to the module/cmd_id/payload it claims.
        let cases: &[(Command, u8, u8, &[u8])] = &[
            (Command::XBoost(true), 0x05, 0x42, &[0xff, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff]),
            (Command::DcOutput(true), 0x05, 0x51, &[0x01]),
            (Command::DcMode(2), 0x05, 0x52, &[0x02]),
            (Command::AcChargeWatts(500), 0x05, 0x45, &[0xf4, 0x01, 0xff]),
            (Command::CarInputMilliamps(6000), 0x05, 0x47, &[0x70, 0x17, 0x00, 0x00]),
            (Command::ChargeLimit(50), 0x03, 0x31, &[0x32]),
            (Command::DischargeLimit(30), 0x03, 0x33, &[0x1e]),
            (Command::UnitTimeoutMinutes(1440), 0x05, 0x21, &[0xa0, 0x05]),
            (Command::ScreenTimeoutSeconds(1800), 0x05, 0x27, &[0x08, 0x07, 0xff]),
            (Command::AcTimeoutMinutes(240), 0x05, 0x99, &[0xf0, 0x00]),
        ];
        for (cmd, dst, cmd_id, payload) in cases {
            let pkt = Packet::from_bytes(&cmd.to_frame())
                .unwrap_or_else(|| panic!("{cmd:?} did not parse"));
            assert_eq!(pkt.src, 0x21, "{cmd:?} src");
            assert_eq!(pkt.dst, *dst, "{cmd:?} dst");
            assert_eq!(pkt.cmd_set, 0x20, "{cmd:?} cmd_set");
            assert_eq!(pkt.cmd_id, *cmd_id, "{cmd:?} cmd_id");
            assert_eq!(pkt.payload, *payload, "{cmd:?} payload");
        }
    }
}
