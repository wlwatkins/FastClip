import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import { filterClips } from "../src/lib/search";
import { SEARCH_EMPTY_RESULT_MESSAGE } from "../src/lib/copy";
import type { Clip } from "../src/lib/contract/types";

// WP-13 (docs/src/work/wp-13-search.md, test-engineer share): "A query
// matching only a label, only a value, and neither. Case insensitivity both
// ways. A query matching nothing shows the empty-result message rather than
// a blank panel. Escape restores the full list. Drag handles inert with a
// query present and active without one. Filtering does not reorder the
// surviving clips."
//
// Drag-handle-inert (all four fronts: disabled, draggable="false", dragover
// refused, keyboard move a no-op) and its restoration on clearing the query
// are already covered by tests/reorder.test.ts's
// "WP-13: drag handles are inert while a search query is active" describe
// block (WP-06's test-engineer share, since search disabling reorder is a
// WP-06/WP-13 seam). Not repeated here.
//
// Risk this suite exists to police (spec §4.7 "Risks", ADR-0002): the query
// is a substring of a clip's `value`. It must never reach `console`, and the
// empty-result string must never echo it back into the DOM.

const A: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Alpha Report", value: "quarterly numbers", colour: "blue" };
const B: Clip = { id: "22222222-2222-2222-2222-222222222222", label: "Bravo Notes", value: "SEEKRIT-VALUE-TOKEN", colour: "teal" };
const C: Clip = { id: "33333333-3333-3333-3333-333333333333", label: "Charlie Memo", value: "unrelated text", colour: "amber" };

async function renderApp(clips: Clip[], overrides: Partial<Record<string, (args: unknown) => unknown>> = {}) {
  const mock = setupTauriMock({
    list_clips: () => clips,
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
    ...overrides,
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByText(clips[0]?.label ?? "No clips yet.")).toBeInTheDocument());
  return { ...utils, mock };
}

async function openSearchAndType(query: string) {
  await fireEvent.click(screen.getByRole("button", { name: "Search clips" }));
  const input = await screen.findByRole("searchbox");
  await fireEvent.input(input, { target: { value: query } });
  return input;
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("filterClips (unit): label-only, value-only, neither, case both ways", () => {
  it("matches a query present only in label", () => {
    expect(filterClips([A, B, C], "Alpha")).toEqual([A]);
  });

  it("matches a query present only in value", () => {
    expect(filterClips([A, B, C], "SEEKRIT")).toEqual([B]);
  });

  it("a query present in neither label nor value matches nothing", () => {
    expect(filterClips([A, B, C], "zzzzznope")).toEqual([]);
  });

  it("is case-insensitive against label (query upper, label mixed-case)", () => {
    expect(filterClips([A, B, C], "ALPHA")).toEqual([A]);
  });

  it("is case-insensitive against value (query lower, value upper)", () => {
    expect(filterClips([A, B, C], "seekrit")).toEqual([B]);
  });

  it("empty query returns the list unchanged, same order", () => {
    expect(filterClips([C, A, B], "")).toEqual([C, A, B]);
  });
});

describe("filterClips (unit): does not reorder survivors", () => {
  it("a subset match keeps the survivors' original relative order, not alphabetical or match-position order", () => {
    // Backend order: A (Zulu), B (Yankee, matches), Xr (Xray, no match), D (Whiskey, matches).
    // "Yankee" > "Whiskey" alphabetically, so a naive alphabetical sort would
    // invert this pair; filterClips must not.
    const zulu: Clip = { id: "z", label: "Zulu", value: "one", colour: "blue" };
    const yankee: Clip = { id: "y", label: "Yankee", value: "MATCH-two", colour: "teal" };
    const xray: Clip = { id: "x", label: "Xray", value: "three", colour: "amber" };
    const whiskey: Clip = { id: "w", label: "Whiskey", value: "match-four", colour: "green" };
    const result = filterClips([zulu, yankee, xray, whiskey], "match");
    expect(result).toEqual([yankee, whiskey]);
  });

  it("when every clip matches, the full array comes back in its original order (guards against an accidental sort with an all-pass filter)", () => {
    const result = filterClips([C, A, B], "a"); // "a" appears in all three labels/values, case-insensitively
    expect(result).toEqual([C, A, B]);
  });
});

describe("end to end: typing filters the rendered list", () => {
  it("a query matching only a label shows just that clip", async () => {
    await renderApp([A, B, C]);
    await openSearchAndType("Alpha");
    expect(screen.getByText(A.label)).toBeInTheDocument();
    expect(screen.queryByText(B.label)).not.toBeInTheDocument();
    expect(screen.queryByText(C.label)).not.toBeInTheDocument();
  });

  it("a query matching only a value shows just that clip", async () => {
    await renderApp([A, B, C]);
    await openSearchAndType("SEEKRIT");
    expect(screen.getByText(B.label)).toBeInTheDocument();
    expect(screen.queryByText(A.label)).not.toBeInTheDocument();
    expect(screen.queryByText(C.label)).not.toBeInTheDocument();
  });

  it("case-insensitivity end to end: lowercase query matches an uppercase value substring", async () => {
    await renderApp([A, B, C]);
    await openSearchAndType("seekrit");
    expect(screen.getByText(B.label)).toBeInTheDocument();
  });

  it("a query matching nothing shows the exact empty-result constant, not a blank panel, and never echoes the query", async () => {
    await renderApp([A, B, C]);
    await openSearchAndType("zzzzznope");
    // The exact copy-deck constant, not a template reconstructed here — this
    // is what pins frontend-dev's constant against a later "helpful" change
    // to interpolate the query.
    expect(screen.getByText(SEARCH_EMPTY_RESULT_MESSAGE)).toBeInTheDocument();
    expect(screen.getByText("No clips match your search.")).toBeInTheDocument();
    // Not a blank panel: the empty-result surface is present (a message, not nothing).
    expect(screen.queryByText(A.label)).not.toBeInTheDocument();
    // The query itself must not appear anywhere in the document.
    expect(document.body.textContent).not.toContain("zzzzznope");
  });

  it("Escape restores the full list and hides the search box", async () => {
    await renderApp([A, B, C]);
    const input = await openSearchAndType("Alpha");
    expect(screen.queryByText(B.label)).not.toBeInTheDocument();

    await fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => expect(screen.getByText(B.label)).toBeInTheDocument());
    expect(screen.getByText(A.label)).toBeInTheDocument();
    expect(screen.getByText(C.label)).toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  });
});

