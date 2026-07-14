import { writable } from 'svelte/store';
import type { Snapshot } from './ipc';

export interface DeviceView {
  uid: string;
  state: 'connecting' | 'online' | 'retrying' | 'failed' | 'disconnected';
  error?: string;
  snapshot?: Snapshot;
  auth?: 'authenticating' | 'authenticated' | 'failed';
  authError?: string;
  serial?: string | null;
  encryptType?: number;
}

export const devices = writable<Record<string, DeviceView>>({});

export function applyEvent(uid: string, kind: string, data: unknown) {
  devices.update((all) => {
    // A disconnect (user-initiated or a dropped link) retires the live view entirely.
    // Keeping a stale entry would (a) leave the detail panel showing the last telemetry
    // snapshot, and (b) hide the device from the rail's "discovered" list — which excludes
    // any address already present here — so a rescan would not surface it until an app
    // restart cleared this store.
    if (kind === 'Disconnected') {
      const next = { ...all };
      delete next[uid];
      return next;
    }

    // Copy into a fresh object so the per-device reference changes on every event.
    // Svelte 5's fine-grained reactivity is reference-based for plain objects: mutating
    // the existing DeviceView in place would leave `$derived($devices[uid])` returning the
    // same reference, so the UI would freeze on the first event's state (e.g. "connecting")
    // and never reflect later Online/Telemetry updates.
    const d: DeviceView = { ...(all[uid] ?? { uid, state: 'connecting' }) };
    switch (kind) {
      case 'Connecting':
        d.state = 'connecting';
        break;
      case 'Online':
        d.state = 'online';
        break;
      case 'Retrying':
        d.state = 'retrying';
        break;
      case 'Failed':
        d.state = 'failed';
        d.error = data as string;
        break;
      case 'Telemetry':
        d.state = 'online';
        d.snapshot = data as Snapshot;
        break;
      case 'Authenticating':
        d.auth = 'authenticating';
        break;
      case 'Authenticated':
        d.auth = 'authenticated';
        break;
      case 'AuthFailed':
        d.auth = 'failed';
        d.authError = data as string;
        break;
      case 'Identified': {
        const info = data as { serial: string | null; encrypt_type: number };
        d.serial = info.serial;
        d.encryptType = info.encrypt_type;
        break;
      }
    }
    return { ...all, [uid]: d };
  });
}
