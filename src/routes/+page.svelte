<script lang="ts">
  import { onMount } from 'svelte';
  import {
    onDeviceEvent,
    scan,
    connect,
    disconnect,
    savedCredentials,
    saveKnownDevice,
    renameKnownDevice,
    forgetDevice,
    type ScanResult
  } from '$lib/ipc';
  import { devices, applyEvent } from '$lib/stores';
  import { known, loadKnown } from '$lib/knownDevices';
  import DeviceRail from '$lib/components/DeviceRail.svelte';
  import DeviceDetail from '$lib/components/DeviceDetail.svelte';
  import SettingsView from '$lib/components/SettingsView.svelte';
  import LabelDialog from '$lib/components/LabelDialog.svelte';

  let found: ScanResult[] = $state([]);
  let scanning = $state(false);
  let selected: string | null = $state(null);
  let settingsOpen = $state(false);

  // Connect credentials, sourced from the settings panel / saved store.
  let userId = $state('');
  let manualSerial = $state('');
  let email = $state('');
  let region = $state('Eu');

  let labelTarget:
    | { id: string; serial: string | null; address: string | null; label: string }
    | null = $state(null);

  // Remembers the BLE address each device connected with, so a connected-but-unknown
  // device can be labeled with an address to reconnect by later (DeviceView has no address).
  let addresses: Record<string, string> = $state({});

  onMount(() => {
    const unlisten = onDeviceEvent((e) => applyEvent(e.uid, e.kind, e.data));
    (async () => {
      try {
        const saved = await savedCredentials();
        userId = saved.user_id ?? '';
        email = saved.email ?? '';
        region = saved.region ?? 'Eu';
      } catch (e) {
        console.error('failed to load credentials', e);
      }
      await loadKnown();
    })();
    return () => {
      unlisten.then((f) => f());
    };
  });

  async function doScan() {
    scanning = true;
    try {
      found = await scan(8);
      // Refresh the cached BLE address of any known device whose address changed,
      // so its rail Connect button keeps working after the address rotates.
      let changed = false;
      for (const r of found) {
        const k = r.serial ? $known[r.serial] : undefined;
        if (k && r.address && k.address !== r.address) {
          await saveKnownDevice({ id: k.id, label: k.label, serial: k.serial, address: r.address });
          changed = true;
        }
      }
      if (changed) await loadKnown();
    } finally {
      scanning = false;
    }
  }

  async function doConnect(r: { address: string; serial: string | null; id: string }) {
    const effectiveSerial = r.serial ?? (manualSerial.trim() || null);
    const id = effectiveSerial ?? r.address;
    selected = id;
    if (r.address) addresses[id] = r.address;
    await connect(r.address, effectiveSerial, userId.trim() || null);
  }

  async function doForget(id: string) {
    await forgetDevice(id);
    await loadKnown();
  }

  async function saveLabel(label: string) {
    if (!labelTarget) return;
    const t = labelTarget;
    // A known device rename vs. saving a new one.
    if ($known[t.id]) await renameKnownDevice(t.id, label);
    else
      await saveKnownDevice({
        id: t.id,
        label,
        serial: t.serial,
        address: t.address
      });
    labelTarget = null;
    await loadKnown();
  }
</script>

{#if settingsOpen}
  <SettingsView
    bind:email
    bind:region
    bind:userId
    bind:manualSerial
    onBack={() => (settingsOpen = false)}
  />
{:else}
  <main>
    <DeviceRail
      {found}
      {selected}
      {scanning}
      {addresses}
      onScan={doScan}
      onConnect={doConnect}
      onDisconnect={(id) => disconnect(id)}
      onLabel={(t) => (labelTarget = t)}
      onForget={doForget}
      onSettings={() => (settingsOpen = true)}
      onSelect={(id) => (selected = id)}
    />
    <section class="detail">
      {#if selected && $devices[selected]}
        <DeviceDetail uid={selected} onDisconnect={() => disconnect(selected!)} />
      {:else}
        <div class="empty glass">Select or connect a device.</div>
      {/if}
    </section>
  </main>
{/if}

{#if labelTarget}
  <LabelDialog
    initial={labelTarget.label}
    onCancel={() => (labelTarget = null)}
    onSave={saveLabel}
  />
{/if}

<style>
  main { display: grid; grid-template-columns: 270px 1fr; gap: 0.75rem; height: 100vh; padding: 0.75rem; }
  .detail { overflow-y: auto; }
  .empty { display: grid; place-items: center; height: 100%; color: var(--text-dim); }
</style>
