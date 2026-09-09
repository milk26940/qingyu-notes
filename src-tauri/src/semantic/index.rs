use super::{client, config, sources};
use crate::{error::AppError, pages::repository::PageRepository};
use std::sync::atomic::{AtomicBool, Ordering};

static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static PROGRESS: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);
pub fn status() -> serde_json::Value {
    PROGRESS
        .lock()
        .ok()
        .and_then(|v| v.clone())
        .unwrap_or(serde_json::json!({}))
}
struct RunningGuard;
impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

pub async fn rebuild(
    repository: &PageRepository,
    root: &std::path::Path,
    full: bool,
) -> Result<serde_json::Value, AppError> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(AppError::new("index_running", "索引更新正在进行"));
    }
    let _guard = RunningGuard;
    CANCEL.store(false, Ordering::SeqCst);
    let settings = config::load(root).map_err(|e| AppError::new("semantic_config", e))?;
    let key = config::load_key(root)
        .map_err(|e| AppError::new("semantic_key", e))?
        .ok_or_else(|| AppError::new("semantic_key", "请先保存 API Key"))?;
    sources::ensure(repository)?;
    let (items, mut errors) = sources::collect(repository)?;
    let total = items.len();
    let mut updated = 0;
    let mut reused = 0;
    let model = format!("{}|{}|extract-v2", settings.base_url, settings.model);
    let ids: Vec<_> = items.iter().map(|s| s.id.clone()).collect();
    for item in items {
        if let Ok(mut progress) = PROGRESS.lock() {
            *progress = Some(
                serde_json::json!({"total":total,"done":updated+reused+errors.len(),"title":item.title}),
            );
        }
        if CANCEL.load(Ordering::SeqCst) {
            return Ok(
                serde_json::json!({"scanned":total,"updated":updated,"reused":reused,"removed":0,"failed":errors.len(),"errors":errors,"cancelled":true}),
            );
        }
        let modality = if item.image.is_some() {
            "image"
        } else {
            "text"
        };
        if !full
            && repository
                .embedding_record(&item.id, &model, settings.dimension, modality)?
                .is_some_and(|r| r.0 == item.hash)
        {
            sources::save(repository, &item, &model)?;
            reused += 1;
            continue;
        }
        let input = match &item.image {
            Some(image) => serde_json::json!([{"image":image}]),
            None => serde_json::json!([{"text":item.text}]),
        };
        match client::embed(
            &settings.base_url,
            &key,
            &settings.model,
            settings.dimension,
            input,
        )
        .await
        {
            Ok(vector) if vector.len() == settings.dimension => {
                if !repository
                    .open_bundle(&item.page_id)
                    .is_ok_and(|bundle| bundle.document.revision == item.revision)
                {
                    errors.push(format!(
                        "{}：处理期间页面已修改，请再次增量更新",
                        item.title
                    ));
                    continue;
                }
                repository.save_embedding(
                    &item.id,
                    &item.page_id,
                    &item.hash,
                    modality,
                    &model,
                    &vector,
                )?;
                sources::save(repository, &item, &model)?;
                updated += 1;
            }
            Ok(_) => errors.push(format!("{}：返回的向量维度不符", item.title)),
            Err(error) => errors.push(format!("{}：{error}", item.title)),
        }
    }
    // 解析失败时保留旧索引，避免临时不可读导致已有证据被清空。
    let removed = if errors.is_empty() {
        repository.delete_embedding_except(&ids)?
    } else {
        0
    };
    Ok(
        serde_json::json!({"scanned":total,"updated":updated,"reused":reused,"removed":removed,"failed":errors.len(),"errors":errors,"cancelled":false}),
    )
}
