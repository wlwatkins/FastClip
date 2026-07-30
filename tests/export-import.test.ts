import { describe, expect, it, afterEach, beforeEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-09 (test-engineer share): export/import at the IPC boundary.
//
// The four store-level items this package names — round trip, one bad
// record changes nothing, colliding ids produce distinct clips, a truncated
// file is rejected — are proved on the Rust side (src-tauri, 244 tests
// passing): every truncation shape at both the parse and command layer, a
// bad record at index 3 across four fault shapes with the store asserted
// unchanged, the `.part` file's disclosure exposure, a store-level import
// rollback via an injected mid-apply trigger, and `locked` returned before
// the file is even opened. None of that is re-proved here with a mocked
// `import_clips` standing in for SQLite — that would be a weaker copy of a
// real test, not new coverage.
//
// What is genuinely frontend-only and covered below:
//  - `exportClips`/`importClips` invoke with the contract's `path` key.
//  - A malformed `ExportResult`/`ImportResult` throws at the boundary
//    (`parseExportResult`/`parseImportResult`) rather than rendering a
//    success message built from a value that was never validated.
//  - The import result is reported to the user with its count.
//  - A rejected import surfaces the field and index via `describeImportError`.
//  - The plaintext warning is shown, and `export_clips` is not invoked,
//    before the save dialog opens — "no code path writes an export the user
//    did not request".
//  - Frontend wiring for `update_clips`: an import success followed by the
//    backend's `update_clips` event repaints the list with the emitted
//    clips. This is the frontend's half of "every clip returns" — the store
//    actually returning them all is the Rust suite's job, not
//    reconstructable from a mock.

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
  open: vi.fn(),
}));

import { save, open } from "@tauri-apps/plugin-dialog";

const CLIP: Clip = {
  id: "11111111-1111-1111-1111-111111111111",
  label: "Greeting",
  value: "Hello there",
  colour: "blue",
};

async function renderAppWithSettingsOpen(overrides: Partial<Record<string, (args: unknown) => unknown>> = {}) {
  const mock = setupTauriMock({
    list_clips: () => [CLIP],
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
    ...overrides,
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());
  await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(screen.getByRole("switch")).toBeInTheDocument());
  return { ...utils, mock };
}

async function statusText(): Promise<string> {
  return (await screen.findByRole("status")).textContent ?? "";
}

beforeEach(() => {
  // restoreAllMocks() in afterEach resets each mock's implementation to a
  // no-op but has been observed here not to clear call history reliably for
  // mocks created inside a vi.mock() factory (as opposed to vi.spyOn); reset
  // explicitly so a call recorded by one test cannot read as a call in the
  // next.
  vi.mocked(save).mockReset();
  vi.mocked(open).mockReset();
});

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("export: the warning is shown, and export_clips is not called, before the save dialog opens", () => {
  it("clicking Export shows the warning without opening the save dialog", async () => {
    const exportClips = vi.fn(() => ({ exported: 1 }));
    vi.mocked(save).mockResolvedValue(null);
    await renderAppWithSettingsOpen({ export_clips: exportClips });

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));

    expect(screen.getByText(/not encrypted/i)).toBeInTheDocument();
    expect(save).not.toHaveBeenCalled();
    expect(exportClips).not.toHaveBeenCalled();
  });

  it("confirming the warning opens the save dialog, and only then; export_clips is not called if the dialog is cancelled", async () => {
    const exportClips = vi.fn(() => ({ exported: 1 }));
    vi.mocked(save).mockResolvedValue(null); // user cancels the native dialog
    await renderAppWithSettingsOpen({ export_clips: exportClips });

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    expect(save).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(exportClips).not.toHaveBeenCalled();
  });

  it("stepping back (Cancel or Escape) from the warning never calls export_clips — there is no path to it besides Confirm", async () => {
    const exportClips = vi.fn(() => ({ exported: 1 }));
    await renderAppWithSettingsOpen({ export_clips: exportClips });

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(save).not.toHaveBeenCalled();
    expect(exportClips).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(save).not.toHaveBeenCalled();
    expect(exportClips).not.toHaveBeenCalled();
  });
});

