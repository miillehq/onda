//! Immutable trust root for downloadable model files, audited 2026-10-03.
use crate::{AsrError, HardwareInfo};
use serde::{Deserialize, Serialize};

pub const MODEL_MANIFEST_VERSION: u32 = 1;
const MODEL_REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
/// Profiles step down only on machines too small for them. Installed RAM is used rather than
/// momentarily free RAM, which Windows keeps low on purpose (standby cache) and which made the
/// chosen profile change from one launch to the next.
const MIN_BALANCED_TOTAL_RAM: u64 = 3 * 1024 * 1024 * 1024;
const MIN_QUALITY_TOTAL_RAM: u64 = 6 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QualityProfile {
    #[default]
    Fast,
    Balanced,
    Quality,
    Advanced,
}

/// Catalog entries use pinned repository revisions and SHA-256 of the real bytes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: &'static str,
    pub version: &'static str,
    pub display_name: &'static str,
    pub filename: &'static str,
    pub expected_size_bytes: u64,
    pub sha256: &'static str,
    pub english_only: bool,
    pub estimated_memory_bytes: u64,
    pub license: &'static str,
}

impl ModelSpec {
    /// Build a trusted HTTPS source without interpolating user input.
    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/{}/{filename}?download=true",
            self.version,
            filename = self.filename
        )
    }
}

static MODELS: [ModelSpec; 6] = [
    ModelSpec {
        id: "whisper-tiny-en",
        version: MODEL_REVISION,
        display_name: "Fast · English",
        filename: "ggml-tiny.en.bin",
        expected_size_bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        english_only: true,
        estimated_memory_bytes: 400 * 1024 * 1024,
        license: "MIT",
    },
    ModelSpec {
        id: "whisper-tiny",
        version: MODEL_REVISION,
        display_name: "Fast · Multilingual",
        filename: "ggml-tiny.bin",
        expected_size_bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        english_only: false,
        estimated_memory_bytes: 400 * 1024 * 1024,
        license: "MIT",
    },
    ModelSpec {
        id: "whisper-base-en-q5",
        version: MODEL_REVISION,
        display_name: "Balanced · English",
        filename: "ggml-base.en-q5_1.bin",
        expected_size_bytes: 59_721_011,
        sha256: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
        english_only: true,
        estimated_memory_bytes: 600 * 1024 * 1024,
        license: "MIT",
    },
    ModelSpec {
        id: "whisper-base-q5",
        version: MODEL_REVISION,
        display_name: "Balanced · Multilingual",
        filename: "ggml-base-q5_1.bin",
        expected_size_bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        english_only: false,
        estimated_memory_bytes: 600 * 1024 * 1024,
        license: "MIT",
    },
    ModelSpec {
        id: "whisper-small-en-q5",
        version: MODEL_REVISION,
        display_name: "Quality · English",
        filename: "ggml-small.en-q5_1.bin",
        expected_size_bytes: 190_098_681,
        sha256: "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
        english_only: true,
        estimated_memory_bytes: 1200 * 1024 * 1024,
        license: "MIT",
    },
    ModelSpec {
        id: "whisper-small-q5",
        version: MODEL_REVISION,
        display_name: "Quality · Multilingual",
        filename: "ggml-small-q5_1.bin",
        expected_size_bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        english_only: false,
        estimated_memory_bytes: 1200 * 1024 * 1024,
        license: "MIT",
    },
];

pub fn builtin_models() -> &'static [ModelSpec] {
    &MODELS
}

pub(crate) fn find_model(id: &str) -> Result<&'static ModelSpec, AsrError> {
    MODELS
        .iter()
        .find(|model| model.id == id)
        .ok_or(AsrError::UnknownModel)
}

/// Choose language-appropriate models and reduce memory demand on low RAM systems.
/// Advanced requires an explicit catalog ID, keeping raw filenames out of the UI API.
pub fn recommended_model(
    profile: QualityProfile,
    language: Option<&str>,
    hardware: &HardwareInfo,
    advanced_model_id: Option<&str>,
) -> Result<&'static ModelSpec, AsrError> {
    if profile == QualityProfile::Advanced {
        return find_model(
            advanced_model_id.ok_or(AsrError::InvalidOptions("advanced profile needs a model"))?,
        );
    }
    let english_only = language == Some("en");
    let installed_ram = hardware.total_memory_bytes;
    let effective_profile = match profile {
        QualityProfile::Quality if installed_ram >= MIN_QUALITY_TOTAL_RAM => {
            QualityProfile::Quality
        }
        QualityProfile::Balanced | QualityProfile::Quality
            if installed_ram >= MIN_BALANCED_TOTAL_RAM =>
        {
            QualityProfile::Balanced
        }
        _ => QualityProfile::Fast,
    };
    let id = match (effective_profile, english_only) {
        (QualityProfile::Fast, true) => "whisper-tiny-en",
        (QualityProfile::Fast, false) => "whisper-tiny",
        (QualityProfile::Balanced, true) => "whisper-base-en-q5",
        (QualityProfile::Balanced, false) => "whisper-base-q5",
        (QualityProfile::Quality, true) => "whisper-small-en-q5",
        _ => "whisper-small-q5",
    };
    find_model(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_pinned_and_language_detection_uses_multilingual() {
        for model in builtin_models() {
            assert_eq!(hex::decode(model.sha256).expect("catalog hex").len(), 32);
            assert_eq!(model.version.len(), 40);
            assert!(model.download_url().starts_with("https://huggingface.co/"));
        }
        let mut hardware = HardwareInfo::detect();
        hardware.total_memory_bytes = 16 << 30;
        // Momentarily low free memory must not change the chosen profile.
        hardware.available_memory_bytes = 512 << 20;
        assert_eq!(
            recommended_model(QualityProfile::Quality, Some("en"), &hardware, None)
                .expect("model")
                .id,
            "whisper-small-en-q5"
        );
        assert!(
            !recommended_model(QualityProfile::Balanced, None, &hardware, None)
                .expect("model")
                .english_only
        );
        hardware.total_memory_bytes = 2 << 30;
        assert_eq!(
            recommended_model(QualityProfile::Quality, Some("en"), &hardware, None)
                .expect("model")
                .id,
            "whisper-tiny-en"
        );
        assert!(recommended_model(
            QualityProfile::Advanced,
            Some("en"),
            &hardware,
            Some("../../bad")
        )
        .is_err());
    }
}
