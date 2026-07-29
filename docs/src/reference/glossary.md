# Glossary

Terms with a specific meaning here. Use them this way in code, in commits, and
in this book.

| Term | Meaning |
| ---- | ------- |
| **clip** | One stored snippet: `id`, `label`, `value`, `colour`. Never "macro", "entry", "item" or "snippet" in code. |
| **label** | The short text on the button. |
| **value** | The text placed on the clipboard. Never the button's caption. |
| **colour** | A palette token name, never a hex string. See [palette](../product/palette.md). |
| **the store** | The encrypted file at `%LOCALAPPDATA%\FastClip\`. Not "the database"; calling it that is how `surrealdb` reached `Cargo.toml`. |
| **the contract** | [`architecture/contract.md`](../architecture/contract.md). |
| **the seam** | The Tauri IPC boundary. Owned by `architect`, crossed only via the contract. |
| **gate** | A stage in the [pipeline](../process/pipeline.md) with a file as its exit artifact. |
| **exit artifact** | The file whose existence proves a gate was passed. Not a chat message. |
| **finding** | A `critic` observation with a location and a concrete failure scenario. Without both it is a preference. |
| **waived** | A finding the orchestrator chose not to fix, with the reason recorded. Distinct from ignored. |
| **migration** | The one-time conversion of a pre-refactor plaintext store to the encrypted format. The highest-risk code in the project. |

## Words to avoid

| Word | Why |
| ---- | --- |
| **macro** | The README's term for a clip. It implies executable behaviour. Being retired. |
| **just** | As in "just add a field". It smuggles an estimate past review. Say what the change touches. |
| **secure** | Unqualified, a claim this software cannot support. State what is protected against what, per [ADR-0002](../architecture/adr/0002-threat-model.md). |
