import { writable } from 'svelte/store';
import { knownDevices, type KnownDevice } from './ipc';

export const known = writable<Record<string, KnownDevice>>({});

export async function loadKnown() {
  const list = await knownDevices();
  const map: Record<string, KnownDevice> = {};
  for (const d of list) map[d.id] = d;
  known.set(map);
}
