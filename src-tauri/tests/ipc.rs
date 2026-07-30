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
//! - **Every mutating command rebuilds the native tray menu**, and no other
//!   command does. That is a completeness check rather than a behaviour test —
//!   see [`every_mutation_rebuilds_the_tray_menu_and_nothing_else_does`] for
//!   what it can and cannot see.
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
//! Neither uses a hook now. This harness calls `Builder::manage`, which
//! registers the state before the `App` exists at all; `lib.rs` calls
//! `app.manage` on the built `App`, because ADR-0012's log sink has to be
//! installed by `Builder::build` before startup recovery opens the store. Both
//! land the store before `App::run`, which is what the property needs — the
//! difference is only where the store is *opened*.
//!
//! **Do not simplify either one into a `setup` hook.** The change would be
//! invisible here: the helper below constructs the application itself, so a
//! `setup` hook added to `lib.rs` alone leaves every test here passing while the
//! shipped binary reopens the window this file exists to close.

use std::path::PathBuf;
use std::sync::atomic::{self, AtomicBool};
use std::sync::{Arc, Mutex, Once};
use std::thread::ThreadId;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

use fast_clip_lib::commands;
use fast_clip_lib::storage::{Store, StorePaths};

type MockApp = App<tauri::test::MockRuntime>;
type MockWindow = WebviewWindow<tauri::test::MockRuntime>;

