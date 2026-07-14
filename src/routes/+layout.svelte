<script lang="ts">
  import '../app.css';
  import { theme } from '$lib/theme';
  let { children } = $props();
  // Touch the store so its subscription (which applies the theme) stays active.
  $effect(() => {
    void $theme;
  });
</script>

{@render children()}
