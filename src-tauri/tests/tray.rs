//! The two properties of the tray that only a built application can answer
//! (WP-08).
//!
//! Everything else about the tray is unit-tested in `src/tray.rs`: the ranking,
//! the lock branch, the truncation, the ampersand escaping and the item ids.
//! What is **not** covered anywhere, and is recorded as a gap rather than
//! implied to be covered, is the native surface itself — that the icon appears
//! in the notification area, that right-clicking it opens the menu, that
//! choosing an item copies without raising the window, and that a click on
//! *Unlock FastClip* reaches [`fast_clip_lib::tray::on_menu_event`] at all.
//! **Nor is the ordering of the emit against the raise observable**: the mock
//! runtime records no window operation, so nothing here can distinguish an event
//! emitted after `set_focus` from one emitted before it. None of that is
//! observable from a test process, and the work package predicted it.
//!
//! ## Manual verification — the native join (review 012 F2 path 1, ADR-0013)
//!
//! No automated test reaches a real tray click raising a real window with the
//! PIN input ending up focused. The backend test above hands `on_menu_event` a
//! synthetic `MenuEvent`; `tests/unlock-requested.test.ts` dispatches a
//! synthetic `focus` in jsdom; the ordering between the native window-focus
//! event and the IPC delivery of `unlock_requested` is a prediction neither
//! side can run. A human running a built app must confirm it, on this
//! checklist, until an automated harness can drive a real notification-area
//! click — which nothing in this project's toolchain does today.
//!
//! **Setup, once:** build and run FastClip (`npm run tauri dev` or a release
//! build). Turn encryption on if it is not already (Settings → enable, set a
//! 6-digit PIN), then either restart the app or use *Lock now* so the store is
//! `locked: true`.
//!
//! **Path 1 — the locked case (the defect review 012 F2 path 1 found):**
//!
//! 1. With the app window open and the store locked, showing the PIN prompt,
//!    click the title bar's minimise button (`decorations: false` means this is
//!    the only way to minimise). **Expected:** the window disappears from the
//!    screen; the minimise button was the last element focused before it did.
//! 2. Right-click the tray icon in the notification area. **Expected:** a
//!    native context menu appears showing exactly one item, *Unlock FastClip*,
//!    and no clip labels (contract §3, "a locked store shows one item").
//! 3. Left-click *Unlock FastClip*. **Expected:** the window unminimises,
//!    comes to the foreground, and shows the PIN prompt.
//! 4. Without clicking anywhere in the window, type six digits on the
//!    keyboard. **Expected:** the digits appear in the PIN field (masked) as
//!    they are typed, and the field shows a filled dot per digit. If focus was
//!    not on the PIN field, the digits go nowhere visible and nothing in the
//!    window reacts to the keystrokes — that is the failure this checklist
//!    exists to catch.
//! 5. Submit (Enter, or click Unlock once 6 digits are entered). **Expected:**
//!    with a correct PIN, the store unlocks and the clip list renders; with an
//!    incorrect one, the "wrong PIN" message appears and the field clears —
//!    either way, proving step 4's keystrokes actually reached the field.
//!
//! **The negative case — same sequence, unlocked (focus must NOT move):**
//!
//! 1. Unlock the store so `locked: false` and the clip list is showing.
//! 2. Click somewhere that gives an element other than any PIN field the
//!    focus — the search toggle button is convenient — so
//!    `document.activeElement` is a known, visible element.
//! 3. Minimise via the title bar, as above.
//! 4. Right-click the tray. **Expected:** the menu now shows up to ten clip
//!    labels and no *Unlock FastClip* item (the item exists only in a locked
//!    menu — contract §3, ADR-0013 "Sender's gate": none).
//! 5. Choose any clip. **Expected:** the window does *not* raise — spec §4.5,
//!    "right-clicking the tray copies without raising the window" — and the
//!    clip's value is now on the clipboard. There is no *Unlock FastClip* item
//!    to choose in this state, which is itself the confirmation that
//!    `unlock_requested` cannot be produced from an unlocked tray: the event
//!    has no sender when the store is already open, so there is nothing for
//!    focus to react to and nothing to check beyond "the item is absent."
//!
//! Record the result (pass/fail, and the build/commit tested against)
//! wherever this package's landing notes live; this file only carries the
//! procedure, not a running log of outcomes.
//!
//! **Why these tests are here and not in `src/tray.rs`.** A binary that builds a
//! Tauri application needs the side-by-side manifest `build.rs` embeds, and
//! cargo's `rustc-link-arg-tests` reaches a target under `tests/` and not the
//! library's own unit-test harness — which dies at load with
//! `STATUS_ENTRYPOINT_NOT_FOUND` if it links that machinery. `tests/window.rs`
//! records the same reason.

