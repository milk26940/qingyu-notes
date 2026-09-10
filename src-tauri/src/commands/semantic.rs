use serde::{Deserialize, Serialize};
use tauri::State;

use super::pages::PageState;
use crate::error::AppError;
use crate::portable::PortablePaths;
use crate::semantic::repository::cosine_similarity;
use crate::semantic::{client, config};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticConfigView {
    pub base_url: String,
    pub model: String,
    pub dimension: usize,
    pub enabled: bool,
    pub concurrency: usize,
    pub has_api_key: bool,
}

fn root() -> Result<std::path::PathBuf, AppError> {
    PortablePaths::current()
        .map(|p| p.data_root)
        .map_err(|e| AppError::new("portable_paths_failed", e.to_string()))
}

#[tauri::command]
pub async fn semantic_get_config() -> Result<SemanticConfigView, AppError> {
    let root = root()?;
    let value = config::load(&root).map_err(|e| AppError::new("semantic_config_read_failed", e))?;
    let has_api_key = config::load_key(&root)
        .map_err(|e| AppError::new("semantic_key_read_failed", e))?
        .is_some();
    Ok(SemanticConfigView {
        base_url: value.base_url,
        model: value.model,
        dimension: value.dimension,
        enabled: value.enabled,
        concurrency: value.concurrency,
        has_api_key,
    })
}

#[tauri::command]
pub fn knowledge_set_concurrency(value: usize) -> Result<(), AppError> {
    crate::semantic::knowledge::set_concurrency(&root()?, value)
}

#[tauri::command]
pub fn semantic_set_concurrency(value: usize) -> Result<(), AppError> {
    crate::semantic::index::set_concurrency(&root()?, value)
}
#[tauri::command]
pub async fn knowledge_catalog(state: State<'_, PageState>) -> Result<serde_json::Value, AppError> {
    crate::semantic::knowledge::catalog(&super::pages::repo(&state).await?)
}

#[tauri::command]
pub async fn semantic_save_config(
    base_url: String,
    model: String,
    dimension: usize,
    enabled: bool,
    api_key: Option<String>,
) -> Result<SemanticConfigView, AppError> {
    if dimension == 0 || dimension > 2560 {
        return Err(AppError::new(
            "semantic_invalid_dimension",
            "向量维度必须在 1 到 2560 之间",
        ));
    }
    let root = root()?;
    let value = config::EmbeddingConfig {
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        model: model.trim().to_string(),
        dimension,
        enabled,
        concurrency: config::load(&root)
            .map_err(|e| AppError::new("semantic_config_read_failed", e))?
            .concurrency,
    };
    config::save(&root, &value).map_err(|e| AppError::new("semantic_config_write_failed", e))?;
    if let Some(key) = api_key.filter(|v| !v.trim().is_empty()) {
        config::save_key(&root, key.trim())
            .map_err(|e| AppError::new("semantic_key_write_failed", e))?;
    }
    Ok(SemanticConfigView {
        base_url: value.base_url,
        model: value.model,
        dimension: value.dimension,
        enabled: value.enabled,
        concurrency: value.concurrency,
        has_api_key: config::load_key(&root)
            .map_err(|e| AppError::new("semantic_key_read_failed", e))?
            .is_some(),
    })
}

#[tauri::command]
pub async fn semantic_test_connection(
    base_url: String,
    model: String,
    dimension: usize,
    api_key: String,
) -> Result<usize, AppError> {
    let api_key = if api_key.trim().is_empty() {
        config::load_key(&root()?)
            .map_err(|e| AppError::new("semantic_key", e))?
            .ok_or_else(|| AppError::new("semantic_missing_key", "请先保存 API Key"))?
    } else {
        api_key
    };
    let vector = client::embed(
        &base_url,
        &api_key,
        &model,
        dimension,
        serde_json::json!([{ "text": "轻羽语义检索连接测试" }]),
    )
    .await
    .map_err(|e| AppError::new("semantic_connection_failed", e))?;
    if vector.len() != dimension {
        return Err(AppError::new(
            "semantic_dimension_mismatch",
            format!("模型返回 {} 维，期望 {} 维", vector.len(), dimension),
        ));
    }
    Ok(vector.len())
}

#[tauri::command]
pub async fn semantic_rebuild_index(
    state: State<'_, PageState>,
    full: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    let repository = super::pages::repo(&state).await?;
    crate::semantic::index::rebuild(&repository, &root()?, full.unwrap_or(false)).await
}

#[tauri::command]
pub fn semantic_cancel_index() {
    crate::semantic::index::cancel();
}

