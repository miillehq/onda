//! The local profile, statistics and the shareable statistics image, plus the resource report.
//! Nothing here is sent anywhere: pictures are chosen and images saved through native dialogs.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// The statistics card is drawn at 1200×630, well under these limits.
const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_EXPORT_DIMENSION: u32 = 4096;
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
        let summary = onda_core::stats::summarize(&store.lifetime_usage()?, &today);
        Ok(UsageReport {
            today,
            days,
            summary,
        })
    }

    /// Asks where to save the statistics image and writes it; `None` when cancelled.
    pub(super) async fn save_stats_image(&self, encoded: String) -> AppResult<Option<String>> {
        let png = tokio::task::spawn_blocking(move || decode_stats_image(&encoded))
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Image worker stopped.", true))??;
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
        let path = path
            .into_path()
            .map_err(|_| AppError::invalid("Choose a folder on this computer."))?;
        let path = tokio::task::spawn_blocking(move || {
            use std::io::Write;
            super::stats_export::write_png(&path, |file| file.write_all(&png))
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Save worker stopped.", true))?
        .map_err(|error| {
            AppError::invalid(match error.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    "That PNG already exists. Select it in the save dialog to replace it."
                }
                std::io::ErrorKind::InvalidInput => "Choose a filename ending in .png.",
                _ => "The image could not be saved there.",
            })
        })?;
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
    if encoded.len() > MAX_EXPORT_BYTES.div_ceil(3) * 4 {
        return Err(invalid());
    }
    let png = STANDARD.decode(encoded).map_err(|_| invalid())?;
    if png.len() > MAX_EXPORT_BYTES {
        return Err(invalid());
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&png));
    let info = decoder.read_header_info().map_err(|_| invalid())?;
    if info.width > MAX_EXPORT_DIMENSION || info.height > MAX_EXPORT_DIMENSION {
        return Err(invalid());
    }
    // Validate every scanline and IEND before the picker offers to replace an existing card.
    // This uses one decoded row at a time instead of allocating a second full image.
    let mut image = decoder.read_info().map_err(|_| invalid())?;
    while image.next_row().map_err(|_| invalid())?.is_some() {}
    image.finish().map_err(|_| invalid())?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        if width == 0 || height == 0 {
            let mut invalid = include_bytes!("../../icons/32x32.png").to_vec();
            invalid[16..20].copy_from_slice(&width.to_be_bytes());
            invalid[20..24].copy_from_slice(&height.to_be_bytes());
            return invalid;
        }
        {
            let encoder = png::Encoder::new(&mut bytes, width, height);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![0; (width * height) as usize])
                .unwrap();
        }
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
    fn an_incomplete_png_is_refused_before_opening_the_save_dialog() {
        let complete = include_bytes!("../../icons/32x32.png");
        assert!(decode_stats_image(&STANDARD.encode(complete)).is_ok());
        assert!(
            decode_stats_image(&STANDARD.encode(&complete[..complete.len() - 4])).is_err(),
            "accepted a truncated PNG"
        );
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
