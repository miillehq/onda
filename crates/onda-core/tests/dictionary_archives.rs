use onda_core::persistence::Store;
use serde_json::{json, Value};

fn vocabulary(id: usize, phrase: String) -> Value {
    json!({ "id": id.to_string(), "kind": "vocabulary", "phrase": phrase, "replacement": null,
        "enabled": true, "starred": false, "source": "manual" })
}

#[test]
fn a_large_valid_dictionary_can_restore_its_own_backup() {
    const COUNT: usize = 12_000;
    let entries: Vec<_> = (0..COUNT)
        .map(|id| vocabulary(id, format!("Term {id:05} {}", "x".repeat(139))))
        .collect();
    let source = json!({ "version": 1, "entries": entries }).to_string();
    let mut original = Store::in_memory().unwrap();
    assert_eq!(original.import_dictionary(&source).unwrap(), COUNT);
    let backup = original.export_dictionary().unwrap();
    let mut restored = Store::in_memory().unwrap();
    assert_eq!(
        restored
            .import_dictionary(&backup)
            .expect("restore Onda's own export"),
        COUNT
    );
    assert_eq!(restored.import_dictionary(&backup).unwrap(), 0);
}

#[test]
fn wide_or_incomplete_entries_identify_the_record_and_leave_existing_words_alone() {
    let valid = vocabulary(1, "Retained".into());
    let mut store = Store::in_memory().unwrap();
    store
        .import_dictionary(&json!({ "version": 1, "entries": [&valid] }).to_string())
        .unwrap();
    let mut wide = vocabulary(2, "Invalid".into());
    wide["extra"] = json!("unknown field");
    let mut narrow = wide.clone();
    let fields = narrow.as_object_mut().unwrap();
    fields.remove("extra");
    fields.remove("enabled");
    let positional = json!(["2", "vocabulary", "Invalid", null, true, false, "manual"]);
    let mut mapped_kind = valid.clone();
    mapped_kind["kind"] = json!({ "vocabulary": null });
    let mut mapped_source = valid.clone();
    mapped_source["source"] = json!({ "manual": null });
    for invalid in [wide, narrow, positional, mapped_kind, mapped_source] {
        let source = format!(
            "{{\n\"version\":1,\n\"entries\":[\n{},\n{}\n]}}",
            vocabulary(3, "New".into()),
            invalid
        );
        let error = store
            .import_dictionary(&source)
            .expect_err("accepted malformed archive row");
        assert!(
            error.message.contains("entry 2") && error.message.contains("line 5"),
            "{}",
            error.message
        );
        assert_eq!(store.dictionary().unwrap().len(), 1);
    }
}

#[test]
fn oversized_record_sets_report_how_many_entries_were_refused() {
    let source = format!(
        r#"{{"version":1,"entries":[{}{{}}]}}"#,
        "{},".repeat(99_999)
    );
    let mut store = Store::in_memory().unwrap();
    let error = store.import_dictionary(&source).unwrap_err();
    assert!(error.message.contains("100000"), "{}", error.message);
    assert!(store.dictionary().unwrap().is_empty());
}
