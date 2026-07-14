<script lang="ts">
  let {
    initial,
    onSave,
    onCancel
  }: { initial: string; onSave: (label: string) => void; onCancel: () => void } = $props();

  let value = $state('');

  $effect(() => {
    value = initial;
  });
</script>

<div class="backdrop" onclick={onCancel} onkeydown={(e) => {if (e.key === 'Escape') onCancel();}} role="presentation">
  <div class="modal glass" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>Device label</h3>
    <input
      type="text"
      bind:value
      placeholder="e.g. EcoFlow Camper"
      onkeydown={(e) => e.key === 'Enter' && value.trim() && onSave(value.trim())}
    />
    <div class="actions">
      <button onclick={onCancel}>Cancel</button>
      <button disabled={!value.trim()} onclick={() => onSave(value.trim())}>Save</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); display: grid; place-items: center; z-index: 20; }
  .modal { width: min(340px, 90vw); padding: 1rem 1.25rem; display: flex; flex-direction: column; gap: 0.6rem; }
  h3 { margin: 0; font-size: 1rem; }
  .actions { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
