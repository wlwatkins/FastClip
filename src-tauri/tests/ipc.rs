//! The command surface as the frontend meets it.
//!
//! An integration test rather than a `#[cfg(test)]` module, for a mechanical
//! reason worth writing down: this is the only test binary that links Tauri's
//! application machinery, so it is the only one that needs the side-by-side
//! manifest `build.rs` embeds — and cargo's `rustc-link-arg-tests` reaches a
//! target under `tests/` and not the library's own unit-test harness.
//!
//! Everything here goes through a real `invoke` against a real
//! `invoke_handler`, on Tauri's mock runtime. That is the point: the unit tests
//! beside each module check the logic, and these check the things only the
//! boundary can be wrong about — the command name, the argument key, the casing,
//! the JSON a rejection actually becomes, and whether the event was emitted at
//! all.
//!
//! Two definition-of-done items are proved here rather than argued:
//!
//! - **An early `invoke` cannot silently fail.** Every test invokes before
//!   `app.run` is ever called, and the store is managed on the **builder**. The
//!   `AppHandle` an event is emitted through is a command parameter rather than
//!   a global populated by a spawned task, so there is no window in which a
//!   command finds no state or an emit finds no handle.
//! - **`update_clips` follows every successful mutation and only a successful
//!   one**, with the complete list in display order.
//!
//! **`Builder::setup` does not run at `Builder::build`. It runs from
//! `App::run`.** That sentence cost a debugging cycle, so it is written down
//! where the next person will meet it.
//!
//! The first version of this suite managed the store in a `setup` hook, on the
//! assumption that the hook runs while the application is being built. It does
//! not — `App::run` calls it, on its way into the event loop — so every test
//! here invoked against an application with no managed state and got the plain
//! string `state not managed for field 'store' on command 'create_clip'`. That
//! is not a `ClipError`, the frontend can only map it to `internal`, and
//! contract §0 forbids exactly that for a modelled condition. `lib.rs` had the
//! same hook and therefore the same window, between `build()` returning and
//! `run()` starting.
//!
//! Both now call `Builder::manage`, which registers the state before the `App`
//! exists at all. **Do not simplify either one back into a `setup` hook.** The
//! change would be invisible here: the helper below constructs the application
//! itself, so a `setup` hook added to `lib.rs` alone leaves all fifteen tests
//! passing while the shipped binary reopens the window this file exists to
//! close.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

use fast_clip_lib::commands;
use fast_clip_lib::storage::{Store, StorePaths};

type MockApp = App<tauri::test::MockRuntime>;
type MockWindow = WebviewWindow<tauri::test::MockRuntime>;

/// The registration under test. This list is the one in `lib.rs`; if the two
/// ever disagree, the tests below are checking a surface the application does
/// not serve.
fn build(paths: StorePaths) -> MockApp {
    let built = mock_builder()
        .invoke_handler(tauri::generate_handler![
            commands::clips::list_clips,
            commands::clips::create_clip,
            commands::clips::update_clip,
            commands::clips::delete_clip,
            commands::clips::copy_clip,
        ])
        // On the builder, exactly as `lib.rs` does it, and not in a `setup`
        // hook — see the module header for what that costs.
        .manage(Store::open(paths))
        .build(mock_context(noop_assets()));

    match built {
        Ok(app) => app,
        Err(e) => panic!("the mock application should build: {e}"),
    }
}

fn window(app: &MockApp) -> MockWindow {
    match WebviewWindowBuilder::new(app, "main", Default::default()).build() {
        Ok(window) => window,
        Err(e) => panic!("the mock window should build: {e}"),
    }
}

fn dir() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    }
}

/// One `invoke`, exactly as the webview issues it.
fn invoke(window: &MockWindow, command: &str, arguments: Value) -> Result<Value, Value> {
    let url = match "http://tauri.localhost".parse() {
        Ok(url) => url,
        Err(e) => panic!("the fixture url should parse: {e}"),
    };

    let response = tauri::test::get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url,
            body: InvokeBody::Json(arguments),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    );

    match response {
        Ok(body) => match body.deserialize::<Value>() {
            Ok(value) => Ok(value),
            Err(e) => panic!("a resolved response should be JSON: {e}"),
        },
        Err(rejection) => Err(rejection),
    }
}

