//! The five clip commands of WP-05: list, create, update, delete, copy.
//!
//! `docs/src/architecture/contract.md` §2 is authoritative for every command
//! name, argument name, return shape, error variant and event. Where this file
//! and that page disagree, this file is the defect.
//!
//! Three rules govern all five.
//!
//! **Lock state is checked in one place.** Every command here is "works while
//! locked: no", and none of them checks lock state itself: they run their work
//! inside [`Store::with_unlocked_store`], which checks on entry and again after
//! it takes the connection mutex. The second check is what makes a concurrent
//! `lock` produce `locked` rather than a torn write
//! (`storage.md` § Locking on demand).
//!
//! **The event follows the commit, and only a successful one.** Each mutating
//! command takes its snapshot of the list *inside* the same guard that made the
//! mutation, so the payload cannot describe a store that a later command has
//! already changed. A command that returns `Err` never reaches the emission.
//!
//! **Nothing here logs a clip.** The arguments hold a `label` and a `value`, and
//! `println!` printing clip values is one of the defects this refactor exists to
//! remove.

use tauri::{AppHandle, Runtime, State};
use uuid::Uuid;

use crate::commands::clipboard::{AppClipboard, ClipboardWriter};
use crate::commands::events;
use crate::commands::wire::{self, Clip, ClipPayload};
use crate::error::ClipError;
use crate::storage::{clips, Store};

/// The complete list, in display order, as the wire sees it.
///
/// One function so that `list_clips` and every `update_clips` payload are the
/// same query. A second spelling of "the list" is how the two drift.
fn snapshot(connection: &rusqlite::Connection) -> Result<Vec<Clip>, ClipError> {
    Ok(clips::list(connection)?
        .into_iter()
        .map(Clip::from)
        .collect())
}

/// Every clip, in display order.
///
/// Errors: `locked`, `storage`. **Never an open-time fault** — it is reachable
/// only once the database is open, so `crypto` and `unsupported_version` cannot
/// originate here (contract § Opening the database).
#[tauri::command(rename_all = "snake_case")]
pub fn list_clips(store: State<'_, Store>) -> Result<Vec<Clip>, ClipError> {
    store.with_unlocked_store(|open| snapshot(open))
}

/// Append a clip. The backend mints the id; `use_count` starts at 0.
///
/// Returns `null` rather than the created clip: `update_clips` is the only path
/// by which list state reaches the frontend, so there is exactly one place to
/// apply it (contract, closed question 14).
#[tauri::command(rename_all = "snake_case")]
pub fn create_clip<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    clip: Option<ClipPayload>,
) -> Result<(), ClipError> {
    let draft = wire::validate_draft(clip)?;

    let listed = store.with_unlocked_store(|open| {
        clips::insert(open, &draft.label, &draft.value, draft.colour)?;
        snapshot(open)
    })?;

    events::emit_update_clips(&app, &listed);
    Ok(())
}

/// Rewrite a clip's content. `id`, `position` and `use_count` are unchanged.
///
/// An unknown id is `not_found`. It never creates a clip — the pre-refactor
/// build shared one `insert_or_update_clip` between both commands, so an update
/// against a stale id silently made a second clip.
#[tauri::command(rename_all = "snake_case")]
pub fn update_clip<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    clip: Option<ClipPayload>,
) -> Result<(), ClipError> {
    let updated = wire::validate_clip(clip)?;

    let listed = store.with_unlocked_store(|open| {
        clips::update(
            open,
            updated.id,
            &updated.label,
            &updated.value,
            updated.colour,
        )?;
        snapshot(open)
    })?;

    events::emit_update_clips(&app, &listed);
    Ok(())
}

/// Delete a clip and renumber the remainder, in one transaction.
///
/// Confirmation is a frontend concern (spec §4.2). The backend deletes when
/// asked.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_clip<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    clip_id: Option<String>,
) -> Result<(), ClipError> {
    let id = wire::require_clip_id(clip_id)?;

    let listed = store.with_unlocked_store(|open| {
        clips::delete(open, id)?;
        snapshot(open)
    })?;

    events::emit_update_clips(&app, &listed);
    Ok(())
}

/// Write a clip to the clipboard and record the copy.
///
/// A thin wrapper over [`copy`], which the tray calls directly (WP-08) so that a
/// tray copy generates no IPC traffic and there is exactly one clipboard
/// implementation.
///
/// Emits **nothing**: a copy changes only `use_count`, which nothing on the
/// frontend displays or orders by, and an event per click would rebuild the tray
/// menu on the hottest path in the application (ADR-0008).
#[tauri::command(rename_all = "snake_case")]
pub fn copy_clip<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    clip_id: Option<String>,
) -> Result<(), ClipError> {
    let id = wire::require_clip_id(clip_id)?;
    copy(&store, &AppClipboard(&app), id)
}

