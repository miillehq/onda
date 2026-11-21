use super::*;
use ApiResponse as O;

pub(super) fn complete_debug_smoke(
    service: &Arc<Runtime>,
    passed: bool,
    checks: Vec<String>,
) -> AppResult<ApiResponse> {
    if !cfg!(debug_assertions) {
        return Err(AppError::invalid(
            "Debug smoke reporting is unavailable in release builds.",
        ));
    }
    let directory = crate::debug_smoke_directory()
        .ok_or_else(|| AppError::invalid("No isolated debug smoke directory was configured."))?;
    if checks.len() > 64
        || checks.iter().any(|check| {
            check.len() > 128
                || !check.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == ' '
                })
        })
    {
        return Err(AppError::invalid("Smoke report was invalid."));
    }
    let report = serde_json::to_vec_pretty(
        &serde_json::json!({"passed":passed,"checks":checks,"version":onda_core::APP_VERSION}),
    )
    .map_err(|_| AppError::storage())?;
    std::fs::write(directory.join("report.json"), report).map_err(|_| AppError::storage())?;
    service.app.exit(if passed { 0 } else { 1 });
    Ok(O::Unit)
}
