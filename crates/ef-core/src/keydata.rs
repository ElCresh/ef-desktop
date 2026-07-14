//! Static key table used to derive the BLE session key.
//!
//! This is the `_data` blob from `ha-ef-ble/eflib/keydata.py`, extracted verbatim
//! (65280 bytes). `genSessionKey` indexes into it with `get8bytes(pos)`.

/// The raw key table (65280 bytes).
pub static KEYDATA: &[u8] = include_bytes!("keydata.bin");

/// Return 8 bytes at `pos` (little-endian u64 source), mirroring `keydata.get8bytes`.
///
/// Returns `None` if the requested window falls outside the table. Real devices
/// send seed values that keep `pos` in range; an out-of-range value signals a
/// protocol/parse problem rather than something to paper over.
pub fn get8(pos: usize) -> Option<[u8; 8]> {
    let end = pos.checked_add(8)?;
    if end > KEYDATA.len() {
        return None;
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(&KEYDATA[pos..end]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_size_and_head() {
        assert_eq!(KEYDATA.len(), 65280);
        // first16 hex from extraction: d1824a0a4b08d296a078d706a46127f1
        assert_eq!(&KEYDATA[..4], &[0xd1, 0x82, 0x4a, 0x0a]);
    }
}
