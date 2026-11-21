use schemars::generate::SchemaSettings;
use serde_json::Value;
use std::{error::Error, fs, path::Path};

/// Generated renderer types carry every property, including properties whose value may be null.
/// Stored settings acquire platform defaults separately, so those defaults do not belong here.
fn close_wire_objects(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    object.remove("default");
    if object.get("format").and_then(Value::as_str) == Some("uint32") {
        object.insert("maximum".into(), Value::from(u32::MAX));
    }
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        let required = properties.keys().cloned().map(Value::String).collect();
        properties.values_mut().for_each(close_wire_objects);
        object.insert("required".into(), Value::Array(required));
        object.insert("additionalProperties".into(), Value::Bool(false));
    }
    if let Some(definitions) = object.get_mut("definitions").and_then(Value::as_object_mut) {
        definitions.values_mut().for_each(close_wire_objects);
    }
    for group in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = object.get_mut(group).and_then(Value::as_array_mut) {
            variants.iter_mut().for_each(close_wire_objects);
        }
    }
    if let Some(items) = object.get_mut("items") {
        close_wire_objects(items);
    }
}

pub(super) fn export(check: bool) -> Result<(), Box<dyn Error>> {
    let schema = SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<onda_app::api::ApiRequest>();
    let mut schema = serde_json::to_value(schema)?;
    close_wire_objects(&mut schema);
    let content = serde_json::to_string_pretty(&schema)? + "\n";
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/desktop-request.schema.json");
    if check {
        if fs::read_to_string(path)?.replace("\r\n", "\n") != content {
            return Err("Desktop request schema drifted; run npm run contracts.".into());
        }
    } else {
        fs::write(path, content)?;
    }
    Ok(())
}
