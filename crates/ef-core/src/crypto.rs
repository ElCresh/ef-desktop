//! MD5, AES-128-CBC (with EcoFlow's "Type7" decrypt quirk) and session-key derivation.
//!
//! Ported from `ha-ef-ble/eflib/encryption.py` (`Type7Encryption`) and
//! `Connection.genSessionKey`.

use aes::Aes128;
use anyhow::{bail, Result};
use cbc::cipher::block_padding::{NoPadding, Pkcs7};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use md5::{Digest, Md5};

use crate::keydata;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;

pub fn md5(data: &[u8]) -> [u8; 16] {
    let mut h = Md5::new();
    h.update(data);
    h.finalize().into()
}

/// Session-level AES-128-CBC used once the ECDH/session key is set (encrypt_type 7).
#[derive(Clone)]
pub struct Type7 {
    key: [u8; 16],
    iv: [u8; 16],
}

impl Type7 {
    pub fn new(key: [u8; 16], iv: [u8; 16]) -> Self {
        Self { key, iv }
    }

    /// PKCS7-pad then AES-CBC encrypt.
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let enc = Aes128CbcEnc::new(&self.key.into(), &self.iv.into());
        enc.encrypt_padded_vec_mut::<Pkcs7>(plaintext)
    }

    /// AES-CBC decrypt, mirroring the firmware quirk: only whole blocks are decrypted,
    /// trailing partial-block bytes are dropped, and PKCS7 unpad is attempted but a
    /// bad pad falls back to the raw decrypted bytes.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Vec<u8> {
        let aligned = ciphertext.len() - (ciphertext.len() % 16);
        if aligned == 0 {
            return ciphertext.to_vec();
        }
        let dec = Aes128CbcDec::new(&self.key.into(), &self.iv.into());
        // Decrypt with no automatic unpadding, then try PKCS7 strip manually.
        let mut buf = ciphertext[..aligned].to_vec();
        let raw = match dec.decrypt_padded_mut::<NoPadding>(&mut buf) {
            Ok(pt) => pt.to_vec(),
            Err(_) => return ciphertext[..aligned].to_vec(),
        };
        strip_pkcs7(&raw)
    }
}

/// Best-effort PKCS7 strip: returns the unpadded slice when the padding is valid,
/// otherwise the input unchanged (matches python's try/except ValueError).
fn strip_pkcs7(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return data.to_vec();
    }
    let pad = *data.last().unwrap() as usize;
    if pad == 0 || pad > 16 || pad > data.len() {
        return data.to_vec();
    }
    if data[data.len() - pad..].iter().all(|&b| b as usize == pad) {
        data[..data.len() - pad].to_vec()
    } else {
        data.to_vec()
    }
}

/// Derive the BLE session key from the key-info response.
///
/// `srand` is the first 16 bytes and `seed` the following 2 bytes of the decrypted
/// key-info payload. The python code round-trips the four u64 values through
/// little-endian pack/unpack, which is a no-op, so this reduces to:
///   session_key = md5( keydata[pos .. pos+16] || srand[0..16] )
/// with pos = seed[0]*0x10 + ((seed[1]-1) & 0xff)*0x100.
pub fn gen_session_key(seed: [u8; 2], srand: &[u8]) -> Result<[u8; 16]> {
    if srand.len() < 16 {
        bail!("srand too short: {} bytes", srand.len());
    }
    let pos = (seed[0] as usize) * 0x10 + ((seed[1].wrapping_sub(1)) as usize) * 0x100;
    let k0 = keydata::get8(pos)
        .ok_or_else(|| anyhow::anyhow!("keydata index {pos} out of range"))?;
    let k1 = keydata::get8(pos + 8)
        .ok_or_else(|| anyhow::anyhow!("keydata index {} out of range", pos + 8))?;

    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&k0);
    data.extend_from_slice(&k1);
    data.extend_from_slice(&srand[..16]);
    Ok(md5(&data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_cbc_roundtrip() {
        let t = Type7::new([0x11; 16], [0x22; 16]);
        let pt = b"hello ecoflow, this is a test payload";
        let ct = t.encrypt(pt);
        assert_eq!(t.decrypt(&ct), pt);
    }

    #[test]
    fn md5_known() {
        // md5("") = d41d8cd98f00b204e9800998ecf8427e
        assert_eq!(
            hex::encode(md5(b"")),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
    }

    #[test]
    fn session_key_is_deterministic() {
        let sk1 = gen_session_key([0x01, 0x01], &[0xAB; 16]).unwrap();
        let sk2 = gen_session_key([0x01, 0x01], &[0xAB; 16]).unwrap();
        assert_eq!(sk1, sk2);
    }
}
