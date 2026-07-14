<script lang="ts">
  import { devices } from '$lib/stores';
  import { known } from '$lib/knownDevices';
  import TelemetryGrid from './TelemetryGrid.svelte';
  import ControlsPanel from './ControlsPanel.svelte';

  let { uid, onDisconnect }: { uid: string; onDisconnect: () => void } = $props();
  let device = $derived($devices[uid]);
  let title = $derived($known[uid]?.label ?? device?.serial ?? uid);
</script>

<div class="detail-inner">
  <header class="glass">
    <div class="titles">
      <h2>{title}</h2>
      <small>{device?.serial ?? uid} · {device?.state}</small>
    </div>
    <div class="right">
      {#if device?.auth === 'authenticating'}
        <span class="badge warn">Authenticating…</span>
      {:else if device?.auth === 'authenticated'}
        <span class="badge ok">Authenticated</span>
      {:else if device?.auth === 'failed'}
        <span class="badge err">Auth failed</span>
      {/if}
      <button onclick={onDisconnect}>Disconnect</button>
    </div>
  </header>

  {#if device?.snapshot}
    <TelemetryGrid snapshot={device.snapshot} />
    {#key uid}
      <ControlsPanel {uid} settings={device.snapshot.settings} />
    {/key}
  {:else}
    <div class="waiting glass">
      <span class="spinner" aria-hidden="true"></span>
      <div class="waiting-text">
        <strong>Waiting for telemetry…</strong>
        <small>Reading live values from the device</small>
      </div>
    </div>
  {/if}
</div>

<style>
  .detail-inner { display: flex; flex-direction: column; gap: 0.75rem; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; }
  .titles h2 { margin: 0; font-size: 1.2rem; }
  .titles small { color: var(--text-dim); }
  .right { display: flex; align-items: center; gap: 0.5rem; }
  .badge { font-size: 0.75rem; font-weight: 600; padding: 0.15rem 0.5rem; border-radius: 999px; border: 1px solid var(--border); }
  .badge.ok { color: var(--ok); }
  .badge.warn { color: var(--warn); }
  .badge.err { color: var(--err); }
  .waiting { display: flex; align-items: center; gap: 0.9rem; padding: 1.5rem 1.25rem; }
  .spinner {
    flex: 0 0 auto;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2.5px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  .waiting-text { display: flex; flex-direction: column; gap: 0.15rem; }
  .waiting-text strong { font-weight: 600; }
  .waiting-text small { color: var(--text-dim); font-size: 0.82rem; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spinner { animation-duration: 2.4s; } }
</style>
