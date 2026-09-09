use tauri::{AppHandle, State};

use crate::codex::runtime::{CodexModelOption, CodexRuntime, CodexStatus};
use crate::error::AppError;

#[tauri::command]
pub async fn codex_status(runtime: State<'_, CodexRuntime>) -> Result<CodexStatus, AppError> {
    Ok(runtime.status().await)
}

#[tauri::command]
pub async fn codex_start(
    app: AppHandle,
    runtime: State<'_, CodexRuntime>,
) -> Result<CodexStatus, AppError> {
    runtime.start(&app).await
}

#[tauri::command]
pub async fn codex_stop(
    app: AppHandle,
    runtime: State<'_, CodexRuntime>,
) -> Result<CodexStatus, AppError> {
    runtime.stop(&app).await
}

#[tauri::command]
pub async fn codex_restart(
    app: AppHandle,
    runtime: State<'_, CodexRuntime>,
) -> Result<CodexStatus, AppError> {
    runtime.stop(&app).await?;
    runtime.start(&app).await
}

#[tauri::command]
pub async fn codex_models(
    app: AppHandle,
    runtime: State<'_, CodexRuntime>,
) -> Result<Vec<CodexModelOption>, AppError> {
    runtime.start(&app).await?;
    runtime.list_models().await
}
