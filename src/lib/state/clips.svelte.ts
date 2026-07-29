// Clip list state. Module-level rather than component-local because it
// outlives any single component: the list view, the edit form and the
// delete confirmation all read it, and the `update_clips` listener (wired
// in App.svelte) writes it from outside the component tree that renders it.

import type { Clip } from "../contract/types";

export const clipListState: { clips: Clip[]; loaded: boolean } = $state({
  clips: [],
  loaded: false,
});

export function setClips(clips: Clip[]): void {
  clipListState.clips = clips;
  clipListState.loaded = true;
}