#[tauri::command]
pub async fn semantic_preview(state: State<'_, PageState>) -> Result<serde_json::Value, AppError> {
    let repository = super::pages::repo(&state).await?;
    let settings = config::load(&root()?).map_err(|e| AppError::new("semantic_config", e))?;
    let model = format!("{}|{}|extract-v2", settings.base_url, settings.model);
    let (items, errors) = crate::semantic::sources::collect(&repository)?;
    let mut reused = 0;
    for item in &items {
        if repository
            .embedding_record(
                &item.id,
                &model,
                settings.dimension,
                if item.image.is_some() {
                    "image"
                } else {
                    "text"
                },
            )?
            .is_some_and(|r| r.0 == item.hash)
        {
            reused += 1;
        }
    }
    Ok(
        serde_json::json!({"total":items.len(),"reused":reused,"pending":items.len()-reused,"images":items.iter().filter(|s|s.image.is_some()).count(),"errors":errors}),
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchResult {
    pub page_id: String,
    pub block_id: String,
    pub page_title: String,
    pub snippet: String,
    pub score: f32,
}

#[tauri::command]
pub async fn semantic_search(
    state: State<'_, PageState>,
    query: String,
) -> Result<Vec<SemanticSearchResult>, AppError> {
    let repository = super::pages::repo(&state).await?;
    search_repository(&repository, &query).await
}

pub(crate) async fn search_repository(
    repository: &crate::pages::repository::PageRepository,
    query: &str,
) -> Result<Vec<SemanticSearchResult>, AppError> {
    let root = root()?;
    let settings =
        config::load(&root).map_err(|e| AppError::new("semantic_config_read_failed", e))?;
    let key = config::load_key(&root)
        .map_err(|e| AppError::new("semantic_key_read_failed", e))?
        .ok_or_else(|| AppError::new("semantic_missing_key", "请先在设置中填写 API Key"))?;
    let query_vector = client::embed(
        &settings.base_url,
        &key,
        &settings.model,
        settings.dimension,
        serde_json::json!([{ "text": query }]),
    )
    .await
    .map_err(|e| AppError::new("semantic_embedding_failed", e))?;
    let model = format!("{}|{}|extract-v2", settings.base_url, settings.model);
    let sources = crate::semantic::sources::read(&repository, &model)?;
    let mut results = Vec::new();
    for (block_id, page_id, vector) in repository.embedding_vectors(settings.dimension)? {
        let Some(source) = sources.get(&block_id) else {
            continue;
        };
        results.push(SemanticSearchResult {
            block_id: source.block_id.clone(),
            page_id,
            page_title: source.title.clone(),
            snippet: source.text.chars().take(240).collect(),
            score: cosine_similarity(&query_vector, &vector),
        });
    }
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(20);
    Ok(results)
}

#[tauri::command]
pub async fn knowledge_get_config() -> Result<serde_json::Value, AppError> {
    let root = root()?;
    let settings = crate::semantic::knowledge::load(&root)?;
    Ok(
        serde_json::json!({"baseUrl":settings.base_url,"model":settings.model,"enabled":settings.enabled,"concurrency":settings.concurrency,"hasApiKey":crate::semantic::knowledge::has_key(&root)?}),
    )
}
#[tauri::command]
pub async fn knowledge_save_config(
    base_url: String,
    model: String,
    enabled: bool,
    api_key: Option<String>,
) -> Result<(), AppError> {
    crate::semantic::knowledge::save_settings(
        &root()?,
        &crate::semantic::knowledge::Settings {
            base_url,
            model,
            enabled,
            concurrency: crate::semantic::knowledge::load(&root()?)?.concurrency,
        },
        api_key,
    )
}
#[tauri::command]
pub async fn knowledge_rebuild(
    state: State<'_, PageState>,
    full: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    let repository = super::pages::repo(&state).await?;
    crate::semantic::knowledge::rebuild(&repository, &root()?, full.unwrap_or(false)).await
}
#[tauri::command]
pub fn knowledge_cancel() {
    crate::semantic::knowledge::cancel();
}
#[tauri::command]
pub async fn knowledge_status(state: State<'_, PageState>) -> Result<serde_json::Value, AppError> {
    crate::semantic::knowledge::status(&super::pages::repo(&state).await?)
}
#[tauri::command]
pub async fn knowledge_search(
    state: State<'_, PageState>,
    query: String,
) -> Result<serde_json::Value, AppError> {
    crate::semantic::knowledge::search(&super::pages::repo(&state).await?, &query)
}

#[tauri::command]
pub async fn knowledge_apply_draft(
    state: State<'_, PageState>,
    page_id: String,
    expected_content: String,
) -> Result<(), AppError> {
    let repo = super::pages::repo(&state).await?;
    let draft: String = repo.with_transaction(|tx| {
        tx.query_row(
            "SELECT draft FROM knowledge_wiki WHERE page_id=?1 AND draft IS NOT NULL",
            [&page_id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::new("wiki_draft", e.to_string()))
    })?;
    let page = repo.get(&page_id, false)?;
    repo.update_checked(
        &page_id,
        page.title.clone(),
        draft.clone(),
        &page.title,
        &expected_content,
    )?;
    repo.with_transaction(|tx| {
        tx.execute(
            "UPDATE knowledge_wiki SET base_content=?1,draft=NULL WHERE page_id=?2",
            rusqlite::params![draft, page_id],
        )
        .map_err(|e| AppError::new("wiki_draft", e.to_string()))?;
        Ok(())
    })
}