fn expect_ok(window: &MockWindow, command: &str, arguments: Value) -> Value {
    match invoke(window, command, arguments) {
        Ok(value) => value,
        Err(rejection) => panic!("{command} should have resolved, and rejected with {rejection}"),
    }
}

fn expect_err(window: &MockWindow, command: &str, arguments: Value) -> Value {
    match invoke(window, command, arguments) {
        Ok(value) => panic!("{command} should have rejected, and resolved with {value}"),
        Err(rejection) => rejection,
    }
}

/// Every `update_clips` payload received, in order.
struct Emissions(Arc<Mutex<Vec<Value>>>);

impl Emissions {
    fn listening(app: &MockApp) -> Self {
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);
        app.listen(commands::UPDATE_CLIPS, move |event| {
            let payload = match serde_json::from_str::<Value>(event.payload()) {
                Ok(payload) => payload,
                Err(e) => panic!("an event payload should be JSON: {e}"),
            };
            match sink.lock() {
                Ok(mut received) => received.push(payload),
                Err(e) => panic!("the event sink was poisoned: {e}"),
            }
        });
        Self(received)
    }

    fn all(&self) -> Vec<Value> {
        match self.0.lock() {
            Ok(received) => received.clone(),
            Err(e) => panic!("the event sink was poisoned: {e}"),
        }
    }

    fn last(&self) -> Value {
        match self.all().pop() {
            Some(payload) => payload,
            None => panic!("no {} event was emitted", commands::UPDATE_CLIPS),
        }
    }

    fn count(&self) -> usize {
        self.all().len()
    }
}

fn labels(payload: &Value) -> Vec<String> {
    let array = match payload.as_array() {
        Some(array) => array,
        None => panic!("{} carries an array", commands::UPDATE_CLIPS),
    };
    array
        .iter()
        .map(|clip| match clip.get("label").and_then(Value::as_str) {
            Some(label) => label.to_string(),
            None => panic!("every clip carries a label"),
        })
        .collect()
}

fn id_at(payload: &Value, index: usize) -> String {
    match payload.get(index).and_then(|clip| clip.get("id")) {
        Some(Value::String(id)) => id.clone(),
        other => panic!("there is no clip at index {index}: {other:?}"),
    }
}

fn draft(label: &str, value: &str, colour: &str) -> Value {
    json!({ "clip": { "label": label, "value": value, "colour": colour } })
}

/// The first `invoke` a frontend makes, made before `app.run`. The
/// pre-refactor build lost this race with a global populated from a spawned
/// task; there is no global here, and this is the assertion that says so.
#[test]
fn the_first_invoke_after_build_is_served_rather_than_dropped() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    assert_eq!(expect_ok(&window, "list_clips", json!({})), json!([]));
}

/// Contract §2: the sixteen command names are exact. These five are
/// implemented, and the eleven that are not must reject rather than appear to
/// work.
#[test]
fn the_registered_names_are_exactly_the_five_this_package_owns() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    for (command, arguments) in [
        ("list_clips", json!({})),
        ("create_clip", draft("l", "v", "teal")),
        ("update_clip", json!({ "clip": {} })),
        ("delete_clip", json!({})),
        ("copy_clip", json!({})),
    ] {
        // Resolved or rejected with a `ClipError`; never "command not found",
        // which arrives as a plain string.
        match invoke(&window, command, arguments) {
            Ok(_) => {}
            Err(rejection) => assert!(
                rejection.get("kind").is_some(),
                "{command} is not registered: {rejection}"
            ),
        }
    }

    let unregistered = expect_err(&window, "get_clips", json!({}));
    assert!(
        unregistered.get("kind").is_none(),
        "the pre-refactor name must not be served: {unregistered}"
    );
}

#[test]
fn a_created_clip_appears_in_the_list_and_in_the_event() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    assert_eq!(
        expect_ok(
            &window,
            "create_clip",
            draft("Support greeting", "Hello", "amber")
        ),
        Value::Null,
        "a mutating command returns null, never the created clip"
    );

    assert_eq!(emissions.count(), 1);
    let emitted = emissions.last();
    assert_eq!(labels(&emitted), vec!["Support greeting"]);
    assert_eq!(
        emitted,
        expect_ok(&window, "list_clips", json!({})),
        "the event carries exactly what list_clips would return"
    );

    // Contract §1: four fields, and no `use_count` in either direction.
    let clip = match emitted.get(0) {
        Some(clip) => clip.clone(),
        None => panic!("the event should carry the created clip"),
    };
    let object = match clip.as_object() {
        Some(object) => object.clone(),
        None => panic!("a clip is an object"),
    };
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["colour", "id", "label", "value"]);
    assert_eq!(object.get("colour"), Some(&json!("amber")));
}

