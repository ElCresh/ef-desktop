//! Device runtime: owns one connection actor per device, publishes DeviceEvents on a
//! broadcast channel. Built multi-device-ready; Milestone 1 drives a single device.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::ble::{scan as ble_scan, BleTransport, Found};
use crate::command::{build_plaintext_auth, now_unix, Command};
use crate::model::Snapshot;
use crate::profile::profile_for;
use crate::session::PlaintextSession;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum DeviceEventKind {
    Connecting,
    Online,
    Identified {
        serial: Option<String>,
        encrypt_type: u8,
    },
    Authenticating,
    Authenticated,
    AuthFailed(String),
    Retrying,
    Failed(String),
    Telemetry(Snapshot),
    Disconnected,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceEvent {
    pub uid: String,
    #[serde(flatten)]
    pub kind: DeviceEventKind,
}

pub struct DeviceManager {
    tx: broadcast::Sender<DeviceEvent>,
    // uid -> cancel token for its actor. Serializes connect to placate WinRT.
    actors: Arc<Mutex<HashMap<String, CancellationToken>>>,
    // uid -> control-frame sender for its live session. Present only while connected.
    commands: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>>,
    connect_lock: Arc<Mutex<()>>,
}

impl DeviceManager {
    pub fn new() -> (DeviceManager, broadcast::Receiver<DeviceEvent>) {
        let (tx, rx) = broadcast::channel(256);
        let mgr = DeviceManager {
            tx,
            actors: Arc::new(Mutex::new(HashMap::new())),
            commands: Arc::new(Mutex::new(HashMap::new())),
            connect_lock: Arc::new(Mutex::new(())),
        };
        (mgr, rx)
    }

    pub async fn scan(&self, secs: u64) -> Result<Vec<Found>> {
        ble_scan(secs).await
    }

    pub async fn connect(
        &self,
        address: String,
        serial: Option<String>,
        user_id: Option<String>,
    ) {
        let uid = serial.clone().unwrap_or_else(|| address.clone());
        let cancel = CancellationToken::new();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        {
            let mut actors = self.actors.lock().await;
            if let Some(old) = actors.remove(&uid) {
                old.cancel();
            }
            actors.insert(uid.clone(), cancel.clone());
        }
        {
            let mut commands = self.commands.lock().await;
            commands.insert(uid.clone(), cmd_tx.clone());
        }

        let tx = self.tx.clone();
        let connect_lock = self.connect_lock.clone();
        let actors = self.actors.clone();
        let commands = self.commands.clone();

        tokio::spawn(async move {
            let emit = |kind| {
                let _ = tx.send(DeviceEvent {
                    uid: uid.clone(),
                    kind,
                });
            };
            emit(DeviceEventKind::Connecting);

            // Serialize the connect phase across all actors (WinRT reliability).
            let transport = {
                let _guard = connect_lock.lock().await;
                BleTransport::connect(&address).await
            };
            let transport = match transport {
                Ok(t) => t,
                Err(e) => {
                    emit(DeviceEventKind::Failed(e.to_string()));
                    actors.lock().await.remove(&uid);
                    return;
                }
            };

            emit(DeviceEventKind::Online);

            let serial = serial.or_else(|| transport.serial.clone());
            emit(DeviceEventKind::Identified {
                serial: serial.clone(),
                encrypt_type: transport.encrypt_type,
            });
            let profile = profile_for(serial.as_deref(), transport.encrypt_type);

            // River 2 speaks the plaintext AA protocol for both telemetry and control.
            // Telemetry needs no credentials; control replays the app's plaintext login
            // (an 0x35/0xA8 packet carrying the account User ID) when one is available.
            // The encrypted ECDH handshake does not apply to this device (proven by the
            // HCI capture), so the session is always plaintext here.
            let account_id = user_id.clone().filter(|s| !s.trim().is_empty());
            if let Some(id) = account_id {
                // Best effort: queue the login as the first frame the session writes.
                // The device has no plaintext auth ack we parse yet, so this is optimistic.
                emit(DeviceEventKind::Authenticating);
                let _ = cmd_tx.send(build_plaintext_auth(&id, now_unix()));
                emit(DeviceEventKind::Authenticated);
            }

            let snapshot_sink = {
                let tx = tx.clone();
                let uid = uid.clone();
                move |snap: Snapshot| {
                    let _ = tx.send(DeviceEvent {
                        uid: uid.clone(),
                        kind: DeviceEventKind::Telemetry(snap),
                    });
                }
            };

            let result = PlaintextSession::run(
                &transport,
                profile.as_ref(),
                serial,
                snapshot_sink,
                cmd_rx,
                cancel.clone(),
            )
            .await;

            if let Err(e) = result {
                emit(DeviceEventKind::Failed(e.to_string()));
            }
            transport.disconnect().await;
            emit(DeviceEventKind::Disconnected);
            actors.lock().await.remove(&uid);
            commands.lock().await.remove(&uid);
        });
    }

    /// Send a control command to a connected device. Errors if the device has no live
    /// session (never connected, or already disconnected).
    pub async fn send_command(&self, uid: &str, command: Command) -> Result<()> {
        let frame = command.to_frame();
        let commands = self.commands.lock().await;
        match commands.get(uid) {
            Some(tx) => tx
                .send(frame)
                .map_err(|_| anyhow!("device {uid} session is not accepting commands")),
            None => Err(anyhow!("device {uid} is not connected")),
        }
    }

    pub async fn disconnect(&self, uid: &str) {
        self.commands.lock().await.remove(uid);
        if let Some(token) = self.actors.lock().await.remove(uid) {
            token.cancel();
        }
    }
}

#[cfg(test)]
mod device_event_wire_shape_tests {
    use super::*;

    #[test]
    fn connecting_serializes_to_flat_shape_without_data() {
        let event = DeviceEvent {
            uid: "u1".to_string(),
            kind: DeviceEventKind::Connecting,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json, serde_json::json!({"uid": "u1", "kind": "Connecting"}));
    }

    #[test]
    fn failed_serializes_to_flat_shape_with_string_data() {
        let event = DeviceEvent {
            uid: "u2".to_string(),
            kind: DeviceEventKind::Failed("boom".to_string()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["uid"], "u2");
        assert_eq!(json["kind"], "Failed");
        assert_eq!(json["data"], "boom");
    }

    #[test]
    fn telemetry_serializes_to_flat_shape_with_snapshot_data() {
        let mut snapshot = Snapshot::default();
        snapshot.battery_charge = Some(50.0);
        let event = DeviceEvent {
            uid: "u3".to_string(),
            kind: DeviceEventKind::Telemetry(snapshot),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "Telemetry");
        assert!(json["data"].is_object());
        assert_eq!(json["data"]["battery_charge"], 50.0);
    }

    #[test]
    fn identified_serializes_to_flat_shape_with_struct_data() {
        let event = DeviceEvent {
            uid: "u4".to_string(),
            kind: DeviceEventKind::Identified {
                serial: Some("ABC123".to_string()),
                encrypt_type: 7,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "uid": "u4",
                "kind": "Identified",
                "data": {"serial": "ABC123", "encrypt_type": 7}
            })
        );
    }

    #[test]
    fn identified_serializes_null_serial_when_unresolved() {
        let event = DeviceEvent {
            uid: "u5".to_string(),
            kind: DeviceEventKind::Identified {
                serial: None,
                encrypt_type: 7,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "uid": "u5",
                "kind": "Identified",
                "data": {"serial": null, "encrypt_type": 7}
            })
        );
    }
}
