//! The system tray icon and its right-click menu (WP-08).
//!
//! [Spec §4.5](../../../docs/src/product/spec.md) is authoritative for what the
//! menu contains; [spec §4.8](../../../docs/src/product/spec.md) is
//! authoritative for what it contains while the store is locked. Where this file
//! and those pages disagree, this file is the defect.
//!
//! Four rules govern everything here.
//!
//! **There is one clipboard implementation.** Choosing a clip calls
//! [`crate::commands::clips::copy`] — the same function `copy_clip` calls — so a
//! tray copy increments `use_count` identically and generates no IPC traffic
//! (contract, `copy_clip`; closed question 6). A second clipboard path in this
//! file would be the defect the closed question exists to prevent, because it
//! would not count.
//!
//! **No clip `value` reaches the menu.** The ranking query selects `id` and
//! `label` and never `value` ([`crate::storage::clips::ranked_for_tray`]), so
//! the type this file builds a menu from has no `value` field to put in a menu
//! string by accident.
//!
//! **No clip `label` reaches a log line**, at any level (ADR-0002,
//! [criterion 6](../../../docs/src/product/spec.md)). [`Entry`] writes its own
//! `Debug` for the same reason `ClipRow` does, and nothing in this file formats
//! a menu string into a log.
//!
//! **The menu is rebuilt from three triggers, not two.** The clip list changing
//! and a `use_count` changing are the two spec §4.5 names. Lock state is the
//! third: while locked the menu is a single *Unlock FastClip* item and no clip
//! labels, so a tray still listing labels after a lock breaches
//! [criterion 10](../../../docs/src/product/spec.md) in the one surface the
//! contract says breaches it exactly as the window would. [`refresh`] reads lock
//! state itself, so WP-07's `lock` and `unlock` need only call it.
//!
//! **The tray sends the webview exactly one thing**, and it is not a copy.
//! Choosing *Unlock FastClip* emits
//! [`crate::commands::events::UNLOCK_REQUESTED`] (contract §3,
//! [ADR-0013](../../../docs/src/architecture/adr/0013-unlock-requested-event.md)),
//! because the PIN input lives in the webview and nothing outside it can focus
//! an input. A tray copy still crosses no seam: the traffic follows the
//! consumer, which is the rule
//! [ADR-0008](../../../docs/src/architecture/adr/0008-use-count-stays-backend-side.md)
//! already applies, and this applies it rather than bending it.

use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use crate::commands::clipboard::AppClipboard;
use crate::commands::clips;
use crate::commands::events;
use crate::error::ClipError;
use crate::redact::Redacted;
use crate::storage::{clips as stored, Store};
use crate::window::MAIN_WINDOW_LABEL;

/// The id [`refresh`] finds the installed tray under.
pub const TRAY_ID: &str = "fast_clip";

/// What [`refresh`] logs when there is no tray to rebuild.
///
/// A named constant rather than a literal because `tests/ipc.rs` counts these
/// records to prove that **every** mutating command rebuilds the menu. Nothing
/// else about a rebuild is observable from a test process: [`tauri::tray::TrayIcon`]
/// exposes no getter for its menu, and building a real one needs a notification
/// area and a Windows message loop. Editing this string is not a cosmetic
/// change — it is the only thing that test can see.
pub const NO_TRAY_TO_REBUILD: &str = "no tray icon is installed; nothing to rebuild";

/// At most ten clips (spec §4.5).
const MENU_LIMIT: usize = 10;

/// Where a long label is cut.
///
/// Forty Unicode scalar values, ratified by
/// [ADR-0014](../../../docs/src/architecture/adr/0014-tray-label-truncation.md).
/// **The criterion is the window:** the menu is about as wide as the 300-pixel
/// window, so it reads as belonging to this application and does not cover the
/// thing a tray copy exists to leave undisturbed. The arithmetic that turns that
/// criterion into forty lives on that page. If a manual check on a running build
/// finds the menu materially wider than the window, this constant moves and the
/// criterion does not.
const LABEL_LIMIT: usize = 40;

/// Appended to a label that was cut. One scalar value, so it costs one column.
const ELLIPSIS: char = '…';

