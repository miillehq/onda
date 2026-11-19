use onda_core::{contracts::Settings, persistence::Store, transforms::builtin_transforms};
use rusqlite::Connection;

#[test]
fn removing_the_default_repairs_the_shortcut_and_preserves_other_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("onda.db");
    let mut store = Store::open(&path).unwrap();
    let mut custom = builtin_transforms().remove(0);
    custom.id = "personal".into();
    custom.built_in = false;
    store.save_transform(&custom).unwrap();
    let settings = Settings {
        default_transform_id: custom.id.clone(),
        history_days: 7,
        ..Settings::default()
    };
    store.save_settings(&settings).unwrap();
    store.delete_transform(&custom.id).unwrap();
    drop(store);
    let mut reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.settings().unwrap().default_transform_id, "polish");
    assert_eq!(reopened.settings().unwrap().history_days, 7);
    assert!(
        reopened.save_settings(&settings).is_err(),
        "a stale save restored the removed default"
    );
    assert!(reopened.delete_transform("polish").is_err());
}

#[test]
fn an_older_dangling_default_recovers_without_overwriting_customized_builtins() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("onda.db");
    let mut store = Store::open(&path).unwrap();
    let mut grammar = store
        .transforms()
        .unwrap()
        .into_iter()
        .find(|item| item.id == "grammar")
        .unwrap();
    grammar.instruction = "My grammar preference".into();
    store.save_transform(&grammar).unwrap();
    store.save_settings(&Settings::default()).unwrap();
    drop(store);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("DELETE FROM transforms WHERE id='polish'", [])
        .unwrap();
    connection.execute("UPDATE settings SET value=json_set(value,'$.defaultTransformId','missing') WHERE key='application'", []).unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.settings().unwrap().default_transform_id, "polish");
    let transforms = store.transforms().unwrap();
    assert!(transforms.iter().any(|item| item.id == "polish"));
    assert!(transforms.contains(&grammar));
}
