# Summary

[Introduction](./introduction.md)

---

# Product

- [Specification](./product/spec.md)
- [Colour palette](./product/palette.md)
- [User-facing copy](./product/copy.md)

# Architecture

- [IPC contract](./architecture/contract.md)
- [Storage and encryption](./architecture/storage.md)
- [Decision records](./architecture/adr/index.md)
  - [0001 — Plain Svelte, not SvelteKit](./architecture/adr/0001-plain-svelte-not-sveltekit.md)
  - [0002 — Threat model and security scope](./architecture/adr/0002-threat-model.md)
  - [0003 — No legacy migration](./architecture/adr/0003-no-legacy-migration.md)
  - [0004 — Optional PIN-gated encryption](./architecture/adr/0004-optional-pin-encryption.md)
  - [0005 — SQLite via SQLCipher](./architecture/adr/0005-sqlite-store.md)
  - [0006 — Tailwind as the styling system](./architecture/adr/0006-tailwind.md)
  - [0007 — List order representation](./architecture/adr/0007-list-order-representation.md)
  - [0008 — use_count stays backend-side](./architecture/adr/0008-use-count-stays-backend-side.md)
  - [0009 — Durability level](./architecture/adr/0009-durability-level.md)
  - [0010 — Manual lock](./architecture/adr/0010-manual-lock.md)
  - [0011 — Flat backoff](./architecture/adr/0011-flat-backoff.md)
  - [0012 — Logging](./architecture/adr/0012-logging.md)
  - [0013 — The unlock_requested event](./architecture/adr/0013-unlock-requested-event.md)
  - [0014 — Tray label truncation](./architecture/adr/0014-tray-label-truncation.md)

# Process

- [The agent team](./process/team.md)
- [The gated pipeline](./process/pipeline.md)
- [Reporting templates](./process/reporting.md)
- [Conventions](./process/conventions.md)

# Work

- [Work packages](./work/index.md)
  - [WP-01 Contract ratification](./work/wp-01-contract.md)
  - [WP-02 Toolchain and CI skeleton](./work/wp-02-toolchain.md)
  - [WP-03 Storage: ordered and crash-safe](./work/wp-03-storage.md)
  - [WP-04 Frontend scaffold](./work/wp-04-frontend-scaffold.md)
  - [WP-05 Clip list, copy, CRUD](./work/wp-05-crud.md)
  - [WP-06 Reordering](./work/wp-06-reorder.md)
  - [WP-08 Tray icon](./work/wp-08-tray.md)
  - [WP-09 Export and import](./work/wp-09-export-import.md)
  - [WP-07 Optional PIN-gated encryption](./work/wp-07-encryption.md)
  - [WP-10 Palette and accessibility](./work/wp-10-palette.md)
  - [WP-11 Copy deck and README](./work/wp-11-copy-deck.md)
  - [WP-13 Search and filter](./work/wp-13-search.md)
  - [WP-14 Settings and window state](./work/wp-14-settings.md)
  - [WP-12 Release pipeline](./work/wp-12-release.md)

# Reviews

- [Review log](./reviews/index.md)
  - [002 — WP-02 Toolchain and CI skeleton](./reviews/002-wp-02-toolchain.md)
  - [003 — WP-01 Contract ratification](./reviews/003-wp-01-contract.md)
  - [004 — WP-10 Palette and accessibility](./reviews/004-wp-10-palette.md)
  - [005 — WP-05 Clip list, copy, CRUD](./reviews/005-wp-05-crud.md)
  - [006 — WP-14 Settings and window state](./reviews/006-wp-14-settings.md)
  - [007 — WP-06 Reordering](./reviews/007-wp-06-reorder.md)
  - [008 — WP-09 Export and import](./reviews/008-wp-09-export-import.md)
  - [009 — WP-13 Search and filter](./reviews/009-wp-13-search.md)
  - [010 — WP-07 Optional PIN-gated encryption](./reviews/010-wp-07-encryption.md)
  - [011 — WP-11 Copy deck and README](./reviews/011-wp-11-copy-deck.md)
  - [012 — WP-08 Tray icon](./reviews/012-wp-08-tray.md)
  - [013 — WP-08 rework: the unlock_requested event](./reviews/013-wp-08-rework.md)

# Reference

- [Inherited debt](./reference/debt.md)
- [Recurring defect classes](./reference/defect-classes.md)
- [Glossary](./reference/glossary.md)