/// The registration under test — **the application's own list, not a copy of
/// it.** `fast_clip_lib::command_handler!` expands to the one
/// `generate_handler!` invocation `lib.rs` registers, so this harness cannot
/// describe a surface the application does not serve.
///
/// It used to be a second literal copy, and the copy was already wrong: WP-06
/// found the registration test asserting `reorder_clips` was *not* registered,
/// so it passed while describing a surface that had changed. With one list, a
/// command added to `lib.rs` becomes registered here too, and
/// [`every_one_of_the_sixteen_contract_commands_is_registered`] fails
/// until it is moved out of the unimplemented set.
fn build(paths: StorePaths) -> MockApp {
    let built = mock_builder()
        .invoke_handler(fast_clip_lib::command_handler!())
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

/// The PIN [`locked_store`] wraps its key under.
const LOCKED_STORE_PIN: &str = "135790";

/// **A complete locked store**: a real SQLCipher database and the key material a
/// known PIN opens it with.
///
/// It used to be sixty-four bytes of `0x1f` and no `keyfile`, which was enough
/// while nothing read key material — the classifier only inspects the header,
/// and a SQLCipher database begins with a random salt rather than the SQLite
/// magic. Once `get_lock_state` began reading `keyfile`, that fixture stopped
/// being a locked store and became a **faulted** one: encrypted, with its only
/// key missing, which is `crypto { bad_key_material }` and a failure screen.
///
/// The distinction is worth the cost of building the real thing. A store that is
/// merely locked and a store whose key is gone are different situations with
/// different remedies, and tests about the first must not be written against the
/// second. [`encrypted_store_with_no_key_material`] is the second, where a test
/// wants it.
///
/// The `TempDir` is returned alongside the paths and **must be held for as long
/// as the application is used**: dropping it deletes the fixture out from under
/// the store.
fn locked_store() -> (tempfile::TempDir, StorePaths) {
    use fast_clip_lib::crypto::{keyfile, Dek, KeyMaterial, Pin};
    use fast_clip_lib::storage::{connection, schema};

    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    if let Err(e) = std::fs::create_dir_all(paths.dir()) {
        panic!("could not create the fixture directory: {e}");
    }

    let dek = match Dek::generate() {
        Ok(dek) => dek,
        Err(e) => panic!("a DEK should generate: {e}"),
    };
    let pin = match Pin::parse("pin", Some(LOCKED_STORE_PIN.to_owned())) {
        Ok(pin) => pin,
        Err(e) => panic!("the fixture PIN should parse: {e}"),
    };
    // **The KDF at its binding floor, not the shipped tuple.** The cost is not
    // what these tests are about, and one of them derives six times; the shipped
    // 128 MiB tuple would add half a minute to the suite for nothing. The
    // shipped tuple is covered end to end by `tests/two_factor.rs` and by the
    // real `enable_encryption` calls elsewhere in this file.
    let material = {
        use fast_clip_lib::crypto::kdf::{self, KdfParams, MIN_M_COST_KIB, MIN_T_COST, P_COST};
        let mut kdf_params = match KdfParams::generate() {
            Ok(params) => params,
            Err(e) => panic!("parameters should generate: {e}"),
        };
        kdf_params.m_cost_kib = MIN_M_COST_KIB;
        kdf_params.t_cost = MIN_T_COST;
        kdf_params.p_cost = P_COST;

        let wrapping = match kdf::derive(&pin, &kdf_params) {
            Ok(key) => key,
            Err(e) => panic!("the wrapping key should derive: {e}"),
        };
        let wrapped_dek = match fast_clip_lib::crypto::wrap::wrap(&wrapping, &dek) {
            Ok(wrapped) => wrapped,
            Err(e) => panic!("the DEK should wrap: {e}"),
        };
        KeyMaterial {
            kdf: kdf_params,
            wrapped_dek,
            failed_attempts: 0,
            locked_until_unix_ms: None,
        }
    };
    if let Err(e) = keyfile::write(&paths, &material) {
        panic!("the fixture key material should write: {e}");
    }

    let connection = match connection::open_encrypted(&paths.db(), &dek) {
        Ok(connection) => connection,
        Err(e) => panic!("the fixture store should open: {e}"),
    };
    if let Err(e) = schema::create(&connection) {
        panic!("the fixture schema should create: {e}");
    }
    connection::checkpoint_and_close(connection);

    (parent, paths)
}

/// An encrypted store whose `keyfile` is absent — the store carried to a machine
/// it was not created on, or a sync client restoring an older profile over it.
fn encrypted_store_with_no_key_material() -> (tempfile::TempDir, StorePaths) {
    let (parent, paths) = locked_store();
    if let Err(e) = std::fs::remove_file(paths.keyfile()) {
        panic!("the fixture keyfile should be removable: {e}");
    }
    (parent, paths)
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

/// Contract §2: the sixteen command names are exact, and **all sixteen are now
/// served** — five from WP-05, one from WP-06, two from WP-09, three from WP-14
/// and five from WP-07. The list below is the whole surface; a name missing from
/// it is a command the frontend can call and this suite never exercises.
#[test]
fn every_one_of_the_sixteen_contract_commands_is_registered() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    let mut served = 0;
    for (command, arguments) in [
        ("list_clips", json!({})),
        ("create_clip", draft("l", "v", "teal")),
        ("update_clip", json!({ "clip": {} })),
        ("delete_clip", json!({})),
        ("copy_clip", json!({})),
        ("reorder_clips", json!({})),
        ("export_clips", json!({})),
        ("import_clips", json!({})),
        // With no `pin` these fail validation, which is the point: the loop
        // asserts they are *served*, and a conversion must not actually run
        // here.
        ("enable_encryption", json!({})),
        ("disable_encryption", json!({})),
        ("change_pin", json!({})),
        ("unlock", json!({})),
        ("lock", json!({})),
        ("get_lock_state", json!({})),
        ("get_settings", json!({})),
        ("set_always_on_top", json!({ "enabled": true })),
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
        served += 1;
    }

    let unregistered = expect_err(&window, "get_clips", json!({}));
    assert!(
        unregistered.get("kind").is_none(),
        "the pre-refactor name must not be served: {unregistered}"
    );

    // **Nothing is left unimplemented.** The count is asserted rather than
    // written in a comment, because sixteen is contract §2's number and a
    // seventeenth row here would mean this file and that page disagree about the
    // surface. It is also what fails if a command is ever removed from
    // `command_handler!` without this list being touched.
    assert_eq!(
        served, 16,
        "contract §2 declares sixteen commands and every one must be served"
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

/// Every id, in the new display order, from an `update_clips` payload.
fn ids(payload: &Value) -> Vec<String> {
    let array = match payload.as_array() {
        Some(array) => array,
        None => panic!("{} carries an array", commands::UPDATE_CLIPS),
    };
    (0..array.len())
        .map(|index| id_at(payload, index))
        .collect()
}

/// WP-06's definition of done, at the seam: the order the user set is the order
/// `list_clips` returns, and the event carries it.
#[test]
fn a_reorder_applies_the_permutation_and_emits_the_new_order() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    for label in ["first", "second", "third"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }
    let created = ids(&emissions.last());
    let moved = vec![created[2].clone(), created[0].clone(), created[1].clone()];

    assert_eq!(
        expect_ok(&window, "reorder_clips", json!({ "order": moved })),
        Value::Null,
        "a mutating command returns null"
    );

    let emitted = emissions.last();
    assert_eq!(labels(&emitted), vec!["third", "first", "second"]);
    assert_eq!(ids(&emitted), moved);
    assert_eq!(
        emitted,
        expect_ok(&window, "list_clips", json!({})),
        "the event carries exactly what list_clips would return"
    );
}

/// The stale-client case ADR-0007 exists to make detectable: a clip was created
/// between the frontend's last list and the drop, so the permutation is short by
/// one. It is rejected whole, nothing moves, and no event is emitted — which is
/// what makes the frontend's recovery (render the last `update_clips`) correct.
#[test]
fn a_stale_order_is_not_a_permutation_and_moves_nothing() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    for label in ["first", "second", "third"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }
    let created = ids(&emissions.last());
    let before = emissions.count();

    let rejected = json!({
        "kind": "invalid_input", "field": "order", "reason": "not_a_permutation"
    });
    let stranger = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    let cases: Vec<Value> = vec![
        // Short by one: a clip was created after the frontend's last list.
        json!([created[1], created[0]]),
        // One id the store does not hold. `not_found` is never the answer here.
        json!([created[2], created[1], stranger]),
        // The right length, with one id twice.
        json!([created[0], created[1], created[1]]),
        // Longer than the store: a clip was deleted after the last list.
        json!([created[0], created[1], created[2], stranger]),
        // Empty, against a store that holds three.
        json!([]),
    ];

    for order in cases {
        assert_eq!(
            expect_err(&window, "reorder_clips", json!({ "order": order })),
            rejected,
            "for the order {order}"
        );
    }

    assert_eq!(emissions.count(), before, "a failed command emits nothing");
    assert_eq!(
        ids(&expect_ok(&window, "list_clips", json!({}))),
        created,
        "not one row may have moved"
    );
}

/// A malformed element rejects before the store is consulted, and it names the
/// argument rather than an index.
#[test]
fn a_malformed_id_in_an_order_is_reported_as_one() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    expect_ok(&window, "create_clip", draft("first", "a value", "slate"));
    let before = emissions.count();

    assert_eq!(
        expect_err(&window, "reorder_clips", json!({ "order": ["not-a-uuid"] })),
        json!({ "kind": "invalid_input", "field": "order", "reason": "malformed_uuid" })
    );
    assert_eq!(emissions.count(), before);
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

// ---- export and import (WP-09) ----

/// **The definition of done, at the seam.** Export, wipe the store, import:
/// every clip returns, and `list_clips` says so. Acceptance criterion 7 is this
/// sequence, and the wipe is a fresh application over a fresh directory.
#[test]
fn a_round_trip_through_the_seam_returns_every_clip() {
    let workspace = dir();
    let target = workspace.path().join("clips.json");
    let path = target.to_string_lossy().into_owned();

    let exported = {
        let parent = dir();
        let app = build(StorePaths::at(parent.path().join(".fast-clip")));
        let window = window(&app);
        for label in ["first", "second", "third"] {
            expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
        }
        assert_eq!(
            expect_ok(&window, "export_clips", json!({ "path": path })),
            json!({ "exported": 3 }),
            "the wire shape is one count"
        );
        expect_ok(&window, "list_clips", json!({}))
    };

    // A different directory: no database, no sidecars, nothing.
    let wiped = dir();
    let app = build(StorePaths::at(wiped.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);
    assert_eq!(expect_ok(&window, "list_clips", json!({})), json!([]));

    assert_eq!(
        expect_ok(&window, "import_clips", json!({ "path": path })),
        json!({ "imported": 3 })
    );

    // Contract §3: `update_clips` follows a successful import, carrying the
    // complete list.
    assert_eq!(emissions.count(), 1);
    let emitted = emissions.last();
    assert_eq!(labels(&emitted), vec!["first", "second", "third"]);
    assert_eq!(emitted, expect_ok(&window, "list_clips", json!({})));

    // The clips came back with the same content and fresh ids.
    assert_eq!(labels(&exported), labels(&emitted));
    assert_ne!(ids(&exported), ids(&emitted), "import mints every id");
}

/// Contract §3: not emitted by a failed command. A file with one bad record
/// changes nothing and emits nothing.
#[test]
fn a_rejected_import_emits_nothing_and_changes_nothing() {
    let workspace = dir();
    let target = workspace.path().join("clips.json");
    let body = json!({
        "format": "fastclip-export",
        "version": 1,
        "clips": [
            { "label": "good", "value": "v", "colour": "teal" },
            { "label": "bad", "value": "v", "colour": "chartreuse" },
        ],
    });
    if let Err(e) = std::fs::write(&target, body.to_string()) {
        panic!("could not write the fixture: {e}");
    }

    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    expect_ok(&window, "create_clip", draft("mine", "a value", "slate"));
    let emissions = Emissions::listening(&app);

    assert_eq!(
        expect_err(
            &window,
            "import_clips",
            json!({ "path": target.to_string_lossy() })
        ),
        json!({
            "kind": "import", "reason": "invalid_value", "field": "colour", "index": 1
        })
    );
    assert_eq!(emissions.count(), 0, "a failed command emits nothing");
    assert_eq!(
        labels(&expect_ok(&window, "list_clips", json!({}))),
        vec!["mine"],
        "not one clip may have been written"
    );
}

/// The `io` variant crosses the seam with the four declared keys and the path
/// the user chose.
#[test]
fn an_import_of_a_file_that_is_not_there_crosses_the_seam_as_io_not_found() {
    let workspace = dir();
    let missing = workspace.path().join("nowhere.json");
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    assert_eq!(
        expect_err(
            &window,
            "import_clips",
            json!({ "path": missing.to_string_lossy() })
        ),
        json!({
            "kind": "io",
            "operation": "read",
            "path": missing.to_string_lossy(),
            "reason": "not_found",
        })
    );
}

/// Contract §2: export and import are "works while locked: no", and the export
/// must write no file — it would be a plaintext copy of a store the user locked.
#[test]
fn a_locked_store_refuses_both_and_writes_no_file() {
    let workspace = dir();
    let target = workspace.path().join("clips.json");
    let (_parent, paths) = locked_store();
    let app = build(paths);
    let window = window(&app);

    for command in ["export_clips", "import_clips"] {
        assert_eq!(
            expect_err(
                &window,
                command,
                json!({ "path": target.to_string_lossy() })
            ),
            json!({ "kind": "locked" }),
            "{command} works while locked: no"
        );
    }
    assert!(!target.exists());
    let mut part = target.clone().into_os_string();
    part.push(".part");
    assert!(!std::path::PathBuf::from(part).exists());
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
        ("reorder_clips", "order"),
        ("export_clips", "path"),
        ("import_clips", "path"),
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

/// Step 2 of the startup sequence, on a machine that has never run FastClip.
/// The documented default, and not an error.
#[test]
fn get_settings_answers_the_default_on_a_first_run() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    assert_eq!(
        expect_ok(&window, "get_settings", json!({})),
        json!({ "always_on_top": false }),
        "the wire shape is one boolean, with no version field"
    );
}

/// **The defect this package exists to fix.** The pre-refactor build held the
/// flag in React state initialised to `false`, so the toggle reset on every
/// launch. Restarting is a second application over the same directory, which is
/// what a launch is.
#[test]
fn the_toggle_survives_a_restart_in_both_directions() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));

    for enabled in [true, false, true] {
        {
            let app = build(paths.clone());
            let window = window(&app);
            assert_eq!(
                expect_ok(&window, "set_always_on_top", json!({ "enabled": enabled })),
                Value::Null
            );
        }

        // A new application over the same directory: the launch after the one
        // that set it.
        let app = build(paths.clone());
        let window = window(&app);
        assert_eq!(
            expect_ok(&window, "get_settings", json!({})),
            json!({ "always_on_top": enabled }),
            "the setting should have survived the restart"
        );
    }
}

/// Contract §1, *Argument deserialisation*. `JSON.stringify` drops a key whose
/// value is `undefined`, and the rejection must name the argument rather than
/// failing inside Tauri with a plain string.
#[test]
fn set_always_on_top_with_no_argument_names_the_argument_and_writes_nothing() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths.clone());
    let window = window(&app);

    expect_ok(&window, "set_always_on_top", json!({ "enabled": true }));
    assert_eq!(
        expect_err(&window, "set_always_on_top", json!({})),
        json!({ "kind": "invalid_input", "field": "enabled", "reason": "required" })
    );
    assert_eq!(
        expect_ok(&window, "get_settings", json!({})),
        json!({ "always_on_top": true }),
        "a rejected call must not have written anything"
    );
}