/// Menu item ids. A clip's is its id behind a prefix, so the handler can tell a
/// clip item from every other item without a lookup table.
const CLIP_ITEM_PREFIX: &str = "clip:";
const UNLOCK_ITEM_ID: &str = "unlock";

/// Copy deck, *Tray item while locked*. Spec §4.8 requires this item to raise
/// the window rather than take input: **the PIN is never typed into a native
/// menu.**
const UNLOCK_ITEM_TEXT: &str = "Unlock FastClip";

/// Hover text on the icon itself. The product name, not new copy.
const TOOLTIP: &str = "FastClip";

/// One row of the menu, before it becomes a native item.
///
/// Split out from the menu building so that the part with the rules in it — the
/// ranking, the lock branch, the truncation and the escaping — is testable
/// without a tray, a window or an event loop. What remains untested is the
/// translation of this list into muda items, which is the untestable surface
/// WP-08 predicted and it is one function long.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    /// The single item shown while the store is locked. No clip labels.
    Unlock,
    /// A clip, by its truncated and escaped label.
    Clip { id: Uuid, text: String },
}

/// Hand-written: `text` is derived from a clip `label`, and a `{:?}` in a log
/// line or a panic message must not print it (ADR-0002).
impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unlock => f.write_str("Unlock"),
            Self::Clip { id, text } => f
                .debug_struct("Clip")
                .field("id", id)
                .field("text", &Redacted::of(text))
                .finish(),
        }
    }
}

/// What the menu should contain right now.
///
/// **The lock branch comes first and returns without touching the store.** While
/// locked the menu is one item and no labels (spec §4.8).
///
/// A store that is neither locked nor open — a recorded startup fault, or a
/// directory that could not be reached — produces an **empty** menu. That case
/// is not specified. An empty menu is the answer that discloses nothing and
/// claims nothing: *Unlock FastClip* would be false over a plaintext store, and
/// inventing a "FastClip could not read its clips" item would be inventing copy
/// the deck does not carry.
pub(crate) fn entries(store: &Store) -> Vec<Entry> {
    if store.is_locked() {
        return vec![Entry::Unlock];
    }

    match store.with_unlocked_store(|open| stored::ranked_for_tray(open, MENU_LIMIT)) {
        Ok(ranked) => ranked
            .into_iter()
            .map(|clip| Entry::Clip {
                id: clip.id,
                text: menu_text(&clip.label),
            })
            .collect(),
        // Locked between the check above and the guard's own. The guard is
        // authoritative, so its answer wins: a menu of labels must never be the
        // outcome of losing that race.
        Err(ClipError::Locked) => vec![Entry::Unlock],
        Err(error) => {
            // `ClipError`'s `Display` carries no clip content by construction
            // (ADR-0012).
            log::error!("the tray menu could not be built from the store: {error}");
            Vec::new()
        }
    }
}

/// A clip label as a native menu string: truncated, then escaped.
///
/// **Truncation is counted in Unicode scalar values**, the unit contract §0
/// counts every length in. Cutting on bytes would split a multi-byte character.
///
/// **`&` is doubled.** muda passes the text straight to `AppendMenuW`, and Win32
/// reads a single `&` as a mnemonic marker — so a label of `R&D snippet` would
/// render as `RD snippet` with the D underlined. Doubling is the Win32 escape
/// for a literal ampersand. This is rendering the label the user typed, not
/// altering it.
///
/// Escaping happens **after** truncation, so a doubled `&` does not eat into the
/// user's forty characters.
fn menu_text(label: &str) -> String {
    let mut text: String = label.chars().take(LABEL_LIMIT).collect();
    if label.chars().count() > LABEL_LIMIT {
        text.push(ELLIPSIS);
    }
    text.replace('&', "&&")
}

/// The wire form of a clip item's id.
fn clip_item_id(id: Uuid) -> String {
    format!("{CLIP_ITEM_PREFIX}{}", id.as_hyphenated())
}

/// The clip a menu item id names, or `None` for any other item.
fn clip_of(item_id: &str) -> Option<Uuid> {
    let rest = item_id.strip_prefix(CLIP_ITEM_PREFIX)?;
    match Uuid::parse_str(rest) {
        Ok(id) => Some(id),
        Err(_) => {
            // Unreachable: this process wrote the id. Logged rather than
            // ignored, because reaching it means the two halves of this file
            // disagree. The id is not clip content.
            log::error!("a tray menu item carried an id that is not a UUID");
            None
        }
    }
}

