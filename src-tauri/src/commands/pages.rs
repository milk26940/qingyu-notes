use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::pages::model::{Page, PageHistoryEntry, PageLink, PageSummary, SearchResult};
use crate::pages::repository::PageRepository;

#[derive(Default)]
pub struct PageState {
    pub repository: Mutex<Option<PageRepository>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageAssetData {
    mime: String,
    bytes: Vec<u8>,
}

#[tauri::command]
pub async fn vault_init(
    path: Option<String>,
    state: State<'_, PageState>,
) -> Result<String, AppError> {
    let root = path
        .map(PathBuf::from)
        .unwrap_or_else(PageRepository::default_root);
    let repo = PageRepository::open(root.clone())?;
    *state.repository.lock().await = Some(repo);
    Ok(root.to_string_lossy().to_string())
}

pub(crate) async fn repo(state: &State<'_, PageState>) -> Result<PageRepository, AppError> {
    state
        .repository
        .lock()
        .await
        .clone()
        .ok_or_else(|| AppError::new("vault_not_initialized", "请先初始化笔记库"))
}

#[tauri::command]
pub async fn page_list(
    state: State<'_, PageState>,
    include_trash: bool,
) -> Result<Vec<PageSummary>, AppError> {
    repo(&state).await?.list(include_trash)
}

#[tauri::command]
pub async fn page_get(
    state: State<'_, PageState>,
    id: String,
    include_trash: bool,
) -> Result<Page, AppError> {
    repo(&state).await?.get(&id, include_trash)
}

#[tauri::command]
pub async fn page_create(
    state: State<'_, PageState>,
    title: String,
    parent_id: Option<String>,
    content: String,
) -> Result<Page, AppError> {
    repo(&state).await?.create(title, parent_id, content)
}

#[tauri::command]
pub async fn page_update(
    state: State<'_, PageState>,
    id: String,
    title: String,
    content: String,
) -> Result<Page, AppError> {
    repo(&state).await?.update(&id, title, content)
}

#[tauri::command]
pub async fn page_store_asset(
    state: State<'_, PageState>,
    page_id: String,
    original_name: String,
    bytes: Vec<u8>,
) -> Result<String, AppError> {
    repo(&state)
        .await?
        .store_asset(&page_id, &original_name, &bytes)
}

#[tauri::command]
pub async fn page_read_asset(
    state: State<'_, PageState>,
    relative_path: String,
) -> Result<PageAssetData, AppError> {
    let (mime, bytes) = repo(&state).await?.read_asset(&relative_path)?;
    Ok(PageAssetData { mime, bytes })
}

#[tauri::command]
pub async fn page_move(
    state: State<'_, PageState>,
    id: String,
    parent_id: Option<String>,
    sort_order: i64,
) -> Result<Page, AppError> {
    repo(&state).await?.move_page(&id, parent_id, sort_order)
}

#[tauri::command]
pub async fn page_trash(state: State<'_, PageState>, id: String) -> Result<Page, AppError> {
    repo(&state).await?.trash(&id)
}

#[tauri::command]
pub async fn page_restore(state: State<'_, PageState>, id: String) -> Result<Page, AppError> {
    repo(&state).await?.restore(&id)
}

#[tauri::command]
pub async fn page_history(
    state: State<'_, PageState>,
    id: String,
) -> Result<Vec<PageHistoryEntry>, AppError> {
    repo(&state).await?.history(&id)
}

#[tauri::command]
pub async fn page_search(
    state: State<'_, PageState>,
    query: String,
) -> Result<Vec<SearchResult>, AppError> {
    repo(&state).await?.search(&query)
}

#[tauri::command]
pub async fn page_links(
    state: State<'_, PageState>,
    id: String,
) -> Result<Vec<PageLink>, AppError> {
    repo(&state).await?.links(&id)
}

#[tauri::command]
pub async fn page_backlinks(
    state: State<'_, PageState>,
    id: String,
) -> Result<Vec<PageLink>, AppError> {
    repo(&state).await?.backlinks(&id)
}