/// Step 3 of the startup sequence, and the answer a new user gets.
#[test]
fn get_lock_state_reports_an_unencrypted_store_on_a_first_run() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    assert_eq!(
        expect_ok(&window, "get_lock_state", json!({})),
        json!({
            "encryption_enabled": false,
            "locked": false,
            "attempts_remaining": null,
            "retry_after_ms": null,
        })
    );
}

/// **A store that will not open reaches the failure screen, not an empty list.**
/// The criterion the package exists to make testable: `get_lock_state` rejects,
/// so the frontend stops at step 3 and never renders a list that would be
/// indistinguishable from a fresh install.
#[test]
fn a_store_that_will_not_open_rejects_from_get_lock_state_rather_than_listing_nothing() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    if let Err(e) = std::fs::create_dir_all(paths.dir()) {
        panic!("could not create the fixture directory: {e}");
    }
    // The SQLite magic, so it classifies as plaintext, over rubbish.
    let mut bytes = b"SQLite format 3\0".to_vec();
    bytes.extend_from_slice(&[0u8; 512]);
    if let Err(e) = std::fs::write(paths.db(), &bytes) {
        panic!("could not write the fixture: {e}");
    }

    let app = build(paths);
    let window = window(&app);

    let relayed = json!({ "kind": "crypto", "reason": "corrupt" });
    assert_eq!(expect_err(&window, "get_lock_state", json!({})), relayed);

    // The same recorded value, from a command that did not discover it. There
    // is one fault, established once, and every report of it is identical
    // (contract § Opening the database).
    assert_eq!(expect_err(&window, "list_clips", json!({})), relayed);
}

/// The other cause of a failure screen at step 3: `clips.db` is present and its
/// header could not be read at all. Nothing was deleted and nothing created, so
/// the message must not advise re-importing — which is why this is `storage`
/// and not `crypto`.
#[test]
fn an_unreadable_store_rejects_from_get_lock_state_as_storage() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    if let Err(e) = std::fs::create_dir_all(paths.dir()) {
        panic!("could not create the fixture directory: {e}");
    }
    if let Err(e) = std::fs::create_dir(paths.db()) {
        panic!("could not create the fixture: {e}");
    }

    let app = build(paths);
    let window = window(&app);

    assert_eq!(
        expect_err(&window, "get_lock_state", json!({})),
        json!({ "kind": "storage" })
    );
}

/// Contract §2: the settings pair works while locked, and that is why the file
/// is outside the database. Every clip command returns `locked` here and both
/// settings commands answer.
#[test]
fn the_settings_commands_work_while_the_store_is_locked() {
    let (_parent, paths) = locked_store();
    let app = build(paths);
    let window = window(&app);

    assert_eq!(
        expect_err(&window, "list_clips", json!({})),
        json!({ "kind": "locked" }),
        "the fixture should be a locked store"
    );
    // `reorder_clips` is "works while locked: no", and the lock is decided
    // before the argument is compared against the stored id set — an empty order
    // is an exact permutation of an empty store, so `locked` here proves the
    // order of the two.
    assert_eq!(
        expect_err(&window, "reorder_clips", json!({ "order": [] })),
        json!({ "kind": "locked" })
    );
    assert_eq!(
        expect_ok(&window, "set_always_on_top", json!({ "enabled": true })),
        Value::Null
    );
    assert_eq!(
        expect_ok(&window, "get_settings", json!({})),
        json!({ "always_on_top": true })
    );
    assert_eq!(
        expect_ok(&window, "get_lock_state", json!({})),
        json!({
            "encryption_enabled": true,
            "locked": true,
            "attempts_remaining": 5,
            "retry_after_ms": null,
        }),
        "WP-07 replaces the attempt state with the values persisted in keyfile"
    );
}

/// A settings file this build cannot interpret must not stop the application.
/// All three of WP-14's failure paths, through a real `invoke`.
#[test]
fn a_broken_settings_file_starts_the_application_at_the_default() {
    for fixture in [
        r#"{"version":1,"always_on_"#,
        r#"{"version":99,"always_on_top":true}"#,
        "",
        "not json at all",
        r#"{"version":1,"always_on_top":"yes","theme":"dark"}"#,
    ] {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        if let Err(e) = std::fs::write(paths.settings(), fixture) {
            panic!("could not write the fixture: {e}");
        }

        let app = build(paths.clone());
        let window = window(&app);

        assert_eq!(
            expect_ok(&window, "get_settings", json!({})),
            json!({ "always_on_top": false }),
            "for the fixture {fixture:?}"
        );
        assert_eq!(
            expect_ok(&window, "get_lock_state", json!({})),
            json!({
                "encryption_enabled": false,
                "locked": false,
                "attempts_remaining": null,
                "retry_after_ms": null,
            }),
            "a broken settings file must not block the launch"
        );

        // And the next write repairs the file at version 1.
        expect_ok(&window, "set_always_on_top", json!({ "enabled": true }));
        match std::fs::read_to_string(paths.settings()) {
            Ok(after) => assert_eq!(after, r#"{"version":1,"always_on_top":true}"#),
            Err(e) => panic!("the settings should be readable after a write: {e}"),
        }
    }
}

// ---- the tray rebuild, and whether every mutation performs one (WP-08) ----

/// Every `tray::refresh` call, tagged with the thread that made it.
///
/// **The rebuild itself cannot be read back.** `tauri::tray::TrayIcon` exposes
/// no getter for its menu, and building a real one needs a notification area and
/// a Windows message loop, neither of which a test process has. What *is*
/// observable is that `refresh` ran: with no tray installed it logs
/// [`fast_clip_lib::tray::NO_TRAY_TO_REBUILD`] and returns before touching
/// anything, and every application this file builds has no tray.
///
/// Tagged by thread because the whole suite shares one process and one global
/// logger, and cargo runs these tests in parallel. A command runs `refresh`
/// inline on the thread that invoked it, so counting one thread's records counts
/// one test's rebuilds.
static REFRESHES: Mutex<Vec<ThreadId>> = Mutex::new(Vec::new());

/// The global logger, installed once. It records the one message above and
/// discards everything else, so it neither grows without bound nor prints.
struct RefreshRecorder;

static RECORDER: RefreshRecorder = RefreshRecorder;
static INSTALL_RECORDER: Once = Once::new();

/// Whether [`RECORDER`] is the logger this process is actually using. See
/// [`install_recorder`] for why a bare `Once` is not enough on its own.
static RECORDER_LIVE: AtomicBool = AtomicBool::new(false);

impl log::Log for RefreshRecorder {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if record.level() != log::Level::Debug {
            return;
        }
        if record.args().to_string() != fast_clip_lib::tray::NO_TRAY_TO_REBUILD {
            return;
        }
        // A poisoned lock is absorbed rather than panicked on: nothing else
        // takes it while it can panic, and a panic inside a logger would fail
        // whichever unrelated test happened to log next.
        if let Ok(mut seen) = REFRESHES.lock() {
            seen.push(std::thread::current().id());
        }
    }

    fn flush(&self) {}
}

/// Install the recorder, and **prove it can see anything at all**.
///
/// The two ways this observation can go quietly blind both end in a rebuild
/// count of zero, which is indistinguishable from a missing `tray::refresh` call
/// and would send whoever met it looking in the wrong file. Both are asserted
/// here instead, so they report themselves by name:
///
/// - another logger installed first, so `set_logger` refused and every record
///   goes somewhere this file cannot read;
/// - the global maximum level filters `Debug` out, so `refresh`'s record is
///   never handed to any logger.
///
/// `Once` makes this safe to call from every test that needs it: a second caller
/// blocks until the first has both installed the recorder and raised the level,
/// so no test can invoke a command through a half-installed sink.
fn install_recorder() {
    INSTALL_RECORDER.call_once(|| {
        if log::set_logger(&RECORDER).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
            RECORDER_LIVE.store(true, atomic::Ordering::SeqCst);
        }
    });

    assert!(
        RECORDER_LIVE.load(atomic::Ordering::SeqCst),
        "another logger was installed before this one, so no tray rebuild in \
         this process is observable and every count below would read zero"
    );
    assert!(
        log::log_enabled!(log::Level::Debug),
        "debug records are being filtered out, so no tray rebuild is observable \
         and every count below would read zero"
    );
}

