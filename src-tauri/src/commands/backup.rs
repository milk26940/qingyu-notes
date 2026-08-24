use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};

use super::pages::PageState;
use crate::backup::model::{BackupManifest, BackupScope};
use crate::backup::repository::BackupRepository;
use crate::error::AppError;
use crate::pages::repository::PageRepository;

async fn repository(pages: &State<'_, PageState>) -> Result<BackupRepository, AppError> {
    let pages = super::pages::repo(pages).await?;
    BackupRepository::open(pages.root())
}

#[tauri::command]
pub async fn backup_checkpoint(
    pages: State<'_, PageState>,
    scope: BackupScope,
    source: Option<String>,
) -> Result<BackupManifest, AppError> {
    repository(&pages)
        .await?
        .checkpoint(scope, source.unwrap_or_else(|| "manual".into()))
}

#[tauri::command]
pub async fn backup_list(pages: State<'_, PageState>) -> Result<Vec<BackupManifest>, AppError> {
    repository(&pages).await?.list()
}

#[tauri::command]
pub async fn backup_restore(
    app: AppHandle,
    pages: State<'_, PageState>,
    snapshot_id: String,
) -> Result<BackupManifest, AppError> {
    let current = super::pages::repo(&pages).await?;
    let root = current.root().to_path_buf();
    let manifest = BackupRepository::open(&root)?.restore(&snapshot_id)?;
    *pages.repository.lock().await = Some(PageRepository::open(root)?);
    let _ = app.emit(
        "vault://changed",
        serde_json::json!({
            "source": "restore", "namespace": "backup", "tool": "restore",
            "pageIds": [], "databaseIds": []
        }),
    );
    Ok(manifest)
}

#[tauri::command]
pub async fn backup_export_zip(
    pages: State<'_, PageState>,
    path: Option<String>,
) -> Result<String, AppError> {
    let destination = path.map(PathBuf::from).unwrap_or_else(default_export_path);
    let path = repository(&pages).await?.export_zip(&destination)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn backup_import_zip(
    app: AppHandle,
    pages: State<'_, PageState>,
    path: String,
) -> Result<BackupManifest, AppError> {
    let current = super::pages::repo(&pages).await?;
    let root = current.root().to_path_buf();
    let manifest = BackupRepository::open(&root)?.import_zip(&PathBuf::from(path))?;
    *pages.repository.lock().await = Some(PageRepository::open(root)?);
    let _ = app.emit(
        "vault://changed",
        serde_json::json!({
            "source": "import", "namespace": "backup", "tool": "import",
            "pageIds": [], "databaseIds": []
        }),
    );
    Ok(manifest)
}

fn default_export_path() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join(format!("轻羽备份-{timestamp}.zip"))
}