#[test]
fn clips_are_listed_and_emitted_in_display_order() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    for label in ["first", "second", "third"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }

    assert_eq!(emissions.count(), 3);
    assert_eq!(labels(&emissions.last()), vec!["first", "second", "third"]);
}

#[test]
fn an_update_rewrites_one_clip_and_leaves_the_order_alone() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    for label in ["first", "second"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }
    let id = id_at(&emissions.last(), 0);

    expect_ok(
        &window,
        "update_clip",
        json!({ "clip": { "id": id, "label": "renamed", "value": "new", "colour": "violet" } }),
    );

    let emitted = emissions.last();
    assert_eq!(labels(&emitted), vec!["renamed", "second"]);
    assert_eq!(
        id_at(&emitted, 0),
        id,
        "the id is stable for the clip's life"
    );
}

/// Contract §3: not emitted by a failed command. `update_clip` on an unknown id
/// is `not_found` and never creates a clip.
#[test]
fn a_failed_update_is_not_found_and_emits_nothing() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    let missing = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    let rejection = expect_err(
        &window,
        "update_clip",
        json!({ "clip": { "id": missing, "label": "l", "value": "v", "colour": "teal" } }),
    );
    assert_eq!(
        rejection,
        json!({ "kind": "not_found", "clip_id": missing })
    );
    assert_eq!(emissions.count(), 0);
    assert_eq!(expect_ok(&window, "list_clips", json!({})), json!([]));
}

#[test]
fn a_delete_removes_the_clip_and_emits_the_remainder() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    for label in ["first", "second", "third"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }
    let id = id_at(&emissions.last(), 0);

    // The argument is a bare `clip_id`, not a wrapper object (contract §0).
    expect_ok(&window, "delete_clip", json!({ "clip_id": id }));

    assert_eq!(labels(&emissions.last()), vec!["second", "third"]);
}

#[test]
fn a_delete_of_an_unknown_clip_is_not_found_and_emits_nothing() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    expect_ok(&window, "create_clip", draft("first", "a value", "slate"));
    let before = emissions.count();

    let missing = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    assert_eq!(
        expect_err(&window, "delete_clip", json!({ "clip_id": missing })),
        json!({ "kind": "not_found", "clip_id": missing })
    );
    assert_eq!(emissions.count(), before, "a failed command emits nothing");
}

/// ADR-0008. A copy changes only `use_count`, which nothing on the frontend
/// displays, so an event per click would rebuild the list and the tray on the
/// hottest path in the application.
///
/// The clipboard write itself is not exercised here — that is
/// `clips::tests`, against a fake writer. This asserts what the *seam* does:
/// `copy_clip` is registered, it validates, and it emits nothing.
#[test]
fn copy_clip_is_registered_validates_its_argument_and_emits_nothing() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    assert_eq!(
        expect_err(&window, "copy_clip", json!({})),
        json!({ "kind": "invalid_input", "field": "clip_id", "reason": "required" })
    );
    assert_eq!(
        expect_err(&window, "copy_clip", json!({ "clip_id": "not-a-uuid" })),
        json!({ "kind": "invalid_input", "field": "clip_id", "reason": "malformed_uuid" })
    );

    // Step 1 of the copy sequence, and it happens before the clipboard is
    // touched, so this is safe to assert without a desktop session.
    let missing = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    assert_eq!(
        expect_err(&window, "copy_clip", json!({ "clip_id": missing })),
        json!({ "kind": "not_found", "clip_id": missing })
    );

    assert_eq!(emissions.count(), 0);
}

