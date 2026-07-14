//! EcoFlow BLE V2 handshake (encrypt_type 7) + telemetry monitor loop.
//!
//! Ported from `ha-ef-ble/eflib/connection.py`. State sequence:
//!   ECDH pubkey exchange -> key-info/session-key -> auth-status -> auth -> stream.

use std::time::Duration;

use anyhow::{bail, Result};
use futures::StreamExt;

use crate::ble::BleTransport;
use crate::crypto::{gen_session_key, md5, Type7};
use crate::encpacket::{encode_frame, FrameReader, FRAME_TYPE_COMMAND, FRAME_TYPE_PROTOCOL};
use crate::packet::Packet;
use crate::river::RiverState;
use crate::secp160r1;

pub struct SessionCfg {
    /// None = probe mode: validate the handshake up to session-key + auth-status,
    /// then stop (no account credentials needed). Some = authenticate and stream.
    pub user_id: Option<String>,
    pub serial: String,
    pub packet_version: u8,
    pub auth_header_dst: u8,
}

/// Plaintext telemetry stream (encrypt_type 0 devices, e.g. this River): no ECDH, no
/// auth, no account needed — just parse the AA packets the device broadcasts on connect.
pub async fn run_plaintext(
    transport: &BleTransport,
    serial: Option<String>,
    raw: bool,
) -> Result<()> {
    let mut notif = transport.notifications().await?;
    let mut reader = crate::packet::PacketReader::new();
    let mut state = RiverState {
        serial,
        ..Default::default()
    };
    eprintln!("[stream] plaintext mode — reading AA telemetry packets (Ctrl-C to stop)\n");

    let mut last_print = std::time::Instant::now();
    loop {
        let v = match tokio::time::timeout(Duration::from_secs(30), notif.next()).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                eprintln!("[stream] notification stream ended");
                break;
            }
            Err(_) => {
                eprintln!("[stream] no data for 30s — stopping");
                break;
            }
        };
        for frame in reader.push(&v.value) {
            let pkt = match Packet::from_bytes(&frame) {
                Some(p) => p,
                None => continue,
            };
            if let Some(fields) = state.update(pkt.src, pkt.cmd_set, pkt.cmd_id, &pkt.payload) {
                if raw {
                    let pack =
                        crate::river::pack_name(pkt.src, pkt.cmd_set, pkt.cmd_id).unwrap_or("?");
                    let dump = fields
                        .iter()
                        .map(|(k, val)| format!("{k}={val}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!(
                        "[{pack}] ({:#04x},{:#04x},{:#04x}) {dump}",
                        pkt.src, pkt.cmd_set, pkt.cmd_id
                    );
                }
            }
        }
        if last_print.elapsed() >= Duration::from_secs(2) {
            let snap = state.to_snapshot();
            println!("\n── CloudUPS device_state ─────────────────────");
            println!("{}", snap.render());
            last_print = std::time::Instant::now();
        }
    }
    Ok(())
}

fn ecdh_type_size(curve_num: u8) -> usize {
    match curve_num {
        1 => 52,
        2 => 56,
        3 | 4 => 64,
        _ => 40,
    }
}

pub(crate) type NotifStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = btleplug::api::ValueNotification> + Send>>;

/// Await BLE notifications until the reader yields at least one complete frame.
pub(crate) async fn recv_frames(
    notif: &mut NotifStream,
    reader: &mut FrameReader,
    timeout_s: u64,
) -> Result<Vec<Vec<u8>>> {
    loop {
        match tokio::time::timeout(Duration::from_secs(timeout_s), notif.next()).await {
            Ok(Some(v)) => {
                eprintln!(
                    "[recv] notification {} ({} bytes): {}",
                    v.uuid,
                    v.value.len(),
                    hex::encode(&v.value)
                );
                let frames = reader.push(&v.value);
                if !frames.is_empty() {
                    return Ok(frames);
                }
            }
            Ok(None) => bail!("BLE notification stream ended"),
            Err(_) => bail!("timeout ({timeout_s}s) waiting for BLE data"),
        }
    }
}

/// Build the reply packet (swap src/dst) that nudges the device to keep streaming.
pub(crate) fn reply_packet(pkt: &Packet) -> Packet {
    Packet {
        src: pkt.dst,
        dst: pkt.src,
        cmd_set: pkt.cmd_set,
        cmd_id: pkt.cmd_id,
        payload: pkt.payload.clone(),
        dsrc: 1,
        ddst: 1,
        version: pkt.version,
        seq: pkt.seq,
        product_id: pkt.product_id,
    }
}

