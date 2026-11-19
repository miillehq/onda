use onda_app::api::ApiRequest;
use onda_core::contracts::Settings;
use serde_json::{json, Value};

fn parse_update(settings: Value) -> Result<ApiRequest, serde_json::Error> {
    serde_json::from_value(json!({ "command": "updateSettings", "args": { "settings": settings } }))
}

#[test]
fn a_settings_replacement_requires_every_field_including_nullable_ones() {
    let old_profile: Settings = serde_json::from_value(json!({ "historyDays": 7 })).unwrap();
    assert_eq!(old_profile.history_days, 7);
    assert_eq!(old_profile.push_to_talk, Settings::default().push_to_talk);
    let settings = serde_json::to_value(Settings::default()).unwrap();
    assert!(parse_update(settings.clone()).is_ok());
    for field in settings.as_object().unwrap().keys() {
        let mut incomplete = settings.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(
            parse_update(incomplete).is_err(),
            "accepted missing {field}"
        );
    }
}

#[test]
fn a_settings_replacement_rejects_unknown_fields_and_wrong_shapes() {
    let mut settings = serde_json::to_value(Settings::default()).unwrap();
    settings["pushToTalk"]["key"] = json!(42);
    for invalid in [
        settings,
        Value::Null,
        json!(2),
        json!([]),
        json!({}),
        json!("2026-01-01"),
    ] {
        assert!(
            parse_update(invalid).is_err(),
            "accepted malformed settings"
        );
    }
}
