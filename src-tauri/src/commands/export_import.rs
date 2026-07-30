//! `export_clips` and `import_clips` (WP-09).
//!
//! `docs/src/architecture/contract.md` §2 is authoritative for both. Export is
//! the only user-facing recovery path this design has — a SQLite store is not
//! hand-readable and [ADR-0005](../../../docs/src/architecture/adr/0005-sqlite-store.md)
//! accepted that — so the rules below are about the file being trustworthy
//! rather than about it being written quickly.
//!
//! **The frontend opens the dialog and passes an absolute path** (closed
//! question 15). Neither command opens one: UI sequencing in the backend would
//! make both untestable without a GUI, and a cancelled dialog is a frontend
//! state that produces no `invoke` at all.
//!
//! **`export_clips` is the only thing in this application that writes an export
//! file.** There is no autosave, no backup-on-quit and no copy left behind: the
//! file is plaintext by design (spec §4.6), so every byte of it that exists on
//! disk is a byte the user asked for. The `<path>.part` the write goes through
//! is deleted on **every** failure path, including a failed rename.
//!
//! **`import_clips` never half-applies.** The whole file is validated in memory
//! before the store is touched, and the whole merge is one `BEGIN IMMEDIATE`
//! transaction. The work package's risk section says an import that half-applies
//! is worse than one that fails, and both halves of that are enforced here
//! rather than argued.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::commands::events;
use crate::commands::wire::{self, Clip};
use crate::error::{ClipError, IoOperation, IoReason};
use crate::export_file;
use crate::storage::clips::{self, ClipContent};
use crate::storage::Store;
use crate::tray;

/// The suffix of the file an export is written to before it is renamed into
/// place. In the target directory, because a rename is atomic only within one
/// volume.
const PART_SUFFIX: &str = ".part";

/// `{ "exported": n }` (contract §1). Zero is a success: an export of an empty
/// store writes an empty `clips` array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExportResult {
    pub exported: u32,
}

/// `{ "imported": n }` (contract §1). Zero is a success: an empty `clips` array
/// imports nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ImportResult {
    pub imported: u32,
}

/// Contract §4: `io` is a **user-chosen** file — an export target or an import
/// source — and never the store.
///
/// `path` is the path the user chose, never the temporary file beside it: the
/// `.part` name is an implementation detail they did not pick and will not find,
/// because it is deleted.
fn io_error(operation: IoOperation, path: &Path, error: &std::io::Error) -> ClipError {
    // The error is logged, the file's contents never are. An export file holds
    // every clip the user owns.
    log::error!(
        "an export or import file could not be reached ({operation:?}): {}",
        error.kind()
    );
    ClipError::Io {
        operation,
        path: path.to_string_lossy().into_owned(),
        reason: io_reason(error),
    }
}

/// Map an operating system error onto the four reasons contract §4 declares.
fn io_reason(error: &std::io::Error) -> IoReason {
    match error.kind() {
        std::io::ErrorKind::NotFound => IoReason::NotFound,
        std::io::ErrorKind::PermissionDenied => IoReason::PermissionDenied,
        std::io::ErrorKind::StorageFull => IoReason::DiskFull,
        // `ErrorKind::StorageFull` is what a modern standard library maps these
        // to, and it is checked above. The raw codes are kept because the
        // mapping is a standard-library implementation detail and the user's
        // remedy for a full disk — free some space — is different enough from
        // "other" to be worth not losing to one.
        _ => match error.raw_os_error() {
            // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL.
            Some(39) | Some(112) => IoReason::DiskFull,
            _ => IoReason::Other,
        },
    }
}

/// `<path>.part`, in the same directory as the target.
fn part_path(path: &Path) -> PathBuf {
    let mut part = path.as_os_str().to_owned();
    part.push(PART_SUFFIX);
    PathBuf::from(part)
}

/// Remove the temporary file, absorbing a failure.
///
/// **Called on every failure path, and this is a disclosure rule rather than
/// tidiness.** The temporary file carries every clip `value` in plaintext, and
/// leaving one under a name the user did not choose is
/// [criterion 6](../../../docs/src/product/spec.md#8-acceptance-criteria)
/// breached (contract, `export_clips`).
///
/// A failure to delete is logged rather than returned: it can only be another
/// process holding the file, and reporting it would replace the true error with
/// a second one about a file the user never asked about.
fn discard(part: &Path) {
    match fs::remove_file(part) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!(
            "an export temporary file could not be removed: {}",
            error.kind()
        ),
    }
}

