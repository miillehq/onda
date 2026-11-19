//! Owns native hotkey registrations, helper configuration, restarts and shutdown handles.
use super::*;

pub(super) struct ShortcutSupervisor {
    pub(super) helper_config: Mutex<onda_input::HelperConfig>,
    pub(super) helper_generation: AtomicU64,
    pub(super) shortcut_status: Mutex<String>,
    pub(super) helper_join: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// macOS: each registered system hotkey's id and the action it stands for.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(super) hotkeys: Mutex<HashMap<u32, (onda_input::ShortcutAction, Option<String>)>>,
}

impl ShortcutSupervisor {
    /// Windows' keyboard hook, or on Linux the shortcuts portal session; both run in the
    /// `onda-input` helper process and report the same events.
    /// Start listening for the dictation shortcuts: the isolated helper process on Windows and
    /// Linux, system hotkeys on macOS.
    pub(super) fn start_helper(&self, host: &Arc<Runtime>) -> AppResult<()> {
        #[cfg(target_os = "macos")]
        {
            self.start_hotkeys(host)
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.start_helper_process(host)
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn start_helper_process(&self, host: &Arc<Runtime>) -> AppResult<()> {
        let service = host.clone();
        let helper = std::thread::Builder::new()
            .name("onda-input-supervisor".into())
            .spawn(move || {
                let mut supervisor = None;
                let mut generation = u64::MAX;
                while !service.operations.shutdown.load(Ordering::Acquire) {
                    let current = service.shortcuts.helper_generation.load(Ordering::Acquire);
                    if supervisor.is_none() || current != generation {
                        drop(supervisor.take());
                        generation = current;
                        let config = match lock(&service.shortcuts.helper_config) {
                            Ok(config) => config.clone(),
                            Err(_) => break,
                        };
                        let helper = if cfg!(windows) {
                            "onda-input.exe"
                        } else {
                            "onda-input"
                        };
                        match onda_input::HelperSupervisor::start(
                            service.recognition.worker_directory.join(helper),
                            config,
                        ) {
                            Ok(value) => {
                                supervisor = Some(value);
                                service.shortcuts.shortcut_status(&service, "ready");
                            }
                            Err(_) => {
                                service.shortcuts.shortcut_status(&service, "unavailable");
                                break;
                            }
                        }
                    }
                    let event = match supervisor.as_mut() {
                        Some(helper) => {
                            let active = lock(&service.operations.state)
                                .map(|state| state.is_active())
                                .unwrap_or(false);
                            if helper.set_operation_active(active).is_err() {
                                service.operations.cancel_immediate(&service);
                                service.shortcuts.shortcut_status(&service, "unavailable");
                                break;
                            }
                            helper.poll(HELPER_POLL_TIMEOUT)
                        }
                        None => continue,
                    };
                    if !handle_helper_event(&service, event) {
                        break;
                    }
                }
            })
            .map_err(|_| {
                AppError::new(
                    ErrorCode::Shortcut,
                    "Shortcut supervisor could not start.",
                    true,
                )
            })?;
        *lock(&self.helper_join)? = Some(helper);
        Ok(())
    }

    /// What a shortcut press or release does, whether it came from the helper process (Windows,
    /// Linux) or a system hotkey (macOS). Releasing dictation or a command stops recording.
    pub(super) fn shortcut_event(
        &self,
        host: &Runtime,
        pressed: bool,
        action: onda_input::ShortcutAction,
        transform_id: Option<String>,
    ) {
        use onda_input::ShortcutAction as A;
        if !pressed {
            if matches!(action, A::Dictation | A::Command) {
                host.operations.queue_helper_action(host, Action::Stop);
            }
            return;
        }
        let action = match action {
            A::Dictation => Some(Action::Start {
                command_mode: false,
            }),
            A::Command => Some(Action::Start { command_mode: true }),
            A::Cancel => {
                host.operations.cancel_immediate(host);
                None
            }
            A::Toggle => Some(Action::Toggle),
            A::Transform => transform_id
                .or_else(|| {
                    lock(&host.data.store)
                        .and_then(|store| store.settings())
                        .ok()
                        .map(|settings| settings.default_transform_id)
                })
                .map(|transform_id| Action::Transform { transform_id }),
        };
        if let Some(action) = action {
            if host
                .operations
                .actions
                .try_send(ActionMessage {
                    action,
                    reply: None,
                })
                .is_err()
            {
                host.operations.cancel_immediate(host);
            }
        }
    }

    /// macOS: a registered system hotkey was pressed or released.
    #[cfg(target_os = "macos")]
    pub(super) fn hotkey(&self, host: &Runtime, id: u32, pressed: bool) {
        let binding = lock(&self.hotkeys)
            .ok()
            .and_then(|hotkeys| hotkeys.get(&id).cloned());
        if let Some((action, transform_id)) = binding {
            self.shortcut_event(host, pressed, action, transform_id);
        }
    }

    /// macOS: system hotkeys need no Accessibility or Input Monitoring permission. They are
    /// registered again whenever the shortcut configuration changes; Escape is registered only
    /// while an operation is active, so it is never taken from other apps.
    #[cfg(target_os = "macos")]
    pub(super) fn start_hotkeys(&self, host: &Arc<Runtime>) -> AppResult<()> {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let service = host.clone();
        let thread = std::thread::Builder::new()
            .name("onda-hotkeys".into())
            .spawn(move || {
                let mut generation = u64::MAX;
                let mut escape: Option<tauri_plugin_global_shortcut::Shortcut> = None;
                while !service.operations.shutdown.load(Ordering::Acquire) {
                    let current = service.shortcuts.helper_generation.load(Ordering::Acquire);
                    if current != generation {
                        generation = current;
                        escape = None;
                        service.shortcuts.register_hotkeys(&service);
                    }
                    let active = lock(&service.operations.state)
                        .map(|state| state.is_active())
                        .unwrap_or(false);
                    if active && escape.is_none() {
                        escape = service.shortcuts.register_escape(&service);
                    } else if !active {
                        if let Some(shortcut) = escape.take() {
                            let _ = service.app.global_shortcut().unregister(shortcut);
                            if let Ok(mut hotkeys) = lock(&service.shortcuts.hotkeys) {
                                hotkeys.remove(&shortcut.id());
                            }
                        }
                    }
                    std::thread::sleep(HELPER_POLL_TIMEOUT);
                }
                let _ = service.app.global_shortcut().unregister_all();
            })
            .map_err(|_| {
                AppError::new(
                    ErrorCode::Shortcut,
                    "Shortcut supervisor could not start.",
                    true,
                )
            })?;
        *lock(&self.helper_join)? = Some(thread);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub(super) fn register_hotkeys(&self, host: &Runtime) {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
        let shortcuts = host.app.global_shortcut();
        let _ = shortcuts.unregister_all();
        let Ok(config) = lock(&self.helper_config).map(|config| config.clone()) else {
            return;
        };
        let mut registered = HashMap::new();
        let mut failed = false;
        for binding in config
            .bindings
            .iter()
            .filter(|binding| binding.action != onda_input::ShortcutAction::Cancel)
        {
            let shortcut =
                onda_input::accelerator(binding).and_then(|text| text.parse::<Shortcut>().ok());
            match shortcut {
                Some(shortcut) if shortcuts.register(shortcut).is_ok() => {
                    registered.insert(
                        shortcut.id(),
                        (binding.action, binding.transform_id.clone()),
                    );
                }
                _ => failed = true,
            }
        }
        if let Ok(mut hotkeys) = lock(&self.hotkeys) {
            *hotkeys = registered;
        }
        // A chord macOS already uses (Spotlight, input sources) cannot be registered.
        self.shortcut_status(host, if failed { "unavailable" } else { "ready" });
    }

    #[cfg(target_os = "macos")]
    pub(super) fn register_escape(
        &self,
        host: &Runtime,
    ) -> Option<tauri_plugin_global_shortcut::Shortcut> {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
        let shortcut: Shortcut = "Escape".parse().ok()?;
        host.app.global_shortcut().register(shortcut).ok()?;
        if let Ok(mut hotkeys) = lock(&self.hotkeys) {
            hotkeys.insert(shortcut.id(), (onda_input::ShortcutAction::Cancel, None));
        }
        Some(shortcut)
    }

    pub(super) fn shortcut_status(&self, host: &Runtime, status: &str) {
        if let Ok(mut value) = lock(&self.shortcut_status) {
            *value = status.into();
        }
        host.event(RuntimeEvent::ShortcutStatus {
            status: status.into(),
        });
    }

    /// Asks the desktop again after the user dismissed the shortcut approval, or restarts the
    /// helper after it stopped.
    pub(super) fn restart_shortcuts(&self, host: &Arc<Runtime>) -> AppResult<()> {
        let finished = lock(&self.helper_join)?
            .as_ref()
            .is_none_or(|helper| helper.is_finished());
        if finished {
            self.shortcut_status(host, "starting");
            host.start_helper()
        } else {
            self.helper_generation.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }
}

impl ShortcutSupervisor {
    pub(super) fn new(config: onda_input::HelperConfig) -> Self {
        Self {
            helper_config: Mutex::new(config),
            helper_generation: AtomicU64::new(0),
            shortcut_status: Mutex::new("starting".into()),
            helper_join: Mutex::default(),
            hotkeys: Mutex::default(),
        }
    }
}

/// Return false when the helper has reached a terminal status for this generation.
#[cfg(not(target_os = "macos"))]
fn handle_helper_event(
    service: &Arc<Runtime>,
    event: Result<Option<onda_input::HelperEvent>, onda_input::InputError>,
) -> bool {
    match event {
        Ok(Some(onda_input::HelperEvent::Ready { .. })) => {
            service.shortcuts.shortcut_status(service, "ready");
        }
        Ok(Some(onda_input::HelperEvent::Pressed {
            action,
            transform_id,
        })) => {
            service
                .shortcuts
                .shortcut_event(service, true, action, transform_id);
        }
        Ok(Some(onda_input::HelperEvent::Released { action, .. })) => {
            service
                .shortcuts
                .shortcut_event(service, false, action, None);
        }
        // Linux: no shortcuts portal, so a desktop shortcut runs `onda --dictate`;
        // or the user dismissed the approval and can ask again in Shortcuts.
        Ok(Some(onda_input::HelperEvent::Error { code }))
            if !cfg!(windows) && (code == "unsupported" || code == "declined") =>
        {
            service.operations.cancel_immediate(service);
            service.shortcuts.shortcut_status(
                service,
                if code == "unsupported" {
                    "desktop"
                } else {
                    "declined"
                },
            );
            return false;
        }
        Ok(Some(onda_input::HelperEvent::Reset { .. } | onda_input::HelperEvent::Error { .. })) => {
            service.operations.cancel_immediate(service);
            service.shortcuts.shortcut_status(service, "recovering");
        }
        Err(_) => {
            service.operations.cancel_immediate(service);
            service.shortcuts.shortcut_status(service, "unavailable");
            return false;
        }
        _ => {}
    }
    true
}
