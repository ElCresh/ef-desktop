//! CRC8 / CRC16 for the EcoFlow BLE wire protocol.
//!
//! Ported from `ha-ef-ble/eflib/crc.py`:
//!   crc8 : width=8,  poly=0x07,   init=0x00, no reflection   (CRC-8/CCITT / I-432-1)
//!   crc16: width=16, poly=0x8005, init=0x0000, reflected in/out (CRC-16/ARC)

/// CRC-8/CCITT (poly 0x07, init 0, no reflection).
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// CRC-16/ARC (poly 0x8005 reflected -> 0xA001, init 0, reflected in/out).
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0x0000;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values from the well-known "123456789" check vectors.
    #[test]
    fn crc16_arc_check_vector() {
        // CRC-16/ARC("123456789") == 0xBB3D
        assert_eq!(crc16(b"123456789"), 0xBB3D);
    }

    #[test]
    fn crc8_check_vector() {
        // CRC-8 (poly 0x07, init 0x00) of "123456789" == 0xF4
        assert_eq!(crc8(b"123456789"), 0xF4);
    }
}
