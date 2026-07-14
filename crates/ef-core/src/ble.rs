//! BLE transport (btleplug): scan for EcoFlow devices, connect, notify + write.
//!
//! EcoFlow advertises a manufacturer-data record (`_ScanRecordV2`) carrying the serial
//! and capability flags (from which `encrypt_type` is derived). The GATT service is
//! either "nordic_uart" or an "rfcomm"-style pair of write/notify characteristics.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use btleplug::api::{
    CharPropFlags, Central, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::stream::Stream;
use uuid::Uuid;

// nordic_uart
const NORDIC_WRITE: Uuid = Uuid::from_u128(0x6e400002_b5a3_f393_e0a9_e50e24dcca9e);
const NORDIC_NOTIFY: Uuid = Uuid::from_u128(0x6e400003_b5a3_f393_e0a9_e50e24dcca9e);
// rfcomm-style
const RFCOMM_WRITE: Uuid = Uuid::from_u128(0x00000002_0000_1000_8000_00805f9b34fb);
const RFCOMM_NOTIFY: Uuid = Uuid::from_u128(0x00000003_0000_1000_8000_00805f9b34fb);
// Standard GATT "Service Changed" (Generic Attribute Service). WinRT owns its indication
// and denies app-level subscribe (HRESULT 0x80070005), so skip it: it is never a telemetry
// source and only produces a misleading warning.
const GATT_SERVICE_CHANGED: Uuid = Uuid::from_u128(0x00002a05_0000_1000_8000_00805f9b34fb);

/// One discovered EcoFlow device.
#[derive(Debug, Clone)]
pub struct Found {
    pub address: String,
    pub name: String,
    pub serial: Option<String>,
    pub encrypt_type: u8,
    pub product_type: u8,
}

/// Parse the EcoFlow `_ScanRecordV2` from a manufacturer-data value.
fn parse_scan_record(md: &[u8]) -> Option<(String, u8, u8)> {
    if md.len() < 17 {
        return None;
    }
    let serial = std::str::from_utf8(&md[1..17]).ok()?.trim_end_matches('\0');
    // serials are printable ascii, EcoFlow River prefixes start with 'R'
    if !serial.chars().all(|c| c.is_ascii_graphic()) || serial.len() < 4 {
        return None;
    }
    let product_type = if md.len() > 18 { md[18] } else { 0 };
    let capability_flags = if md.len() > 22 { md[22] } else { 0b0111000 };
    let encrypt_type = (capability_flags & 0b0111000) >> 3;
    Some((serial.to_string(), encrypt_type, product_type))
}

fn best_scan_record(md: &HashMap<u16, Vec<u8>>) -> Option<(String, u8, u8)> {
    md.values().find_map(|v| parse_scan_record(v))
}

async fn get_adapter() -> Result<Adapter> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    adapters
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no Bluetooth adapter found"))
}

/// Scan for `secs` seconds and return EcoFlow devices (deduped by address).
pub async fn scan(secs: u64) -> Result<Vec<Found>> {
    let adapter = get_adapter().await?;
    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(Duration::from_secs(secs)).await;

    let mut out: Vec<Found> = Vec::new();
    for p in adapter.peripherals().await? {
        let props = match p.properties().await? {
            Some(pr) => pr,
            None => continue,
        };
        let name = props.local_name.clone().unwrap_or_default();
        let record = best_scan_record(&props.manufacturer_data);
        if name.starts_with("EF-") {
            for (cid, val) in &props.manufacturer_data {
                eprintln!(
                    "[debug] {} manufacturer_data company=0x{cid:04x} ({} bytes): {}",
                    p.address(),
                    val.len(),
                    hex::encode(val)
                );
            }
            eprintln!("[debug] {} service_data: {:?}", p.address(), props.service_data);
        }
        let looks_ecoflow = name.starts_with("EF-")
            || record
                .as_ref()
                .map(|(sn, _, _)| sn.starts_with('R') || sn.starts_with("EF"))
                .unwrap_or(false);
        if !looks_ecoflow {
            continue;
        }
        let (serial, encrypt_type, product_type) = match record {
            Some((s, e, pt)) => (Some(s), e, pt),
            None => (None, 7, 0),
        };
        out.push(Found {
            address: p.address().to_string(),
            name,
            serial,
            encrypt_type,
            product_type,
        });
    }
    let _ = adapter.stop_scan().await;
    Ok(out)
}

/// A connected EcoFlow BLE device with resolved write/notify characteristics.
pub struct BleTransport {
    peripheral: Peripheral,
    write_char: btleplug::api::Characteristic,
    notify_char: btleplug::api::Characteristic,
    pub serial: Option<String>,
    pub encrypt_type: u8,
}

