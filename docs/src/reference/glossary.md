# Glossary

Terms with a specific meaning here. Use them this way in code, in commits, and
in this book.

| Term | Meaning |
| ---- | ------- |
| **clip** | One stored snippet: `id`, `label`, `value`, `colour`, `use_count`. Never "macro", "entry", "item" or "snippet" in code. |
| **label** | The short text on the button. |
| **value** | The text placed on the clipboard. Never the button's caption. |
| **colour** | A palette token name, never a hex string. See [palette](../product/palette.md). |
| **the store** | The SQLite database at `~/.fast-clip/`, encrypted only if the user opted in. The pre-refactor JSON file at `%LOCALAPPDATA%\FastClip\db` is *not* the store and is never read. |
| **the contract** | [`architecture/contract.md`](../architecture/contract.md). |
| **the seam** | The Tauri IPC boundary. Owned by `architect`, crossed only via the contract. |
| **gate** | A stage in the [pipeline](../process/pipeline.md) with a file as its exit artifact. |
| **exit artifact** | The file whose existence proves a gate was passed. Not a chat message. |
| **finding** | A `critic` observation with a location and a concrete failure scenario. Without both it is a preference. |
| **waived** | A finding the orchestrator chose not to fix, with the reason recorded. Distinct from ignored. |
| **migration** | A change to this project's own SQLite schema, versioned with `PRAGMA user_version`. It never means reading pre-refactor data — [ADR-0003](../architecture/adr/0003-no-legacy-migration.md) removed that entirely. |
| **locked** | Encryption is on and the PIN has not been entered. No label or value is reachable, including from the tray. |
| **DEK** | The random 256-bit data encryption key. SQLCipher consumes it; it is wrapped by `Argon2id(PIN, salt)` and then DPAPI-protected ([ADR-0004](../architecture/adr/0004-optional-pin-encryption.md)). |

## Words to avoid

| Word | Why |
| ---- | --- |
| **macro** | The README's term for a clip. It implies executable behaviour. Being retired. |
| **just** | As in "just add a field". It smuggles an estimate past review. Say what the change touches. |
| **secure** | Unqualified, a claim this software cannot support. State what is protected against what, per [ADR-0002](../architecture/adr/0002-threat-model.md). |
