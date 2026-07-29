#!/usr/bin/env node
// Catches a desynced package-lock.json before it reaches CI.
//
// `npm ci` refuses to install when package.json and package-lock.json
// disagree, which is correct, but the frontend CI job only discovers that
// after checkout — every check in that job (typecheck, svelte-check,
// vitest, build, npm audit) reports as failed for the wrong reason, and the
// gate becomes a no-op until someone reads the log far enough to see
// "Missing: <pkg> from lock file".
//
// This script runs the same check locally, in seconds, with nothing else
// attached, so the failure is diagnosed before a push rather than after.
//
// Run: node scripts/check-lockfile.mjs
// Not wired into package.json's "scripts" — that file is under active work
// (WP-04) by frontend-dev. Wiring this in as a pre-commit/pre-push hook or
// an npm script is a decision for whoever owns that file next.

import { spawnSync } from "node:child_process";

const result = spawnSync("npm", ["ci", "--dry-run", "--ignore-scripts"], {
  stdio: "inherit",
  shell: process.platform === "win32",
});

if (result.status !== 0) {
  console.error(
    "\ncheck-lockfile: package.json and package-lock.json are out of sync.\n" +
      "Run `npm install` to update the lockfile, review the diff, and commit both files together.",
  );
  process.exit(result.status ?? 1);
}

console.log("check-lockfile: ok, package-lock.json matches package.json");
