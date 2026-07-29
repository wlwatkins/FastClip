import { describe, expect, it, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";

// WP-02 wiring only: this test exists to prove that Vitest, jsdom and the
// Tauri mock IPC bridge run in CI, not to cover any real command. There is
// no application code behind "ping" — it is invented for this test alone.
describe("toolchain wiring (WP-02)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("runs in jsdom and round-trips a mocked Tauri IPC call", async () => {
    mockIPC((cmd) => {
      if (cmd === "ping") {
        return "pong";
      }
      throw new Error(`unexpected command: ${cmd}`);
    });

    await expect(invoke("ping")).resolves.toBe("pong");
    expect(typeof window).toBe("object");
  });
});
