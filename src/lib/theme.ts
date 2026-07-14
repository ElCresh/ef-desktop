import { writable } from 'svelte/store';

export type Theme = 'auto' | 'dark' | 'light';
const KEY = 'ecoflow-theme';

function initial(): Theme {
  if (typeof localStorage === 'undefined') return 'auto';
  const v = localStorage.getItem(KEY);
  return v === 'dark' || v === 'light' ? v : 'auto';
}

export const theme = writable<Theme>(initial());

export function applyTheme(t: Theme) {
  if (typeof document === 'undefined') return;
  const el = document.documentElement;
  if (t === 'auto') delete el.dataset.theme;
  else el.dataset.theme = t;
  if (typeof localStorage !== 'undefined') localStorage.setItem(KEY, t);
}

// Apply on every change (and once on load, from the initial value).
theme.subscribe(applyTheme);
