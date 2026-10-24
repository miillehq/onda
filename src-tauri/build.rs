fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(&["application_call"]));
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("Tauri build failed: {error}");
    }
    println!("cargo:rerun-if-env-changed=EAGLE_UPDATE_ENDPOINT");
    println!("cargo:rerun-if-env-changed=EAGLE_UPDATE_PUBLIC_KEY");
}
