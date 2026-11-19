use super::*;

impl Runtime {
    /// Initializes native services; isolated debug tests omit system-wide keyboard hooks.
    pub fn new(
        app: AppHandle,
        data_directory: PathBuf,
        worker_directory: PathBuf,
        register_global_shortcuts: bool,
    ) -> AppResult<Arc<Self>> {
        let mut store = Store::open(&data_directory.join("onda.db"))?;
        let settings = store.settings()?;
        let history = History::load(&mut store, settings.history_days, now_ms(), |bytes| {
            onda_platform::unprotect_user_data(bytes).ok()
        })?;
        let index = DictionaryIndex::build(&store.dictionary()?, &store.snippets()?)?;
        let helper_config = shortcut_config(&settings, &store.transforms()?)?;
        let accounts = ConnectedAccounts::new(&data_directory);
        let model_library = ModelLibrary::open(&settings, &data_directory)?;
        let (operations, receiver) = OperationActor::new();
        let service = Arc::new(Self {
            app,
            targets: TargetDelivery::default(),
            data: LocalData::new(store, index),
            operations,
            microphone: MicrophoneSession::new(),
            results: OperationResults::default(),
            retention: HistoryService {
                history: Mutex::new(history),
            },
            recognition: SpeechRecognition::new(worker_directory),
            rewrites: RewriteEngine {
                accounts,
                provider_access: ProviderAccess::new(&data_directory),
            },
            model_library,
            performance: Performance::default(),
            shortcuts: ShortcutSupervisor::new(helper_config),
            updates: Updates::default(),
            pill: crate::pill::Pill::new(settings.show_pill, settings.play_sounds),
        });
        let actor = service.clone();
        Self::start_history_expiry(&service);
        tauri::async_runtime::spawn(async move {
            actor.operations.action_loop(&actor, receiver).await;
        });
        if register_global_shortcuts {
            service.start_helper()?;
        } else {
            *lock(&service.shortcuts.shortcut_status)? = "disabled for isolated test".into();
        }
        #[cfg(target_os = "linux")]
        {
            onda_platform::start_focus_tracking();
            // A permission allowed at an earlier launch restarts without a dialog.
            let _ = std::thread::Builder::new()
                .name("onda-typing-restore".into())
                .spawn(move || onda_platform::restore_typing(&data_directory));
        }
        Ok(service)
    }
}
