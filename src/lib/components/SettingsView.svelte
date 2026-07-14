<script lang="ts">
  import { fetchUserId } from '$lib/ipc';
  import { theme, type Theme } from '$lib/theme';

  let {
    email = $bindable(''),
    region = $bindable('Eu'),
    userId = $bindable(''),
    manualSerial = $bindable(''),
    onBack
  }: {
    email: string;
    region: string;
    userId: string;
    manualSerial: string;
    onBack: () => void;
  } = $props();

  let password = $state('');
  let fetching = $state(false);
  let fetchError = $state('');

  async function doFetch() {
    fetching = true;
    fetchError = '';
    try {
      userId = await fetchUserId(email, password, region);
      password = '';
    } catch (e) {
      fetchError = `Failed to fetch User ID: ${e}`;
    } finally {
      fetching = false;
    }
  }

  const THEMES: Theme[] = ['auto', 'dark', 'light'];

  // Settings sections. Only "General" today; the left menu is built from this list so
  // future sections (devices, notifications, about, …) just get added here.
  const SECTIONS = [{ id: 'general', label: 'General' }];
  let activeSection = $state('general');
</script>

<section class="settings-view">
  <header class="bar">
    <button class="back" onclick={onBack}>← Devices</button>
    <h2>Settings</h2>
    <span class="spacer"></span>
  </header>

  <div class="body">
    <nav class="menu glass">
      {#each SECTIONS as s}
        <button class:active={activeSection === s.id} onclick={() => (activeSection = s.id)}>
          {s.label}
        </button>
      {/each}
    </nav>

    <div class="pane glass">
      {#if activeSection === 'general'}
        <h4>Theme</h4>
        <div class="themes">
          {#each THEMES as t}
            <button class:active={$theme === t} onclick={() => theme.set(t)}>{t}</button>
          {/each}
        </div>

        <h4>EcoFlow account</h4>
        <label>Email<input type="email" bind:value={email} placeholder="you@example.com" /></label>
        <label>Password<input type="password" bind:value={password} placeholder="password" /></label>
        <label>Region
          <select bind:value={region}><option value="Eu">EU</option><option value="Us">US</option></select>
        </label>
        <button onclick={doFetch} disabled={fetching || !email || !password}>
          {fetching ? 'Fetching…' : 'Fetch User ID'}
        </button>
        {#if fetchError}<small class="error">{fetchError}</small>{/if}

        <h4>Advanced</h4>
        <label>User ID <small>(optional)</small><input type="text" bind:value={userId} placeholder="numeric account id" /></label>
        <label>Manual serial <small>(optional)</small><input type="text" bind:value={manualSerial} placeholder="overrides scan serial" /></label>
        <small class="hint">Leave User ID empty for plaintext read/control. Manual serial only if the device does not advertise one.</small>
      {/if}
    </div>
  </div>
</section>

<style>
  /* Full-view opaque section that replaces the device shell — no translucency over content. */
  .settings-view {
    height: 100vh;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    background: var(--bg);
    background-attachment: fixed;
    overflow: hidden;
  }

  /* Centered title with the back button pinned left (three-column grid, title in the middle). */
  .bar { display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; }
  .bar .back { justify-self: start; font-weight: 600; }
  .bar h2 { grid-column: 2; margin: 0; font-size: 1.1rem; text-align: center; }

  /* Same split as the home: menu rail on the left, options pane on the right. */
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 220px 1fr; gap: 0.75rem; }

  .menu { padding: 0.5rem; display: flex; flex-direction: column; gap: 0.25rem; }
  .menu button { width: 100%; text-align: left; background: transparent; border-color: transparent; }
  .menu button:hover { background: var(--surface-2); }
  .menu button.active { background: var(--surface-2); border-color: var(--accent); color: var(--accent); }

  .pane {
    padding: 1.25rem 1.5rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .pane label,
  .pane .themes,
  .pane > button,
  .pane small { max-width: 460px; }

  h4 { margin: 0.75rem 0 0.15rem; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-dim); }
  label { display: flex; flex-direction: column; gap: 0.15rem; font-size: 0.85rem; }
  .themes { display: flex; gap: 0.35rem; }
  .themes button { text-transform: capitalize; }
  .themes button.active { border-color: var(--accent); color: var(--accent); }
  .hint, .error { font-size: 0.78rem; }
  .hint { color: var(--text-dim); }
  .error { color: var(--err); }
</style>
