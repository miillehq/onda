use onda_app::api::ApiRequest;
use serde_json::{json, Value};

#[test]
fn every_registered_command_rejects_wrong_arguments_before_dispatch() {
    let contract: Value =
        serde_json::from_str(include_str!("../../contracts/desktop-request.schema.json")).unwrap();
    for branch in contract["oneOf"].as_array().unwrap() {
        let name = branch["properties"]["command"]["const"].as_str().unwrap();
        for args in [
            Value::Null,
            json!(17),
            json!([]),
            json!({"unexpected": true}),
            json!("2026-11-19T00:00:00Z"),
        ] {
            assert!(
                ApiRequest::decode(json!({"command": name, "args": args})).is_err(),
                "{name} accepted {args}"
            );
        }
    }
}

#[test]
fn clearing_a_profile_requires_an_explicit_null_and_a_json_object() {
    assert!(ApiRequest::decode(json!({"command":"setProfileName","args":{"name":null}})).is_ok());
    for args in [
        json!({}),
        json!([null]),
        Value::Null,
        json!(4),
        json!("2026-11-19"),
    ] {
        assert!(
            ApiRequest::decode(json!({"command":"setProfileName","args":args})).is_err(),
            "accepted {args}"
        );
    }
}
