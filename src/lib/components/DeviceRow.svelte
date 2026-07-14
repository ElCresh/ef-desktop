<script lang="ts">
  let {
    id,
    label,
    sub,
    state = undefined,
    soc = null,
    connected,
    selected,
    onSelect,
    onConnect,
    onDisconnect,
    onLabel,
    onForget = undefined
  }: {
    id: string;
    label: string;
    sub: string;
    state?: string;
    soc?: number | null;
    connected: boolean;
    selected: boolean;
    onSelect: () => void;
    onConnect: () => void;
    onDisconnect: () => void;
    onLabel: () => void;
    onForget?: () => void;
  } = $props();

  const dotColor = (s?: string) =>
    ({ online: 'var(--ok)', connecting: 'var(--warn)', retrying: 'var(--warn)', failed: 'var(--err)' }[
      s ?? ''
    ] ?? 'var(--text-dim)');
</script>

<div class="row" class:selected>
  <button class="main" onclick={onSelect}>
    <span class="dot" style="background:{dotColor(state)}"></span>
    <span class="text">
      <strong>{label}</strong>
      <small>{sub}{state ? ` · ${state}` : ''}</small>
    </span>
    {#if soc != null}<span class="soc">{soc.toFixed(0)}%</span>{/if}
  </button>
  <div class="actions">
    {#if connected}
      <button title="Disconnect" onclick={onDisconnect}>■</button>
    {:else}
      <button title="Connect" onclick={onConnect}>▶</button>
    {/if}
    <button title="Label" onclick={onLabel}>✎</button>
    {#if onForget}<button title="Forget" onclick={onForget}>🗑</button>{/if}
  </div>
</div>

<style>
  .row { display: flex; align-items: center; gap: 0.25rem; padding: 0.15rem; border-radius: 10px; }
  .row.selected { background: var(--surface-2); }
  .main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.5rem; background: transparent; border: none; text-align: left; }
  .dot { width: 9px; height: 9px; border-radius: 50%; flex: 0 0 auto; }
  .text { display: flex; flex-direction: column; min-width: 0; overflow: hidden; }
  .text strong, .text small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .text small { color: var(--text-dim); font-size: 0.72rem; }
  .soc { flex: 0 0 auto; margin-left: auto; font-variant-numeric: tabular-nums; color: var(--text-dim); }
  .actions { flex: 0 0 auto; display: flex; gap: 0.15rem; }
  .actions button { padding: 0.2rem 0.4rem; font-size: 0.8rem; }
</style>