/// Turn the entries into a native menu.
///
/// `None` on failure, already logged. Every failure here is Tauri's dispatch to
/// the main thread failing — muda's own Windows path for an item with no
/// accelerator and no icon reports none — so it means the event loop has gone,
/// which is [`refresh`]'s problem and is discussed there.
fn build_menu<R: Runtime>(app: &AppHandle<R>, entries: &[Entry]) -> Option<Menu<R>> {
    let menu = match Menu::new(app) {
        Ok(menu) => menu,
        Err(error) => {
            log::error!("the tray menu could not be created: {error}");
            return None;
        }
    };

    for entry in entries {
        let (id, text) = match entry {
            Entry::Unlock => (UNLOCK_ITEM_ID.to_owned(), UNLOCK_ITEM_TEXT.to_owned()),
            Entry::Clip { id, text } => (clip_item_id(*id), text.clone()),
        };

        // No accelerator: a tray menu has no keyboard scheme in this
        // application, and muda's only fallible step on Windows is parsing one.
        let item = match MenuItem::with_id(app, id, text, true, None::<&str>) {
            Ok(item) => item,
            Err(error) => {
                // The error describes the dispatch, never the item text.
                log::error!("a tray menu item could not be created: {error}");
                return None;
            }
        };

        if let Err(error) = menu.append(&item) {
            log::error!("a tray menu item could not be appended: {error}");
            return None;
        }
    }

    Some(menu)
}

/// Create the tray icon. Called once, from [`crate::run`], after the window.
///
/// **A failure does not stop the process**, unlike a failure to create the
/// window. The window is the surface an error can be shown on and the tray is an
/// accessory to it, so an application with a window and no tray is degraded
/// while an application with neither is invisible. A tray that was not created
/// discloses nothing, which is the only property criterion 10 asks of it.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let entries = match app.try_state::<Store>() {
        Some(store) => entries(&store),
        None => {
            log::error!("the store is not managed; the tray menu was built empty");
            Vec::new()
        }
    };

    let Some(menu) = build_menu(app, &entries) else {
        log::error!("the tray icon was not created: its menu could not be built");
        return;
    };

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .on_menu_event(on_menu_event);

    match app.default_window_icon() {
        Some(icon) => builder = builder.icon(icon.clone()),
        // Not fatal, and not silent: an icon-less tray entry is hard to find but
        // the menu still works.
        None => log::error!("the application has no default icon; the tray icon will be blank"),
    }

    if let Err(error) = builder.build(app) {
        log::error!("the tray icon could not be created: {error}");
    }
}

/// Rebuild the menu from the store as it is now.
///
/// **Call this after anything that changes the clip list, any `use_count`, or
/// the lock state** (spec §4.5, spec §4.8). It reads lock state itself, so a
/// caller never decides which menu to show.
///
/// **A failure is absorbed and logged**, and review 003's F4 asked whether that
/// can leave clip labels in the tray over a locked store. It cannot, and the
/// reason is narrower than "unlikely":
///
/// - Building the items cannot fail on Windows for the items this file builds.
///   `muda::Menu::add_menu_item` discards the `AppendMenuW` return value and its
///   only `?` is parsing an accelerator, and these items carry none.
/// - Every remaining failure — `Menu::new`, `MenuItem::with_id`, `Menu::append`,
///   `TrayIcon::set_menu` — is `run_on_main_thread` failing to send or to
///   receive, which happens only when the main-thread event loop has stopped.
/// - An event loop that has stopped is a process that is exiting, which has no
///   tray to leave labels in and no window to invoke `lock` from.
///
/// So the absorbed branch is not "the tray kept the old menu"; it is "there is
/// no longer an application". Reporting it would give `lock` an error it has no
/// variant for and contradict ADR-0010's "once locking begins it cannot fail",
/// which is the other half of F4.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        // No tray: either `install` failed and already said so, or this is a
        // test harness that never created one. Not an error on its own, and it
        // is the record `tests/ipc.rs` counts to prove this function was
        // reached — see [`NO_TRAY_TO_REBUILD`].
        log::debug!("{NO_TRAY_TO_REBUILD}");
        return;
    };

    let entries = match app.try_state::<Store>() {
        Some(store) => entries(&store),
        None => {
            log::error!("the store is not managed; the tray menu was rebuilt empty");
            Vec::new()
        }
    };

    // **The store's connection lock is not held here.** `entries` returns owned
    // data and the guard is dropped before this line, which matters because
    // building a menu waits on the main thread and the main thread is where the
    // commands that take that lock run.
    let Some(menu) = build_menu(app, &entries) else {
        return;
    };

    if let Err(error) = tray.set_menu(Some(menu)) {
        log::error!("the tray menu could not be replaced: {error}");
    }
}

