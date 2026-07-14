<script lang="ts">
  import { devices } from '$lib/stores';
  import { known } from '$lib/knownDevices';
  import type { ScanResult } from '$lib/ipc';
  import DeviceRow from './DeviceRow.svelte';

  let {
    found,
    selected,
    scanning,
    addresses,
    onScan,
    onConnect,
    onDisconnect,
    onLabel,
    onForget,
    onSettings,
    onSelect
  }: {
    found: ScanResult[];
    selected: string | null;
    scanning: boolean;
    addresses: Record<string, string>;
    onScan: () => void;
    onConnect: (r: { address: string; serial: string | null; id: string }) => void;
    onDisconnect: (id: string) => void;
    onLabel: (t: { id: string; serial: string | null; address: string | null; label: string }) => void;
    onForget: (id: string) => void;
    onSettings: () => void;
    onSelect: (id: string) => void;
  } = $props();

  // Known devices, each with any live state overlaid from $devices (keyed by serial).
  let knownRows = $derived(
    Object.values($known).map((k) => ({
      known: k,
      live: $devices[k.id]
    }))
  );

  // Connected devices we do not know yet (offer to save+label them).
  let unknownConnected = $derived(
    Object.values($devices).filter(
      (d) => !$known[d.uid] && d.state !== 'disconnected' && d.state !== 'failed'
    )
  );

  // Scan hits that are neither known nor already connected.
  let discovered = $derived(
    found.filter((r) => {
      const id = r.serial ?? r.address;
      return !$known[id] && !$devices[id];
    })
  );
</script>

<aside class="rail glass">
  <header>
    <span class="brand">⚡ EcoFlow</span>
    <button onclick={onScan} disabled={scanning}>{scanning ? '…' : 'Scan'}</button>
  </header>

  <section>
    <h4>Devices</h4>
    {#each knownRows as { known: k, live } (k.id)}
      <DeviceRow
        id={k.id}
        label={k.label}
        sub={k.serial ?? k.id}
        state={live?.state}
        soc={live?.snapshot?.battery_charge ?? null}
        connected={!!live && live.state !== 'disconnected' && live.state !== 'failed'}
        selected={selected === k.id}
        onSelect={() => onSelect(k.id)}
        onConnect={() => onConnect({ address: k.address ?? '', serial: k.serial, id: k.id })}
        onDisconnect={() => onDisconnect(k.id)}
        onLabel={() => onLabel({ id: k.id, serial: k.serial, address: k.address, label: k.label })}
        onForget={() => onForget(k.id)}
      />
    {/each}
    {#each unknownConnected as d (d.uid)}
      <DeviceRow
        id={d.uid}
        label={d.serial ?? d.uid}
        sub={d.uid}
        state={d.state}
        soc={d.snapshot?.battery_charge ?? null}
        connected={d.state !== 'disconnected' && d.state !== 'failed'}
        selected={selected === d.uid}
        onSelect={() => onSelect(d.uid)}
        onConnect={() => {}}
        onDisconnect={() => onDisconnect(d.uid)}
        onLabel={() => onLabel({ id: d.uid, serial: d.serial ?? null, address: addresses[d.uid] ?? null, label: d.serial ?? d.uid })}
      />
    {/each}
  </section>

  {#if discovered.length}
    <section>
      <h4>Discovered</h4>
      {#each discovered as r (r.address)}
        <DeviceRow
          id={r.serial ?? r.address}
          label={r.name || r.serial || r.address}
          sub={r.serial ?? r.address}
          connected={false}
          selected={false}
          onSelect={() => onConnect({ address: r.address, serial: r.serial, id: r.serial ?? r.address })}
          onConnect={() => onConnect({ address: r.address, serial: r.serial, id: r.serial ?? r.address })}
          onDisconnect={() => {}}
          onLabel={() => onLabel({ id: r.serial ?? r.address, serial: r.serial, address: r.address, label: r.name || r.serial || r.address })}
        />
      {/each}
    </section>
  {/if}

  <footer>
    <button class="settings" onclick={onSettings}>⚙ Settings</button>
  </footer>
</aside>

<style>
  .rail { display: flex; flex-direction: column; gap: 0.5rem; padding: 0.75rem; height: 100%; overflow-y: auto; }
  header { display: flex; align-items: center; justify-content: space-between; }
  .brand { font-weight: 700; }
  h4 { margin: 0.5rem 0 0.25rem; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-dim); }
  footer { margin-top: auto; }
  .settings { width: 100%; }
</style>
