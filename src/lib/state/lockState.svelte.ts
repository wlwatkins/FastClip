// Lock state. Module-level: the `lock_state` event listener (wired in
// App.svelte) writes it, and the `update_clips` listener reads it to decide
// whether to discard a payload that arrived after the store locked (contract
// §3 `update_clips`: "The frontend discards an `update_clips` that arrives
// while its last received `lock_state` says `locked: true`.").
//
// contract §1 `LockState`. Defaults to the unlocked, unencrypted state; the
// startup sequence overwrites it with the backend's answer from
// `get_lock_state` before anything else can observe a stale default.

import type { LockState } from "../contract/types";

export const lockState: LockState = $state({
  encryption_enabled: false,
  locked: false,
  attempts_remaining: null,
  retry_after_ms: null,
});

export function setLockState(next: LockState): void {
  lockState.encryption_enabled = next.encryption_enabled;
  lockState.locked = next.locked;
  lockState.attempts_remaining = next.attempts_remaining;
  lockState.retry_after_ms = next.retry_after_ms;
}
