//! Live telemetry sessions. The connect/handshake mechanics are unchanged from the PoC;
//! the difference is that decoded snapshots are pushed to a callback and the loop stops
//! when its CancellationToken fires, so the DeviceManager can own the connection.

use std::time::Duration;

use anyhow::{bail, Result};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::auth::{handshake_encrypted, recv_frames, reply_packet, SessionCfg};
use crate::ble::BleTransport;
use crate::encpacket::{encode_frame, FrameReader, FRAME_TYPE_PROTOCOL};
use crate::model::Snapshot;
use crate::packet::{Packet, PacketReader};
use crate::profile::{Profile, TelemetryDecoder};

/// Decode one plaintext inner-packet frame into the decoder. Returns whether the route
/// was recognised. Pure and unit-testable.
pub fn route_plaintext_frame(decoder: &mut dyn TelemetryDecoder, frame: &[u8]) -> bool {
    match Packet::from_bytes(frame) {
        Some(pkt) => {
            let before = crate::river::pack_name(pkt.src, pkt.cmd_set, pkt.cmd_id).is_some();
            decoder.ingest(pkt.src, pkt.cmd_set, pkt.cmd_id, &pkt.payload);
            before
        }
        None => false,
    }
}

/// Plaintext telemetry (encrypt_type 0 devices): no ECDH/auth, just parse the AA packets.
pub struct PlaintextSession;

impl PlaintextSession {
    pub async fn run(
        transport: &BleTransport,
        profile: &dyn Profile,
        serial: Option<String>,
        mut on_snapshot: impl FnMut(Snapshot),
        mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let mut notif = transport.notifications().await?;
        let mut reader = PacketReader::new();
        let mut decoder = profile.new_decoder(serial);
        let mut last_emit = tokio::time::Instant::now();

        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                // Control frames are written on this same task so there is only ever one
                // writer to the transport. Write-without-response mirrors the app's ATT
                // Write Command; a failure is logged but does not tear down the read loop.
                Some(frame) = cmd_rx.recv() => {
                    if let Err(e) = transport.write(&frame, false).await {
                        eprintln!("[control] write failed: {e}");
                    }
                    continue;
                }
                v = tokio::time::timeout(Duration::from_secs(30), notif.next()) => v,
            };
            let v = match next {
                Ok(Some(v)) => v,
                Ok(None) => break,        // stream ended
                Err(_) => break,          // 30s idle
            };
            for frame in reader.push(&v.value) {
                route_plaintext_frame(decoder.as_mut(), &frame);
            }
            if last_emit.elapsed() >= Duration::from_secs(2) {
                on_snapshot(decoder.snapshot());
                last_emit = tokio::time::Instant::now();
            }
        }
        Ok(())
    }
}

/// Encrypted telemetry (encrypt_type 7 devices): ECDH handshake + AES session, then the
/// same decode-and-emit loop as the plaintext path but over decrypted frames. Requires
/// the EcoFlow User ID (in `cfg`) to authenticate.
pub struct EncryptedSession;

impl EncryptedSession {
    pub async fn run(
        transport: &BleTransport,
        profile: &dyn Profile,
        cfg: SessionCfg,
        mut on_snapshot: impl FnMut(Snapshot),
        mut on_authenticated: impl FnMut(),
        cancel: CancellationToken,
    ) -> Result<()> {
        let mut notif = transport.notifications().await?;
        let mut reader = FrameReader::new();
        // Bound the whole handshake. A device that only speaks the plaintext AA protocol
        // (e.g. River 2) never sends the 0x5A5A frames this handshake waits for, and its
        // continuous AA telemetry starves the per-notification timeout inside recv_frames,
        // so without an overall deadline the handshake would spin forever.
        let session = match tokio::time::timeout(
            Duration::from_secs(12),
            handshake_encrypted(transport, &cfg, &mut notif, &mut reader),
        )
        .await
        {
            Ok(r) => r?,
            Err(_) => bail!(
                "encrypted handshake timed out — the device did not respond on the encrypted \
                 channel. This device uses the plaintext protocol (no User ID needed); \
                 falling back to plaintext read."
            ),
        };

        let mut decoder = profile.new_decoder(Some(cfg.serial.clone()));
        let mut last_emit = tokio::time::Instant::now();
        let mut authenticated = false;
        let mut signaled_auth = false;

        loop {
            let frames = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                r = recv_frames(&mut notif, &mut reader, 30) => match r {
                    Ok(f) => f,
                    Err(_) => break, // stream ended or 30s idle
                },
            };
            for frame in frames {
                let inner = session.decrypt(&frame);
                let pkt = match Packet::from_bytes(&inner) {
                    Some(p) => p,
                    None => continue,
                };

                // Signal "authenticated" on the first frame that decrypts and parses.
                // NOTE: the AES session key is ECDH-derived and independent of the User ID
                // (which only enters the 0x86 auth payload), so this proves the encrypted
                // session is up and the device is responding — not that the User ID was
                // accepted for control. M3b promotes this to a true "auth accepted" signal
                // by inspecting the 0x86 reply body.
                if !signaled_auth {
                    signaled_auth = true;
                    on_authenticated();
                }

                // The first echo of our auth packet (0x86) is the auth reply, not data.
                let is_auth_reply =
                    pkt.src == cfg.auth_header_dst && pkt.cmd_set == 0x35 && pkt.cmd_id == 0x86;
                if is_auth_reply && !authenticated {
                    authenticated = true;
                    continue;
                }
                authenticated = true;

                let recognised =
                    crate::river::pack_name(pkt.src, pkt.cmd_set, pkt.cmd_id).is_some();
                decoder.ingest(pkt.src, pkt.cmd_set, pkt.cmd_id, &pkt.payload);
                if recognised {
                    // Nudge the device to keep streaming advanced params.
                    let reply = reply_packet(&pkt);
                    let _ = transport
                        .write(
                            &encode_frame(FRAME_TYPE_PROTOCOL, &reply.to_bytes(), Some(&session)),
                            true,
                        )
                        .await;
                }
            }
            if last_emit.elapsed() >= Duration::from_secs(2) {
                on_snapshot(decoder.snapshot());
                last_emit = tokio::time::Instant::now();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::profile_for;

    #[test]
    fn route_plaintext_frame_feeds_decoder() {
        let profile = profile_for(Some("R601ZEB0000000A"), 0);
        let mut decoder = profile.new_decoder(Some("R601ZEB0000000A".into()));
        // Build a plaintext inner packet (BMS route 0x03,0x20,0x32) carrying f32ShowSoc.
        let mut payload = vec![0u8; 69];
        payload[53..57].copy_from_slice(&90.0f32.to_le_bytes());
        let pkt = crate::packet::Packet::new(0x03, 0x21, 0x20, 0x32, payload, 2);
        let frame = pkt.to_bytes();
        let recognised = route_plaintext_frame(decoder.as_mut(), &frame);
        assert!(recognised);
        assert_eq!(decoder.snapshot().battery_charge, Some(90.0));
    }
}
