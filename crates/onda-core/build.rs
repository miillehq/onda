//! One desktop configuration owns the provisional product identity.
fn main() {
    let configuration =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/tauri.conf.json");
    println!("cargo:rerun-if-changed={}", configuration.display());
    let source =
        std::fs::read_to_string(configuration).expect("Desktop branding configuration must exist");
    let value: serde_json::Value =
        serde_json::from_str(&source).expect("Desktop configuration must be valid JSON");
    for (property, variable) in [
        ("productName", "ONDA_APP_NAME"),
        ("identifier", "ONDA_APP_IDENTIFIER"),
    ] {
        let text = value[property]
            .as_str()
            .expect("Branding property must be a string");
        assert!(
            !text.contains(['\r', '\n']),
            "Branding must be a single line"
        );
        println!("cargo:rustc-env={variable}={text}");
    }
    assert_eq!(
        value["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "Desktop and Cargo versions must agree"
    );
}