/// How many rebuilds this thread has made since the process started.
fn rebuilds() -> usize {
    let this = std::thread::current().id();
    match REFRESHES.lock() {
        Ok(seen) => seen.iter().filter(|made_by| **made_by == this).count(),
        Err(e) => panic!("the refresh recorder was poisoned: {e}"),
    }
}

/// What a command must do to the native tray menu.
///
/// **There is no "cannot be observed" variant, and there must not be one.**
/// `copy_clip` briefly had one, on the grounds that its rebuild was reachable
/// only through a clipboard write no test process can make. The answer was to
/// fix `copy_clip` — it now rebuilds whether the copy succeeded or not, exactly
/// as the tray's own caller of `clips::copy` always did — not to widen this
/// enum. An escape hatch here is a place for the next hard row to hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rebuild {
    /// It changes what the menu shows — the clip list, a `use_count` or the
    /// lock state — so it must call `tray::refresh` exactly once.
    Required,
    /// It changes nothing the menu shows, so it must not rebuild.
    Forbidden,
    /// WP-07 owns it. It must not be registered, and the moment it is, the
    /// `NotRegistered` assertion below fails and whoever registered it has to
    /// choose one of the two above.
    NotRegistered,
}

/// Whether the call a row makes resolves or rejects.
///
/// Two axes rather than one, because they are independent: `copy_clip` rejects
/// **and** rebuilds, which is the whole point of it, and folding that into a
/// single `RejectsAndRebuilds` would put it back in a category of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Resolves,
    /// Rejects with a `ClipError` — never with Tauri's "command not found",
    /// which is asserted separately because it would mean the command is gone.
    Rejects,
}

/// The store the row's arguments are computed against: three clips named
/// `first`, `second`, `third`, and a scratch directory for a file argument.
struct Seeded {
    ids: Vec<String>,
    workspace: PathBuf,
}

/// One command of contract §2.
struct Case {
    command: &'static str,
    /// Ignored for a [`Rebuild::NotRegistered`] row, which cannot answer at all.
    answer: Answer,
    rebuild: Rebuild,
    /// The arguments of the single call this row makes.
    prepare: fn(&Seeded) -> Value,
}

/// A valid export file for `import_clips` to merge, written into the scratch
/// directory.
fn an_import_file(seeded: &Seeded) -> Value {
    let target = seeded.workspace.join("import.json");
    let body = json!({
        "format": "fastclip-export",
        "version": 1,
        "clips": [{ "label": "imported", "value": "a value", "colour": "teal" }],
    });
    if let Err(e) = std::fs::write(&target, body.to_string()) {
        panic!("could not write the import fixture: {e}");
    }
    json!({ "path": target.to_string_lossy() })
}

/// **All sixteen commands of contract §2, with what each must do to the tray.**
///
/// This table is the completeness check. Nothing else makes a mutating command
/// call `tray::refresh`: it is six separate call sites today and WP-07 adds two
/// more, of which `lock` is the one that matters — a tray still listing clip
/// labels over a locked store is
/// [criterion 10](../../../docs/src/product/spec.md) breached in the one surface
/// the contract says breaches it exactly as the window would.
const CASES: &[Case] = &[
    Case {
        command: "list_clips",
        answer: Answer::Resolves,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({}),
    },
    Case {
        command: "create_clip",
        answer: Answer::Resolves,
        rebuild: Rebuild::Required,
        prepare: |_| draft("a fourth", "a value", "teal"),
    },
    Case {
        command: "update_clip",
        answer: Answer::Resolves,
        rebuild: Rebuild::Required,
        prepare: |seeded| {
            json!({ "clip": {
                "id": seeded.ids[0], "label": "renamed", "value": "v", "colour": "violet",
            }})
        },
    },
    Case {
        command: "delete_clip",
        answer: Answer::Resolves,
        rebuild: Rebuild::Required,
        prepare: |seeded| json!({ "clip_id": seeded.ids[0] }),
    },
    Case {
        // **The one row that rejects and rebuilds anyway.** `copy_clip`'s
        // rebuild is not conditional on success — see its doc comment, and
        // `a_copy_refused_by_a_locked_store_still_rebuilds_the_tray_menu` below
        // for the case that makes it necessary rather than tidy. A success would
        // need a clipboard write, which no test process can make without
        // clobbering the developer's clipboard, so the call driven here is the
        // `not_found` one: an id the store does not hold, rejected inside the
        // guard and before the clipboard is touched.
        command: "copy_clip",
        answer: Answer::Rejects,
        rebuild: Rebuild::Required,
        prepare: |_| json!({ "clip_id": "3f2504e0-4f89-41d3-9a0c-0305e82c3301" }),
    },
    Case {
        command: "reorder_clips",
        answer: Answer::Resolves,
        rebuild: Rebuild::Required,
        prepare: |seeded| json!({ "order": [seeded.ids[2], seeded.ids[0], seeded.ids[1]] }),
    },
    Case {
        // Reading the store out to a file changes nothing the menu shows.
        command: "export_clips",
        answer: Answer::Resolves,
        rebuild: Rebuild::Forbidden,
        prepare: |seeded| json!({ "path": seeded.workspace.join("export.json").to_string_lossy() }),
    },
    Case {
        command: "import_clips",
        answer: Answer::Resolves,
        rebuild: Rebuild::Required,
        prepare: an_import_file,
    },
    Case {
        command: "get_lock_state",
        answer: Answer::Resolves,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({}),
    },
    Case {
        command: "get_settings",
        answer: Answer::Resolves,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({}),
    },
    Case {
        // Always-on-top is a window property. The menu does not show it.
        command: "set_always_on_top",
        answer: Answer::Resolves,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({ "enabled": true }),
    },
    // WP-07's session commands, against the **plaintext** store every row here
    // is given: all three refuse with `wrong_state { encrypted }`, and a refusal
    // rebuilds nothing. Their real behaviour needs an encrypted store, so it is
    // asserted where one exists:
    //
    // | Command | Where its rebuild is asserted |
    // | ------- | ----------------------------- |
    // | `lock` | `a_lock_gives_up_the_key_and_rebuilds_the_tray_menu` |
    // | `unlock` | `an_unlock_reopens_the_store_and_rebuilds_the_tray_menu` |
    // | `change_pin` | `changing_the_pin_changes_no_lock_state_and_rebuilds_nothing` |
    Case {
        command: "unlock",
        answer: Answer::Rejects,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({ "pin": "135790" }),
    },
    Case {
        command: "lock",
        answer: Answer::Rejects,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({}),
    },
    Case {
        // **A real conversion, driven through the seam.** The store this row is
        // given is plaintext with three clips; this encrypts it. Lock state
        // changed, so the menu must be rebuilt — spec §4.5's third trigger.
        command: "enable_encryption",
        answer: Answer::Resolves,
        rebuild: Rebuild::Required,
        prepare: |_| json!({ "pin": "123456" }),
    },
    Case {
        // Against the same plaintext store this row rejects with
        // `wrong_state { encrypted }`, and nothing committed, so nothing
        // rebuilds. The **successful** disable and its rebuild are asserted by
        // `a_round_trip_through_the_seam_encrypts_and_decrypts_the_store`,
        // which is the only place a store exists that can be disabled.
        command: "disable_encryption",
        answer: Answer::Rejects,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({ "pin": "123456" }),
    },
    Case {
        command: "change_pin",
        answer: Answer::Rejects,
        rebuild: Rebuild::Forbidden,
        prepare: |_| json!({ "current_pin": "135790", "new_pin": "246801" }),
    },
];