describe("export_clips: invoke argument key and result validation", () => {
  it("invokes export_clips with { path } carrying the dialog's chosen path", async () => {
    const exportClips = vi.fn(() => ({ exported: 1 }));
    vi.mocked(save).mockResolvedValue("C:\\Users\\me\\clips.json");
    const { mock } = await renderAppWithSettingsOpen({ export_clips: exportClips });

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(() => expect(exportClips).toHaveBeenCalled());
    const call = mock.calls.find((c) => c.cmd === "export_clips");
    expect(call?.args).toEqual({ path: "C:\\Users\\me\\clips.json" });
  });

  it("reports the exported count to the user on success", async () => {
    vi.mocked(save).mockResolvedValue("C:\\Users\\me\\clips.json");
    await renderAppWithSettingsOpen({ export_clips: () => ({ exported: 3 }) });

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(async () => expect(await statusText()).toContain("Exported 3 clips."));
  });

  it("a malformed ExportResult (wrong-typed exported) throws at the boundary: no success message is rendered", async () => {
    vi.mocked(save).mockResolvedValue("C:\\Users\\me\\clips.json");
    // @ts-expect-error deliberately malformed for the boundary-validation test
    await renderAppWithSettingsOpen({ export_clips: () => ({ exported: "3" }) });

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(async () => expect(await statusText()).not.toBe(""));
    const text = await statusText();
    expect(text).not.toMatch(/Exported/);
    expect(text).toContain("An unexpected error occurred.");
  });
});

describe("import_clips: invoke argument key, result validation, and reported count", () => {
  it("invokes import_clips with { path } carrying the dialog's chosen path", async () => {
    const importClips = vi.fn(() => ({ imported: 2 }));
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    const { mock } = await renderAppWithSettingsOpen({ import_clips: importClips });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(() => expect(importClips).toHaveBeenCalled());
    const call = mock.calls.find((c) => c.cmd === "import_clips");
    expect(call?.args).toEqual({ path: "C:\\Users\\me\\clips.json" });
  });

  it("reports the imported count to the user on success", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    await renderAppWithSettingsOpen({ import_clips: () => ({ imported: 5 }) });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(async () => expect(await statusText()).toContain("Imported 5 clips."));
  });

  it("a malformed ImportResult (missing imported) throws at the boundary: no success message is rendered", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    // @ts-expect-error deliberately malformed for the boundary-validation test
    await renderAppWithSettingsOpen({ import_clips: () => ({}) });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(async () => expect(await statusText()).not.toBe(""));
    const text = await statusText();
    expect(text).not.toMatch(/Imported/);
    expect(text).toContain("An unexpected error occurred.");
  });

  it("a cancelled Open dialog (path: null) never calls import_clips", async () => {
    const importClips = vi.fn(() => ({ imported: 1 }));
    vi.mocked(open).mockResolvedValue(null);
    await renderAppWithSettingsOpen({ import_clips: importClips });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(importClips).not.toHaveBeenCalled();
  });
});

describe("import_clips: a rejected import surfaces the field and index (describeImportError)", () => {
  it("missing_field at index 2 with a named field reports \"Clip 3 ... (\"label\").\"", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    await renderAppWithSettingsOpen({
      import_clips: () => {
        throw { kind: "import", reason: "missing_field", field: "label", index: 2 };
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(async () => expect(await statusText()).toContain('Clip 3 is missing a required field ("label").'));
  });

  it("a top-level fault (malformed_json, field and index both null) reports the whole-file message, not a clip index", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    await renderAppWithSettingsOpen({
      import_clips: () => {
        throw { kind: "import", reason: "malformed_json", field: null, index: null };
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(async () => expect(await statusText()).toContain("This is not a FastClip export file."));
    expect(await statusText()).not.toMatch(/Clip \d/);
  });

  it("unsupported_version reports the newer-version message", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    await renderAppWithSettingsOpen({
      import_clips: () => {
        throw { kind: "import", reason: "unsupported_version", field: null, index: null };
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await waitFor(async () => expect(await statusText()).toContain("This export file is from a newer version of FastClip."));
  });
});

describe("import success: the frontend's update_clips wiring repaints the list", () => {
  it("after a successful import, an update_clips event with the merged list is rendered", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Users\\me\\clips.json");
    const IMPORTED: Clip = {
      id: "22222222-2222-2222-2222-222222222222",
      label: "Imported one",
      value: "value",
      colour: "green",
    };
    const { mock } = await renderAppWithSettingsOpen({ import_clips: () => ({ imported: 1 }) });

    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));
    await waitFor(async () => expect(await statusText()).toContain("Imported 1 clip."));

    // The backend, not the frontend, decides the merged order; this only
    // proves the frontend renders whatever `update_clips` carries.
    mock.emit("update_clips", [CLIP, IMPORTED]);

    await waitFor(() => expect(screen.getByText("Imported one")).toBeInTheDocument());
    expect(screen.getByText(CLIP.label)).toBeInTheDocument();
  });
});
