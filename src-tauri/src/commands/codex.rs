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

#[tauri::command]
pub fn agent_settings_get() -> Result<crate::codex::settings::AgentSettings, AppError> {
    crate::codex::settings::get()
}

#[tauri::command]
pub async fn agent_settings_save(
    app: AppHandle, runtime: State<'_, CodexRuntime>, base_url: String, model: String,
    api_key: Option<String>, clear_api_key: Option<bool>,
) -> Result<crate::codex::settings::AgentSettings, AppError> {
    use tauri::Emitter;
    let settings = crate::codex::settings::save(base_url, model, api_key, clear_api_key.unwrap_or(false))?;
    runtime.start(&app).await?;
    let _ = app.emit("codex://config-changed", ());
    Ok(settings)
}

#[tauri::command]
pub async fn agent_test_connection(model: Option<String>) -> Result<serde_json::Value, AppError> {
    crate::codex::settings::test_connection(model.as_deref()).await
}