/// **The completeness check for `tray::refresh`.**
///
/// Six call sites rebuild the menu today and nothing makes that set complete.
/// This walks contract §2's whole command surface and asserts, per command,
/// that a successful call rebuilds the menu exactly when it should.
///
/// What it can see is one debug record from `refresh` — see [`REFRESHES`] for
/// why that is the only observable. Every registered command is driven through
/// a real `invoke`; there is no row it cannot reach.
#[test]
fn every_mutation_rebuilds_the_tray_menu_and_nothing_else_does() {
    install_recorder();

    assert_eq!(
        CASES.len(),
        16,
        "contract §2 declares sixteen commands and every one needs a row"
    );

    for case in CASES {
        let workspace = dir();
        let parent = dir();
        let app = build(StorePaths::at(parent.path().join(".fast-clip")));
        let window = window(&app);

        if case.rebuild == Rebuild::NotRegistered {
            // Anything but Tauri's "command not found" — which arrives as a
            // rejection with no `kind` — means the command is now served.
            let served = match invoke(&window, case.command, json!({})) {
                Ok(value) => Some(value.to_string()),
                Err(rejection) if rejection.get("kind").is_some() => Some(rejection.to_string()),
                Err(_) => None,
            };
            assert!(
                served.is_none(),
                "{} is registered now, so this row must say what it does to the \
                 tray menu rather than that it does not exist: {}",
                case.command,
                served.unwrap_or_default()
            );
            continue;
        }

        for label in ["first", "second", "third"] {
            expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
        }
        let seeded = Seeded {
            ids: ids(&expect_ok(&window, "list_clips", json!({}))),
            workspace: workspace.path().to_owned(),
        };
        let arguments = (case.prepare)(&seeded);

        let before = rebuilds();
        match case.answer {
            Answer::Resolves => {
                expect_ok(&window, case.command, arguments);
            }
            Answer::Rejects => {
                let rejection = expect_err(&window, case.command, arguments);
                assert!(
                    rejection.get("kind").is_some(),
                    "{} rejected without a ClipError, which means it is not \
                     registered at all: {rejection}",
                    case.command
                );
            }
        }

        match case.rebuild {
            Rebuild::Required => assert_eq!(
                rebuilds() - before,
                1,
                "{} changes what the tray menu shows and must call \
                 tray::refresh exactly once",
                case.command
            ),
            Rebuild::Forbidden => assert_eq!(
                rebuilds() - before,
                0,
                "{} changes nothing the tray menu shows",
                case.command
            ),
            Rebuild::NotRegistered => unreachable!("handled above"),
        }
    }
}

/// The other half of the rule for the commands that **emit**: a command that
/// failed rebuilt nothing, for the same reason it emitted nothing (contract §3).
/// A rebuild after a rejected mutation would be harmless in itself, but it would
/// mean the call site is in the wrong place — before the commit rather than
/// after it.
///
/// `copy_clip` is not in this list and cannot be: it emits nothing at all, and
/// it rebuilds after a failure on purpose
/// ([below](#a_copy_refused_by_a_locked_store_still_rebuilds_the_tray_menu)).
#[test]
fn a_rejected_emitting_mutation_rebuilds_the_tray_menu_no_more_than_it_emits() {
    install_recorder();

    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let emissions = Emissions::listening(&app);

    expect_ok(&window, "create_clip", draft("first", "a value", "slate"));
    let before = rebuilds();
    let emitted = emissions.count();

    let missing = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    let rejected: Vec<(&str, Value)> = vec![
        ("create_clip", draft("", "v", "teal")),
        (
            "update_clip",
            json!({ "clip": {
                "id": missing, "label": "l", "value": "v", "colour": "teal",
            }}),
        ),
        ("delete_clip", json!({ "clip_id": missing })),
        ("reorder_clips", json!({ "order": [missing] })),
        ("import_clips", json!({ "path": "nowhere.json" })),
    ];

    for (command, arguments) in rejected {
        expect_err(&window, command, arguments);
    }

    assert_eq!(rebuilds() - before, 0, "a failed command rebuilds nothing");
    assert_eq!(emissions.count(), emitted, "and emits nothing");
}

/// **The case `copy_clip`'s unconditional rebuild exists for**, and the reason
/// it is not merely tidiness: a `locked` rejection means the tray menu is
/// showing clip labels over a locked store, which is
/// [criterion 10](../../../docs/src/product/spec.md) breached in the one surface
/// the contract says breaches it exactly as the window would. A rebuild corrects
/// that; leaving the menu alone does not.
///
/// `tray::copy_from_tray` — the other caller of the same `clips::copy` — has
/// always rebuilt either way. The two disagreed, and this is the assertion that
/// keeps them from disagreeing again.
#[test]
fn a_copy_refused_by_a_locked_store_still_rebuilds_the_tray_menu() {
    install_recorder();

    let (_parent, paths) = locked_store();
    let app = build(paths);
    let window = window(&app);
    let before = rebuilds();

    assert_eq!(
        expect_err(
            &window,
            "copy_clip",
            json!({ "clip_id": "3f2504e0-4f89-41d3-9a0c-0305e82c3301" })
        ),
        json!({ "kind": "locked" }),
        "the lock is decided before the id is looked up"
    );
    assert_eq!(rebuilds() - before, 1);
}

/// A rejected **argument** is the one `copy_clip` failure that does not rebuild.
/// `require_clip_id` fails before the store is consulted, so nothing has been
/// learned about what the menu should show.
#[test]
fn a_copy_with_a_malformed_argument_rebuilds_nothing() {
    install_recorder();

    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let before = rebuilds();

    for arguments in [json!({}), json!({ "clip_id": "not-a-uuid" })] {
        expect_err(&window, "copy_clip", arguments);
    }
    assert_eq!(rebuilds() - before, 0);
}

/// A locked store is the case [criterion 10](../../../docs/src/product/spec.md)
/// turns on: the menu must be one *Unlock FastClip* item and no clip labels. The
/// two settings commands that work while locked must not rebuild — which is why
/// WP-07's `lock` has to carry the rebuild itself rather than inherit one from
/// something the frontend happens to call afterwards.
#[test]
fn nothing_that_works_while_locked_rebuilds_the_tray_menu() {
    install_recorder();

    let (_parent, paths) = locked_store();
    let app = build(paths);
    let window = window(&app);
    let before = rebuilds();

    assert_eq!(
        expect_err(&window, "create_clip", draft("l", "v", "teal")),
        json!({ "kind": "locked" }),
        "the fixture should be a locked store"
    );
    expect_ok(&window, "set_always_on_top", json!({ "enabled": true }));
    expect_ok(&window, "get_settings", json!({}));
    expect_ok(&window, "get_lock_state", json!({}));

    assert_eq!(rebuilds() - before, 0);
}

// ---- the conversions (WP-07) ----

/// Every `lock_state` payload received, in order. The sibling of [`Emissions`].
struct LockStates(Arc<Mutex<Vec<Value>>>);

impl LockStates {
    fn listening(app: &MockApp) -> Self {
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);
        app.listen("lock_state", move |event| {
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
            None => panic!("no lock_state event was emitted"),
        }
    }
}

/// **The definition of done for the conversions, at the seam.** Turn encryption
/// on, read the clips back, turn it off, read them back again — and check that
/// the file on disk really was encrypted in between.
///
/// This is also the only place a store exists that `disable_encryption` can
/// succeed against, which is why the tray table's row for it drives the
/// rejection instead.
#[test]
fn a_round_trip_through_the_seam_encrypts_and_decrypts_the_store() {
    install_recorder();

    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths.clone());
    let window = window(&app);
    let states = LockStates::listening(&app);

    for label in ["first", "second", "third"] {
        expect_ok(
            &window,
            "create_clip",
            draft(label, "a distinctive value", "slate"),
        );
    }
    let before = expect_ok(&window, "list_clips", json!({}));

    // ---- on ----
    let rebuilds_before = rebuilds();
    assert_eq!(
        expect_ok(&window, "enable_encryption", json!({ "pin": "135790" })),
        Value::Null
    );
    assert_eq!(
        states.last(),
        json!({
            "encryption_enabled": true,
            "locked": false,
            // Contract §1: null whenever `locked` is false. The same value
            // `get_lock_state` would give, because it is the same function.
            "attempts_remaining": null,
            "retry_after_ms": null,
        }),
        "the store stays unlocked: the user just set the PIN"
    );
    assert_eq!(rebuilds() - rebuilds_before, 1, "lock state changed");

    // The clips are still there, and the file is genuinely encrypted.
    assert_eq!(expect_ok(&window, "list_clips", json!({})), before);
    let bytes = match std::fs::read(paths.db()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the store should be readable as bytes: {e}"),
    };
    assert!(!bytes.starts_with(b"SQLite format 3\0"));
    assert!(
        !bytes
            .windows(19)
            .any(|window| window == b"a distinctive value"),
        "a clip value survived the conversion in the clear"
    );
    assert!(paths.keyfile().exists(), "the key material must exist");

    // Enabling again is `wrong_state`, and it must not convert twice.
    assert_eq!(
        expect_err(&window, "enable_encryption", json!({ "pin": "135790" })),
        json!({ "kind": "wrong_state", "required": "unencrypted" })
    );

    // ---- a wrong PIN on the way out changes nothing ----
    let states_before = states.all().len();
    assert_eq!(
        expect_err(&window, "disable_encryption", json!({ "pin": "111111" })),
        json!({ "kind": "bad_pin", "attempts_remaining": null, "retry_after_ms": null }),
        "disable is not subject to backoff and does not touch the counter"
    );
    assert_eq!(
        states.all().len(),
        states_before,
        "a failed command emits nothing"
    );
    assert_eq!(expect_ok(&window, "list_clips", json!({})), before);

    // ---- off ----
    let rebuilds_before = rebuilds();
    assert_eq!(
        expect_ok(&window, "disable_encryption", json!({ "pin": "135790" })),
        Value::Null
    );
    assert_eq!(
        states.last(),
        json!({
            "encryption_enabled": false,
            "locked": false,
            "attempts_remaining": null,
            "retry_after_ms": null,
        })
    );
    assert_eq!(rebuilds() - rebuilds_before, 1);

    assert_eq!(
        expect_ok(&window, "list_clips", json!({})),
        before,
        "every clip must survive the round trip"
    );
    assert!(
        !paths.keyfile().exists(),
        "the key material must be deleted when encryption is off"
    );
    assert!(!paths.db_new().exists(), "no intermediate may survive");
}