/// Write the whole body and flush it to the device before returning.
///
/// `sync_all` is what makes the rename a commit point rather than a promise: a
/// renamed file whose contents are still in the operating system's cache is a
/// file a power cut can truncate after the rename has already happened. It is
/// the same rule [`crate::settings::write`] follows, for the same reason.
///
/// **This must stay a separate function.** The handle is closed by `file` being
/// dropped at the end of *this* scope, which is what lets the caller rename over
/// the target and delete the temporary file — Windows refuses both while a
/// handle is open. Inlining the three lines into the caller holds the handle
/// until the end of the caller, and the rename then fails on the commit path.
/// The same shape is load-bearing in `crate::crypto::keyfile`, where the file it
/// renames is the store's only key.
fn write_and_sync(path: &Path, body: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(body)?;
    file.sync_all()
}

/// Export every clip to a user-chosen `.json` path.
///
/// A thin wrapper over [`export`], on the same pattern as
/// [`crate::commands::clips::copy_clip`]: the command validates its argument and
/// the plain function does the work, so every rule below is reachable from a
/// test that has no application, no window and no runtime — including one that
/// kills the process in the middle of it.
#[tauri::command(rename_all = "snake_case")]
pub fn export_clips(
    store: State<'_, Store>,
    path: Option<String>,
) -> Result<ExportResult, ClipError> {
    let path = wire::require_path(path)?;
    export(&store, Path::new(&path))
}

/// The export itself.
///
/// **The file is written whole and atomically**: to `<path>.part` in the target
/// directory, then renamed onto `path`.
///
/// **It is read back and parsed before the rename**, and before the caller is
/// told it succeeded: reopen `<path>.part`, parse it as an `ExportFile`, and
/// assert the clip count equals the number exported. Failure is
/// `io { operation: "write", path, reason: "other" }` — `other` because the
/// write itself reported no fault, and what failed is the check that it produced
/// a readable file.
///
/// That readback is load-bearing rather than belt-and-braces. Export is the only
/// recovery path this design has, and
/// [criterion 7](../../../docs/src/product/spec.md#8-acceptance-criteria) has the
/// user wipe their store between the export and the import — so an export that
/// silently wrote a truncated file is the one failure that turns a backup into
/// data loss. A write that is never read back is not a backup.
///
/// Export requires the store to be unlocked (spec §4.8) and mutates nothing.
pub fn export(store: &Store, path: &Path) -> Result<ExportResult, ClipError> {
    // The guard decides `locked` and relays any recorded open-time fault. The
    // snapshot is taken inside it, so the file describes one consistent moment
    // rather than a list that changed while it was being written.
    let clips: Vec<Clip> = store.with_unlocked_store(|open| {
        Ok(clips::list(open)?
            .into_iter()
            .map(Clip::from)
            .collect::<Vec<Clip>>())
    })?;

    let body = export_file::render(&clips)?;
    let part = part_path(path);

    if let Err(error) = write_and_sync(&part, &body) {
        let failure = io_error(IoOperation::Write, path, &error);
        discard(&part);
        return Err(failure);
    }

    // The readback. Reopen the bytes that are on the disk — not the buffer that
    // was meant to reach it.
    if let Err(failure) = verify(&part, clips.len()) {
        discard(&part);
        return Err(failure);
    }

    // The commit point. On Windows this replaces the target in one operation, so
    // a reader sees the old file or the new one and never a partial write.
    if let Err(error) = fs::rename(&part, path) {
        let failure = io_error(IoOperation::Write, path, &error);
        // Contract, `export_clips`: deleted on every failure path, **including a
        // failed rename**. Without this line a complete plaintext copy of every
        // clip survives under a name the user did not choose.
        discard(&part);
        return Err(failure);
    }

    Ok(ExportResult {
        // A store cannot hold more clips than a `u32` counts; the saturation is
        // so that no arithmetic here can panic.
        exported: u32::try_from(clips.len()).unwrap_or(u32::MAX),
    })
}

/// Read `part` back, parse it, and assert it holds `expected` clips.
///
/// Both failures are `io { operation: "write", reason: "other" }` against the
/// user's chosen path, because from the caller's position one thing happened:
/// the export did not produce a file that can be imported.
fn verify(part: &Path, expected: usize) -> Result<(), ClipError> {
    let written = match fs::read(part) {
        Ok(written) => written,
        Err(error) => {
            log::error!(
                "an export could not be read back before the rename: {}",
                error.kind()
            );
            return Err(unreadable_export(part));
        }
    };

    match export_file::parsed_count(&written) {
        Ok(count) if count == expected => Ok(()),
        Ok(count) => {
            log::error!("an export wrote {count} clips where {expected} were exported");
            Err(unreadable_export(part))
        }
        Err(error) => {
            log::error!("an export did not parse as an export file: {error}");
            Err(unreadable_export(part))
        }
    }
}

