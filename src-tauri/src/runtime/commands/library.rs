use super::*;
use ApiResponse as O;

pub(super) async fn save_dictionary_entry(
    service: &Arc<Runtime>,
    entry: DictionaryEntry,
) -> AppResult<ApiResponse> {
    let _dictionary = service.data.dictionary_gate.lock().await;
    lock(&service.data.store)?.save_dictionary_entry(&entry)?;
    service.data.rebuild_dictionary().await?;
    Ok(O::Dictionary(lock(&service.data.store)?.dictionary()?))
}

pub(super) async fn delete_dictionary_entry(
    service: &Arc<Runtime>,
    id: String,
) -> AppResult<ApiResponse> {
    let _dictionary = service.data.dictionary_gate.lock().await;
    lock(&service.data.store)?.delete_dictionary_entry(&id)?;
    service.data.rebuild_dictionary().await?;
    Ok(O::Unit)
}

pub(super) async fn import_dictionary(
    service: &Arc<Runtime>,
    json: String,
) -> AppResult<ApiResponse> {
    let _dictionary = service.data.dictionary_gate.lock().await;
    let added = lock(&service.data.store)?.import_dictionary(&json)?;
    service.data.rebuild_dictionary().await?;
    Ok(O::Count(added))
}

pub(super) async fn save_snippet(
    service: &Arc<Runtime>,
    snippet: Snippet,
) -> AppResult<ApiResponse> {
    let _dictionary = service.data.dictionary_gate.lock().await;
    lock(&service.data.store)?.save_snippet(&snippet)?;
    service.data.rebuild_dictionary().await?;
    Ok(O::Snippets(lock(&service.data.store)?.snippets()?))
}

pub(super) async fn delete_snippet(service: &Arc<Runtime>, id: String) -> AppResult<ApiResponse> {
    let _dictionary = service.data.dictionary_gate.lock().await;
    lock(&service.data.store)?.delete_snippet(&id)?;
    service.data.rebuild_dictionary().await?;
    Ok(O::Unit)
}

pub(super) async fn save_transform(
    service: &Arc<Runtime>,
    transform: Transform,
) -> AppResult<ApiResponse> {
    let _configuration = service.data.configuration_gate.lock().await;
    let settings = lock(&service.data.store)?.settings()?;
    let mut transforms = lock(&service.data.store)?.transforms()?;
    transforms.retain(|entry| entry.id != transform.id);
    transforms.push(transform.clone());
    let config = shortcut_config(&settings, &transforms)?;
    lock(&service.data.store)?.save_transform(&transform)?;
    *lock(&service.shortcuts.helper_config)? = config;
    service
        .shortcuts
        .helper_generation
        .fetch_add(1, Ordering::AcqRel);
    Ok(O::Transforms(transforms))
}

pub(super) async fn delete_transform(service: &Arc<Runtime>, id: String) -> AppResult<ApiResponse> {
    let _configuration = service.data.configuration_gate.lock().await;
    lock(&service.data.store)?.delete_transform(&id)?;
    let settings = lock(&service.data.store)?.settings()?;
    let config = shortcut_config(&settings, &lock(&service.data.store)?.transforms()?)?;
    *lock(&service.shortcuts.helper_config)? = config;
    service
        .shortcuts
        .helper_generation
        .fetch_add(1, Ordering::AcqRel);
    Ok(O::Unit)
}

pub(super) async fn accept_learning(
    service: &Arc<Runtime>,
    proposal_id: String,
) -> AppResult<ApiResponse> {
    let _dictionary = service.data.dictionary_gate.lock().await;
    let proposal = lock(&service.results.proposal)?
        .clone()
        .filter(|proposal| proposal.id == proposal_id)
        .ok_or_else(|| AppError::invalid("Learning proposal expired."))?;
    let word = DictionaryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        kind: DictionaryKind::Replacement,
        phrase: proposal.original,
        replacement: Some(proposal.corrected),
        enabled: true,
        starred: false,
        source: EntrySource::AutoLearned,
    };
    lock(&service.data.store)?.save_dictionary_entry(&word)?;
    service.data.rebuild_dictionary().await?;
    *lock(&service.results.proposal)? = None;
    Ok(O::Unit)
}

impl LocalData {
    pub(in crate::runtime) async fn rebuild_dictionary(&self) -> AppResult<()> {
        let (entries, snippets) = {
            let store = lock(&self.store)?;
            (store.dictionary()?, store.snippets()?)
        };
        let index =
            tokio::task::spawn_blocking(move || DictionaryIndex::build(&entries, &snippets))
                .await
                .map_err(|_| {
                    AppError::new(
                        ErrorCode::Internal,
                        "Dictionary index worker stopped.",
                        true,
                    )
                })??;
        *lock(&self.index)? = index;
        Ok(())
    }
}