/// **The pushed state and the polled state are the same value.**
///
/// Contract §2 has the frontend call `get_lock_state` once at startup and take
/// every later change from the `lock_state` event. If the two ever disagree, the
/// settings view shows one thing at launch and another after a conversion, and
/// nothing tells the user which is true.
///
/// They disagreed once. An earlier draft of `enable_encryption` built its own
/// payload and reported `attempts_remaining: 5` over an unlocked store, where
/// contract §1 says the field is `null` whenever `locked` is false — so the
/// event and the command answered differently about the same store. This is the
/// assertion that would have caught it.
#[test]
fn the_emitted_lock_state_is_exactly_what_get_lock_state_would_return() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    let states = LockStates::listening(&app);

    // Before any conversion.
    assert_eq!(
        expect_ok(&window, "get_lock_state", json!({})),
        json!({
            "encryption_enabled": false,
            "locked": false,
            "attempts_remaining": null,
            "retry_after_ms": null,
        })
    );

    for (command, pin) in [
        ("enable_encryption", "135790"),
        ("disable_encryption", "135790"),
    ] {
        expect_ok(&window, command, json!({ "pin": pin }));
        assert_eq!(
            states.last(),
            expect_ok(&window, "get_lock_state", json!({})),
            "{command} emitted a state that get_lock_state disagrees with"
        );
    }
}

/// Contract §2: `disable_encryption` on a store that is not encrypted is
/// `wrong_state`, and it names the state it needs rather than the one it found.
#[test]
fn disabling_encryption_that_is_not_on_is_wrong_state() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    assert_eq!(
        expect_err(&window, "disable_encryption", json!({ "pin": "135790" })),
        json!({ "kind": "wrong_state", "required": "encrypted" })
    );
}

/// Contract §2: both conversions take exactly six ASCII digits, and an absent
/// argument is `required` rather than `not_six_digits` — the frontend has to
/// tell a bug in its own code from a user typing too few digits.
#[test]
fn both_conversions_validate_the_pin_before_touching_anything() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths.clone());
    let window = window(&app);
    expect_ok(&window, "create_clip", draft("first", "a value", "slate"));

    for command in ["enable_encryption", "disable_encryption"] {
        assert_eq!(
            expect_err(&window, command, json!({})),
            json!({ "kind": "invalid_input", "field": "pin", "reason": "required" }),
            "{command} with no argument"
        );
        for bad in ["12345", "1234567", "12345a", ""] {
            assert_eq!(
                expect_err(&window, command, json!({ "pin": bad })),
                json!({ "kind": "invalid_input", "field": "pin", "reason": "not_six_digits" }),
                "{command} with {bad:?}"
            );
        }
    }

    // Nothing was converted and no key material was written.
    assert!(!paths.keyfile().exists());
    assert_eq!(
        expect_ok(&window, "get_lock_state", json!({})),
        json!({
            "encryption_enabled": false,
            "locked": false,
            "attempts_remaining": null,
            "retry_after_ms": null,
        })
    );
}

// ---- the session commands: unlock, lock, change_pin (WP-07) ----

/// **The definition of done for `lock`** (ADR-0010, criterion 10).
///
/// Locking closes the database and gives up the key, so the process becomes
/// unable to read the store rather than merely unwilling. The tray is rebuilt
/// before `lock` returns, which is the call site where forgetting would leave
/// clip labels in a native menu over a locked store.
#[test]
fn a_lock_gives_up_the_key_and_rebuilds_the_tray_menu() {
    install_recorder();

    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths.clone());
    let window = window(&app);
    let states = LockStates::listening(&app);

    expect_ok(
        &window,
        "create_clip",
        draft("first", "a secret value", "slate"),
    );
    expect_ok(&window, "enable_encryption", json!({ "pin": "135790" }));
    assert_eq!(
        expect_ok(&window, "list_clips", json!({}))
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let before = rebuilds();
    assert_eq!(expect_ok(&window, "lock", json!({})), Value::Null);

    assert_eq!(
        rebuilds() - before,
        1,
        "the tray must be rebuilt before lock returns"
    );
    assert_eq!(
        states.last(),
        json!({
            "encryption_enabled": true,
            "locked": true,
            "attempts_remaining": 5,
            "retry_after_ms": null,
        })
    );

    // **The process cannot read the store any more.** Not "declines to": the
    // connection is closed and the key is zeroised.
    for (command, arguments) in [
        ("list_clips", json!({})),
        (
            "copy_clip",
            json!({ "clip_id": "3f2504e0-4f89-41d3-9a0c-0305e82c3301" }),
        ),
        (
            "export_clips",
            json!({ "path": parent.path().join("x.json").to_string_lossy() }),
        ),
    ] {
        assert_eq!(
            expect_err(&window, command, arguments),
            json!({ "kind": "locked" }),
            "{command} after a lock"
        );
    }

    // Locking again succeeds and says the same thing (contract: idempotent).
    let before = rebuilds();
    assert_eq!(expect_ok(&window, "lock", json!({})), Value::Null);
    assert_eq!(
        states.last(),
        json!({
            "encryption_enabled": true,
            "locked": true,
            "attempts_remaining": 5,
            "retry_after_ms": null,
        }),
        "an already-locked store re-sends the payload unchanged"
    );
    assert_eq!(
        rebuilds() - before,
        1,
        "the no-op still rebuilds; the payload is re-sent"
    );
}

/// **Review G3's F1.** `lock` must succeed and must emit, even when the key
/// material has become unreadable between the unlock and the lock.
///
/// `lock` used to derive its payload, and deriving reads `keyfile`. That read
/// happens *after* the lock has taken effect — the connection is closed and the
/// DEK is zeroised — so a `?` on it returned `crypto { bad_key_material }`, a
/// variant contract §2 excludes, **and skipped the emission**. The frontend's
/// `locked` stayed false, so the clip list, any open form and the search query
/// were all still on screen over a store that had given up its key: criterion 10
/// on the one gesture that exists to prevent it.
///
/// The window is not hypothetical. ADR-0010 names a sync client restoring the
/// file, deletion, and DPAPI refusing as the reasons `unlock` had to declare the
/// key-material variants in the first place.
#[test]
fn a_lock_still_emits_when_the_key_material_has_become_unreadable() {
    install_recorder();

    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths.clone());
    let window = window(&app);
    let states = LockStates::listening(&app);

    expect_ok(
        &window,
        "create_clip",
        draft("first", "a secret value", "slate"),
    );
    expect_ok(&window, "enable_encryption", json!({ "pin": "135790" }));

    // A sync client restores an older profile, or the user deletes it.
    if let Err(e) = std::fs::remove_file(paths.keyfile()) {
        panic!("the fixture keyfile should be removable: {e}");
    }

    let before = rebuilds();
    assert_eq!(
        expect_ok(&window, "lock", json!({})),
        Value::Null,
        "lock declares one error and this is not it"
    );
    assert_eq!(
        states.last(),
        json!({
            "encryption_enabled": true,
            "locked": true,
            "attempts_remaining": 5,
            "retry_after_ms": null,
        }),
        "the frontend must be told the store is locked, or it keeps the clips on screen"
    );
    assert_eq!(rebuilds() - before, 1);

    // And the store really is locked.
    assert_eq!(
        expect_err(&window, "list_clips", json!({})),
        json!({ "kind": "locked" })
    );
}