/// The one variant a failed readback produces.
///
/// `reason` is `other` and not `disk_full`: the write reported no fault, so
/// guessing at the cause would put a remedy in front of the user that may have
/// nothing to do with what happened.
fn unreadable_export(part: &Path) -> ClipError {
    // The target, not the temporary file. `part` is the path this failed on, and
    // it is trimmed back to the name the user chose, because that file is the
    // one they will look for.
    let target = part
        .to_string_lossy()
        .strip_suffix(PART_SUFFIX)
        .map(str::to_owned)
        .unwrap_or_else(|| part.to_string_lossy().into_owned());

    ClipError::Io {
        operation: IoOperation::Write,
        path: target,
        reason: IoReason::Other,
    }
}

/// Merge a user-chosen `.json` file into the store.
///
/// A thin wrapper over [`import`], which does everything except the emission.
/// The event follows the commit and only a successful one (contract §3), and it
/// carries the list [`import`] took inside the same guard that made the merge —
/// so the payload cannot describe a store a later command has already changed.
#[tauri::command(rename_all = "snake_case")]
pub fn import_clips<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    path: Option<String>,
) -> Result<ImportResult, ClipError> {
    let path = wire::require_path(path)?;
    let (result, listed) = import(&store, Path::new(&path))?;
    events::emit_update_clips(&app, &listed);
    // The clip list changed, so the tray ranking may have (WP-08, spec §4.5).
    // Imported clips arrive with `use_count` 0, so they appear in the menu only
    // where list order reaches them.
    tray::refresh(&app);
    Ok(result)
}