/// The single clipboard write, for the window and the tray alike (spec §4.5).
///
/// **The order is fixed by spec §4.1 and acceptance criterion 9**, and the whole
/// sequence runs inside one guard so that the clip cannot be deleted between the
/// lookup and the increment:
///
/// 1. Look up the clip. Unknown id → `not_found`, nothing else happens.
/// 2. Write `value` to the clipboard. Failure → `clipboard`, and `use_count` is
///    not incremented.
/// 3. `UPDATE clips SET use_count = use_count + 1`, committed.
/// 4. Return.
///
/// The clipboard write comes first because a count recorded for a copy that
/// never reached the clipboard puts a phantom into the tray ranking, which is
/// worse than a count that is one low. The commit completes before returning
/// because spec §4.1 requires it; what it costs is settled by ADR-0009 —
/// `synchronous = NORMAL` in WAL mode, so the commit has left the process when
/// `execute` returns and a process kill cannot lose it.
///
/// The clipboard write holds the connection mutex, so a concurrent `lock` waits
/// behind it rather than closing the database underneath step 3. That is the
/// same guarantee `lock` gives every other command, bounded here by one
/// clipboard call.
///
/// **One case the contract does not name.** It says a step 3 failure is
/// `storage`, and the only way step 3 can find no row after step 1 found one is
/// a second FastClip process deleting it in between — this process cannot,
/// because both statements run on one connection behind one mutex. That
/// `not_found` is returned as itself rather than recast as `storage`: it is
/// already in `copy_clip`'s declared error set, it is the truth, and it makes
/// the frontend resynchronise, whereas "the store could not be written" would
/// describe a disk fault that did not happen.
pub fn copy(store: &Store, clipboard: &dyn ClipboardWriter, id: Uuid) -> Result<(), ClipError> {
    store.with_unlocked_store(|open| {
        let value = clips::value_of(open, id)?;
        clipboard.write_text(&value)?;
        clips::increment_use_count(open, id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;
    use crate::commands::clipboard::fake::FakeClipboard;
    use crate::storage::StorePaths;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn store_in(parent: &tempfile::TempDir) -> Store {
        Store::open(StorePaths::at(parent.path().join(".fast-clip")))
    }

    fn add(store: &Store, label: &str, value: &str) -> Uuid {
        let inserted =
            store.with_unlocked_store(|open| clips::insert(open, label, value, Colour::DEFAULT));
        match inserted {
            Ok(id) => id,
            Err(e) => panic!("the insert should succeed: {e}"),
        }
    }

    fn use_count(store: &Store, id: Uuid) -> i64 {
        let listed = match store.with_unlocked_store(|open| clips::list(open)) {
            Ok(listed) => listed,
            Err(e) => panic!("the list should succeed: {e}"),
        };
        match listed.into_iter().find(|row| row.id == id) {
            Some(row) => row.use_count,
            None => panic!("the clip should still be stored"),
        }
    }

    #[test]
    fn a_copy_writes_the_clipboard_then_increments_the_count_by_exactly_one() {
        let parent = dir();
        let store = store_in(&parent);
        let id = add(&store, "greeting", "Hello, thank you for waiting");
        let clipboard = FakeClipboard::working();

        assert_eq!(copy(&store, &clipboard, id), Ok(()));
        assert_eq!(
            clipboard.written(),
            vec!["Hello, thank you for waiting".to_string()]
        );
        assert_eq!(use_count(&store, id), 1);

        assert_eq!(copy(&store, &clipboard, id), Ok(()));
        assert_eq!(use_count(&store, id), 2);
    }

    /// Step 1. Nothing else happens: no clipboard write, and no count anywhere
    /// moves.
    #[test]
    fn a_copy_of_an_unknown_clip_is_not_found_and_touches_nothing() {
        let parent = dir();
        let store = store_in(&parent);
        let present = add(&store, "greeting", "Hello");
        let missing = Uuid::new_v4();
        let clipboard = FakeClipboard::working();

        assert_eq!(
            copy(&store, &clipboard, missing),
            Err(ClipError::NotFound {
                clip_id: missing.as_hyphenated().to_string()
            })
        );
        assert!(clipboard.written().is_empty(), "the clipboard was written");
        assert_eq!(use_count(&store, present), 0);
    }

    /// Step 2. A count recorded for a copy that never reached the clipboard
    /// puts a phantom into the tray ranking, which is worse than a count that is
    /// one low.
    #[test]
    fn a_failed_clipboard_write_is_reported_and_increments_nothing() {
        let parent = dir();
        let store = store_in(&parent);
        let id = add(&store, "greeting", "Hello");

        assert_eq!(
            copy(&store, &FakeClipboard::failing(), id),
            Err(ClipError::Clipboard)
        );
        assert_eq!(use_count(&store, id), 0);
    }

    /// Acceptance criterion 9: the new value survives an immediate process
    /// kill. Leaking the connection without closing it is the closest a unit
    /// test gets to a kill — nothing is checkpointed and no destructor runs, so
    /// the increment exists only in the write-ahead log when the store is
    /// reopened. ADR-0009 is the guarantee being tested, and
    /// `synchronous = NORMAL` is what makes it hold.
    #[test]
    fn an_incremented_count_survives_a_process_kill_immediately_after_the_copy() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        let id = {
            let store = Store::open(paths.clone());
            let id = add(&store, "greeting", "Hello");
            match copy(&store, &FakeClipboard::working(), id) {
                Ok(()) => {}
                Err(e) => panic!("the copy should succeed: {e}"),
            }
            // No `shutdown()`, no checkpoint, no close.
            std::mem::forget(store);
            id
        };

        let reopened = Store::open(paths);
        assert_eq!(use_count(&reopened, id), 1);
    }

    /// The guard is the only lock check, so a locked store refuses the copy
    /// before the clipboard is touched. A clip `value` reaching the clipboard of
    /// a locked store would be the disclosure spec §4.8 forbids.
    #[test]
    fn a_locked_store_refuses_a_copy_before_writing_the_clipboard() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // A SQLCipher database begins with a random salt, so the header does
        // not match the SQLite magic and the store classifies as encrypted.
        if let Err(e) = std::fs::write(paths.db(), [0x1f; 64]) {
            panic!("could not write the fixture: {e}");
        }

        let store = Store::open(paths);
        let clipboard = FakeClipboard::working();
        assert_eq!(
            copy(&store, &clipboard, Uuid::new_v4()),
            Err(ClipError::Locked)
        );
        assert!(clipboard.written().is_empty());
    }
}
