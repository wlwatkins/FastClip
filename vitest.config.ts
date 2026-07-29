import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// jsdom is the environment so `@testing-library/svelte` has a DOM to mount
// into. The svelte plugin is registered so a `.svelte` file can be imported
// and compiled from a test, not just from the app build.
//
// setupFiles registers @testing-library/jest-dom's matchers on `expect`.
// jest-dom v7 does not self-register on import elsewhere — without this,
// `expect(el).toBeInTheDocument()` fails with "not a function", not a
// meaningful assertion failure.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  // Svelte 5 ships separate client and server runtimes selected by package
  // export condition. Without forcing "browser" here, Vite/Vitest resolve
  // the server (SSR) build for a component under test and `mount()` throws
  // "not available on the server" the moment any test tries to render one —
  // this is the standard fix documented for @testing-library/svelte + Vitest.
  resolve: {
    conditions: ["browser"],
  },
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.ts", "src/**/*.test.ts"],
    setupFiles: ["./tests/setup.ts"],
  },
});