use std::sync::{Arc, Mutex};

use tauri::menu::{MenuEvent, MenuId};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener};

use fast_clip_lib::commands::events::UNLOCK_REQUESTED;
use fast_clip_lib::tray::on_menu_event;

/// The tray icon is `app.default_window_icon()`, which `generate_context!`
/// embeds from the `bundle.icon` list in `tauri.conf.json`.
///
/// **Nothing else guards that list.** Removing the `.png` entries would leave
/// the application building, the tests passing and the tray icon blank — a
/// defect visible only by looking at the notification area, which is the one
/// place this project has no automated eyes on.
#[test]
fn the_build_context_carries_an_icon_for_the_tray_to_use() {
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    assert!(
        context.default_window_icon().is_some(),
        "tauri.conf.json's bundle.icon list has no image the tray can use"
    );
}

// ---- the unlock request (contract §3, ADR-0013) ----

/// A mock application listening for `unlock_requested`, with a way to choose a
/// menu item on it.
///
/// The event is handed to `on_menu_event` directly. That is the function
/// `install` registers with the tray and the one Tauri calls with an event of
/// exactly this shape, so everything between the id and the emit is under test;
/// the click that produces the id is not.
struct Chooser {
    app: App<MockRuntime>,
    received: Arc<Mutex<Vec<String>>>,
}

impl Chooser {
    fn listening() -> Self {
        let app = match mock_builder().build(mock_context(noop_assets())) {
            Ok(app) => app,
            Err(e) => panic!("the mock application should build: {e}"),
        };
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);
        app.listen(UNLOCK_REQUESTED, move |event| match sink.lock() {
            Ok(mut received) => received.push(event.payload().to_owned()),
            Err(e) => panic!("the event sink was poisoned: {e}"),
        });
        Self { app, received }
    }

    /// `id` is a menu item id, not a wire name: `UNLOCK_ITEM_ID` stays private
    /// to `src/tray.rs` and the tests below spell `"unlock"` out. A change there
    /// that is not made here fails the first test rather than passing quietly,
    /// because nothing else in the process mints that id.
    fn choose(&self, id: &str) {
        on_menu_event(
            self.app.handle(),
            MenuEvent {
                id: MenuId::new(id),
            },
        );
    }

    /// Every payload received, verbatim and in order. Verbatim because `null` is
    /// the payload the contract fixes, and a `serde_json::Value` round trip would
    /// hide the difference between `null` and an absent body.
    fn payloads(&self) -> Vec<String> {
        match self.received.lock() {
            Ok(received) => received.clone(),
            Err(e) => panic!("the event sink was poisoned: {e}"),
        }
    }
}

/// Contract §3: one event per choice, carrying `null`.
///
/// The mock context declares no window, so the raise takes its "there is no
/// window" branch and logs. That is the strongest available statement of the
/// rule the ADR asks for: the emit is **not** conditional on the raise having
/// worked, because a webview that did not come forward still has a prompt to
/// focus.
#[test]
fn choosing_the_unlock_item_emits_one_unlock_request_carrying_null() {
    let chooser = Chooser::listening();
    chooser.choose("unlock");
    assert_eq!(chooser.payloads(), vec!["null".to_owned()]);
}

/// One event per choice. The item is not a toggle and nothing deduplicates.
#[test]
fn two_choices_emit_two_events() {
    let chooser = Chooser::listening();
    chooser.choose("unlock");
    chooser.choose("unlock");
    assert_eq!(chooser.payloads().len(), 2);
}

/// **No other menu item emits it**, and no command does either — contract §3
/// says a command that emitted this event would be a defect, which is why
/// `tests/ipc.rs` gains no row for it.
///
/// The store is not managed on this application, so the clip item takes its own
/// error branch. That is the case most likely to grow a stray emit and it must
/// not have one: a failed copy is not a request to unlock.
#[test]
fn no_other_menu_item_emits_an_unlock_request() {
    let chooser = Chooser::listening();
    chooser.choose(&format!("clip:{}", uuid::Uuid::nil().as_hyphenated()));
    chooser.choose("clip:not-a-uuid");
    chooser.choose("something_else");
    chooser.choose("");
    assert_eq!(chooser.payloads(), Vec::<String>::new());
}

/// The name is on the wire, so contract §3 fixes the literal and contract §0
/// fixes its casing. Changing it is a contract change in both halves, not a
/// rename.
#[test]
fn the_event_name_is_the_literal_the_contract_fixes() {
    assert_eq!(UNLOCK_REQUESTED, "unlock_requested");
}
