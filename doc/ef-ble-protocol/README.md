# EcoFlow River 2 BLE protocol

Documentation of the Bluetooth LE protocol behind **EF Desktop**, reverse-engineered from
the traffic of the official EcoFlow Android app. It is implemented in the `ef-core` crate
(`crates/ef-core/`).

**Reverse-engineered, unofficial.** Everything here was recovered from an Android HCI snoop
capture (`adb bugreport` → `btsnoop_hci.log`) of a **River 2 Max** (serial prefix `R611…`)
and confirmed by correlating each app command with the byte that changed in the telemetry
stream. The parsers and the raw analysis are in `../../captures/` (`ANALYSIS.md`,
`parse_snoop.py`, `decode_tx.py`, `analyze_state.py`). Other models may differ.

## Two framings

Devices speak one of two framings on the GATT link:

- **Plaintext `0xAA`** — protocol V2/V3. This is what River 2 uses for **both** telemetry
  and control. No encryption, no key exchange.
- **Encrypted `0x5A5A`** — an ECDH (secp160r1) + AES-CBC session. Implemented in `ef-core`
  (`crypto`, `encpacket`, `auth`, `secp160r1`) for future protobuf-based devices
  (River 3 / Delta). River 2 does **not** use it; the code path is unproven on hardware.

The rest of this document describes the plaintext protocol, which is the one validated on
River 2 Max.

## GATT transport

EcoFlow advertises a manufacturer-data scan record carrying the serial (bytes 1..17, ASCII)
and capability flags. Two characteristic layouts are seen; the transport uses whichever is
present:

- Nordic UART — write `6e400002-…`, notify `6e400003-…`
- RFCOMM-style — write `00000002-0000-1000-8000-00805f9b34fb`, notify `00000003-…`

Commands are written as ATT Write-Without-Response; telemetry arrives as notifications.
A single notification may contain several packets, and a packet may span notifications, so
the reader reassembles by scanning for the `0xAA` prefix and using the length field.

## Packet layout (V2 / V3)

```
AA | ver(1) | len(2 LE) | crc8(header, 4B) | product(1) | seq(4) | 00 00
   | src | dst | [dsrc | ddst  (V3+ only)] | cmd_set | cmd_id | payload | crc16(2 LE)
```

- `len` is the payload length (little-endian).
- `crc8` covers the first 4 bytes (`AA ver len`); `crc16` covers everything except the
  trailing 2 CRC bytes. Both are the EcoFlow variants in `ef-core`'s `crc.rs`
  (CRC-8 poly 0x07; CRC-16/MODBUS).
- `product` is `0x0d` for the app's writes.
- `seq` is `00 00 00 00` on every app write. When `seq[0] != 0`, the payload is XORed with
  `seq[0]`; with a zero seq this is a no-op.
- V2 has no `dsrc/ddst`; V3 inserts them before `cmd_set`.
- Addresses seen: host `0x21`; PD / inverter module `0x05`; BMS `0x03`; and telemetry
  sources `0x02`–`0x05`.

`Packet::to_bytes` / `Packet::from_bytes` implement this, and `Command::to_frame` reproduces
the captured control frames byte-for-byte (there are golden-frame tests in `command.rs`).

## Login (optional)

Before controlling, the app sends a plaintext login. **It is not required** — control and
telemetry both work without it (confirmed live: AC toggled with no User ID sent). The client
replays it only when a User ID is available.

```
V3   src=0x21  dst=0x35  cmd_set=0x35  cmd_id=0xA8
payload = 01 | userId (ASCII, right-padded to 64 bytes with 0x00) | unixTs (u32 LE)
```

## Control commands

All are V2, from the host (`src=0x21`), `cmd_set=0x20`. `dst=0x05` targets the PD/inverter
module; the state-of-charge limits target the BMS at `dst=0x03`. Each was confirmed by
driving the matching control in the app and matching the value in the payload.

