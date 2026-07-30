// Settings state. Module-level because it is read from the toolbar (the
// settings toggle) and written by the startup sequence in App.svelte, which
// are not in an ancestor/descendant relationship with each other's props.
//
// contract §1 `Settings`: `{ always_on_top: boolean }`, readable and writable
// while the store is locked. This holds only the value last confirmed by the
// backend — `get_settings` at startup, or a successful `set_always_on_top`.

export const settingsState: { alwaysOnTop: boolean } = $state({ alwaysOnTop: false });

export function setAlwaysOnTopState(value: boolean): void {
  settingsState.alwaysOnTop = value;
}