/// Handle a click on a tray menu item.
///
/// Registered on the tray, but Tauri delivers **every** menu event to it, so an
/// id this file did not mint is ignored rather than guessed at.
///
/// **Public only so that `tests/tray.rs` can deliver a menu event.** [`install`]
/// is the sole caller in the application. The test lives there rather than in
/// the module below because a binary that builds a Tauri application needs the
/// side-by-side manifest `build.rs` embeds into targets under `tests/`, and the
/// library's own unit-test harness does not receive it — it dies at load with
/// `STATUS_ENTRYPOINT_NOT_FOUND`. The same reason put `tests/window.rs` there.
pub fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();

    if id == UNLOCK_ITEM_ID {
        // Both halves of spec §4.8's one sentence, in the order contract §3
        // fixes: the backend raises the window, then asks the webview to focus
        // the prompt. Nothing between them depends on the raise having worked.
        raise_window(app);
        request_unlock(app);
        return;
    }

    if let Some(clip_id) = clip_of(id) {
        copy_from_tray(app, clip_id);
    }
}

/// Copy a clip chosen from the tray, without raising the window.
///
/// Goes through [`crate::commands::clips::copy`], which is the function
/// `copy_clip` calls. There is no second clipboard write in this file and there
/// must never be one: a tray copy that bypassed it would not increment
/// `use_count`, and the ranking above would then be built from half the user's
/// copies.
fn copy_from_tray<R: Runtime>(app: &AppHandle<R>, id: Uuid) {
    let outcome = match app.try_state::<Store>() {
        Some(store) => clips::copy(&store, &AppClipboard(app), id),
        None => {
            log::error!("the store is not managed; the tray copy was discarded");
            return;
        }
    };

    if let Err(error) = outcome {
        // No user-facing surface exists for this: the tray is a native menu and
        // spec §4.5's whole point is that it does not raise the window. The log
        // is the report. `ClipError`'s `Display` carries no clip content.
        log::error!("a copy from the tray failed: {error}");
    }

    // **Unconditionally, including after a failure.** A success changed
    // `use_count` and therefore the ranking. A failure means the menu described
    // a store that has since changed — `not_found` says the clip is gone, and
    // `locked` says the menu is showing labels it must not be showing. Both are
    // states a rebuild corrects and staleness does not.
    refresh(app);
}

/// Tell the webview the user asked to unlock.
///
/// Spec §4.8 says the tray item raises the window **with the PIN prompt
/// focused**. Raising is a window operation and [`raise_window`] does it;
/// focusing an input is a DOM operation and only the webview can do it, so this
/// event is what carries the gesture across
/// ([ADR-0013](../../../docs/src/architecture/adr/0013-unlock-requested-event.md)).
/// What the frontend then does with it is spec §4.8's, not this file's.
///
/// **No gate.** The item exists only in a locked menu and the frontend gates on
/// its own last `lock_state` in any case, so a `store.is_locked()` check here
/// would remove nothing from the receiving side and give one decision two owners
/// (contract §0). That decision is this file's, which is why this function
/// exists at all rather than the call below appearing inline in
/// [`on_menu_event`]: it is the named half of spec §4.8 that is not a window
/// operation, and it is where the absence of a gate is recorded.
///
/// The event name, the `null` payload and the absorbed failure belong to
/// [`events::emit_unlock_requested`], with the other two wire events.
fn request_unlock<R: Runtime>(app: &AppHandle<R>) {
    events::emit_unlock_requested(app);
}

