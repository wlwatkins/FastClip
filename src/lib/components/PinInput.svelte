<script lang="ts">
  // A labelled 6-digit PIN field, shared by the launch prompt and every PIN
  // entry in the settings flows (contract "Validation rules, stated once":
  // `pin`, `current_pin`, `new_pin` are exactly 6 ASCII digits). Masked like
  // a password field — a PIN is a secret entered in view of whoever is
  // nearby. Never logs, and the value is never read by anything outside the
  // caller that owns it.

  let {
    id,
    label,
    value = $bindable(""),
    error = null,
    disabled = false,
    autofocus = false,
  }: {
    id: string;
    label: string;
    value?: string;
    error?: string | null;
    disabled?: boolean;
    autofocus?: boolean;
  } = $props();

  let inputEl = $state<HTMLInputElement | undefined>(undefined);

  $effect(() => {
    if (autofocus) inputEl?.focus();
  });

  /**
   * Imperative focus, exposed via `bind:this` for a caller that needs to
   * refocus this field outside the mount-time `autofocus` effect above —
   * `PinPrompt` uses this for the tray's `unlock_requested` event (contract
   * §3, ADR-0013), where the field is already mounted and `autofocus` does
   * not run again.
   */
  export function focus(): void {
    inputEl?.focus();
  }

  function handleInput(event: Event) {
    const target = event.currentTarget as HTMLInputElement;
    value = target.value.replace(/[^0-9]/g, "").slice(0, 6);
  }
</script>

<div>
  <label for={id} class="mb-1 block text-sm text-zinc-300">{label}</label>
  <input
    {id}
    bind:this={inputEl}
    type="password"
    inputmode="numeric"
    autocomplete="off"
    maxlength="6"
    {value}
    oninput={handleInput}
    {disabled}
    aria-invalid={error !== null}
    aria-describedby={error !== null ? `${id}-error` : undefined}
    class="w-full rounded border border-zinc-600 bg-zinc-900 px-2 py-1.5 tracking-[0.3em] text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 disabled:opacity-60"
  />
  {#if error !== null}
    <p id="{id}-error" class="mt-1 text-xs text-red-400">{error}</p>
  {/if}
</div>