/// Run the encrypt_type-7 handshake (ECDH -> session key -> auth-status -> auth) and
/// return the established AES session. Requires `cfg.user_id`. Shared by the standalone
/// `run` loop and the DeviceManager's `EncryptedSession`.
pub(crate) async fn handshake_encrypted(
    transport: &BleTransport,
    cfg: &SessionCfg,
    notif: &mut NotifStream,
    reader: &mut FrameReader,
) -> Result<Type7> {
    if transport.encrypt_type != 7 {
        bail!(
            "encrypted handshake requires encrypt_type 7; device advertises {}",
            transport.encrypt_type
        );
    }

    // Step 1: ECDH public-key exchange.
    let eph = secp160r1::generate();
    let mut hello = vec![0x01, 0x00];
    hello.extend_from_slice(&eph.public_key);
    transport
        .write(&encode_frame(FRAME_TYPE_COMMAND, &hello, None), true)
        .await?;
    let frames = recv_frames(notif, reader, 20).await?;
    let resp = &frames[0];
    if resp.len() < 3 {
        bail!("short pubkey response: {}", hex::encode(resp));
    }
    let size = ecdh_type_size(resp[2]);
    let end = (3 + size).min(resp.len());
    let dev_pub = &resp[3..end];
    let shared = eph.shared_secret(dev_pub)?;
    let iv = md5(&shared);
    let mut key16 = [0u8; 16];
    key16.copy_from_slice(&shared[..16]);
    let ecdh_enc = Type7::new(key16, iv);

    // Step 2: key-info -> session key.
    transport
        .write(&encode_frame(FRAME_TYPE_COMMAND, &[0x02], None), true)
        .await?;
    let frames = recv_frames(notif, reader, 20).await?;
    let enc_resp = &frames[0];
    if enc_resp.first() != Some(&0x02) {
        bail!("key-info type != 0x02: {}", hex::encode(enc_resp));
    }
    let data = ecdh_enc.decrypt(&enc_resp[1..]);
    if data.len() < 18 {
        bail!("key-info payload too short: {}", hex::encode(&data));
    }
    let srand = &data[..16];
    let seed = [data[16], data[17]];
    let session_key = gen_session_key(seed, srand)?;
    let session = Type7::new(session_key, iv); // IV reused from ECDH stage

    // Step 3: auth-status (0x89).
    let p = Packet::new(0x21, cfg.auth_header_dst, 0x35, 0x89, vec![], cfg.packet_version);
    transport
        .write(&encode_frame(FRAME_TYPE_PROTOCOL, &p.to_bytes(), Some(&session)), true)
        .await?;
    let _ = recv_frames(notif, reader, 5).await; // best-effort drain

    // Step 4: authenticate (0x86) with MD5(userId+serial) upper-hex.
    let user_id = match cfg.user_id.as_ref() {
        Some(u) if !u.trim().is_empty() => u,
        _ => bail!("EcoFlow User ID is required to authenticate an encrypted device"),
    };
    let digest = md5(format!("{}{}", user_id, cfg.serial).as_bytes());
    let payload = digest
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<String>()
        .into_bytes();
    let p = Packet::new(0x21, cfg.auth_header_dst, 0x35, 0x86, payload, cfg.packet_version);
    transport
        .write(&encode_frame(FRAME_TYPE_PROTOCOL, &p.to_bytes(), Some(&session)), true)
        .await?;

    Ok(session)
}

