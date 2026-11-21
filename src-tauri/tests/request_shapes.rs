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

fn contract_branches() -> Vec<Value> {
    let contract: Value =
        serde_json::from_str(include_str!("../../contracts/desktop-request.schema.json")).unwrap();
    contract["oneOf"].as_array().unwrap().clone()
}

/// The argument properties of one command, empty for a command without arguments.
fn argument_schemas(branch: &Value) -> Vec<(String, Value)> {
    branch["properties"]["args"]["properties"]
        .as_object()
        .map(|properties| {
            properties
                .iter()
                .map(|(name, schema)| (name.clone(), schema.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_command_name_must_be_a_plain_string() {
    // serde also reads a unit variant from a one-key map. The schema and the mock refuse it.
    let mut checked = 0;
    for branch in contract_branches() {
        if !argument_schemas(&branch).is_empty() {
            continue;
        }
        let name = branch["properties"]["command"]["const"].as_str().unwrap();
        assert!(ApiRequest::decode(json!({ "command": name })).is_ok());
        for command in [json!({ name: null }), json!({ name: {} }), json!([name])] {
            assert!(
                ApiRequest::decode(json!({ "command": command })).is_err(),
                "accepted the command {command}"
            );
        }
        checked += 1;
    }
    assert!(checked > 20, "only {checked} commands without arguments");
}

#[test]
fn a_provider_must_be_a_plain_string() {
    let with_provider = |command: &str, provider: Value| match command {
        "saveProviderKey" => {
            json!({"command": command, "args": {"provider": provider, "key": "test-key"}})
        }
        _ => json!({"command": command, "args": {"provider": provider}}),
    };
    let listed = ["listProviderModels", "removeProviderKey", "saveProviderKey"];
    // A new argument that names one of several choices needs its own case here.
    let mut referencing: Vec<String> = contract_branches()
        .iter()
        .filter(|branch| {
            argument_schemas(branch)
                .iter()
                .any(|(_, schema)| schema.get("$ref").is_some())
        })
        .map(|branch| {
            branch["properties"]["command"]["const"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .filter(|command| !COMPLETE_OBJECT_COMMANDS.contains(&command.as_str()))
        .collect();
    referencing.sort();
    assert_eq!(referencing, listed);
    for command in listed {
        assert!(ApiRequest::decode(with_provider(command, json!("anthropic"))).is_ok());
        for provider in [
            json!({"anthropic": null}),
            json!({"anthropic": {}}),
            json!(["anthropic"]),
        ] {
            assert!(
                ApiRequest::decode(with_provider(command, provider.clone())).is_err(),
                "{command} accepted the provider {provider}"
            );
        }
    }
}

/// These carry a whole record, which `complete_object` already compares with what was received.
const COMPLETE_OBJECT_COMMANDS: [&str; 4] = [
    "saveDictionaryEntry",
    "saveSnippet",
    "saveTransform",
    "updateSettings",
];

#[test]
fn every_nullable_argument_must_be_present() {
    let mut checked = Vec::new();
    for branch in contract_branches() {
        let name = branch["properties"]["command"]["const"].as_str().unwrap();
        let arguments = argument_schemas(&branch);
        for (argument, schema) in &arguments {
            let nullable = schema["type"]
                .as_array()
                .is_some_and(|types| types.contains(&json!("null")));
            if !nullable {
                continue;
            }
            // Leaving out one of several arguments needs valid values for the others: write that
            // case by hand when a command first combines a nullable argument with another.
            assert_eq!(arguments.len(), 1, "{name}.{argument} needs its own case");
            assert!(
                ApiRequest::decode(json!({"command": name, "args": {argument: null}})).is_ok(),
                "{name} refused an explicit null {argument}"
            );
            assert!(
                ApiRequest::decode(json!({"command": name, "args": {}})).is_err(),
                "{name} accepted a missing {argument}"
            );
            checked.push(format!("{name}.{argument}"));
        }
    }
    checked.sort();
    assert_eq!(
        checked,
        [
            "openMainWindow.section",
            "setProfileName.name",
            "startChatgptSignIn.profileId"
        ]
    );
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
