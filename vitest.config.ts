import { defineConfig } from "vitest/config";

// Wiring only (WP-02). The frontend is still React; Svelte arrives in WP-04.
// jsdom is the environment so `@testing-library/svelte` has a DOM to mount
// into once there is something Svelte to mount.
//
// setupFiles registers @testing-library/jest-dom's matchers on `expect`.
// jest-dom v7 does not self-register on import elsewhere — without this,
// `expect(el).toBeInTheDocument()` fails with "not a function", not a
// meaningful assertion failure.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.ts", "src/**/*.test.ts"],
    setupFiles: ["./tests/setup.ts"],
  },
});