| Function | dst | cmd_id | payload |
|----------|-----|--------|---------|
| AC output + X-Boost | 0x05 | 0x42 | `[ac, xboost, ff, ff, ff, ff, ff]` |
| DC (12V) output | 0x05 | 0x51 | `[state]` |
| DC input mode | 0x05 | 0x52 | `[mode]` — 0=Auto, 1=Solar, 2=Car |
| AC charge speed (W) | 0x05 | 0x45 | `[watts u16 LE, ff]` |
| Car / DC input current (mA) | 0x05 | 0x47 | `[milliamps u32 LE]` |
| Charge limit (max SoC %) | 0x03 | 0x31 | `[percent]` |
| Discharge limit (min SoC %) | 0x03 | 0x33 | `[percent]` |
| Unit standby timeout (min) | 0x05 | 0x21 | `[minutes u16 LE]` |
| Screen (LCD) timeout (sec) | 0x05 | 0x27 | `[seconds u16 LE, ff]` |
| AC standby timeout (min) | 0x05 | 0x99 | `[minutes u16 LE]` |
| Set RTC | 0x05 | 0x22 | `[year u16 LE, month, day, hour, minute, second, weekday]` |

Notes:

- `0x42` is a combined AC-config command: byte 0 is the AC-output flag, byte 1 the X-Boost
  flag, and `0xff` in a slot means "leave that setting unchanged" — which is why AC and
  X-Boost share one cmd_id. AC on = `01 ff ff ff ff ff ff`; X-Boost on = `ff 01 ff ff ff ff ff`.
- State bytes are `00` = off, `01` = on.

Examples (payload → meaning): `f4 01 ff` = AC charge 500 W; `70 17 00 00` = 6000 mA (6 A);
`a0 05` = 1440 min (24 h); `08 07 ff` = 1800 s (30 min); `f0 00` = 240 min (4 h).

## Telemetry

The device streams fixed-width little-endian heartbeat packs (not protobuf). Recognised
routes `(src, cmd_set, cmd_id)` and the fields decoded in `river.rs`:

| Route | Pack | Notable fields (payload offset) |
|-------|------|---------------------------------|
| `0x02, 0x20, 0x02` | PD | soc @14, wattsOut @15 (u16), wattsIn @17 (u16) |
| `0x03, 0x20, 0x02` | EMS | maxChargeSoc @12, minDsgSoc @43, chg/dsg remaining time |
| `0x03, 0x20, 0x32` | BMS | f32 showSoc @53, temp @20, in/out watts |
| `0x04, *, 0x02`    | INV | input/output watts, AC in/out voltage & frequency |
| `0x05, 0x20, 0x02` | config | AC/DC/X-Boost state, charge/input limits, timeouts |

### Reading current settings back

So the UI shows real values instead of guesses, each setting is read from the heartbeat.
Offsets are payload-relative (after `cmd_id`) and were pinned by diffing the telemetry
against the control write that changed each one (`analyze_state.py`): every value change
lands on the command that caused it.

EMS pack `0x03, 0x20, 0x02`:

| Setting | offset | type |
|---------|--------|------|
| Charge limit (max SoC %) | 12 | u8 |
| Discharge limit (min SoC %) | 43 | u8 |

Config pack `0x05, 0x20, 0x02`:

| Setting | offset | type |
|---------|--------|------|
| DC input mode | 31 | u8 |
| DC output state | 56 | u8 (0/1) |
| Car input current (mA) | 61 | u16 LE |
| AC output state | 66 | u8 (0/1) |
| X-Boost state | 67 | u8 (0/1) |
| AC charge speed (W) | 73 | u16 LE |
| AC standby timeout (min) | 75 | u16 LE |
| Unit standby timeout (min) | 80 | u16 LE |
| Screen timeout (sec) | 82 | u16 LE |

## Where it lives in the code (`crates/ef-core/`)

- `ble` — scan, connect, GATT characteristic resolution, notify/write.
- `packet` — the `0xAA` V2/V3 codec and the streaming reassembler.
- `crc` — CRC-8 and CRC-16 used by the framing.
- `command` — the `Command` enum, frame builders, and the plaintext login builder.
- `river` — telemetry heartbeat decoding and the `DeviceSettings` readback.
- `model` — the normalised `Snapshot` and `DeviceSettings`.
- `session` — the live read/control loop (plaintext), plus the encrypted session.
- `manager` — the async device runtime: one actor per device, events on a broadcast channel.
- `profile` — per-model decode selection.
- `cloud` — EcoFlow account login to resolve the User ID (for the optional plaintext auth).
- `encpacket`, `crypto`, `auth`, `secp160r1`, `keydata` — the `0x5A5A` encrypted path
  (future devices; not used by River 2).

## Model coverage

Validated on **River 2 Max (`R611…`)**. The command IDs, offsets and enum values above are
specific to that firmware. Other River 2 variants likely match; River 3 / Delta use a
different (protobuf, encrypted) protocol and are not covered by the plaintext tables here.
