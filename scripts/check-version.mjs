#!/usr/bin/env node
// Enforces the single source of truth for the project version.
//
// Cargo.toml (src-tauri/Cargo.toml) is authoritative. package.json must
// declare the identical version, because npm tooling (vitest, npm audit
// output, release tags) reads it there. tauri.conf.json must NOT declare a
// "version" field: Tauri falls back to Cargo.toml's package.version when
// the field is absent, which is how the config stays out of the sync
// problem entirely rather than being a third place that can drift.
//
// Run: node scripts/check-version.mjs

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

function fail(message) {
  console.error(`check-version: ${message}`);
  process.exitCode = 1;
}

function readCargoVersion() {
  const cargoPath = path.join(repoRoot, "src-tauri", "Cargo.toml");
  const text = readFileSync(cargoPath, "utf8");
  // Split into per-section chunks on lines that open a new TOML table, then
  // isolate the one headed "[package]". This does not depend on `version`
  // being the first key, or on no other key in the section holding a
  // literal "[" (e.g. `authors = ["you"]`, or an array-valued key like
  // `keywords`/`categories`/`exclude` landing above it).
  const sections = text.split(/^(?=\[)/m);
  const packageSection = sections.find((section) => /^\[package\]/.test(section));
  if (!packageSection) {
    fail(`could not find [package] section in ${cargoPath}`);
    return null;
  }
  const match = packageSection.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    fail(`could not find version in [package] section of ${cargoPath}`);
    return null;
  }
  return match[1];
}

function readPackageJsonVersion() {
  const pkgPath = path.join(repoRoot, "package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  if (!pkg.version) {
    fail(`package.json has no "version" field`);
    return null;
  }
  return pkg.version;
}

function readTauriConfVersion() {
  const confPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
  const conf = JSON.parse(readFileSync(confPath, "utf8"));
  return Object.prototype.hasOwnProperty.call(conf, "version") ? conf.version : undefined;
}

const cargoVersion = readCargoVersion();
const packageVersion = readPackageJsonVersion();
const tauriConfVersion = readTauriConfVersion();

if (cargoVersion && !SEMVER.test(cargoVersion)) {
  fail(`src-tauri/Cargo.toml version "${cargoVersion}" is not valid semver`);
}

if (cargoVersion && packageVersion && cargoVersion !== packageVersion) {
  fail(
    `version mismatch: src-tauri/Cargo.toml has "${cargoVersion}", package.json has "${packageVersion}"`,
  );
}

if (tauriConfVersion !== undefined && tauriConfVersion !== cargoVersion) {
  fail(
    `src-tauri/tauri.conf.json declares "version": "${tauriConfVersion}", which no longer ` +
      `agrees with src-tauri/Cargo.toml ("${cargoVersion}"). Remove the field so Tauri ` +
      `inherits from Cargo.toml, or update it to match.`,
  );
}

if (process.exitCode) {
  process.exit(process.exitCode);
}

console.log(`check-version: ok, version is ${cargoVersion}`);
