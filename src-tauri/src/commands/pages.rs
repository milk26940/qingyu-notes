use notify::RecommendedWatcher;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::backup::model::BackupScope;
use crate::backup::repository::BackupRepository;
use crate::error::AppError;
use crate::pages::model::{Page, PageDocument, PageHistoryEntry, PageLink, PageOpenBundle, PageSummary, SearchResult};
use crate::pages::repository::PageRepository;
use crate::pages::watcher::watch_vault;
use crate::projection::SearchProjection;

#[derive(Default)]
pub struct PageState {
    pub repository: Mutex<Option<PageRepository>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    projection: Mutex<Option<SearchProjection>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageAssetData {
    mime: String,
    bytes: Vec<u8>,
}

#[tauri::command]
pub async fn vault_init(
    _path: Option<String>,
    state: State<'_, PageState>,
    app: AppHandle,
) -> Result<String, AppError> {
    let root = PageRepository::default_root();
    let repo = PageRepository::open(root.clone())?;
    crate::semantic::knowledge::repair_empty_wiki(&repo)?;
    let projection = SearchProjection::open(&root)?;
    projection.rebuild(&repo)?;
    let event_app = app.clone();
    let event_repo = repo.clone();
    let event_projection = projection.clone();
    let watcher = watch_vault(&root, move |paths| {
        let projection_status = match event_projection.rebuild(&event_repo) {
            Ok(()) => serde_json::json!({ "state": "ready" }),
            Err(error) => {
                serde_json::json!({ "state": "failed", "code": error.code, "message": error.message })
            }
        };
        let _ = event_app.emit("projection://status", projection_status);
        let paths = paths
            .into_iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        let _ = event_app.emit(
            "vault://changed",
            serde_json::json!({
                "source": "external",
                "namespace": "filesystem",
                "tool": "change",
                "paths": paths,
                "pageIds": [],
                "databaseIds": []
            }),
        );
    })?;
    *state.repository.lock().await = Some(repo);
    *state.watcher.lock().await = Some(watcher);
    *state.projection.lock().await = Some(projection);
    let _ = app.emit(
        "projection://status",
        serde_json::json!({ "state": "ready" }),
    );
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
pub async fn page_open_bundle(
    state: State<'_, PageState>,
    id: String,
) -> Result<PageOpenBundle, AppError> {
    repo(&state).await?.open_bundle(&id)
}

#[tauri::command]
pub async fn page_save_document(
    state: State<'_, PageState>,
    document: PageDocument,
    expected_revision: i64,
) -> Result<PageDocument, AppError> {
    repo(&state).await?.save_document(document, expected_revision)
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
    expected_title: String,
    expected_content: String,
) -> Result<Page, AppError> {
    repo(&state)
        .await?
        .update_checked(&id, title, content, &expected_title, &expected_content)
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
pub async fn page_trash_recursive(state: State<'_, PageState>, id: String) -> Result<Page, AppError> {
    repo(&state).await?.trash_recursive(&id)
}

#[tauri::command]
pub async fn page_restore(state: State<'_, PageState>, id: String) -> Result<Page, AppError> {
    repo(&state).await?.restore(&id)
}

#[tauri::command]
pub async fn page_purge(state: State<'_, PageState>, id: String) -> Result<(), AppError> {
    let pages = repo(&state).await?;
    BackupRepository::open(pages.root())?
        .checkpoint(BackupScope::Vault, "beforePermanentDelete")?;
    pages.purge(&id)
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
