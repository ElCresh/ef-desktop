# EF Desktop

A cross-platform desktop app to monitor and control EcoFlow power stations over
Bluetooth LE. Built with Tauri (Rust core + web frontend) and SvelteKit.

The BLE protocol is reverse-engineered — this is an unofficial project, not affiliated
with or endorsed by EcoFlow.

## What it does

- Live telemetry: battery %, input/output power and voltage, temperature, runtime.
- Control: AC output, X-Boost, DC (12V) output, DC input mode (Auto/Solar/Car), AC charge
  speed, car input current, charge/discharge SoC limits, and standby/screen timeouts.
- Reads the current value of every setting back from the telemetry stream, so controls
  reflect the device's real state instead of guesses.
- Multiple devices in one window, with persistent user labels (e.g. "EcoFlow Camper").
- Dark / light / auto (follows the OS) glass theme.

BLE only for now. WiFi/cloud is out of scope.

## Tested device
- River 2 Max (`R611…`)

Help us test on more device :)

## Project layout

```
.
├── Cargo.toml              Rust workspace (ef-core + the Tauri app crate)
├── package.json            Frontend package (ef-desktop) and scripts
├── src/                    SvelteKit frontend (Svelte 5 runes, adapter-static SPA)
│   ├── app.css             Glass design tokens (dark/light/auto)
│   ├── app.html            Shell + pre-paint theme script
│   ├── routes/             +layout.svelte (theme), +page.svelte (app shell)
│   └── lib/
│       ├── ipc.ts          Typed wrappers over the Tauri commands + event stream
│       ├── stores.ts       Device view-model reduced from device events
│       ├── theme.ts        Theme store (localStorage, data-theme)
│       ├── knownDevices.ts Persisted labelled devices (frontend store)
│       └── components/     DeviceRail, DeviceRow, DeviceDetail, TelemetryGrid,
│                           ControlsPanel, SettingsView, LabelDialog
├── src-tauri/              Tauri app crate (`app`)
│   ├── tauri.conf.json     Product name / identifier / window
│   └── src/
│       ├── main.rs         IPC commands (scan, connect, send_command, credentials, devices)
│       └── store.rs        Local JSON persistence (credentials, known devices)
├── crates/
│   └── ef-core/            BLE protocol + async device runtime (library crate)
│       └── src/            ble, session, manager, packet, encpacket, crypto, auth,
│                           cloud, command, model, profile, river, crc, secp160r1, keydata
├── doc/ef-ble-protocol/    Decoded BLE protocol reference (documentation)
├── captures/               BLE reverse-engineering: ANALYSIS.md + snoop-log parsers
└── docs/superpowers/       Design specs and implementation plans
```

The `ef-core` crate is standalone (no Tauri dependency) and holds all the BLE and
protocol logic; `src-tauri` is a thin IPC layer over it, and the frontend is pure UI.
The decoded protocol is documented in [`doc/ef-ble-protocol/`](doc/ef-ble-protocol/README.md).

## Prerequisites

- **Rust** (stable, 1.77+) with Cargo.
- **Node.js** 18+ and npm.
- **Tauri CLI v2**: `cargo install tauri-cli --version "^2"` (provides `cargo tauri …`).
- A Bluetooth LE adapter.
- Platform Webview / BLE stack:
  - **Windows**: WebView2 runtime (preinstalled on Windows 11); BLE via WinRT.
  - **Linux**: `webkit2gtk`, `libayatana-appindicator`, and BlueZ (`bluez`) with a running
    `bluetooth` service.

## Development

```bash
npm install
cargo tauri dev        # or: npm run tauri dev if you add the wrapper script
```

`cargo tauri dev` runs the Vite dev server (`npm run dev`) and the Tauri shell together
with hot reload.

## Build

```bash
cargo tauri build
```

Outputs:
- the executable at `target/release/app.exe` (the Rust crate is named `app`),
- installers under `target/release/bundle/` — on Windows an MSI and an NSIS
  `EF Desktop_<version>_x64-setup.exe`.

## Tests and checks

```bash
cargo test        # Rust: protocol codec, command encoders, telemetry decode, persistence
npm run check     # Frontend: svelte-check (types) — must be 0 errors
```

## Notes

- Windows BLE (WinRT) can be unreliable with these devices; the transport retries the
  connection and bounds each attempt. A Linux/BlueZ host is the more reliable path.
- Telemetry and control on River 2 need no credentials. The EcoFlow account User ID is
  optional (used only for the plaintext login the official app sends); it is not required
  for control.
- The decoded protocol, packet layout, command set and telemetry offsets are documented in
  [`doc/ef-ble-protocol/README.md`](doc/ef-ble-protocol/README.md).

## License

GPL-3.0-or-later.
