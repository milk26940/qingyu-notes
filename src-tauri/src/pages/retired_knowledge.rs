use std::{fs, path::Path, time::Duration};

use rusqlite::Connection;
use uuid::Uuid;

use crate::{error::AppError, portable::PortablePaths};

const RETIRED_TABLES: &[&str] = &[
    "knowledge_relations", "knowledge_entities", "knowledge_state", "knowledge_units",
    "knowledge_wiki", "semantic_sources", "block_embeddings", "embedding_jobs", "embedding_models",
];

// 旧库和旧备份只清理登记过的生成页面，不按标题猜测，也不递归删除手建页面。
pub(super) fn remove(connection: &mut Connection, database_path: &Path) -> Result<(), AppError> {
    let tables = {
        let mut statement = connection.prepare("SELECT name FROM sqlite_master WHERE type='table'").map_err(error)?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(error)?
            .collect::<Result<Vec<_>, _>>().map_err(error)?;
        rows
    };
    if RETIRED_TABLES.iter().any(|name| tables.iter().any(|table| table == name)) {
        let backup_root = PortablePaths::current().map_err(error)?.backups_root.join("feature-retirement");
        fs::create_dir_all(&backup_root).map_err(error)?;
        let backup_path = backup_root.join(format!("before-remove-rag-wiki-{}.sqlite3", Uuid::new_v4()));
        let mut target = Connection::open(&backup_path).map_err(error)?;
        rusqlite::backup::Backup::new(connection, &mut target)
            .and_then(|backup| backup.run_to_completion(64, Duration::from_millis(5), None))
            .map_err(error)?;
        drop(target);

        let transaction = connection.transaction().map_err(error)?;
        if tables.iter().any(|table| table == "knowledge_wiki") {
            transaction.execute_batch(
                "CREATE TEMP TABLE retired_wiki_pages(id TEXT PRIMARY KEY);
                 INSERT OR IGNORE INTO retired_wiki_pages SELECT page_id FROM knowledge_wiki;
                 UPDATE pages SET parent_id=NULL
                   WHERE parent_id IN (SELECT id FROM retired_wiki_pages)
                   AND id NOT IN (SELECT id FROM retired_wiki_pages);
                 DELETE FROM blocks_fts WHERE page_id IN (SELECT id FROM retired_wiki_pages);
                 DELETE FROM pages WHERE id IN (SELECT id FROM retired_wiki_pages);
                 DROP TABLE retired_wiki_pages;"
            ).map_err(error)?;
        }
        for table in RETIRED_TABLES {
            transaction.execute_batch(&format!("DROP TABLE IF EXISTS {table};")).map_err(error)?;
        }
        transaction.commit().map_err(error)?;
    }

    // 仅清理此数据库旁的专用配置和密钥，普通搜索索引、附件及对话文件不动。
    if let Some(data_root) = database_path.parent() {
        for relative in ["settings/embedding.json", "settings/embedding.key", "settings/knowledge.json", "knowledge/settings/embedding.key"] {
            match fs::remove_file(data_root.join(relative)) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(failure) => return Err(error(failure)),
            }
        }
    }
    Ok(())
}

fn error(error: impl std::fmt::Display) -> AppError {
    AppError::new("knowledge_retirement_failed", format!("清理旧知识索引失败：{error}"))
}
