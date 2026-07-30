import type { Clip } from "./contract/types";

/**
 * Client-side filter over the clip list already in memory. Contract §2:
 * "Search is the one specified feature with no command and no event — it
 * filters the list already in memory, and a round trip per keystroke is
 * wrong for a tool whose value is speed." (WP-13)
 *
 * Spec §4.7: a clip matches when `query` appears anywhere in its `label` or
 * its `value`, case-insensitively, by substring containment. Not fuzzy, not
 * regex, not ranked (spec §6). `Array.prototype.filter` preserves the
 * source order, so the survivors keep the user's order — this is a filter,
 * never a sort.
 */
export function filterClips(clips: Clip[], query: string): Clip[] {
  if (query === "") return clips;
  const needle = query.toLowerCase();
  return clips.filter(
    (clip) => clip.label.toLowerCase().includes(needle) || clip.value.toLowerCase().includes(needle),
  );
}