/// Contract §2 fixes `lock`'s payload at `attempts_remaining: 5` "in every
/// case". A counter left stale on disk by an absorbed reset must not leak into
/// it — the constant is the contract's answer, and the next launch's
/// `get_lock_state` reads the file and corrects it.
#[test]
fn a_lock_emits_the_fixed_payload_even_when_the_counter_on_disk_is_stale() {
    let (_parent, paths) = locked_store();
    let app = build(paths.clone());
    let window = window(&app);
    let states = LockStates::listening(&app);

    // Two failures, then a successful unlock. The reset is absorbed, so force
    // the stale state directly: fail twice and unlock, then rewrite the counter
    // behind the command's back.
    expect_err(&window, "unlock", json!({ "pin": "000000" }));
    expect_err(&window, "unlock", json!({ "pin": "000001" }));
    expect_ok(&window, "unlock", json!({ "pin": LOCKED_STORE_PIN }));

    {
        use fast_clip_lib::crypto::keyfile;
        let mut material = match keyfile::read(&paths) {
            Ok(material) => material,
            Err(e) => panic!("the key material should read: {e}"),
        };
        material.failed_attempts = 3;
        if let Err(e) = keyfile::write(&paths, &material) {
            panic!("the stale counter should write: {e}");
        }
    }

    expect_ok(&window, "lock", json!({}));
    assert_eq!(
        states.last().get("attempts_remaining"),
        Some(&json!(5)),
        "the payload is fixed, not derived from a counter that may be stale"
    );
}

/// **The definition of done for `unlock`**: the way back in is the launch path,
/// and it emits `lock_state` then `update_clips`.
#[test]
fn an_unlock_reopens_the_store_and_rebuilds_the_tray_menu() {
    install_recorder();

    let (_parent, paths) = locked_store();
    let app = build(paths);
    let window = window(&app);
    let states = LockStates::listening(&app);
    let clips = Emissions::listening(&app);

    assert_eq!(
        expect_err(&window, "list_clips", json!({})),
        json!({ "kind": "locked" }),
        "the fixture should start locked"
    );

    let before = rebuilds();
    assert_eq!(
        expect_ok(&window, "unlock", json!({ "pin": LOCKED_STORE_PIN })),
        Value::Null
    );

    assert_eq!(rebuilds() - before, 1);
    assert_eq!(
        states.last(),
        json!({
            "encryption_enabled": true,
            "locked": false,
            "attempts_remaining": null,
            "retry_after_ms": null,
        })
    );
    // Contract: `lock_state` then `update_clips`, and the frontend does not call
    // `list_clips` afterwards.
    assert_eq!(clips.count(), 1, "update_clips follows a successful unlock");
    assert_eq!(clips.last(), expect_ok(&window, "list_clips", json!({})));

    // The store really is open.
    expect_ok(
        &window,
        "create_clip",
        draft("after unlocking", "a value", "teal"),
    );
}

/// A full session: lock, fail, unlock, and the clips are all still there.
#[test]
fn a_lock_and_unlock_round_trip_leaves_every_clip_readable() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths);
    let window = window(&app);

    for label in ["first", "second", "third"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }
    let before = expect_ok(&window, "list_clips", json!({}));

    expect_ok(&window, "enable_encryption", json!({ "pin": "135790" }));
    expect_ok(&window, "lock", json!({}));

    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "111111" })),
        json!({ "kind": "bad_pin", "attempts_remaining": 4, "retry_after_ms": null })
    );
    expect_ok(&window, "unlock", json!({ "pin": "135790" }));

    assert_eq!(
        expect_ok(&window, "list_clips", json!({})),
        before,
        "every clip must survive a lock, a wrong PIN and an unlock"
    );
}

/// **ADR-0011, at the seam.** Five wrong PINs count down, the fifth arms a flat
/// 30 seconds, and every attempt after it reports the identical error. Nothing
/// is destroyed.
#[test]
fn five_wrong_pins_arm_a_flat_thirty_second_wait_and_destroy_nothing() {
    let (_parent, paths) = locked_store();
    let app = build(paths.clone());
    let window = window(&app);

    let before = match std::fs::read(paths.db()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the store should be readable: {e}"),
    };

    // Four attempts count down and arm nothing.
    for remaining in [4u32, 3, 2, 1] {
        assert_eq!(
            expect_err(&window, "unlock", json!({ "pin": "000000" })),
            json!({ "kind": "bad_pin", "attempts_remaining": remaining, "retry_after_ms": null }),
        );
    }

    // The fifth exhausts the allowance and reports the wait it just armed.
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "000000" })),
        json!({ "kind": "bad_pin", "attempts_remaining": 0, "retry_after_ms": 30000 }),
    );

    // **The wait is now running, so the PIN is not evaluated at all** — and that
    // is why the correct one below does not get in.
    let rejection = expect_err(&window, "unlock", json!({ "pin": LOCKED_STORE_PIN }));
    assert_eq!(rejection.get("kind"), Some(&json!("backoff")));
    let wait = match rejection.get("retry_after_ms").and_then(Value::as_u64) {
        Some(wait) => wait,
        None => panic!("backoff must carry a wait: {rejection}"),
    };
    assert!(wait > 0 && wait <= 30_000, "the wait is a snapshot: {wait}");

    // **Nothing is ever wiped.** The database is byte-for-byte what it was and
    // the key material still unwraps.
    let after = match std::fs::read(paths.db()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the store should still be readable: {e}"),
    };
    assert_eq!(before, after, "failed attempts must not touch the store");
    assert!(paths.keyfile().exists(), "the only key must still be there");

    // And the count is what `get_lock_state` reports.
    let state = expect_ok(&window, "get_lock_state", json!({}));
    assert_eq!(state.get("attempts_remaining"), Some(&json!(0)));
    assert_eq!(state.get("locked"), Some(&json!(true)));
}

/// **The counter is persisted, or five attempts becomes unlimited** (ADR-0011).
/// A relaunch is a second application over the same directory.
#[test]
fn the_attempt_counter_survives_a_relaunch() {
    let (_parent, paths) = locked_store();

    {
        let app = build(paths.clone());
        let window = window(&app);
        for remaining in [4u32, 3] {
            assert_eq!(
                expect_err(&window, "unlock", json!({ "pin": "000000" })).get("attempts_remaining"),
                Some(&json!(remaining))
            );
        }
    }

    // The launch after the one that failed.
    let app = build(paths);
    let window = window(&app);
    assert_eq!(
        expect_ok(&window, "get_lock_state", json!({})),
        json!({
            "encryption_enabled": true,
            "locked": true,
            "attempts_remaining": 3,
            "retry_after_ms": null,
        }),
        "restarting must not clear the count"
    );
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "000000" })).get("attempts_remaining"),
        Some(&json!(2)),
        "the count continues from where it was"
    );
}

/// A successful unlock clears the counter, so a user who mistypes twice and then
/// gets it right starts fresh.
#[test]
fn a_successful_unlock_clears_the_attempt_counter() {
    let (_parent, paths) = locked_store();
    let app = build(paths);
    let window = window(&app);

    expect_err(&window, "unlock", json!({ "pin": "000000" }));
    expect_err(&window, "unlock", json!({ "pin": "000001" }));
    expect_ok(&window, "unlock", json!({ "pin": LOCKED_STORE_PIN }));
    expect_ok(&window, "lock", json!({}));

    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "000000" })).get("attempts_remaining"),
        Some(&json!(4)),
        "a successful unlock resets the count to five"
    );
}

/// `change_pin` re-wraps the key: the new PIN works, the old one stops, and
/// every clip is still readable because the database was never re-encrypted.
#[test]
fn changing_the_pin_invalidates_the_old_one_and_keeps_every_clip() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths);
    let window = window(&app);

    for label in ["first", "second"] {
        expect_ok(&window, "create_clip", draft(label, "a value", "slate"));
    }
    let before = expect_ok(&window, "list_clips", json!({}));
    expect_ok(&window, "enable_encryption", json!({ "pin": "135790" }));

    // A wrong current PIN changes nothing, and is not subject to backoff.
    assert_eq!(
        expect_err(
            &window,
            "change_pin",
            json!({ "current_pin": "111111", "new_pin": "246801" })
        ),
        json!({ "kind": "bad_pin", "attempts_remaining": null, "retry_after_ms": null })
    );

    assert_eq!(
        expect_ok(
            &window,
            "change_pin",
            json!({ "current_pin": "135790", "new_pin": "246801" })
        ),
        Value::Null
    );

    assert_eq!(
        expect_ok(&window, "list_clips", json!({})),
        before,
        "the database is not re-encrypted, so nothing moves"
    );

    expect_ok(&window, "lock", json!({}));
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "135790" })).get("kind"),
        Some(&json!("bad_pin")),
        "the old PIN must stop working the moment change_pin returns"
    );
    expect_ok(&window, "unlock", json!({ "pin": "246801" }));
    assert_eq!(expect_ok(&window, "list_clips", json!({})), before);
}

