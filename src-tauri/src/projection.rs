use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

use crate::codex::conversation::ConversationMetadata;
use crate::database::model::DatabaseDefinition;
use crate::error::AppError;
use crate::pages::repository::PageRepository;
use crate::portable::PortablePaths;

#[derive(Clone)]
pub struct SearchProjection {
    connection: Arc<Mutex<Connection>>,
}

impl SearchProjection {
    pub fn open(vault_root: &Path) -> Result<Self, AppError> {
        let _ = vault_root;
        let paths = PortablePaths::current()
            .map_err(|error| AppError::new("projection_init_failed", error.to_string()))?;
        Self::open_at(&paths.indexes_root.join("search.sqlite3"))
    }

    fn open_at(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| AppError::new("projection_init_failed", error.to_string()))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| AppError::new("projection_init_failed", error.to_string()))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS pages (
                   id TEXT PRIMARY KEY, title TEXT NOT NULL, parent_id TEXT, kind TEXT NOT NULL,
                   path TEXT NOT NULL, sort_order INTEGER NOT NULL, trashed_at TEXT
                 );
                 CREATE TABLE IF NOT EXISTS links (
                   source_id TEXT NOT NULL, target_id TEXT, label TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS databases (
                   id TEXT PRIMARY KEY, page_id TEXT NOT NULL, definition_json TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS conversations (
                   id TEXT PRIMARY KEY, thread_id TEXT, title TEXT NOT NULL, scope_json TEXT NOT NULL,
                   created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                 );",
            )
            .map_err(|error| AppError::new("projection_init_failed", error.to_string()))?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn rebuild(&self, pages: &PageRepository) -> Result<(), AppError> {
        let summaries = pages.list(true)?;
        let database_definitions = read_json_directory::<DatabaseDefinition>(
            &pages.root().join(".noteapp").join("databases"),
        )?;
        let conversations = read_conversations(pages.root())?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("projection_lock_failed", "索引锁已损坏"))?;
        let transaction = connection
            .transaction()
            .map_err(|error| AppError::new("projection_write_failed", error.to_string()))?;
        transaction
            .execute_batch("DELETE FROM links; DELETE FROM pages; DELETE FROM databases; DELETE FROM conversations;")
            .map_err(|error| AppError::new("projection_write_failed", error.to_string()))?;

        for page in &summaries {
            transaction
                .execute(
                    "INSERT INTO pages (id, title, parent_id, kind, path, sort_order, trashed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![page.id, page.title, page.parent_id, serde_json::to_string(&page.kind).unwrap(), page.path, page.sort_order, page.trashed_at],
                )
                .map_err(|error| AppError::new("projection_write_failed", error.to_string()))?;
            if page.trashed_at.is_none() {
                for link in pages.links(&page.id)? {
                    transaction
                        .execute(
                            "INSERT INTO links (source_id, target_id, label) VALUES (?1, ?2, ?3)",
                            params![link.source_id, link.target_id, link.label],
                        )
                        .map_err(|error| {
                            AppError::new("projection_write_failed", error.to_string())
                        })?;
                }
            }
        }
        for definition in database_definitions {
            transaction
                .execute(
                    "INSERT INTO databases (id, page_id, definition_json) VALUES (?1, ?2, ?3)",
                    params![
                        definition.id,
                        definition.page_id,
                        serde_json::to_string(&definition).unwrap()
                    ],
                )
                .map_err(|error| AppError::new("projection_write_failed", error.to_string()))?;
        }
        for conversation in conversations {
            transaction
                .execute(
                    "INSERT INTO conversations (id, thread_id, title, scope_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![conversation.id, conversation.thread_id, conversation.title, serde_json::to_string(&conversation.scope).unwrap(), conversation.created_at, conversation.updated_at],
                )
                .map_err(|error| AppError::new("projection_write_failed", error.to_string()))?;
        }
        transaction
            .commit()
            .map_err(|error| AppError::new("projection_write_failed", error.to_string()))
    }

    #[cfg(test)]
    fn page_count(&self) -> Result<i64, AppError> {
        self.connection
            .lock()
            .map_err(|_| AppError::new("projection_lock_failed", "索引锁已损坏"))?
            .query_row("SELECT COUNT(*) FROM pages", [], |row| row.get(0))
            .map_err(|error| AppError::new("projection_read_failed", error.to_string()))
    }
}

fn read_json_directory<T: serde::de::DeserializeOwned>(
    directory: &Path,
) -> Result<Vec<T>, AppError> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| AppError::new("projection_read_failed", error.to_string()))?
    {
        let path = entry
            .map_err(|error| AppError::new("projection_read_failed", error.to_string()))?
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(path)
            .map_err(|error| AppError::new("projection_read_failed", error.to_string()))?;
        values.push(
            serde_json::from_slice(&bytes)
                .map_err(|error| AppError::new("projection_invalid_data", error.to_string()))?,
        );
    }
    Ok(values)
}

fn read_conversations(root: &Path) -> Result<Vec<ConversationMetadata>, AppError> {
    let index = root
        .join(".noteapp")
        .join("conversations")
        .join("index.json");
    if !index.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(index)
        .map_err(|error| AppError::new("projection_read_failed", error.to_string()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| AppError::new("projection_invalid_data", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    use crate::pages::repository::PageRepository;

    #[test]
    fn deleted_projection_rebuilds_from_authoritative_files() {
        let root = std::env::temp_dir().join(format!("qingyu-projection-{}", Uuid::new_v4()));
        let vault = root.join("vault");
        let database = root.join("index.sqlite3");
        let pages = PageRepository::open(vault).unwrap();
        pages.create("第一篇".into(), None, "正文".into()).unwrap();

        let projection = SearchProjection::open_at(&database).unwrap();
        projection.rebuild(&pages).unwrap();
        assert_eq!(projection.page_count().unwrap(), 1);
        drop(projection);
        fs::remove_file(&database).unwrap();

        let rebuilt = SearchProjection::open_at(&database).unwrap();
        rebuilt.rebuild(&pages).unwrap();
        assert_eq!(rebuilt.page_count().unwrap(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
