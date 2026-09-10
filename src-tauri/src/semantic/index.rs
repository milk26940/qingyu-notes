use super::{client, config, sources};
use crate::{error::AppError, pages::repository::PageRepository};
use std::{
    collections::VecDeque,
    path::Path,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static CONCURRENCY: AtomicUsize = AtomicUsize::new(6);
static PROGRESS: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

pub fn set_concurrency(root: &Path, value: usize) -> Result<(), AppError> {
    if !(1..=32).contains(&value) {
        return Err(AppError::new(
            "semantic_invalid_concurrency",
            "并发数量请输入 1 到 32 的整数",
        ));
    }
    let mut settings = config::load(root).map_err(|e| AppError::new("semantic_config", e))?;
    settings.concurrency = value;
    config::save(root, &settings).map_err(|e| AppError::new("semantic_config", e))?;
    CONCURRENCY.store(value, Ordering::SeqCst);
    Ok(())
}
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
    CONCURRENCY.store(settings.concurrency.clamp(1, 32), Ordering::SeqCst);
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
    let mut queue = VecDeque::new();
    for item in items {
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
        } else {
            queue.push_back(item);
        }
    }
    let mut tasks = tokio::task::JoinSet::new();
    let mut done = reused + errors.len();
    if let Ok(mut progress) = PROGRESS.lock() {
        *progress = Some(
            serde_json::json!({"total":total,"done":done,"title":"准备并行更新","active":0,"concurrency":CONCURRENCY.load(Ordering::SeqCst)}),
        );
    }
    loop {
        while tasks.len() < CONCURRENCY.load(Ordering::SeqCst) && !CANCEL.load(Ordering::SeqCst) {
            let Some(item) = queue.pop_front() else {
                break;
            };
            let base_url = settings.base_url.clone();
            let api_key = key.clone();
            let model_name = settings.model.clone();
            let dimension = settings.dimension;
            tasks.spawn(async move {
                let input = match &item.image {
                    Some(image) => serde_json::json!([{"image":image}]),
                    None => serde_json::json!([{"text":item.text}]),
                };
                let result =
                    client::embed(&base_url, &api_key, &model_name, dimension, input).await;
                (item, result)
            });
        }
        if let Ok(mut progress) = PROGRESS.lock() {
            if let Some(value) = progress.as_mut() {
                value["active"] = serde_json::json!(tasks.len());
                value["concurrency"] = serde_json::json!(CONCURRENCY.load(Ordering::SeqCst));
            }
        }
        if CANCEL.load(Ordering::SeqCst) && tasks.is_empty() {
            return Ok(
                serde_json::json!({"scanned":total,"updated":updated,"reused":reused,"removed":0,"failed":errors.len(),"errors":errors,"cancelled":true}),
            );
        }
        let Some(joined) = tasks.join_next().await else {
            break;
        };
        let (item, result) = match joined {
            Ok(value) => value,
            Err(error) => {
                errors.push(format!("索引任务失败：{error}"));
                done += 1;
                continue;
            }
        };
        done += 1;
        let modality = if item.image.is_some() {
            "image"
        } else {
            "text"
        };
        match result {
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
        if let Ok(mut progress) = PROGRESS.lock() {
            *progress = Some(
                serde_json::json!({"total":total,"done":done,"title":item.title,"active":tasks.len(),"concurrency":CONCURRENCY.load(Ordering::SeqCst)}),
            );
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
