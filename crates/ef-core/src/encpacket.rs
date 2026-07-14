//! Outer BLE frame ("EncPacket", 0x5A5A prefix) encode + reassembly.
//!
//! Ported from `ha-ef-ble/eflib/encpacket.py` and the `EncPacketAssembler` /
//! `SimplePacketAssembler` in `frame_assembler.py`.
//!
//! Frame layout:
//! ```text
//!   5A 5A | frame_type<<4 | 01 | (len(payload)+2) as u16 LE | payload | crc16 LE
//! ```
//! `payload` is AES-CBC encrypted for protocol frames (encrypt_type 7) and plaintext
//! for the handshake command/response frames.

use crate::crc::crc16;
use crate::crypto::Type7;

pub const FRAME_TYPE_COMMAND: u8 = 0x00;
pub const FRAME_TYPE_PROTOCOL: u8 = 0x01;

const PREFIX: [u8; 2] = [0x5a, 0x5a];

/// Encode one EncPacket frame. When `cipher` is Some, the payload is AES-CBC encrypted.
pub fn encode_frame(frame_type: u8, inner: &[u8], cipher: Option<&Type7>) -> Vec<u8> {
    let payload = match cipher {
        Some(c) => c.encrypt(inner),
        None => inner.to_vec(),
    };
    let mut data = Vec::with_capacity(8 + payload.len());
    data.extend_from_slice(&PREFIX);
    data.push(frame_type << 4);
    data.push(0x01);
    data.extend_from_slice(&((payload.len() + 2) as u16).to_le_bytes());
    data.extend_from_slice(&payload);
    let c = crc16(&data);
    data.extend_from_slice(&c.to_le_bytes());
    data
}

fn find_prefix(hay: &[u8]) -> Option<usize> {
    hay.windows(2).position(|w| w == PREFIX)
}

/// Streaming reassembler that yields the raw (still-encrypted) payload of each complete,
/// CRC-valid frame. Callers decrypt afterwards (or not, during the handshake).
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Feed newly received bytes, get back zero or more complete frame payloads.
    pub fn push(&mut self, incoming: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(incoming);
        let mut data = std::mem::take(&mut self.buf);
        let mut out = Vec::new();

        loop {
            match find_prefix(&data) {
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
            if data.len() < 8 {
                break;
            }
            let payload_len = u16::from_le_bytes([data[4], data[5]]) as usize;
            if payload_len > 10_000 {
                data.drain(0..2);
                continue;
            }
            let data_end = 6 + payload_len;
            if data_end > data.len() {
                // Possible incomplete frame OR a false prefix inside payload bytes.
                if let Some(np) = find_prefix(&data[2..]) {
                    data.drain(0..2 + np);
                    continue;
                }
                break;
            }
            let payload_data = data[6..data_end - 2].to_vec();
            let payload_crc = u16::from_le_bytes([data[data_end - 2], data[data_end - 1]]);

            let mut check = data[0..6].to_vec();
            check.extend_from_slice(&payload_data);
            if crc16(&check) != payload_crc {
                data.drain(0..2);
                continue;
            }
            data.drain(0..data_end);
            out.push(payload_data);
        }

        self.buf = data;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_then_reassemble_plaintext() {
        let inner = b"\x01\x00abcdef";
        let frame = encode_frame(FRAME_TYPE_COMMAND, inner, None);
        let mut r = FrameReader::new();
        let frames = r.push(&frame);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], inner);
    }

    #[test]
    fn split_across_notifications() {
        let inner = b"hello-frame-payload";
        let frame = encode_frame(FRAME_TYPE_COMMAND, inner, None);
        let mut r = FrameReader::new();
        let (a, b) = frame.split_at(5);
        assert!(r.push(a).is_empty());
        let frames = r.push(b);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], inner);
    }

    #[test]
    fn encrypted_roundtrip() {
        let cipher = Type7::new([0x33; 16], [0x44; 16]);
        let inner = b"encrypted inner packet bytes";
        let frame = encode_frame(FRAME_TYPE_PROTOCOL, inner, Some(&cipher));
        let mut r = FrameReader::new();
        let frames = r.push(&frame);
        assert_eq!(frames.len(), 1);
        assert_eq!(cipher.decrypt(&frames[0]), inner);
    }
}