/// Raise the window for *Unlock FastClip*.
///
/// The frontend shows the PIN prompt whenever its last `lock_state` says
/// `locked: true` (contract, startup sequence step 5). Focusing that prompt is
/// [`request_unlock`]'s half; this function is only the window operation. **The
/// PIN is never typed into a native menu** (spec §4.8).
///
/// **Each step is attempted whatever the one before it did.** A window that is
/// shown but could not be focused is still in front of the user, and stopping at
/// the first error would leave it minimised.
fn raise_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        log::error!("there is no window to raise for the tray unlock item");
        return;
    };

    if let Err(error) = window.unminimize() {
        log::error!("the window could not be unminimised: {error}");
    }
    if let Err(error) = window.show() {
        log::error!("the window could not be shown: {error}");
    }
    if let Err(error) = window.set_focus() {
        log::error!("the window could not be focused: {error}");
    }
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

    /// A store that classifies as encrypted, so it is locked. A SQLCipher
    /// database begins with a random salt rather than the SQLite magic, which is
    /// what the classifier reads.
    fn locked_store_in(parent: &tempfile::TempDir) -> Store {
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        if let Err(e) = std::fs::write(paths.db(), [0x1f; 64]) {
            panic!("could not write the fixture: {e}");
        }
        Store::open(paths)
    }

    fn add(store: &Store, label: &str) -> Uuid {
        let inserted = store
            .with_unlocked_store(|open| stored::insert(open, label, "a value", Colour::DEFAULT));
        match inserted {
            Ok(id) => id,
            Err(e) => panic!("the insert should succeed: {e}"),
        }
    }

    fn texts(store: &Store) -> Vec<String> {
        entries(store)
            .into_iter()
            .map(|entry| match entry {
                Entry::Unlock => "<unlock>".to_owned(),
                Entry::Clip { text, .. } => text,
            })
            .collect()
    }

    // ---- the ranking, as the menu sees it ----

    #[test]
    fn a_fresh_install_has_a_populated_menu_in_list_order() {
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "first");
        add(&store, "second");

        assert_eq!(texts(&store), vec!["first", "second"]);
    }

    #[test]
    fn an_empty_store_produces_an_empty_menu() {
        let parent = dir();
        let store = store_in(&parent);
        assert_eq!(entries(&store), Vec::new());
    }

    /// Definition of done: a tray copy increments `use_count`, and the menu
    /// tracks the ranking that follows from it. One copy, through the same
    /// function `copy_clip` calls.
    #[test]
    fn a_copy_reorders_the_menu_and_goes_through_the_one_clipboard_path() {
        let parent = dir();
        let store = store_in(&parent);
        add(&store, "first");
        let second = add(&store, "second");
        let clipboard = FakeClipboard::working();

        assert_eq!(texts(&store), vec!["first", "second"]);
        assert_eq!(clips::copy(&store, &clipboard, second), Ok(()));
        assert_eq!(clipboard.written(), vec!["a value".to_string()]);
        assert_eq!(texts(&store), vec!["second", "first"]);
    }

    #[test]
    fn the_menu_holds_at_most_ten_clips() {
        let parent = dir();
        let store = store_in(&parent);
        for n in 0..14 {
            add(&store, &format!("clip {n}"));
        }
        assert_eq!(texts(&store).len(), MENU_LIMIT);
    }

    #[test]
    fn the_menu_tracks_a_create_and_a_delete() {
        let parent = dir();
        let store = store_in(&parent);
        let first = add(&store, "first");
        assert_eq!(texts(&store), vec!["first"]);

        add(&store, "second");
        assert_eq!(texts(&store), vec!["first", "second"]);

        if let Err(e) = store.with_unlocked_store(|open| stored::delete(open, first)) {
            panic!("the delete should succeed: {e}");
        }
        assert_eq!(texts(&store), vec!["second"]);
    }

    /// A reorder moves unused clips, because with every count equal the ranking
    /// *is* the list order.
    #[test]
    fn the_menu_tracks_a_reorder() {
        let parent = dir();
        let store = store_in(&parent);
        let first = add(&store, "first");
        let second = add(&store, "second");
        let third = add(&store, "third");

        if let Err(e) =
            store.with_unlocked_store(|open| stored::reorder(open, &[third, first, second]))
        {
            panic!("the reorder should succeed: {e}");
        }
        assert_eq!(texts(&store), vec!["third", "first", "second"]);
    }

    // ---- the lock branch: the third rebuild trigger ----

    /// Criterion 10 in the tray. One item, and not one clip label anywhere in
    /// the menu the tray would be given.
    #[test]
    fn a_locked_store_shows_one_unlock_item_and_no_clip_labels() {
        let parent = dir();
        let store = locked_store_in(&parent);

        assert!(store.is_locked());
        assert_eq!(entries(&store), vec![Entry::Unlock]);
    }

    /// A store that will not open is not a locked store: offering a PIN prompt
    /// over a plaintext store puts up a prompt the user cannot satisfy.
    #[test]
    fn a_faulted_store_shows_nothing_rather_than_an_unlock_item() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // A plaintext SQLite header over a file that is not a database.
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend_from_slice(&[0u8; 512]);
        if let Err(e) = std::fs::write(paths.db(), &bytes) {
            panic!("could not write the fixture: {e}");
        }

        let store = Store::open(paths);
        assert!(!store.is_locked());
        assert_eq!(entries(&store), Vec::new());
    }

    // ---- the menu string ----

    #[test]
    fn a_short_label_is_unchanged() {
        assert_eq!(menu_text("Support greeting"), "Support greeting");
    }

    #[test]
    fn a_label_at_the_limit_is_not_truncated() {
        let label = "x".repeat(LABEL_LIMIT);
        assert_eq!(menu_text(&label), label);
    }

    #[test]
    fn a_label_one_over_the_limit_is_cut_and_marked() {
        let label = "x".repeat(LABEL_LIMIT + 1);
        let text = menu_text(&label);
        assert_eq!(text.chars().count(), LABEL_LIMIT + 1);
        assert!(text.ends_with(ELLIPSIS));
        assert_eq!(text.chars().filter(|c| *c == 'x').count(), LABEL_LIMIT);
    }

    /// Contract §0 counts every length in Unicode scalar values. Truncating on
    /// bytes would cut this label in the middle of a character.
    #[test]
    fn truncation_counts_scalar_values_and_not_bytes() {
        let label = "\u{1F600}".repeat(LABEL_LIMIT + 5);
        let text = menu_text(&label);
        assert_eq!(text.chars().count(), LABEL_LIMIT + 1);
    }

    /// Win32 reads a single `&` as a mnemonic marker, so an unescaped label
    /// would render with a character missing.
    #[test]
    fn an_ampersand_is_doubled_so_the_label_renders_as_typed() {
        assert_eq!(menu_text("R&D snippet"), "R&&D snippet");
        assert_eq!(menu_text("&&"), "&&&&");
    }

    /// Escaping after truncation, so the doubled character does not spend the
    /// user's budget.
    #[test]
    fn escaping_does_not_eat_into_the_truncation_budget() {
        let label = "&".repeat(LABEL_LIMIT + 3);
        let text = menu_text(&label);
        assert_eq!(text.chars().filter(|c| *c == '&').count(), LABEL_LIMIT * 2);
        assert!(text.ends_with(ELLIPSIS));
    }

    // ---- item ids ----

    #[test]
    fn a_clip_item_id_round_trips() {
        let id = Uuid::new_v4();
        assert_eq!(clip_of(&clip_item_id(id)), Some(id));
    }

    #[test]
    fn an_item_that_is_not_a_clip_is_ignored() {
        assert_eq!(clip_of(UNLOCK_ITEM_ID), None);
        assert_eq!(clip_of("something_else"), None);
        assert_eq!(clip_of("clip:not-a-uuid"), None);
    }

    // ---- disclosure ----

    /// ADR-0002: no clip `label` reaches a log line, including through a
    /// `Debug`.
    #[test]
    fn the_debug_of_an_entry_redacts_the_label() {
        let entry = Entry::Clip {
            id: Uuid::new_v4(),
            text: menu_text("a secret label"),
        };
        let rendered = format!("{entry:?}");
        assert!(!rendered.contains("a secret label"), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }
}
