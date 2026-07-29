// WP-02 wiring only. There is no Svelte in the tree yet — the scaffold is
// WP-04. This file exists so `svelte-check` has a config to load instead of
// falling back to reading vite.config.ts (which is still the React/Vite
// config and has no Svelte plugin registered).
export default {};