impl BleTransport {
    /// Connect to a device by BLE address (must be scanned first this session).
    pub async fn connect(address: &str) -> Result<BleTransport> {
        let adapter = get_adapter().await?;
        // Keep scanning through the whole connect: on WinRT, stopping the scan can
        // invalidate the peripheral handle and make connect() hang or fail.
        adapter.start_scan(ScanFilter::default()).await?;
        tokio::time::sleep(Duration::from_secs(8)).await;

        let mut target: Option<Peripheral> = None;
        let mut serial = None;
        let mut encrypt_type = 7u8;
        for p in adapter.peripherals().await? {
            if p.address().to_string().eq_ignore_ascii_case(address) {
                if let Some(props) = p.properties().await? {
                    if let Some((s, e, _)) = best_scan_record(&props.manufacturer_data) {
                        serial = Some(s);
                        encrypt_type = e;
                    }
                }
                target = Some(p);
                break;
            }
        }
        let peripheral = target.ok_or_else(|| anyhow!("device {address} not found in scan"))?;

        // WinRT can return from connect() before the link is ready, and a device that is
        // already connected elsewhere (the phone app) rejects us. Retry and verify.
        let mut connected = false;
        'attempts: for attempt in 1..=4 {
            eprintln!("[connect] attempt {attempt}/4…");
            // Bound connect() so a WinRT hang fails fast and we can retry.
            match tokio::time::timeout(Duration::from_secs(12), peripheral.connect()).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("[connect] attempt {attempt} error: {e}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                Err(_) => {
                    eprintln!("[connect] attempt {attempt} timed out (12s)");
                    continue;
                }
            }
            for _ in 0..15 {
                if peripheral.is_connected().await.unwrap_or(false) {
                    connected = true;
                    break 'attempts;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            eprintln!("[connect] attempt {attempt}: link did not settle, retrying…");
        }
        let _ = adapter.stop_scan().await;
        if !connected {
            bail!(
                "could not establish a BLE connection to {address} after 4 attempts. This is a \
                 Windows/WinRT BLE limitation with this device; a Linux/BlueZ host is the reliable path."
            );
        }
        // let the GATT stack settle before service discovery
        tokio::time::sleep(Duration::from_millis(600)).await;
        peripheral.discover_services().await?;

        let chars = peripheral.characteristics();
        eprintln!("[info] discovered {} characteristics:", chars.len());
        for c in &chars {
            eprintln!("[info]   {} props {:?}", c.uuid, c.properties);
        }

        // Fallback: many devices expose the serial via the standard Device Information
        // Service (Serial Number String, 0x2A25) — readable without EcoFlow auth.
        if serial.is_none() {
            const DIS_SERIAL: Uuid = Uuid::from_u128(0x00002a25_0000_1000_8000_00805f9b34fb);
            if let Some(c) = chars.iter().find(|c| c.uuid == DIS_SERIAL) {
                if let Ok(val) = peripheral.read(c).await {
                    if let Ok(s) = std::str::from_utf8(&val) {
                        let s = s.trim_end_matches('\0').trim();
                        if !s.is_empty() {
                            eprintln!("[info] serial from Device Information Service: {s}");
                            serial = Some(s.to_string());
                        }
                    }
                }
            }
        }

        let find = |u: Uuid| chars.iter().find(|c| c.uuid == u).cloned();
        let (write_char, notify_char) = if let (Some(w), Some(n)) =
            (find(NORDIC_WRITE), find(NORDIC_NOTIFY))
        {
            (w, n)
        } else if let (Some(w), Some(n)) = (find(RFCOMM_WRITE), find(RFCOMM_NOTIFY)) {
            (w, n)
        } else {
            bail!(
                "no known write/notify characteristics; available: {:?}",
                chars.iter().map(|c| c.uuid).collect::<Vec<_>>()
            );
        };

        eprintln!(
            "[info] write char  {} props {:?}",
            write_char.uuid, write_char.properties
        );
        eprintln!(
            "[info] notify char {} props {:?}",
            notify_char.uuid, notify_char.properties
        );

        // Subscribe to every notify/indicate characteristic, so a response on any of
        // them is captured (the device may not reply on the one paired with write).
        for c in &chars {
            if c.uuid == GATT_SERVICE_CHANGED {
                continue;
            }
            if c.properties
                .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
            {
                match peripheral.subscribe(c).await {
                    Ok(()) => eprintln!("[info] subscribed to {}", c.uuid),
                    Err(e) => eprintln!("[warn] subscribe {} failed: {e}", c.uuid),
                }
            }
        }

        Ok(BleTransport {
            peripheral,
            write_char,
            notify_char,
            serial,
            encrypt_type,
        })
    }

    pub async fn write(&self, data: &[u8], prefer_response: bool) -> Result<()> {
        let props = self.write_char.properties;
        // WinRT hangs on write-with-response if the characteristic only supports
        // write-without-response — pick a type the characteristic actually advertises.
        let kind = if prefer_response && props.contains(CharPropFlags::WRITE) {
            WriteType::WithResponse
        } else if props.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
            WriteType::WithoutResponse
        } else {
            WriteType::WithResponse
        };
        match tokio::time::timeout(
            Duration::from_secs(8),
            self.peripheral.write(&self.write_char, data, kind),
        )
        .await
        {
            Ok(r) => r?,
            Err(_) => bail!(
                "BLE write timed out (8s) on {} ({:?})",
                self.write_char.uuid,
                kind
            ),
        }
        Ok(())
    }

    /// Notification stream of raw BLE payloads from the notify characteristic.
    pub async fn notifications(
        &self,
    ) -> Result<std::pin::Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        Ok(self.peripheral.notifications().await?)
    }

    pub async fn disconnect(&self) {
        let _ = self.peripheral.unsubscribe(&self.notify_char).await;
        let _ = self.peripheral.disconnect().await;
    }
}