/// The import itself, and the complete list to emit after it.
///
/// **Two phases, and the store is not touched during the first** (contract,
/// `import_clips`; closed question 10):
///
/// 1. **Validate whole.** Read the file, parse it, and validate every clip. The
///    first failure aborts with an `import` error naming the reason, the field
///    and the index of the offending clip. Nothing has been written.
/// 2. **Apply whole.** `BEGIN IMMEDIATE`, insert every clip with a freshly
///    minted id and `use_count` 0, `COMMIT`. Any error rolls back and is
///    `storage`.
///
/// **Merge semantics** (spec §4.6): every imported clip is *added*, appended in
/// file order after the user's existing clips. Nothing is deleted or
/// overwritten, there is no replace-all, and there is no deduplication —
/// importing the same file twice produces two copies of every clip.
pub fn import(store: &Store, path: &Path) -> Result<(ImportResult, Vec<Clip>), ClipError> {
    // Contract §2: a command marked "works while locked: no" returns `locked`
    // **before doing anything else**. Reading the user's file first and then
    // discovering the store is locked would be the same answer arrived at after
    // pulling every clip in that file into the memory of a locked application.
    if store.is_locked() {
        return Err(ClipError::Locked);
    }

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return Err(io_error(IoOperation::Read, path, &error)),
    };

    // Phase one. Nothing below this line has touched the store.
    let parsed = export_file::parse(&bytes)?;

    let pending: Vec<ClipContent<'_>> = parsed
        .iter()
        .map(|clip| ClipContent {
            label: &clip.label,
            value: &clip.value,
            colour: clip.colour,
        })
        .collect();

    // Phase two, inside the guard: the lock is re-checked after the connection
    // mutex is taken, and the whole merge is one transaction.
    let (imported, listed) = store.with_unlocked_store(|open| {
        let imported = clips::import(open, &pending)?;
        let listed: Vec<Clip> = clips::list(open)?.into_iter().map(Clip::from).collect();
        Ok((imported, listed))
    })?;

    Ok((
        ImportResult {
            imported: u32::try_from(imported).unwrap_or(u32::MAX),
        },
        listed,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;
    use crate::error::ImportReason;
    use crate::storage::StorePaths;
    use serde_json::{json, Value};

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn store_in(parent: &tempfile::TempDir) -> Store {
        Store::open(StorePaths::at(parent.path().join(".fast-clip")))
    }

    fn add(store: &Store, label: &str, value: &str) {
        let inserted =
            store.with_unlocked_store(|open| clips::insert(open, label, value, Colour::DEFAULT));
        if let Err(e) = inserted {
            panic!("the insert should succeed: {e}");
        }
    }

    fn listed(store: &Store) -> Vec<(String, String)> {
        match store.with_unlocked_store(|open| clips::list(open)) {
            Ok(rows) => rows.into_iter().map(|row| (row.label, row.value)).collect(),
            Err(e) => panic!("the list should succeed: {e}"),
        }
    }

    /// [`import`] without its emission payload, so an assertion reads as the
    /// command's return value rather than as a tuple.
    fn imported(store: &Store, path: &Path) -> Result<ImportResult, ClipError> {
        import(store, path).map(|(result, _)| result)
    }

    fn put(path: &Path, body: &str) {
        if let Err(e) = fs::write(path, body) {
            panic!("could not write the fixture: {e}");
        }
    }

    // ---- the round trip ----

    /// **The definition of done, and acceptance criterion 7.** Export, wipe the
    /// store, import: every clip returns. The wipe is a whole new store over a
    /// new directory, which is what "wipe" means to a user who deleted
    /// `~/.fast-clip/`.
    #[test]
    fn a_round_trip_through_a_wiped_store_returns_every_clip() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");

        let before = {
            let parent = dir();
            let store = store_in(&parent);
            for n in 0..7 {
                add(&store, &format!("clip {n}"), &format!("value {n}"));
            }
            let exported = match export(&store, &target) {
                Ok(result) => result,
                Err(e) => panic!("the export should succeed: {e}"),
            };
            assert_eq!(exported, ExportResult { exported: 7 });
            listed(&store)
        };

        // A different directory entirely: no database, no sidecars, nothing.
        let wiped = dir();
        let store = store_in(&wiped);
        assert_eq!(listed(&store), vec![]);

        assert_eq!(imported(&store, &target), Ok(ImportResult { imported: 7 }));
        assert_eq!(listed(&store), before);
    }

    /// The rename consumes the temporary file. A successful export leaves one
    /// file, and it is the one the user named.
    #[test]
    fn a_successful_export_leaves_no_temporary_file_behind() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "secret", "hunter2");

        if let Err(e) = export(&store, &target) {
            panic!("the export should succeed: {e}");
        }
        assert!(target.is_file());
        assert!(
            !part_path(&target).exists(),
            "a plaintext copy of every clip survived a successful export"
        );

        // And the directory holds exactly the one file.
        let entries: Vec<String> = match fs::read_dir(workspace.path()) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(e) => panic!("the directory should be readable: {e}"),
        };
        assert_eq!(entries, vec!["clips.json".to_string()]);
    }

    /// Exporting twice over one path replaces the file whole. The rename is the
    /// commit point, so there is no instant at which the target holds the first
    /// export's tail after the second export's head.
    #[test]
    fn a_second_export_replaces_the_first_whole() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = store_in(&parent);
        for n in 0..9 {
            add(&store, &format!("clip {n}"), &"a long value".repeat(30));
        }
        if let Err(e) = export(&store, &target) {
            panic!("the first export should succeed: {e}");
        }

        // A shorter store, so a partial overwrite would leave the first
        // export's tail behind and the file would not parse.
        let smaller = dir();
        let smaller_store = store_in(&smaller);
        add(&smaller_store, "only", "v");
        assert_eq!(
            export(&smaller_store, &target),
            Ok(ExportResult { exported: 1 })
        );

        let bytes = match fs::read(&target) {
            Ok(bytes) => bytes,
            Err(e) => panic!("the export should be readable: {e}"),
        };
        assert_eq!(export_file::parse(&bytes).map(|clips| clips.len()), Ok(1));
    }

    /// Contract §1: both results are one count, `snake_case`, and nothing else.
    #[test]
    fn the_result_shapes_are_exactly_one_count_each() {
        match serde_json::to_string(&ExportResult { exported: 7 }) {
            Ok(json) => assert_eq!(json, r#"{"exported":7}"#),
            Err(e) => panic!("the result should serialise: {e}"),
        }
        match serde_json::to_string(&ImportResult { imported: 0 }) {
            Ok(json) => assert_eq!(json, r#"{"imported":0}"#),
            Err(e) => panic!("the result should serialise: {e}"),
        }
    }

    #[test]
    fn an_export_of_an_empty_store_succeeds_and_imports_as_nothing() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = store_in(&parent);

        assert_eq!(export(&store, &target), Ok(ExportResult { exported: 0 }));
        assert_eq!(imported(&store, &target), Ok(ImportResult { imported: 0 }));
        assert_eq!(listed(&store), vec![]);
    }

    /// Merge, not replace. The user's clips keep their order and their place,
    /// and the imported ones are appended after them (contract, `import_clips`).
    #[test]
    fn an_import_appends_and_never_deletes_or_overwrites() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");

        let source = dir();
        let source_store = store_in(&source);
        add(&source_store, "imported one", "i1");
        add(&source_store, "imported two", "i2");
        if let Err(e) = export(&source_store, &target) {
            panic!("the export should succeed: {e}");
        }

        let parent = dir();
        let store = store_in(&parent);
        add(&store, "mine one", "m1");
        add(&store, "mine two", "m2");

        assert_eq!(imported(&store, &target), Ok(ImportResult { imported: 2 }));
        assert_eq!(
            listed(&store),
            vec![
                ("mine one".into(), "m1".into()),
                ("mine two".into(), "m2".into()),
                ("imported one".into(), "i1".into()),
                ("imported two".into(), "i2".into()),
            ]
        );
    }

    /// No deduplication. It follows from "import never overwrites" and is
    /// asserted so it is not mistaken for a defect (contract, `import_clips`).
    #[test]
    fn importing_the_same_file_twice_produces_two_copies_of_every_clip() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "only", "v");
        if let Err(e) = export(&store, &target) {
            panic!("the export should succeed: {e}");
        }

        assert_eq!(imported(&store, &target), Ok(ImportResult { imported: 1 }));
        assert_eq!(imported(&store, &target), Ok(ImportResult { imported: 1 }));
        assert_eq!(listed(&store).len(), 3);
    }

    /// Contract §1 `ExportFile`: two clips carrying one id produce two distinct
    /// clips, because import mints a fresh id for every one.
    #[test]
    fn colliding_ids_in_one_file_produce_distinct_clips() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let duplicated = "8a1f0c6e-0000-4000-8000-000000000001";
        put(
            &target,
            &json!({
                "format": export_file::FORMAT,
                "version": 1,
                "clips": [
                    { "id": duplicated, "label": "first", "value": "v1", "colour": "teal" },
                    { "id": duplicated, "label": "second", "value": "v2", "colour": "teal" },
                    { "id": duplicated, "label": "third", "value": "v3", "colour": "teal" },
                ],
            })
            .to_string(),
        );

        let parent = dir();
        let store = store_in(&parent);
        assert_eq!(imported(&store, &target), Ok(ImportResult { imported: 3 }));

        let ids = match store.with_unlocked_store(|open| clips::ids_in_order(open)) {
            Ok(ids) => ids,
            Err(e) => panic!("the list should succeed: {e}"),
        };
        let distinct: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(distinct.len(), 3, "every imported clip gets a fresh id");
        assert!(
            !ids.iter()
                .any(|id| id.as_hyphenated().to_string() == duplicated),
            "the file's id must never be stored"
        );
    }

    /// The imported clips start at zero, whatever the exporting machine's counts
    /// were (spec §4.6, ADR-0008). The file cannot carry one, so this asserts the
    /// insert rather than the parse.
    #[test]
    fn imported_clips_start_at_a_use_count_of_zero_and_dense_positions() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let source = dir();
        let source_store = store_in(&source);
        for n in 0..3 {
            add(&source_store, &format!("clip {n}"), "v");
        }
        if let Err(e) = export(&source_store, &target) {
            panic!("the export should succeed: {e}");
        }

        let parent = dir();
        let store = store_in(&parent);
        add(&store, "mine", "v");
        if let Err(e) = imported(&store, &target) {
            panic!("the import should succeed: {e}");
        }

        let rows = match store.with_unlocked_store(|open| clips::list(open)) {
            Ok(rows) => rows,
            Err(e) => panic!("the list should succeed: {e}"),
        };
        assert!(rows.iter().all(|row| row.use_count == 0));
        assert_eq!(
            rows.iter().map(|row| row.position).collect::<Vec<i64>>(),
            vec![0, 1, 2, 3]
        );
    }

    // ---- the failure paths ----

    /// **The package's risk section, made to happen.** One bad record anywhere
    /// in the file changes nothing at all.
    #[test]
    fn an_import_with_one_bad_record_changes_nothing() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");

        let faults: Vec<(Value, ImportReason, Option<&str>, Option<u32>)> = vec![
            (
                json!("chartreuse"),
                ImportReason::InvalidValue,
                Some("colour"),
                Some(3),
            ),
            (
                json!(""),
                ImportReason::InvalidValue,
                Some("colour"),
                Some(3),
            ),
            (
                json!(42),
                ImportReason::InvalidValue,
                Some("colour"),
                Some(3),
            ),
            (
                json!(null),
                ImportReason::InvalidValue,
                Some("colour"),
                Some(3),
            ),
        ];

        for (bad, reason, field, index) in faults {
            let mut clips: Vec<Value> = (0..6)
                .map(|n| json!({ "label": format!("clip {n}"), "value": "v", "colour": "teal" }))
                .collect();
            if let Some(object) = clips[3].as_object_mut() {
                object.insert("colour".into(), bad.clone());
            }
            put(
                &target,
                &json!({ "format": export_file::FORMAT, "version": 1, "clips": clips }).to_string(),
            );

            let parent = dir();
            let store = store_in(&parent);
            add(&store, "mine", "m");

            assert_eq!(
                imported(&store, &target),
                Err(ClipError::Import {
                    reason,
                    field: field.map(str::to_owned),
                    index,
                }),
                "for the fault {bad}"
            );
            assert_eq!(
                listed(&store),
                vec![("mine".into(), "m".into())],
                "not one clip may have been written"
            );
        }
    }

    /// A truncated file is rejected and the store is untouched. Every cut, so
    /// the test does not depend on guessing where a real truncation lands.
    #[test]
    fn every_truncation_of_an_export_is_rejected_and_writes_nothing() {
        let workspace = dir();
        let source = dir();
        let source_store = store_in(&source);
        for n in 0..4 {
            add(&source_store, &format!("clip {n}"), "a value");
        }
        let whole = workspace.path().join("whole.json");
        if let Err(e) = export(&source_store, &whole) {
            panic!("the export should succeed: {e}");
        }
        let bytes = match fs::read(&whole) {
            Ok(bytes) => bytes,
            Err(e) => panic!("the export should be readable: {e}"),
        };

        let parent = dir();
        let store = store_in(&parent);
        add(&store, "mine", "m");

        // The renderer ends the file with a newline, and cutting that alone
        // leaves a complete document — JSON is not whitespace-sensitive, so
        // asserting that prefix fails would assert the wrong thing.
        assert_eq!(bytes.last(), Some(&b'\n'));
        let significant = bytes.len() - 1;

        let truncated = workspace.path().join("truncated.json");
        for cut in 0..significant {
            if let Err(e) = fs::write(&truncated, &bytes[..cut]) {
                panic!("could not write the fixture: {e}");
            }
            match imported(&store, &truncated) {
                Err(ClipError::Import { .. }) => {}
                other => panic!("a file cut to {cut} bytes gave {other:?}"),
            }
            assert_eq!(listed(&store), vec![("mine".into(), "m".into())]);
        }
    }

    #[test]
    fn an_import_of_a_file_that_is_not_there_is_io_not_found() {
        let workspace = dir();
        let missing = workspace.path().join("nowhere.json");
        let parent = dir();
        let store = store_in(&parent);

        assert_eq!(
            imported(&store, &missing),
            Err(ClipError::Io {
                operation: IoOperation::Read,
                path: missing.to_string_lossy().into_owned(),
                reason: IoReason::NotFound,
            })
        );
    }

    /// A directory where the file should be. The read fails for a reason other
    /// than absence, and the answer must not be `not_found` — the user's remedy
    /// is different.
    #[test]
    fn an_import_of_something_that_is_not_a_file_is_io_and_not_not_found() {
        let workspace = dir();
        let target = workspace.path().join("a-directory.json");
        if let Err(e) = fs::create_dir(&target) {
            panic!("could not create the fixture: {e}");
        }
        let parent = dir();
        let store = store_in(&parent);

        match imported(&store, &target) {
            Err(ClipError::Io {
                operation: IoOperation::Read,
                reason,
                ..
            }) => assert_ne!(reason, IoReason::NotFound),
            other => panic!("expected an io error, got {other:?}"),
        }
    }

    /// An export into a directory that does not exist fails, and — the point of
    /// the test — leaves nothing behind. A `.part` holding every clip in
    /// plaintext must not survive a failure.
    #[test]
    fn a_failed_export_leaves_no_temporary_file() {
        let workspace = dir();
        let target = workspace.path().join("nowhere").join("clips.json");
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "secret", "hunter2");

        match export(&store, &target) {
            Err(ClipError::Io {
                operation: IoOperation::Write,
                ..
            }) => {}
            other => panic!("expected an io write error, got {other:?}"),
        }
        assert!(!part_path(&target).exists());
        assert!(!target.exists());
    }

    /// **A failed rename is a failure path too**, and the contract names it
    /// explicitly. A directory at the target makes the rename fail after the
    /// temporary file has been written and verified, which is the one moment a
    /// complete plaintext copy of every clip exists under a name the user did
    /// not choose.
    #[test]
    fn a_failed_rename_deletes_the_temporary_file_that_holds_every_clip() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        if let Err(e) = fs::create_dir(&target) {
            panic!("could not create the fixture: {e}");
        }
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "secret", "hunter2");

        let part = part_path(&target);
        match export(&store, &target) {
            Err(ClipError::Io {
                operation: IoOperation::Write,
                ..
            }) => {}
            other => panic!("expected an io write error, got {other:?}"),
        }
        assert!(
            !part.exists(),
            "a plaintext copy of every clip survived a failed rename"
        );
    }

    /// A failed export must not damage a file that was already there. The
    /// previous export is still a usable backup.
    #[test]
    fn a_failed_export_leaves_an_earlier_export_intact() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "first", "v1");
        if let Err(e) = export(&store, &target) {
            panic!("the first export should succeed: {e}");
        }
        let first = match fs::read(&target) {
            Ok(bytes) => bytes,
            Err(e) => panic!("the export should be readable: {e}"),
        };

        // A directory at the temporary path: `File::create` cannot replace it,
        // so the write fails before the readback and before the rename.
        add(&store, "second", "v2");
        if let Err(e) = fs::create_dir(part_path(&target)) {
            panic!("could not create the fixture: {e}");
        }
        match export(&store, &target) {
            Err(ClipError::Io { .. }) => {}
            other => panic!("expected an io error, got {other:?}"),
        }

        match fs::read(&target) {
            Ok(after) => assert_eq!(after, first, "the earlier export was damaged"),
            Err(e) => panic!("the earlier export should still be readable: {e}"),
        }
    }

    /// **The readback is not decoration.** A `.part` whose contents were
    /// replaced between the write and the check is caught, the file is deleted,
    /// and the export reports failure rather than reporting success over a
    /// truncated backup.
    #[test]
    fn a_temporary_file_that_does_not_read_back_fails_the_export() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let part = part_path(&target);

        // The count assertion: a file that parses but holds the wrong number.
        put(
            &part,
            &json!({ "format": export_file::FORMAT, "version": 1, "clips": [] }).to_string(),
        );
        assert_eq!(
            verify(&part, 3),
            Err(ClipError::Io {
                operation: IoOperation::Write,
                path: target.to_string_lossy().into_owned(),
                reason: IoReason::Other,
            })
        );

        // The parse assertion: a file that is not an export at all.
        put(&part, "{\"format\": \"fastclip-export\", \"vers");
        match verify(&part, 0) {
            Err(ClipError::Io {
                reason: IoReason::Other,
                ..
            }) => {}
            other => panic!("expected io/other, got {other:?}"),
        }

        // And a file that is not there.
        discard(&part);
        match verify(&part, 0) {
            Err(ClipError::Io {
                reason: IoReason::Other,
                ..
            }) => {}
            other => panic!("expected io/other, got {other:?}"),
        }
    }

    #[test]
    fn the_readback_reports_the_users_path_rather_than_the_temporary_one() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let part = part_path(&target);
        match unreadable_export(&part) {
            ClipError::Io { path, .. } => {
                assert_eq!(path, target.to_string_lossy());
                assert!(!path.ends_with(PART_SUFFIX));
            }
            other => panic!("expected an io error, got {other:?}"),
        }
    }

    // ---- locking ----

    fn locked_store(parent: &tempfile::TempDir) -> Store {
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // A SQLCipher database begins with a random salt, so the header does not
        // match the SQLite magic and the store classifies as encrypted.
        if let Err(e) = fs::write(paths.db(), [0x1f; 64]) {
            panic!("could not write the fixture: {e}");
        }
        Store::open(paths)
    }

    /// Spec §4.8: export requires the store to be unlocked. Nothing is written,
    /// which matters more here than for most commands — the file would be a
    /// plaintext copy of a store the user locked.
    #[test]
    fn a_locked_store_refuses_an_export_and_writes_no_file() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = locked_store(&parent);

        assert_eq!(export(&store, &target).map(drop), Err(ClipError::Locked));
        assert!(!target.exists());
        assert!(!part_path(&target).exists());
    }

    /// And the import is refused **before the file is read**, so a locked
    /// application never pulls a file full of clips into memory.
    #[test]
    fn a_locked_store_refuses_an_import_before_reading_the_file() {
        let workspace = dir();
        let missing = workspace.path().join("nowhere.json");
        let parent = dir();
        let store = locked_store(&parent);

        // The file does not exist. `locked` rather than `io { not_found }` is
        // what proves the lock check came first.
        assert_eq!(imported(&store, &missing).map(drop), Err(ClipError::Locked));
    }

    // ---- durability ----

    /// An import that committed survives an immediate process kill. Leaking the
    /// connection without closing it is the closest a unit test gets to one —
    /// nothing is checkpointed and no destructor runs, so the inserted rows exist
    /// only in the write-ahead log when the store is reopened (ADR-0009).
    #[test]
    fn an_imported_clip_survives_a_process_kill_immediately_after_the_import() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let source = dir();
        let source_store = store_in(&source);
        for n in 0..3 {
            add(&source_store, &format!("clip {n}"), "a value");
        }
        if let Err(e) = export(&source_store, &target) {
            panic!("the export should succeed: {e}");
        }

        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        {
            let store = Store::open(paths.clone());
            if let Err(e) = imported(&store, &target) {
                panic!("the import should succeed: {e}");
            }
            // No `shutdown()`, no checkpoint, no close.
            std::mem::forget(store);
        }

        let reopened = Store::open(paths);
        assert_eq!(listed(&reopened).len(), 3);
    }

    /// **The half-applied import, made to happen.** The transaction is abandoned
    /// after every clip has been inserted and before the commit, which is what a
    /// process killed between the last insert and the commit leaves behind. The
    /// store must read back as it was.
    #[test]
    fn an_import_abandoned_before_its_commit_leaves_the_store_exactly_as_it_was() {
        use rusqlite::TransactionBehavior;

        let parent = dir();
        let store = store_in(&parent);
        add(&store, "mine", "m");

        let outcome = store.with_unlocked_store(|open| {
            let transaction = match open.transaction_with_behavior(TransactionBehavior::Immediate) {
                Ok(transaction) => transaction,
                Err(e) => panic!("the transaction should open: {e}"),
            };
            let base: i64 = match transaction.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM clips",
                [],
                |row| row.get(0),
            ) {
                Ok(base) => base,
                Err(e) => panic!("the base position should be readable: {e}"),
            };
            for n in 0..5 {
                if let Err(e) = transaction.execute(
                    "INSERT INTO clips (id, label, value, colour, use_count, position)
                     VALUES (?1, ?2, 'v', 'teal', 0, ?3)",
                    (
                        uuid::Uuid::new_v4().as_hyphenated().to_string(),
                        format!("imported {n}"),
                        base + n,
                    ),
                ) {
                    panic!("the fixture insert should succeed: {e}");
                }
            }
            // Inside the transaction all five are visible.
            let inside = clips::count(&transaction)?;
            // No commit. Dropping rolls it back.
            Ok(inside)
        });
        assert_eq!(outcome, Ok(6));

        assert_eq!(listed(&store), vec![("mine".into(), "m".into())]);
    }

    // ---- nothing leaks ----

    /// No clip `label` or `value` reaches an error that crosses the seam
    /// (ADR-0002). The path does, because the contract's `io` variant carries
    /// it and the user chose it.
    #[test]
    fn no_error_from_either_command_carries_a_clip() {
        let workspace = dir();
        let secret = "hunter2-hunter2";
        let target = workspace.path().join("clips.json");
        put(
            &target,
            &json!({
                "format": export_file::FORMAT,
                "version": 1,
                "clips": [{ "label": secret, "value": secret, "colour": "chartreuse" }],
            })
            .to_string(),
        );

        let parent = dir();
        let store = store_in(&parent);
        match imported(&store, &target) {
            Ok(_) => panic!("this fixture should have been refused"),
            Err(error) => {
                let rendered = format!("{error} {error:?}");
                assert!(!rendered.contains(secret), "{rendered}");
            }
        }

        // And the export's failure path, over a store that holds a secret.
        add(&store, secret, secret);
        let unreachable = workspace.path().join("nowhere").join("clips.json");
        match export(&store, &unreachable) {
            Ok(_) => panic!("this export should have failed"),
            Err(error) => {
                let rendered = format!("{error} {error:?}");
                assert!(!rendered.contains(secret), "{rendered}");
            }
        }
    }

    /// `ClipContent` holds a label and a value and must never gain a `Debug`
    /// derive. Asserted structurally: the type is constructed and the test
    /// compiles only while nothing formats it.
    #[test]
    fn the_exported_file_holds_the_clip_and_the_error_does_not() {
        let workspace = dir();
        let target = workspace.path().join("clips.json");
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "Support greeting", "hunter2");
        if let Err(e) = export(&store, &target) {
            panic!("the export should succeed: {e}");
        }

        match fs::read_to_string(&target) {
            Ok(body) => {
                // The file is plaintext by design (spec §4.6). That is the whole
                // reason the warning exists on the frontend.
                assert!(body.contains("hunter2"), "the export lost the clip");
            }
            Err(e) => panic!("the export should be readable: {e}"),
        }
    }
}