/// Contract §1, *Argument deserialisation*. `JSON.stringify` drops a key whose
/// value is `undefined`, so `invoke("delete_clip", {})` is the shape of any
/// frontend bug that lets a variable go undefined. It must be
/// `invalid_input { reason: "required" }` naming the argument, not a plain
/// string from inside Tauri that the frontend can only read as `internal`.
#[test]
fn an_absent_argument_is_invalid_input_naming_the_argument() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    for (command, field) in [
        ("create_clip", "clip"),
        ("update_clip", "clip"),
        ("delete_clip", "clip_id"),
        ("copy_clip", "clip_id"),
    ] {
        assert_eq!(
            expect_err(&window, command, json!({})),
            json!({ "kind": "invalid_input", "field": field, "reason": "required" }),
            "{command} with no arguments"
        );
    }
}

/// The rejection is a `ClipError` object with a `kind` discriminant, on the
/// wire, in `snake_case`. A rejection that is not one is a backend defect the
/// frontend can only map to `internal`.
#[test]
fn every_validation_failure_crosses_the_seam_as_a_clip_error_object() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    let cases: Vec<(Value, Value)> = vec![
        (
            draft("", "v", "teal"),
            json!({ "kind": "invalid_input", "field": "label", "reason": "required" }),
        ),
        (
            draft(&"a".repeat(101), "v", "teal"),
            json!({ "kind": "invalid_input", "field": "label", "reason": "too_long" }),
        ),
        (
            draft("a\nb", "v", "teal"),
            json!({ "kind": "invalid_input", "field": "label", "reason": "contains_control_characters" }),
        ),
        (
            draft("l", "v", "chartreuse"),
            json!({ "kind": "invalid_input", "field": "colour", "reason": "not_a_palette_token" }),
        ),
        (
            json!({ "clip": { "label": "l", "value": "v", "colour": "teal", "use_count": 3 } }),
            json!({ "kind": "invalid_input", "field": "use_count", "reason": "not_permitted" }),
        ),
        (
            json!({ "clip": { "label": "l", "value": "v", "colour": "teal", "icon": "x" } }),
            json!({ "kind": "invalid_input", "field": "icon", "reason": "unknown_field" }),
        ),
    ];

    for (arguments, expected) in cases {
        assert_eq!(
            expect_err(&window, "create_clip", arguments.clone()),
            expected,
            "for {arguments}"
        );
    }
}

/// The backend mints every id (contract §5, *Identity*). WP-05's own test
/// requirement: a create with a client-supplied id is rejected.
#[test]
fn a_create_carrying_a_client_supplied_id_is_rejected_and_stores_nothing() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    let arguments = json!({ "clip": {
        "id": "3f2504e0-4f89-41d3-9a0c-0305e82c3301",
        "label": "l",
        "value": "v",
        "colour": "teal",
    }});
    assert_eq!(
        expect_err(&window, "create_clip", arguments),
        json!({ "kind": "invalid_input", "field": "id", "reason": "not_permitted" })
    );
    assert_eq!(emissions.count(), 0);
    assert_eq!(expect_ok(&window, "list_clips", json!({})), json!([]));
}

/// Contract §0: `snake_case` everywhere, without exception. Tauri's default is
/// `camelCase`, so this fails the moment the `rename_all` attribute is dropped
/// from a command.
#[test]
fn an_argument_sent_in_camel_case_is_not_recognised() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    let id = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    assert_eq!(
        expect_err(&window, "delete_clip", json!({ "clipId": id })),
        json!({ "kind": "invalid_input", "field": "clip_id", "reason": "required" }),
        "clipId is not an argument name on this contract"
    );
}

/// Contract §1: a wrongly-typed argument is not modelled. It fails inside Tauri
/// before the command body runs and rejects with a plain string, which the
/// frontend maps to `internal`. This is asserted so that the line between
/// "absent is modelled" and "wrongly typed is not" stays where the contract
/// drew it.
#[test]
fn a_wrongly_typed_argument_rejects_with_something_that_is_not_a_clip_error() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    let rejection = expect_err(&window, "delete_clip", json!({ "clip_id": 7 }));
    assert!(
        rejection.get("kind").is_none(),
        "this condition is deliberately unmodelled: {rejection}"
    );
}

/// Two windows would both receive the event, because it is emitted through the
/// `AppHandle`. There is one window; this asserts the emission reaches a
/// listener rather than only the emitting webview.
#[test]
fn the_event_is_emitted_through_the_app_handle_rather_than_one_webview() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    expect_ok(&window, "create_clip", draft("first", "a value", "slate"));
    assert_eq!(emissions.count(), 1);
}