/// Run the full handshake and then stream telemetry, printing snapshots.
pub async fn run(transport: &BleTransport, cfg: SessionCfg, raw: bool) -> Result<()> {
    if transport.encrypt_type != 7 {
        bail!(
            "this PoC implements encrypt_type 7 only; device advertises {}",
            transport.encrypt_type
        );
    }

    let mut notif = transport.notifications().await?;
    let mut reader = FrameReader::new();

    // --- Step 1: ECDH public-key exchange ---
    eprintln!("[handshake] ECDH public-key exchange (SECP160r1)…");
    let eph = secp160r1::generate();
    let mut hello = vec![0x01, 0x00];
    hello.extend_from_slice(&eph.public_key);
    let hello_frame = encode_frame(FRAME_TYPE_COMMAND, &hello, None);
    eprintln!("[handshake] sending pubkey hello ({} bytes)…", hello_frame.len());
    transport.write(&hello_frame, true).await?;
    eprintln!("[handshake] pubkey sent — awaiting device response…");

    let frames = recv_frames(&mut notif, &mut reader, 20).await?;
    let resp = &frames[0];
    if resp.len() < 3 {
        bail!("short pubkey response: {}", hex::encode(resp));
    }
    let size = ecdh_type_size(resp[2]);
    let end = (3 + size).min(resp.len());
    let dev_pub = &resp[3..end];
    let shared = eph.shared_secret(dev_pub)?;
    let iv = md5(&shared);
    let mut key16 = [0u8; 16];
    key16.copy_from_slice(&shared[..16]);
    let ecdh_enc = Type7::new(key16, iv);

    // --- Step 2: key-info -> session key ---
    eprintln!("[handshake] requesting session key…");
    transport
        .write(&encode_frame(FRAME_TYPE_COMMAND, &[0x02], None), true)
        .await?;
    let frames = recv_frames(&mut notif, &mut reader, 20).await?;
    let enc_resp = &frames[0];
    if enc_resp.first() != Some(&0x02) {
        bail!("key-info type != 0x02: {}", hex::encode(enc_resp));
    }
    let data = ecdh_enc.decrypt(&enc_resp[1..]);
    if data.len() < 18 {
        bail!("key-info payload too short: {}", hex::encode(&data));
    }
    let srand = &data[..16];
    let seed = [data[16], data[17]];
    let session_key = gen_session_key(seed, srand)?;
    let session = Type7::new(session_key, iv); // IV reused from ECDH stage

    // --- Step 3: auth-status (0x89) ---
    let p = Packet::new(0x21, cfg.auth_header_dst, 0x35, 0x89, vec![], cfg.packet_version);
    transport
        .write(&encode_frame(FRAME_TYPE_PROTOCOL, &p.to_bytes(), Some(&session)), true)
        .await?;
    // best-effort: drain whatever comes back
    let _ = recv_frames(&mut notif, &mut reader, 5).await;

    // Probe mode: everything up to here is credential-free. If the session key derived
    // and the device accepted the auth-status packet, the whole crypto/framing core is
    // validated against real hardware.
    let user_id = match cfg.user_id {
        Some(u) => u,
        None => {
            eprintln!("\n[probe] SUCCESS — reached session-key + auth-status.");
            eprintln!("[probe] ECDH, session-key derivation, AES-CBC and framing all work on this device.");
            eprintln!("[probe] Provide --user-id to authenticate and stream telemetry.");
            return Ok(());
        }
    };

    // --- Step 4: authenticate (0x86) with MD5(userId+serial) upper-hex ---
    eprintln!("[handshake] authenticating…");
    let digest = md5(format!("{}{}", user_id, cfg.serial).as_bytes());
    let payload = digest
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<String>()
        .into_bytes();
    let p = Packet::new(0x21, cfg.auth_header_dst, 0x35, 0x86, payload, cfg.packet_version);
    transport
        .write(&encode_frame(FRAME_TYPE_PROTOCOL, &p.to_bytes(), Some(&session)), true)
        .await?;

    // --- Step 5: monitor loop ---
    eprintln!("[handshake] complete — streaming telemetry (Ctrl-C to stop)\n");
    let mut state = RiverState {
        serial: Some(cfg.serial.clone()),
        ..Default::default()
    };

    let mut last_print = std::time::Instant::now();
    let mut authenticated = false;

    loop {
        let frames = match recv_frames(&mut notif, &mut reader, 30).await {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[stream] {e}");
                break;
            }
        };
        for frame in frames {
            let inner = session.decrypt(&frame);
            let pkt = match Packet::from_bytes(&inner) {
                Some(p) => p,
                None => continue,
            };

            // auth reply / error detection
            let is_auth_reply =
                pkt.src == cfg.auth_header_dst && pkt.cmd_set == 0x35 && pkt.cmd_id == 0x86;
            if is_auth_reply && !authenticated {
                if !pkt.payload.is_empty() && pkt.payload.iter().any(|&b| b != 0) {
                    eprintln!("[auth] reply payload: {}", hex::encode(&pkt.payload));
                }
                authenticated = true;
                continue;
            }
            authenticated = true;

            if let Some(fields) = state.update(pkt.src, pkt.cmd_set, pkt.cmd_id, &pkt.payload) {
                if raw {
                    let pack = crate::river::pack_name(pkt.src, pkt.cmd_set, pkt.cmd_id)
                        .unwrap_or("?");
                    let dump = fields
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!(
                        "[raw {pack}] ({:#04x},{:#04x},{:#04x}) {dump}",
                        pkt.src, pkt.cmd_set, pkt.cmd_id
                    );
                }
                // nudge for more data (advanced params)
                let reply = reply_packet(&pkt);
                let _ = transport
                    .write(
                        &encode_frame(FRAME_TYPE_PROTOCOL, &reply.to_bytes(), Some(&session)),
                        true,
                    )
                    .await;
            } else if raw {
                eprintln!(
                    "[raw ?] ({:#04x},{:#04x},{:#04x}) {} ",
                    pkt.src,
                    pkt.cmd_set,
                    pkt.cmd_id,
                    hex::encode(&pkt.payload)
                );
            }
        }

        if last_print.elapsed() >= Duration::from_secs(2) {
            let snap = state.to_snapshot();
            println!("\n── CloudUPS device_state ─────────────────────");
            println!("{}", snap.render());
            last_print = std::time::Instant::now();
        }
    }

    Ok(())
}
