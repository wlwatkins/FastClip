//! The main window's creation, and what happens when it cannot be created.
//!
//! An integration test rather than a `#[cfg(test)]` module, for the mechanical
//! reason `tests/ipc.rs` records: a binary that links Tauri's application
//! machinery needs the side-by-side manifest `build.rs` embeds, and cargo's
//! `rustc-link-arg-tests` reaches a target under `tests/` and not the library's
//! own unit-test harness.
//!
//! **The case under test is the one the WP-14 review found (F1).**
//! [`create_main`] used to return `()`, so `run()` could not know it had failed
//! and entered `app.run` with zero windows — an event loop that never
//! terminates, no interface, no console in a release build and no tray, leaving
//! Task Manager as the only remedy. It now returns whether the window exists and
//! `run()` stops on `false`.
//!
//! What this file can prove and what it cannot: the mock runtime's context
//! declares no windows, so the "no window labelled `main`" branch is reachable
//! here and is the one asserted. The other two — a configuration that will not
//! read, and a `build()` that fails because WebView2 is absent — need a real
//! runtime and a broken machine. All three return through the same value, and
//! the `#[must_use]` on it is what stops a caller ignoring any of them.

use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::App;

use fast_clip_lib::window::{create_main, MAIN_WINDOW_LABEL};

type MockApp = App<tauri::test::MockRuntime>;

fn app() -> MockApp {
    match mock_builder().build(mock_context(noop_assets())) {
        Ok(app) => app,
        Err(e) => panic!("the mock application should build: {e}"),
    }
}

/// **F1.** A window that cannot be created is reported to the caller, so that
/// `run()` can stop instead of looping forever with nothing on screen.
#[test]
fn a_window_that_cannot_be_created_reports_failure_rather_than_succeeding_silently() {
    let app = app();
    // The mock context declares no windows at all, which is the first of the
    // three failure branches.
    assert!(
        app.config()
            .app
            .windows
            .iter()
            .all(|window| window.label != MAIN_WINDOW_LABEL),
        "the fixture depends on the mock context declaring no main window"
    );

    assert!(
        !create_main(&app, true),
        "create_main must tell the caller there is no window"
    );
    assert!(
        !create_main(&app, false),
        "and the always-on-top value does not change that"
    );
}

/// The return value carries the whole decision, so a caller that drops it is a
/// defect the compiler catches. This test states the label the three files agree
/// on, which is the other half of "the window could not be found".
#[test]
fn the_window_label_is_the_one_the_configuration_and_the_capability_name() {
    assert_eq!(MAIN_WINDOW_LABEL, "main");

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in ["tauri.conf.json", "capabilities/default.json"] {
        let path = manifest.join(file);
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(e) => panic!("{} could not be read: {e}", path.display()),
        };
        assert!(
            source.contains("\"main\""),
            "{} does not name the main window",
            path.display()
        );
    }
}