/// `change_pin` emits nothing and rebuilds nothing: no field of `LockState`
/// changes, and the tray menu shows the same clips it did before.
#[test]
fn changing_the_pin_changes_no_lock_state_and_rebuilds_nothing() {
    install_recorder();

    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);
    expect_ok(&window, "create_clip", draft("first", "a value", "slate"));
    expect_ok(&window, "enable_encryption", json!({ "pin": "135790" }));

    let states = LockStates::listening(&app);
    let before = rebuilds();

    expect_ok(
        &window,
        "change_pin",
        json!({ "current_pin": "135790", "new_pin": "246801" }),
    );

    assert_eq!(states.all().len(), 0, "change_pin emits nothing");
    assert_eq!(rebuilds() - before, 0, "and rebuilds nothing");
}

/// Contract §2: the three `wrong_state` cases, each naming the state it needs
/// rather than the one it found.
#[test]
fn the_session_commands_refuse_the_states_they_cannot_serve() {
    let parent = dir();
    let app = build(StorePaths::at(parent.path().join(".fast-clip")));
    let window = window(&app);

    // Encryption off.
    let needs_encryption = json!({ "kind": "wrong_state", "required": "encrypted" });
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "135790" })),
        needs_encryption
    );
    assert_eq!(expect_err(&window, "lock", json!({})), needs_encryption);
    assert_eq!(
        expect_err(
            &window,
            "change_pin",
            json!({ "current_pin": "135790", "new_pin": "246801" })
        ),
        needs_encryption
    );

    // Encryption on and already unlocked.
    expect_ok(&window, "enable_encryption", json!({ "pin": "135790" }));
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "135790" })),
        json!({ "kind": "wrong_state", "required": "locked" }),
        "unlocking an unlocked store is not a silent success: that would accept a wrong PIN"
    );

    // Locked, so the two that need the store open refuse with `locked`.
    expect_ok(&window, "lock", json!({}));
    assert_eq!(
        expect_err(
            &window,
            "change_pin",
            json!({ "current_pin": "135790", "new_pin": "246801" })
        ),
        json!({ "kind": "locked" })
    );
    assert_eq!(
        expect_err(&window, "disable_encryption", json!({ "pin": "135790" })),
        json!({ "kind": "locked" })
    );
}

/// **An encrypted store whose key material is gone.** This is a different fault
/// from a locked store and gets a different message: `crypto`, which the copy
/// deck renders as "another Windows account", not "type your PIN".
#[test]
fn an_encrypted_store_with_no_key_material_reports_crypto_not_a_pin_prompt() {
    let (_parent, paths) = encrypted_store_with_no_key_material();
    let app = build(paths);
    let window = window(&app);

    let bad_key_material = json!({ "kind": "crypto", "reason": "bad_key_material" });
    assert_eq!(
        expect_err(&window, "get_lock_state", json!({})),
        bad_key_material,
        "the startup sequence stops at step 3 rather than offering a PIN prompt"
    );
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": "135790" })),
        bad_key_material,
        "and no PIN can help"
    );
}

/// Contract §2: every reader of `keyfile` declares
/// `unsupported_version { key_material }`, because the version byte sits outside
/// the DPAPI blob precisely so it is parsed before anything is decrypted.
#[test]
fn key_material_from_a_newer_build_is_an_unsupported_version_everywhere() {
    let (_parent, paths) = locked_store();

    let mut bytes = match std::fs::read(paths.keyfile()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the fixture keyfile should be readable: {e}"),
    };
    bytes[0] = 99;
    if let Err(e) = std::fs::write(paths.keyfile(), &bytes) {
        panic!("the fixture should write: {e}");
    }

    let app = build(paths);
    let window = window(&app);
    let too_new = json!({
        "kind": "unsupported_version",
        "component": "key_material",
        "found": 99,
        "supported": 1,
    });

    assert_eq!(expect_err(&window, "get_lock_state", json!({})), too_new);
    assert_eq!(
        expect_err(&window, "unlock", json!({ "pin": LOCKED_STORE_PIN })),
        too_new
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

// ---- G3 review, F1: the guard proved at the seam rather than in isolation ----

/// **F1 from the G3 review of WP-07.** `store.begin_conversion()`'s only
/// justification (`storage/store.rs`, the doc comment on `begin_conversion`) is
/// a race between two overlapping conversions, and the only existing test of
/// it — `only_one_conversion_may_hold_the_guard_at_a_time` in
/// `storage/store.rs` — calls `begin_conversion` directly from spawned
/// threads. That proves `std::sync::Mutex` excludes, which is a property of
/// the standard library, and it would keep passing if the guard were deleted
/// from both `enable_encryption` and `disable_encryption` in
/// `commands/encryption.rs`. This test drives the guard through the real
/// command instead.
///
/// `WebviewWindow<MockRuntime>` (aliased here as [`MockWindow`]) is
/// `Send + Sync` — this file's own `build`/`window` helpers construct it with
/// no `unsafe`, and the compiler accepts a shared `&MockWindow` reused across
/// the two scoped threads below. That was the part the critic flagged as an
/// unverified prediction; it holds.
///
/// Two threads invoke `enable_encryption` with different PINs against the
/// **same store**, started together on a `Barrier` so neither gets a head
/// start. Both spend the first few hundred milliseconds of the call inside
/// `write_and_prove_key_material`'s Argon2id derivation — the real, shipped
/// KDF tuple, because this goes through the actual command rather than a test
/// fixture that could cheapen the cost — which is a wide window for the two
/// bodies to genuinely overlap. Nothing in this file sleeps to create that
/// window; it is the production cost of the derivation, already paid by the
/// command under test.
///
/// The postcondition is the one the guard exists to prevent, named in
/// `store.rs`'s own doc comment: **never an encrypted `clips.db` sitting
/// beside an absent `keyfile`.** That is the state B's clobber-then-abort
/// sequence produces once A has committed under a key `keyfile` no longer
/// holds.
#[test]
fn two_concurrent_enables_never_leave_an_encrypted_store_with_no_key() {
    let parent = dir();
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    let app = build(paths.clone());
    let main_window = window(&app);

    let start = std::sync::Barrier::new(2);
    let outcomes: Vec<Result<Value, Value>> = std::thread::scope(|scope| {
        let handles: Vec<_> = ["111111", "222222"]
            .into_iter()
            .map(|pin| {
                let window = &main_window;
                let start = &start;
                scope.spawn(move || {
                    start.wait();
                    invoke(window, "enable_encryption", json!({ "pin": pin }))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| match handle.join() {
                Ok(outcome) => outcome,
                Err(e) => std::panic::resume_unwind(e),
            })
            .collect()
    });

    let bytes = match std::fs::read(paths.db()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the store file should be readable after the race: {e}"),
    };
    let looks_encrypted = !bytes.starts_with(b"SQLite format 3\0");

    if looks_encrypted {
        assert!(
            paths.keyfile().exists(),
            "the database is encrypted and keyfile is gone — no PIN can ever open it \
             again. Outcomes were {outcomes:?}"
        );

        // The weaker check above (a file exists at `keyfile`) is not the whole
        // postcondition: `keyfile` could hold the *wrong* wrapped key — the
        // other thread's, left behind by a redundant second commit — which is
        // the same "no PIN recovers it" failure with a file sitting there to
        // hide it. The real postcondition is recoverability: at least one of
        // the two PINs this race used must still open the store, each tried
        // against a fresh application over the same directory so no in-memory
        // state from either racing call leaks into the check.
        let mut recovered = false;
        for pin in ["111111", "222222"] {
            let checking_app = build(paths.clone());
            let checking_window = window(&checking_app);
            if invoke(&checking_window, "unlock", json!({ "pin": pin })).is_ok() {
                recovered = true;
                break;
            }
        }
        assert!(
            recovered,
            "the store is encrypted and neither PIN used to enable it unlocks it any \
             more. Outcomes were {outcomes:?}"
        );
    }
}