describe("ADR-0002: the query never reaches console", () => {
  it("typing a query, filtering to zero results, and clearing it calls no console method", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
    const infoSpy = vi.spyOn(console, "info").mockImplementation(() => {});

    await renderApp([A, B, C]);
    const input = await openSearchAndType("SEEKRIT-VALUE-TOKEN-does-not-exist-anywhere-else");
    await waitFor(() => expect(screen.getByText(SEARCH_EMPTY_RESULT_MESSAGE)).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("searchbox")).not.toBeInTheDocument());

    expect(logSpy).not.toHaveBeenCalled();
    expect(warnSpy).not.toHaveBeenCalled();
    expect(errorSpy).not.toHaveBeenCalled();
    expect(debugSpy).not.toHaveBeenCalled();
    expect(infoSpy).not.toHaveBeenCalled();
  });
});

describe("Ctrl+F opens and focuses; suppressed while a dialog is open or outside the ready phase", () => {
  it("Ctrl+F reveals the search box and focuses its input, from a ready phase with no dialog open", async () => {
    await renderApp([A, B, C]);
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();

    await fireEvent.keyDown(window, { key: "f", ctrlKey: true });

    const input = await screen.findByRole("searchbox");
    await waitFor(() => expect(document.activeElement).toBe(input));
  });

  it("Ctrl+F is suppressed while a dialog (the create-clip form) is open", async () => {
    await renderApp([A, B, C]);
    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    // The form dialog is open; a window-level Ctrl+F must not reveal search underneath it.
    await fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  });

  it("Ctrl+F is suppressed outside the ready phase (still loading: get_lock_state has not resolved)", async () => {
    let releaseLockState: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => {
      releaseLockState = resolve;
    });
    setupTauriMock({
      list_clips: () => [A],
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: async () => {
        await gate;
        return { encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null };
      },
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);
    await waitFor(() => expect(screen.getByText("Loading…")).toBeInTheDocument());

    await fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();

    releaseLockState!();
    await waitFor(() => expect(screen.getByText(A.label)).toBeInTheDocument());
  });
});

describe("entering the locked phase clears the search query (contract lock section; asserted at the phase-transition level, not at specific call sites, because WP-07 is adding a live mid-session lock transition)", () => {
  // F1 (test-engineer review): the toolbar and search box are inside the
  // "ready"-only branch of App.svelte, so `phase` moving to "locked" unmounts
  // them regardless of whether `searchQuery` and `searchOpen` were actually
  // cleared. Asserting only that they are gone during the locked phase is
  // therefore satisfied by the unmount alone, not by proof of a clear — a
  // test that passes whether or not the state was reset is not discriminating.
  // Carrying the scenario through the unlock leg (contract §2 `unlock`: a
  // `lock_state { locked: false }` followed by `update_clips`, with no
  // `list_clips` call of the frontend's own) is what makes it observable:
  // if `searchQuery` had survived the lock, the re-mounted toolbar would come
  // back with the query still applied and the list still filtered to one clip.
  it("a lock_state event arriving mid-session with locked: true, while a search query is active, clears the query rather than merely hiding it — proven by unlocking again and finding the full list, not the stale filter", async () => {
    const { mock } = await renderApp([A, B, C]);
    await openSearchAndType("Alpha");
    expect(await screen.findByRole("searchbox")).toBeInTheDocument();
    expect(screen.queryByText(B.label)).not.toBeInTheDocument(); // filtered down first, to prove there was something to clear

    mock.emit("lock_state", { encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null });

    // The phase transition (ADR-0010's load-bearing effect) discards the
    // toolbar entirely, so the search box's disappearance alone is not proof
    // the query was cleared — see the note above. What follows is.
    await waitFor(() => expect(screen.queryByRole("button", { name: "Search clips" })).not.toBeInTheDocument());
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();

    // The way back: `unlock` emits `lock_state { locked: false }` then
    // `update_clips` (contract §2); the frontend never calls `list_clips`
    // itself on this path.
    mock.emit("lock_state", { encryption_enabled: true, locked: false, attempts_remaining: null, retry_after_ms: null });
    mock.emit("update_clips", [A, B, C]);

    await waitFor(() => expect(screen.getByRole("button", { name: "Search clips" })).toBeInTheDocument());
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
    expect(screen.getByText(A.label)).toBeInTheDocument();
    expect(screen.getByText(B.label)).toBeInTheDocument();
    expect(screen.getByText(C.label)).toBeInTheDocument();
  });
});
