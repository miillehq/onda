//! Optional-cloud storage failures must be redacted, fail closed, and preserve data.

use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

const PROFILE_DIRECTORY: &str = "accounts";
const HOST_FILENAME: &str = "host.json";
const SENTINEL_SECRET: &str = "test-only-secret-that-must-not-be-displayed";

fn service_at(directory: &Path) -> ChatGptService {
    ChatGptService::new(CloudConfig {
        data_directory: directory.to_owned(),
        app_name: "Eagle".into(),
    })
    .unwrap()
}

fn record_path(directory: &Path, id: &str) -> PathBuf {
    directory.join(PROFILE_DIRECTORY).join(format!("{id}.bin"))
}

fn test_record() -> ProfileRecord {
    ProfileRecord {
        id: Uuid::new_v4().to_string(),
        client_id: "oaiapp_fixture".into(),
        issuer: ISSUER.into(),
        subject: Some("fixture-subject".into()),
        email: Some("fixture@example.invalid".into()),
        name: Some("Fixture account".into()),
        preferred_model: None,
        refresh_pending: false,
        tokens: Some(SecretTokens {
            access_token: SENTINEL_SECRET.into(),
            refresh_token: format!("{SENTINEL_SECRET}-refresh"),
            id_token: format!("{SENTINEL_SECRET}-identity"),
            scopes: vec!["chatgpt.tokens.use.direct".into()],
            expires_at: unix_time().unwrap() + 3600,
            earliest_refresh_at: None,
        }),
    }
}

fn assert_redacted(error: &CloudError, directory: &Path) {
    let diagnostics = format!(
        "{} {:?} {}",
        error,
        error,
        serde_json::to_string(error).unwrap()
    );
    assert!(!diagnostics.contains(SENTINEL_SECRET));
    assert!(!diagnostics.contains(directory.to_string_lossy().as_ref()));
    assert!(matches!(
        error,
        CloudError::Storage | CloudError::UnknownProfile
    ));
}

#[test]
fn unavailable_account_directory_is_a_redacted_constructor_error_and_never_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let collision = directory.path().join(PROFILE_DIRECTORY);
    fs::write(&collision, SENTINEL_SECRET).unwrap();
    let error = ChatGptService::new(CloudConfig {
        data_directory: directory.path().to_owned(),
        app_name: "Eagle".into(),
    })
    .err()
    .unwrap();
    assert_redacted(&error, directory.path());
    assert_eq!(fs::read(&collision).unwrap(), SENTINEL_SECRET.as_bytes());
    assert!(!directory.path().join(HOST_FILENAME).exists());
}

#[tokio::test]
async fn corrupted_host_identity_is_preserved_with_every_protected_profile() {
    let directory = tempfile::tempdir().unwrap();
    let service = service_at(directory.path());
    assert!(service.list_profiles().await.unwrap().is_empty());
    let record = test_record();
    service.inner.store.save(&record).unwrap();
    let profile_path = record_path(directory.path(), &record.id);
    let ciphertext_before = fs::read(&profile_path).unwrap();
    let host_path = directory.path().join(HOST_FILENAME);
    let corrupt_host = format!(
        "{{\"version\":1,\"ext_agent_host_id\":\"{SENTINEL_SECRET}\",\"active_profile\":null}}"
    );
    fs::write(&host_path, &corrupt_host).unwrap();
    let error = service.list_profiles().await.err().unwrap();
    assert_redacted(&error, directory.path());
    assert_eq!(fs::read(&host_path).unwrap(), corrupt_host.as_bytes());
    assert_eq!(fs::read(&profile_path).unwrap(), ciphertext_before);
    assert!(service
        .inner
        .store
        .load(&record.id)
        .unwrap()
        .tokens
        .is_some());
    // Construction remains lazy: snapshot/read failures must be handled by the
    // application's optional-cloud boundary rather than rewriting host identity.
    assert!(ChatGptService::new(CloudConfig {
        data_directory: directory.path().to_owned(),
        app_name: "Eagle".into()
    })
    .is_ok());
    assert_eq!(fs::read(&host_path).unwrap(), corrupt_host.as_bytes());
}

#[tokio::test]
async fn unreadable_profile_fails_closed_without_selecting_or_modifying_another_account() {
    let directory = tempfile::tempdir().unwrap();
    let service = service_at(directory.path());
    let healthy = test_record();
    let broken = test_record();
    service.inner.store.save(&healthy).unwrap();
    service.inner.store.save(&broken).unwrap();
    {
        let _host_lock = service.inner.store.lock_host().await.unwrap();
        service.inner.store.select(Some(broken.id.clone())).unwrap();
    }
    let healthy_path = record_path(directory.path(), &healthy.id);
    let broken_path = record_path(directory.path(), &broken.id);
    fs::write(&broken_path, format!("EAGLE-DPAPI-1\0{SENTINEL_SECRET}")).unwrap();
    let host_before = fs::read(directory.path().join(HOST_FILENAME)).unwrap();
    let healthy_before = fs::read(&healthy_path).unwrap();
    let broken_before = fs::read(&broken_path).unwrap();
    let list_error = service.list_profiles().await.err().unwrap();
    assert_redacted(&list_error, directory.path());
    let select_error = service.select_profile(&broken.id).await.err().unwrap();
    assert_redacted(&select_error, directory.path());
    let signout_error = service.sign_out(&broken.id).await.err().unwrap();
    assert_redacted(&signout_error, directory.path());
    assert_eq!(
        service.inner.store.active_profile().unwrap().as_deref(),
        Some(broken.id.as_str())
    );
    assert_eq!(
        fs::read(directory.path().join(HOST_FILENAME)).unwrap(),
        host_before
    );
    assert_eq!(fs::read(&healthy_path).unwrap(), healthy_before);
    assert_eq!(fs::read(&broken_path).unwrap(), broken_before);
    let public = service.inner.store.load(&healthy.id).unwrap().public(false);
    assert!(!serde_json::to_string(&public)
        .unwrap()
        .contains(SENTINEL_SECRET));
}

#[tokio::test]
async fn malformed_registration_metadata_cannot_be_exposed_or_implicitly_repaired() {
    let directory = tempfile::tempdir().unwrap();
    let service = service_at(directory.path());
    service.list_profiles().await.unwrap();
    let mut record = test_record();
    record.client_id = DYNAMIC_CLIENT.into();
    service.inner.store.save(&record).unwrap();
    let path = record_path(directory.path(), &record.id);
    let encrypted_before = fs::read(&path).unwrap();
    let error = service.list_profiles().await.err().unwrap();
    assert_redacted(&error, directory.path());
    assert_eq!(fs::read(path).unwrap(), encrypted_before);
}
