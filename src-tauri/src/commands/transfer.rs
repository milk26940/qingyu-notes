use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};

use super::pages::PageState;
use crate::error::AppError;
use crate::pages::repository::PageRepository;
use crate::transfer::{self, ImportPreview, ImportReport};

fn default_export_path(kind: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    crate::portable::PortablePaths::current()
        .map(|paths| paths.exports_root)
        .unwrap_or_else(|_| PathBuf::from("data").join("exports"))
        .join(format!("轻羽-{kind}-{timestamp}.zip"))
}

#[tauri::command]
pub async fn transfer_export_lossless(
    pages: State<'_, PageState>,
    path: Option<String>,
) -> Result<String, AppError> {
    let destination = path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_export_path("无损包"));
    let repository = super::pages::repo(&pages).await?;
    Ok(transfer::export_lossless(&repository, &destination)?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub async fn transfer_export_markdown(
    pages: State<'_, PageState>,
    path: Option<String>,
) -> Result<String, AppError> {
    let destination = path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_export_path("Markdown"));
    let repository = super::pages::repo(&pages).await?;
    Ok(transfer::export_markdown(&repository, &destination)?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub async fn transfer_preview_content(path: String) -> Result<ImportPreview, AppError> {
    transfer::preview_content(&PathBuf::from(path))
}

#[tauri::command]
pub async fn transfer_import_content(
    app: AppHandle,
    pages: State<'_, PageState>,
    path: String,
) -> Result<ImportReport, AppError> {
    let repository = super::pages::repo(&pages).await?;
    let root = repository.root().to_path_buf();
    let report = transfer::import_content(&repository, &PathBuf::from(path))?;
    *pages.repository.lock().await = Some(PageRepository::open(root)?);
    let _ = app.emit(
        "vault://changed",
        serde_json::json!({
            "source": "import", "namespace": "transfer", "tool": "import",
            "pageIds": [], "databaseIds": []
        }),
    );
    Ok(report)
}
