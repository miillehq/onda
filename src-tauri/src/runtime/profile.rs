//! The local profile, statistics and the shareable statistics image, plus the resource report.
//! Nothing here is sent anywhere: pictures are chosen and images saved through native dialogs.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// The statistics card is drawn at 1200×630 (2× for sharp text), well under these limits.
const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_EXPORT_DIMENSION: u32 = 4096;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Two samples this far apart give a CPU share without keeping the report waiting long.
const RESOURCE_SAMPLE: Duration = Duration::from_millis(500);

impl Runtime {
    pub(super) fn profile(&self) -> AppResult<Profile> {
        let (name, picture) = lock(&self.store)?.profile()?;
        Ok(Profile {
            name,
            picture: picture.map(|bytes| STANDARD.encode(bytes)),
        })
    }

    pub(super) fn set_profile_name(&self, raw: Option<&str>) -> AppResult<Profile> {
        let name = onda_core::profile::normalize_name(raw.unwrap_or_default())?;
        lock(&self.store)?.set_profile_name(name.as_deref())?;
        self.profile()
    }

    /// Opens the system picker natively; a cancelled picker leaves the profile unchanged.
    pub(super) async fn choose_profile_picture(&self) -> AppResult<Profile> {
        let (sender, receiver) = oneshot::channel();
        self.app
            .dialog()
            .file()
            .set_title("Choose a profile picture")
            .add_filter("Pictures", &["png", "jpg", "jpeg", "webp"])
            .pick_file(move |path| {
                let _ = sender.send(path);
            });
        let Some(path) = receiver.await.ok().flatten() else {
            return self.profile();
        };
        let path = path
            .into_path()
            .map_err(|_| AppError::invalid("Choose a picture file on this computer."))?;
        let picture = tokio::task::spawn_blocking(move || -> AppResult<Vec<u8>> {
            let size = std::fs::metadata(&path)
                .map_err(|_| AppError::invalid("The picture could not be opened."))?
                .len();
            if size > onda_core::profile::MAX_PICTURE_INPUT_BYTES as u64 {
                return Err(AppError::invalid("Choose a picture smaller than 10 MB."));
            }
            let bytes = std::fs::read(&path)
                .map_err(|_| AppError::invalid("The picture could not be opened."))?;
            onda_core::profile::normalize_picture(&bytes)
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Picture worker stopped.", true))??;
        lock(&self.store)?.set_profile_picture(Some(&picture))?;
        self.profile()
    }

    pub(super) fn remove_profile_picture(&self) -> AppResult<Profile> {
        lock(&self.store)?.set_profile_picture(None)?;
        self.profile()
    }

    pub(super) fn usage_report(&self, days: u32) -> AppResult<UsageReport> {
        let today = onda_platform::local_date();
        let store = lock(&self.store)?;
        let days = store.daily_usage(days)?;
        let summary = onda_core::stats::summarize(&store.lifetime_usage()?, &days, &today);
        Ok(UsageReport {
            today,
            days,
            summary,
        })
    }

    /// Asks where to save the statistics image and writes it; `None` when cancelled.
    pub(super) async fn save_stats_image(&self, encoded: &str) -> AppResult<Option<String>> {
        let png = decode_stats_image(encoded)?;
        let (sender, receiver) = oneshot::channel();
        let mut dialog = self
            .app
            .dialog()
            .file()
            .set_title("Save your Onda stats")
            .set_file_name(format!(
                "{} stats {}.png",
                onda_core::APP_NAME,
                onda_platform::local_date()
            ))
            .add_filter("PNG image", &["png"]);
        if let Ok(pictures) = self.app.path().picture_dir() {
            dialog = dialog.set_directory(pictures);
        }
        dialog.save_file(move |path| {
            let _ = sender.send(path);
        });
        let Some(path) = receiver.await.ok().flatten() else {
            return Ok(None);
        };
        let mut path = path
            .into_path()
            .map_err(|_| AppError::invalid("Choose a folder on this computer."))?;
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        {
            path.set_extension("png");
        }
        let target = path.clone();
        tokio::task::spawn_blocking(move || std::fs::write(&target, png))
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Save worker stopped.", true))?
            .map_err(|_| AppError::invalid("The image could not be saved there."))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }

    /// Onda's process and everything it started, measured twice for a CPU share.
    pub(super) async fn resource_usage(&self) -> AppResult<ResourceUsage> {
        let root = std::process::id();
        let first = sample_tree(root).await?;
        tokio::time::sleep(RESOURCE_SAMPLE).await;
        let (samples, measure) = sample_tree(root).await?;
        let elapsed = RESOURCE_SAMPLE.as_secs_f64();
        let cpus = self.asr.hardware().logical_cpus.max(1) as f64;
        let before: HashMap<u32, Duration> = first
            .0
            .iter()
            .map(|sample| (sample.pid, sample.cpu_time))
            .collect();
        let processes: Vec<ProcessUsage> = samples
            .iter()
            .map(|sample| {
                let used = before.get(&sample.pid).map_or(Duration::ZERO, |earlier| {
                    sample.cpu_time.saturating_sub(*earlier)
                });
                ProcessUsage {
                    pid: sample.pid,
                    name: sample.name.clone(),
                    role: process_role(root, sample).into(),
                    memory_bytes: sample.memory_bytes as f64,
                    cpu_percent: used.as_secs_f64() / elapsed / cpus * 100.0,
                }
            })
            .collect();
        Ok(ResourceUsage {
            total_memory_bytes: processes.iter().map(|process| process.memory_bytes).sum(),
            total_cpu_percent: processes.iter().map(|process| process.cpu_percent).sum(),
            processes,
            memory_measure: match measure {
                onda_platform::MemoryMeasure::PrivateWorkingSet => "privateWorkingSet",
                onda_platform::MemoryMeasure::PrivateCommit => "privateCommit",
                onda_platform::MemoryMeasure::PrivateResident => "privateResident",
            }
            .into(),
            sample_ms: RESOURCE_SAMPLE.as_millis() as u32,
        })
    }
}

async fn sample_tree(
    root: u32,
) -> AppResult<(
    Vec<onda_platform::ProcessSample>,
    onda_platform::MemoryMeasure,
)> {
    tokio::task::spawn_blocking(move || onda_platform::process_tree(root))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Process sampling stopped.", true))?
        .map_err(errors::platform)
}

/// Groups by what the process is for; names come from Onda's own executables and the web view
/// engines (WebView2's msedgewebview2.exe, WebKitGTK's WebKit*Process).
fn process_role(root: u32, sample: &onda_platform::ProcessSample) -> &'static str {
    let name = sample.name.to_ascii_lowercase();
    if sample.pid == root {
        "app"
    } else if name.starts_with("onda-input") {
        "helper"
    } else if name.starts_with("onda-asr") {
        "speech"
    } else if name.starts_with("msedgewebview2") || name.starts_with("webkit") {
        "webview"
    } else {
        "other"
    }
}

/// A complete PNG of sensible size; anything else from the webview is refused.
fn decode_stats_image(encoded: &str) -> AppResult<Vec<u8>> {
    let invalid = || AppError::invalid("The statistics image could not be created.");
    if encoded.len() > MAX_EXPORT_BYTES / 3 * 4 + 4 {
        return Err(invalid());
    }
    let png = STANDARD.decode(encoded).map_err(|_| invalid())?;
    // Signature, then the IHDR chunk: length (4), type (4), width (4), height (4).
    if png.len() < 24 || !png.starts_with(PNG_SIGNATURE) || &png[12..16] != b"IHDR" {
        return Err(invalid());
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    if width == 0 || height == 0 || width > MAX_EXPORT_DIMENSION || height > MAX_EXPORT_DIMENSION {
        return Err(invalid());
    }
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn only_bounded_png_images_are_saved() {
        assert!(decode_stats_image(&STANDARD.encode(png(2400, 1260))).is_ok());
        assert!(decode_stats_image(&STANDARD.encode(png(5000, 10))).is_err());
        assert!(decode_stats_image(&STANDARD.encode(png(0, 10))).is_err());
        assert!(decode_stats_image(&STANDARD.encode(b"<svg/>")).is_err());
        assert!(decode_stats_image("not base64!").is_err());
        assert!(decode_stats_image(&"A".repeat(MAX_EXPORT_BYTES * 2)).is_err());
    }

    #[test]
    fn processes_are_grouped_by_purpose() {
        let sample = |pid, name: &str| onda_platform::ProcessSample {
            pid,
            parent: 1,
            name: name.into(),
            memory_bytes: 0,
            cpu_time: Duration::ZERO,
        };
        assert_eq!(process_role(7, &sample(7, "onda.exe")), "app");
        assert_eq!(process_role(7, &sample(8, "msedgewebview2.exe")), "webview");
        assert_eq!(process_role(7, &sample(9, "WebKitWebProces")), "webview");
        assert_eq!(
            process_role(7, &sample(10, "onda-asr-avx2-worker.exe")),
            "speech"
        );
        assert_eq!(process_role(7, &sample(11, "onda-input.exe")), "helper");
        assert_eq!(process_role(7, &sample(12, "crashpad_handler")), "other");
    }
}
