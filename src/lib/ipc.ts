import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface ScanResult {
  address: string;
  name: string;
  serial: string | null;
  encrypt_type: number;
  product_type: number;
}

export interface DeviceSettings {
  ac_enabled: boolean | null;
  xboost_enabled: boolean | null;
  dc_enabled: boolean | null;
  dc_mode: number | null;
  ac_charge_watts: number | null;
  car_input_ma: number | null;
  charge_limit: number | null;
  discharge_limit: number | null;
  unit_timeout_min: number | null;
  screen_timeout_sec: number | null;
  ac_timeout_min: number | null;
}

export interface Snapshot {
  device_uid: string | null;
  battery_charge: number | null;
  input_voltage: number | null;
  output_voltage: number | null;
  load_pct: number | null;
  battery_runtime_s: number | null;
  input_power_w: number | null;
  output_power_w: number | null;
  input_frequency: number | null;
  temperature: number | null;
  status_flags: number;
  settings: DeviceSettings;
}

export type DeviceEventKind =
  | { kind: 'Connecting' }
  | { kind: 'Online' }
  | { kind: 'Retrying' }
  | { kind: 'Failed'; data: string }
  | { kind: 'Telemetry'; data: Snapshot }
  | { kind: 'Disconnected' }
  | { kind: 'Authenticating' }
  | { kind: 'Authenticated' }
  | { kind: 'AuthFailed'; data: string }
  | { kind: 'Identified'; data: { serial: string | null; encrypt_type: number } };

export interface DeviceEvent {
  uid: string;
  kind: DeviceEventKind['kind'];
  data?: unknown;
}

export interface SavedCreds {
  user_id: string | null;
  email: string | null;
  region: string;
}

export interface KnownDevice {
  id: string;
  label: string;
  serial: string | null;
  address: string | null;
}

// Mirrors ef_core::command::Command (serde tag "kind", content "value").
export type Command =
  | { kind: 'AcOutput'; value: boolean }
  | { kind: 'XBoost'; value: boolean }
  | { kind: 'DcOutput'; value: boolean }
  | { kind: 'DcMode'; value: number }
  | { kind: 'AcChargeWatts'; value: number }
  | { kind: 'CarInputMilliamps'; value: number }
  | { kind: 'ChargeLimit'; value: number }
  | { kind: 'DischargeLimit'; value: number }
  | { kind: 'UnitTimeoutMinutes'; value: number }
  | { kind: 'ScreenTimeoutSeconds'; value: number }
  | { kind: 'AcTimeoutMinutes'; value: number };

export const scan = (secs: number) => invoke<ScanResult[]>('scan', { secs });
export const connect = (address: string, serial: string | null, userId: string | null) =>
  invoke<void>('connect', { address, serial, userId });
export const disconnect = (uid: string) => invoke<void>('disconnect', { uid });
export const sendCommand = (uid: string, command: Command) =>
  invoke<void>('send_command', { uid, command });
export const fetchUserId = (email: string, password: string, region: string) =>
  invoke<string>('fetch_user_id', { email, password, region });
export const savedCredentials = () => invoke<SavedCreds>('saved_credentials');
export const knownDevices = () => invoke<KnownDevice[]>('known_devices');
export const saveKnownDevice = (device: KnownDevice) =>
  invoke<void>('save_known_device', { device });
export const renameKnownDevice = (id: string, label: string) =>
  invoke<void>('rename_known_device', { id, label });
export const forgetDevice = (id: string) => invoke<void>('forget_device', { id });

export const onDeviceEvent = (cb: (e: DeviceEvent) => void): Promise<UnlistenFn> =>
  listen<DeviceEvent>('device-event', (e) => cb(e.payload));
