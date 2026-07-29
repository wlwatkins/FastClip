fn main() {
    embed_the_common_controls_manifest_into_test_binaries();
    tauri_build::build()
}

/// Give test binaries the side-by-side manifest the application binary gets
/// from `tauri_build`.
///
/// Without it, a test binary that links Tauri's application machinery dies at
/// load with `STATUS_ENTRYPOINT_NOT_FOUND` — the reasoning is in
/// `tests.manifest`. The flags are scoped to test targets, so nothing about the
/// shipped binary changes; `tauri_build` still writes its own manifest for that.
fn embed_the_common_controls_manifest_into_test_binaries() {
    if !cfg!(windows) {
        return;
    }

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests.manifest");
    println!("cargo:rerun-if-changed=tests.manifest");
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
