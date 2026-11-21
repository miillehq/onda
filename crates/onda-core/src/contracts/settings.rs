use super::*;

impl Default for Settings {
    /// What a fresh profile starts with on the platform Onda was built for.
    fn default() -> Self {
        Self::defaults_for(cfg!(target_os = "macos"))
    }
}

impl Settings {
    /// What a fresh profile starts with on macOS (`macos`), or on Windows and Linux.
    pub fn defaults_for(macos: bool) -> Self {
        Self {
            execution_preference: ExecutionPreference::Auto,
            quality_profile: QualityProfile::Fast,
            model_id: None,
            model_directory: None,
            language: None,
            microphone_id: None,
            max_recording_seconds: DEFAULT_MAX_RECORDING_SECONDS,
            enhancement_enabled: false,
            context_enabled: false,
            learning_enabled: false,
            push_to_talk: default_push_to_talk(macos),
            transform_shortcut: Shortcut {
                keys: vec![0xA2, 0xA4, 0x50],
            },
            command_shortcut: Shortcut {
                keys: vec![0xA2, 0xA4, 0x43],
            },
            toggle_shortcut: Shortcut {
                keys: vec![0xA2, 0xA4, 0x44],
            },
            toggle_enabled: false,
            default_transform_id: crate::transforms::DEFAULT_TRANSFORM_ID.into(),
            app_profiles: Vec::new(),
            theme: ThemePreference::System,
            launch_at_login: false,
            show_pill: true,
            play_sounds: true,
            onboarding_completed: false,
            history_days: DEFAULT_HISTORY_DAYS,
            tray_icon: TrayIconStyle::Auto,
            ai_provider: AiProvider::Chatgpt,
            openai_base_url: None,
            openai_model: None,
            anthropic_model: None,
            local_base_url: None,
            local_model: None,
        }
    }
}

impl Settings {
    /// Reject invalid preferences before persisting or changing live shortcuts.
    pub fn validate(&self) -> crate::error::AppResult<()> {
        if self.history_days > MAX_HISTORY_DAYS {
            return Err(crate::error::AppError::invalid(
                "History can be kept for at most 30 days.",
            ));
        }
        if !(1..=MAX_RECORDING_SECONDS).contains(&self.max_recording_seconds) {
            return Err(crate::error::AppError::invalid(
                "Recording limit must be between 1 and 600 seconds.",
            ));
        }
        self.validate_shortcuts()?;
        if self.language.as_ref().is_some_and(|language| {
            language.len() > 12
                || !language
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '-')
        }) {
            return Err(crate::error::AppError::invalid(
                "Language must be a supported language code.",
            ));
        }
        if self
            .model_directory
            .as_ref()
            .is_some_and(|path| path.is_empty() || path.len() > 32767 || path.contains('\0'))
        {
            return Err(crate::error::AppError::invalid(
                "Model directory is invalid.",
            ));
        }
        // The network policy for these URLs (HTTPS, loopback for local) lives in onda-cloud; this
        // only keeps malformed values out of the database.
        for value in [
            &self.openai_base_url,
            &self.openai_model,
            &self.anthropic_model,
            &self.local_base_url,
            &self.local_model,
        ]
        .into_iter()
        .flatten()
        {
            if value.is_empty()
                || value.len() > MAX_PROVIDER_SETTING_BYTES
                || value.chars().any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(crate::error::AppError::invalid(
                    "AI provider addresses and model names cannot be empty or contain spaces.",
                ));
            }
        }
        if self.app_profiles.len() > 100
            || self.app_profiles.iter().any(|profile| {
                profile.executable.is_empty()
                    || profile.executable.len() > 260
                    || profile
                        .cleanup_instruction
                        .as_ref()
                        .is_some_and(|text| text.len() > 4096)
            })
        {
            return Err(crate::error::AppError::invalid(
                "Application profiles are invalid or too large.",
            ));
        }
        Ok(())
    }

    pub fn profile_for(&self, executable: &str) -> Option<&AppProfile> {
        self.app_profiles
            .iter()
            .find(|profile| profile.executable.eq_ignore_ascii_case(executable))
    }
}

impl Settings {
    /// Check every chord together because two actions cannot claim the same chord.
    fn validate_shortcuts(&self) -> crate::error::AppResult<()> {
        let mut chords = std::collections::HashSet::new();
        for shortcut in [
            &self.push_to_talk,
            &self.transform_shortcut,
            &self.command_shortcut,
            &self.toggle_shortcut,
        ] {
            if shortcut.keys.is_empty()
                || shortcut.keys.len() > 4
                || shortcut
                    .keys
                    .iter()
                    .any(|key| *key == 0 || *key > 254 || *key == 0x1B)
            {
                return Err(crate::error::AppError::invalid("Shortcuts need 1–4 valid Windows virtual keys; Escape is reserved for cancellation."));
            }
            let mut sorted = shortcut.keys.clone();
            sorted.sort_unstable();
            sorted.dedup();
            if sorted.len() != shortcut.keys.len() || !chords.insert(sorted) {
                return Err(crate::error::AppError::invalid(
                    "Shortcuts must be distinct and cannot repeat a key.",
                ));
            }
        }
        Ok(())
    }
}
