<script lang="ts">
  import type { Snapshot } from '$lib/ipc';
  let { snapshot }: { snapshot: Snapshot } = $props();
  const fmt = (v: number | null | undefined, unit: string, d = 0) =>
    v === null || v === undefined ? '—' : `${v.toFixed(d)}${unit}`;

  // Battery runtime reads better as a duration than as raw seconds. Show the two largest
  // non-zero units (e.g. "4d 3h", "2h 15m", "45m"); sub-minute values stay in seconds.
  const fmtRuntime = (s: number | null | undefined) => {
    if (s === null || s === undefined) return '—';
    if (s < 60) return `${s} s`;
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  };

  let tiles = $derived([
    { k: 'Battery', v: fmt(snapshot.battery_charge, '%', 1) },
    { k: 'Input', v: fmt(snapshot.input_power_w, ' W') },
    { k: 'Output', v: fmt(snapshot.output_power_w, ' W') },
    { k: 'Input V', v: fmt(snapshot.input_voltage, ' V', 1) },
    { k: 'Output V', v: fmt(snapshot.output_voltage, ' V', 1) },
    { k: 'Temp', v: fmt(snapshot.temperature, ' °C') },
    { k: 'Load', v: fmt(snapshot.load_pct, '%', 1) },
    { k: 'Runtime', v: fmtRuntime(snapshot.battery_runtime_s) }
  ]);
</script>

<div class="grid">
  {#each tiles as t (t.k)}
    <div class="tile glass"><span>{t.k}</span><strong>{t.v}</strong></div>
  {/each}
</div>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 0.75rem; }
  .tile { padding: 0.9rem; display: flex; flex-direction: column; gap: 0.2rem; }
  .tile span { color: var(--text-dim); font-size: 0.78rem; }
  .tile strong { font-size: 1.5rem; }
</style>
