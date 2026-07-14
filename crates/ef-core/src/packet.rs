//! Inner EcoFlow "Packet" codec (protocol V2 / V3).
//!
//! Ported from `ha-ef-ble/eflib/packet.py` (the V2/V3 branch; V4/V19 are out of scope
//! for River 2/3, which use V2). Wire layout for V2/V3:
//!
//! ```text
//!   AA | ver(1) | len(2 LE) | crc8(header 4B) | product(1) | seq(4) | 00 00 |
//!   src | dst | [dsrc | ddst  (V3+ only)] | cmd_set | cmd_id | payload | crc16(2 LE)
//! ```

use crate::crc::{crc8, crc16};

#[derive(Clone, Debug)]
pub struct Packet {
    pub src: u8,
    pub dst: u8,
    pub cmd_set: u8,
    pub cmd_id: u8,
    pub payload: Vec<u8>,
    pub dsrc: u8,
    pub ddst: u8,
    pub version: u8, // full version byte (e.g. 0x02, 0x03)
    pub seq: [u8; 4],
    pub product_id: i32,
}

impl Packet {
    pub const PREFIX: u8 = 0xAA;

    pub fn new(src: u8, dst: u8, cmd_set: u8, cmd_id: u8, payload: Vec<u8>, version: u8) -> Self {
        Self {
            src,
            dst,
            cmd_set,
            cmd_id,
            payload,
            dsrc: 1,
            ddst: 1,
            version,
            seq: [0, 0, 0, 0],
            product_id: 0,
        }
    }

    fn product_byte(&self) -> u8 {
        if self.product_id >= 0 {
            0x0d
        } else {
            0x0c
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(24 + self.payload.len());
        data.push(Self::PREFIX);
        data.push(self.version);
        data.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
        // header crc8 over the 4 bytes so far
        let hc = crc8(&data);
        data.push(hc);
        data.push(self.product_byte());
        data.extend_from_slice(&self.seq);
        data.extend_from_slice(&[0x00, 0x00]);
        data.push(self.src);
        data.push(self.dst);
        if (self.version & 0x0f) >= 0x03 {
            data.push(self.dsrc);
            data.push(self.ddst);
        }
        data.push(self.cmd_set);
        data.push(self.cmd_id);
        data.extend_from_slice(&self.payload);
        let c = crc16(&data);
        data.extend_from_slice(&c.to_le_bytes());
        data
    }

    /// Parse a V2/V3 packet. Returns None on any validation failure.
    pub fn from_bytes(data: &[u8]) -> Option<Packet> {
        if data.first() != Some(&Self::PREFIX) {
            return None;
        }
        let version_byte = data[1];
        let version = version_byte & 0x0f;
        let sentinel_format = (version_byte & 0x10) != 0;

        if (version == 2 && data.len() < 18) || (version == 3 && data.len() < 20) {
            return None;
        }
        if version != 2 && version != 3 {
            return None;
        }

        let payload_length = u16::from_le_bytes([data[2], data[3]]) as usize;

        // CRC16 over everything but the trailing 2 bytes (skipped for sentinel frames).
        if !sentinel_format {
            if data.len() < 2 {
                return None;
            }
            let want = u16::from_le_bytes([data[data.len() - 2], data[data.len() - 1]]);
            if crc16(&data[..data.len() - 2]) != want {
                return None;
            }
        }
        // header CRC8
        if crc8(&data[..4]) != data[4] {
            return None;
        }

        let mut seq = [0u8; 4];
        seq.copy_from_slice(&data[6..10]);
        let src = data[12];
        let dst = data[13];

        let (dsrc, ddst, payload_start);
        if version == 2 {
            dsrc = 0;
            ddst = 0;
            payload_start = 16;
        } else {
            dsrc = data[14];
            ddst = data[15];
            payload_start = 18;
        }
        let cmd_set;
        let cmd_id;
        if version == 2 {
            cmd_set = data[14];
            cmd_id = data[15];
        } else {
            cmd_set = data[16];
            cmd_id = data[17];
        }

        let mut payload = Vec::new();
        if payload_length > 0 && payload_start + payload_length <= data.len() {
            payload = data[payload_start..payload_start + payload_length].to_vec();
            // This device XORs the payload with seq[0] (observed on real River packets;
            // when seq[0]==0 the payload is already plaintext, so this is a no-op).
            if seq[0] != 0 {
                for b in payload.iter_mut() {
                    *b ^= seq[0];
                }
            }
            if sentinel_format && payload.len() >= 2 && &payload[payload.len() - 2..] == b"\xbb\xbb" {
                payload.truncate(payload.len() - 2);
            }
        }

        Some(Packet {
            src,
            dst,
            cmd_set,
            cmd_id,
            payload,
            dsrc,
            ddst,
            version: version_byte,
            seq,
            product_id: 0,
        })
    }
}

/// Streaming reassembler for plaintext AA packets (encrypt_type 0 devices).
///
/// Scans for the 0xAA prefix, validates the header CRC8, uses the length field to cut
/// each frame, and yields complete frame byte-slices ready for `Packet::from_bytes`.
/// Handles multiple packets per BLE notification and packets split across notifications.
#[derive(Default)]
pub struct PacketReader {
    buf: Vec<u8>,
}

impl PacketReader {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn push(&mut self, incoming: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(incoming);
        let mut data = std::mem::take(&mut self.buf);
        let mut out = Vec::new();

        loop {
            // find the prefix
            match data.iter().position(|&b| b == Packet::PREFIX) {
                None => {
                    data.clear();
                    break;
                }
                Some(s) => {
                    if s > 0 {
                        data.drain(0..s);
                    }
                }
            }
            if data.len() < 5 {
                break;
            }
            // header CRC8 must validate, else this 0xAA is spurious — skip it
            if crc8(&data[..4]) != data[4] {
                data.drain(0..1);
                continue;
            }
            let payload_length = u16::from_le_bytes([data[2], data[3]]) as usize;
            let version = data[1] & 0x0f;
            let payload_start = if version >= 3 { 18 } else { 16 };
            let frame_len = payload_start + payload_length + 2; // + CRC16

            if data.len() < frame_len {
                break; // wait for more bytes
            }
            out.push(data[..frame_len].to_vec());
            data.drain(0..frame_len);
        }

        self.buf = data;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_roundtrip() {
        let p = Packet::new(0x21, 0x35, 0x35, 0x86, vec![1, 2, 3, 4], 2);
        let bytes = p.to_bytes();
        let parsed = Packet::from_bytes(&bytes).expect("parse");
        assert_eq!(parsed.src, 0x21);
        assert_eq!(parsed.dst, 0x35);
        assert_eq!(parsed.cmd_set, 0x35);
        assert_eq!(parsed.cmd_id, 0x86);
        assert_eq!(parsed.payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn v3_roundtrip() {
        let p = Packet::new(0x21, 0x35, 0x20, 0x02, vec![9, 8, 7], 3);
        let bytes = p.to_bytes();
        let parsed = Packet::from_bytes(&bytes).expect("parse");
        assert_eq!(parsed.cmd_set, 0x20);
        assert_eq!(parsed.cmd_id, 0x02);
        assert_eq!(parsed.payload, vec![9, 8, 7]);
    }
}
