<script lang="ts">
  import { sendCommand, type Command, type DeviceSettings } from '$lib/ipc';

  let { uid, settings }: { uid: string; settings: DeviceSettings } = $props();

  // Gate: nothing is editable until the first real readback arrives, so we never
  // act against unknown state.
  let ready = $derived(!!settings && Object.values(settings).some((v) => v !== null));

  let acWatts = $state(200);
  let carAmps = $state(4);
  let chargeLimit = $state(100);
  let dischargeLimit = $state(5);
  let dcMode = $state(0);
  let unitTimeoutMin = $state(1440);
  let screenTimeoutSec = $state(180);
  let acTimeoutMin = $state(120);
  let seeded = $state(false);

  $effect(() => {
    if (seeded || !ready) return;
    if (settings.ac_charge_watts != null) acWatts = settings.ac_charge_watts;
    if (settings.car_input_ma != null) carAmps = Math.round(settings.car_input_ma / 1000);
    if (settings.charge_limit != null) chargeLimit = settings.charge_limit;
    if (settings.discharge_limit != null) dischargeLimit = settings.discharge_limit;
    if (settings.dc_mode != null) dcMode = settings.dc_mode;
    if (settings.unit_timeout_min != null) unitTimeoutMin = settings.unit_timeout_min;
    if (settings.screen_timeout_sec != null) screenTimeoutSec = settings.screen_timeout_sec;
    if (settings.ac_timeout_min != null) acTimeoutMin = settings.ac_timeout_min;
    seeded = true;
  });

  let busy = $state(false);
  let error = $state('');
  let lastSent = $state('');

  async function send(command: Command, label: string) {
    busy = true;
    error = '';
    try {
      await sendCommand(uid, command);
      lastSent = label;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const DC_MODES = [
    { value: 0, label: 'Auto' },
    { value: 1, label: 'Solar' },
    { value: 2, label: 'Car' }
  ];
  const dcModeLabel = (v: number | null | undefined) =>
    DC_MODES.find((m) => m.value === v)?.label ?? '—';
  const stateText = (v: boolean | null | undefined) => (v == null ? '—' : v ? 'ON' : 'OFF');
  let off = $derived(busy || !ready);
</script>

<div class="controls glass">
  <div class="head">
    <h3>Controls</h3>
    {#if !ready}<span class="reading">Reading current values…</span>{/if}
  </div>

  <section>
    <span class="label">AC output</span>
    <span class="cur" class:on={settings?.ac_enabled}>{stateText(settings?.ac_enabled)}</span>
    <button disabled={off} onclick={() => send({ kind: 'AcOutput', value: true }, 'AC on')}>On</button>
    <button disabled={off} onclick={() => send({ kind: 'AcOutput', value: false }, 'AC off')}>Off</button>
  </section>

  <section>
    <span class="label">X-Boost</span>
    <span class="cur" class:on={settings?.xboost_enabled}>{stateText(settings?.xboost_enabled)}</span>
    <button disabled={off} onclick={() => send({ kind: 'XBoost', value: true }, 'X-Boost on')}>On</button>
    <button disabled={off} onclick={() => send({ kind: 'XBoost', value: false }, 'X-Boost off')}>Off</button>
  </section>

  <section>
    <span class="label">DC (12V) output</span>
    <span class="cur" class:on={settings?.dc_enabled}>{stateText(settings?.dc_enabled)}</span>
    <button disabled={off} onclick={() => send({ kind: 'DcOutput', value: true }, 'DC on')}>On</button>
    <button disabled={off} onclick={() => send({ kind: 'DcOutput', value: false }, 'DC off')}>Off</button>
  </section>

  <section>
    <span class="label">DC input mode</span>
    <span class="cur">now {dcModeLabel(settings?.dc_mode)}</span>
    <select bind:value={dcMode} disabled={off}>
      {#each DC_MODES as m}<option value={m.value}>{m.label}</option>{/each}
    </select>
    <button disabled={off} onclick={() => send({ kind: 'DcMode', value: dcMode }, `DC mode ${dcModeLabel(dcMode)}`)}>Apply</button>
  </section>

  <section>
    <span class="label">AC charge speed</span>
    <span class="cur">now {settings?.ac_charge_watts ?? '—'} W</span>
    <input type="range" min="100" max="660" step="10" bind:value={acWatts} disabled={off} />
    <span class="val">{acWatts} W</span>
    <button disabled={off} onclick={() => send({ kind: 'AcChargeWatts', value: acWatts }, `AC charge ${acWatts}W`)}>Apply</button>
  </section>

  <section>
    <span class="label">Car input</span>
    <span class="cur">now {settings?.car_input_ma != null ? Math.round(settings.car_input_ma / 1000) + ' A' : '—'}</span>
    <select bind:value={carAmps} disabled={off}>
      <option value={2}>2 A</option>
      <option value={4}>4 A</option>
      <option value={6}>6 A</option>
      <option value={8}>8 A</option>
    </select>
    <button disabled={off} onclick={() => send({ kind: 'CarInputMilliamps', value: carAmps * 1000 }, `Car input ${carAmps}A`)}>Apply</button>
  </section>

  <section>
    <span class="label">Charge limit</span>
    <span class="cur">now {settings?.charge_limit ?? '—'}%</span>
    <input type="range" min="50" max="100" step="1" bind:value={chargeLimit} disabled={off} />
    <span class="val">{chargeLimit}%</span>
    <button disabled={off} onclick={() => send({ kind: 'ChargeLimit', value: chargeLimit }, `Charge limit ${chargeLimit}%`)}>Apply</button>
  </section>

  <section>
    <span class="label">Discharge limit</span>
    <span class="cur">now {settings?.discharge_limit ?? '—'}%</span>
    <input type="range" min="0" max="30" step="1" bind:value={dischargeLimit} disabled={off} />
    <span class="val">{dischargeLimit}%</span>
    <button disabled={off} onclick={() => send({ kind: 'DischargeLimit', value: dischargeLimit }, `Discharge limit ${dischargeLimit}%`)}>Apply</button>
  </section>

  <details>
    <summary>Timeouts</summary>
    <section>
      <span class="label">Unit standby</span>
      <span class="cur">now {settings?.unit_timeout_min ?? '—'} min</span>
      <input type="number" min="0" bind:value={unitTimeoutMin} disabled={off} />
      <button disabled={off} onclick={() => send({ kind: 'UnitTimeoutMinutes', value: unitTimeoutMin }, 'Unit timeout')}>Apply</button>
    </section>
    <section>
      <span class="label">Screen</span>
      <span class="cur">now {settings?.screen_timeout_sec ?? '—'} s</span>
      <input type="number" min="0" bind:value={screenTimeoutSec} disabled={off} />
      <button disabled={off} onclick={() => send({ kind: 'ScreenTimeoutSeconds', value: screenTimeoutSec }, 'Screen timeout')}>Apply</button>
    </section>
    <section>
      <span class="label">AC standby</span>
      <span class="cur">now {settings?.ac_timeout_min ?? '—'} min</span>
      <input type="number" min="0" bind:value={acTimeoutMin} disabled={off} />
      <button disabled={off} onclick={() => send({ kind: 'AcTimeoutMinutes', value: acTimeoutMin }, 'AC timeout')}>Apply</button>
    </section>
  </details>

  {#if error}<small class="error">Command failed: {error}</small>
  {:else if lastSent}<small class="ok">Sent: {lastSent}</small>{/if}
</div>

<style>
  .controls { padding: 1rem; font-size: 0.85rem; }
  .head { display: flex; align-items: baseline; gap: 0.75rem; }
  .head h3 { margin: 0 0 0.5rem 0; font-size: 0.95rem; }
  .reading { color: var(--warn); font-size: 0.8rem; }
  section { display: flex; align-items: center; gap: 0.5rem; margin: 0.4rem 0; }
  .label { flex: 0 0 8rem; color: var(--text-dim); }
  .cur { flex: 0 0 6rem; color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .cur.on { color: var(--ok); font-weight: 600; }
  .val { flex: 0 0 3.5rem; text-align: right; font-variant-numeric: tabular-nums; }
  input[type='number'] { width: 5rem; }
  input[type='range'] { flex: 1; }
  details { margin-top: 0.5rem; }
  summary { cursor: pointer; color: var(--text-dim); }
  .error { color: var(--err); }
  .ok { color: var(--ok); }
</style>
