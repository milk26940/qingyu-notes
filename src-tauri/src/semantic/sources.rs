use crate::{error::AppError, pages::repository::PageRepository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub page_id: String,
    pub block_id: String,
    pub title: String,
    pub text: String,
    pub image: Option<String>,
    pub hash: String,
    pub revision: i64,
}

pub fn collect(repository: &PageRepository) -> Result<(Vec<Source>, Vec<String>), AppError> {
    let mut sources = Vec::new();
    let mut errors = Vec::new();
    let mut documents = HashMap::new();
    let mut titles = HashMap::new();
    let mut revisions = HashMap::new();
    let generated = super::knowledge::generated_ids(repository)?;
    for (_, page_id, title) in repository.embedding_titles()? {
        if generated.contains(&page_id) {
            continue;
        }
        let bundle = repository.open_bundle(&page_id)?;
        revisions.insert(page_id.clone(), bundle.document.revision);
        documents.insert(
            page_id.clone(),
            serde_json::to_string(&bundle.document).unwrap_or_default(),
        );
        titles.insert(page_id.clone(), title.clone());
        add_text(
            &mut sources,
            &format!("page:{page_id}"),
            &page_id,
            "",
            &title,
            &title,
        );
        if matches!(bundle.page.kind, crate::pages::model::PageKind::Database) {
            let definition =
                crate::database::repository::DatabaseRepository::from_pages(repository.clone())?
                    .get(&page_id)?;
            add_text(
                &mut sources,
                &format!("database:{page_id}"),
                &page_id,
                "",
                &title,
                &serde_json::to_string(&definition)
                    .map_err(|e| AppError::new("database_index", e.to_string()))?,
            );
        }
        if !bundle.page.properties.is_empty() {
            add_text(
                &mut sources,
                &format!("properties:{page_id}"),
                &page_id,
                "",
                &title,
                &serde_json::to_string(&bundle.page.properties).unwrap_or_default(),
            );
        }
    }
    for (id, page_id, text) in repository.embedding_blocks(None)? {
        if generated.contains(&page_id) {
            continue;
        }
        add_text(
            &mut sources,
            &id,
            &page_id,
            &id,
            titles.get(&page_id).map(String::as_str).unwrap_or(""),
            &text,
        );
    }
    let root = repository.root().parent().unwrap_or(repository.root());
    for (id, page_id, relative, mime, _) in repository.embedding_attachments()? {
        let filename = std::path::Path::new(&relative)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("");
        if filename.is_empty()
            || !documents
                .get(&page_id)
                .is_some_and(|doc| doc.contains(filename))
        {
            continue;
        }
        let path = root.join(&relative);
        let block_id=documents.get(&page_id).and_then(|doc|serde_json::from_str::<serde_json::Value>(doc).ok()).and_then(|doc|asset_block(&doc,filename)).unwrap_or_default();
        let result = (|| -> Result<(), AppError> {
            let canonical = path
                .canonicalize()
                .map_err(|e| AppError::new("attachment_missing", e.to_string()))?;
            let allowed = root
                .canonicalize()
                .map_err(|e| AppError::new("attachment_root", e.to_string()))?;
            if !canonical.starts_with(allowed) {
                return Err(AppError::new(
                    "attachment_outside",
                    "附件路径超出笔记数据目录",
                ));
            }
            let title = titles.get(&page_id).map(String::as_str).unwrap_or("");
            if mime.starts_with("image/") {
                let bytes = std::fs::read(canonical)
                    .map_err(|e| AppError::new("attachment_read", e.to_string()))?;
                let hash = format!("{:x}", Sha256::digest(&bytes));
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
                sources.push(Source {
                    id: id.clone(),
                    page_id: page_id.clone(),
                    block_id: block_id.clone(),
                    title: title.into(),
                    text: format!("{title} · 图片 {filename}"),
                    image: Some(format!("data:{mime};base64,{encoded}")),
                    hash,
                    revision: 0,
                });
            } else {
                let text = super::extract::text(&canonical)?;
                add_text(&mut sources, &id, &page_id, &block_id, title, &text);
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(format!("{relative}：{}", error.message));
        }
    }
    for source in &mut sources {
        source.revision = *revisions.get(&source.page_id).unwrap_or(&0);
    }
    Ok((sources, errors))
}

fn asset_block(value:&serde_json::Value,filename:&str)->Option<String> {
    if value.get("attrs").is_some_and(|attrs|attrs.to_string().contains(filename)) {
        if let Some(id)=value.get("id").and_then(serde_json::Value::as_str) {return Some(id.into());}
    }
    for key in ["blocks","children"] { if let Some(items)=value.get(key).and_then(serde_json::Value::as_array) { for item in items {if let Some(id)=asset_block(item,filename) {return Some(id);}} } }
    None
}

fn add_text(
    sources: &mut Vec<Source>,
    id: &str,
    page_id: &str,
    block_id: &str,
    title: &str,
    text: &str,
) {
    for (index, text) in super::extract::chunks(text).into_iter().enumerate() {
        sources.push(Source {
            id: format!("{id}:{index}"),
            page_id: page_id.into(),
            block_id: block_id.into(),
            title: title.into(),
            hash: format!("{:x}", Sha256::digest(text.as_bytes())),
            text,
            image: None,
            revision: 0,
        });
    }
}

pub fn ensure(repository: &PageRepository) -> Result<(), AppError> {
    repository.with_transaction(|tx| tx.execute_batch("CREATE TABLE IF NOT EXISTS semantic_sources(id TEXT PRIMARY KEY,page_id TEXT NOT NULL,block_id TEXT NOT NULL,title TEXT NOT NULL,text TEXT NOT NULL,hash TEXT NOT NULL,model TEXT NOT NULL);")
        .map_err(|e| AppError::new("semantic_schema", e.to_string())))
}

pub fn save(repository: &PageRepository, source: &Source, model: &str) -> Result<(), AppError> {
    repository.with_transaction(|tx| {
        tx.execute("INSERT INTO semantic_sources VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET page_id=excluded.page_id,block_id=excluded.block_id,title=excluded.title,text=excluded.text,hash=excluded.hash,model=excluded.model",
            rusqlite::params![source.id,source.page_id,source.block_id,source.title,source.text,source.hash,model]).map_err(|e| AppError::new("semantic_save", e.to_string()))?;
        Ok(())
    })
}

pub fn read(repository: &PageRepository, model: &str) -> Result<HashMap<String, Source>, AppError> {
    ensure(repository)?;
    repository.with_transaction(|tx| {
        let mut statement = tx.prepare("SELECT s.id,s.page_id,s.block_id,p.title,s.text,s.hash FROM semantic_sources s JOIN pages p ON p.id=s.page_id WHERE s.model=?1 AND p.trashed_at IS NULL").map_err(|e| AppError::new("semantic_read",e.to_string()))?;
        let rows = statement.query_map([model], |r| Ok(Source { id:r.get(0)?,page_id:r.get(1)?,block_id:r.get(2)?,title:r.get(3)?,text:r.get(4)?,hash:r.get(5)?,image:None,revision:0 })).map_err(|e| AppError::new("semantic_read",e.to_string()))?;
        let mut result = HashMap::new();
        for row in rows { let source = row.map_err(|e| AppError::new("semantic_read",e.to_string()))?; result.insert(source.id.clone(),source); }
        Ok(result)
    })
}
