use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::Value;
use uuid::Uuid;

use super::model::{
    BlockNode, Page, PageDocument, PageHistoryEntry, PageKind, PageLink, PageOpenBundle,
    PageSummary, SearchResult,
};
use crate::error::AppError;
use crate::database::model::DatabaseDefinition;
use crate::portable::PortablePaths;

#[derive(Clone)]
pub struct PageRepository {
    root: PathBuf,
    database_path: PathBuf,
    connection: Arc<Mutex<Connection>>,
    write_lock: Arc<Mutex<()>>,
}

static VAULT_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

const SCHEMA_VERSION: i64 = 2;

fn open_connection(path: &Path) -> Result<Connection, AppError> {
    let mut connection = Connection::open(path)
        .map_err(|error| AppError::new("sqlite_open_failed", error.to_string()))?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
        .map_err(|error| AppError::new("sqlite_init_failed", error.to_string()))?;
    initialize_schema(&connection)?;
    super::retired_knowledge::remove(&mut connection, path)?;
    Ok(connection)
}

fn initialize_schema(connection: &Connection) -> Result<(), AppError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS migration_state (key TEXT PRIMARY KEY, completed_at TEXT NOT NULL);
         INSERT INTO schema_version(version) SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM schema_version);
         CREATE TABLE IF NOT EXISTS pages (
           id TEXT PRIMARY KEY, title TEXT NOT NULL, parent_id TEXT REFERENCES pages(id) ON DELETE CASCADE,
           kind TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
           trashed_at TEXT, sort_order INTEGER NOT NULL DEFAULT 0,
           properties_json TEXT NOT NULL DEFAULT '{}', database_id TEXT, revision INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_pages_parent_order ON pages(parent_id, sort_order, title);
         CREATE INDEX IF NOT EXISTS idx_pages_trash ON pages(trashed_at);
         CREATE TABLE IF NOT EXISTS blocks (
           id TEXT PRIMARY KEY, page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
           parent_block_id TEXT REFERENCES blocks(id) ON DELETE CASCADE,
           position INTEGER NOT NULL, block_type TEXT NOT NULL, text TEXT NOT NULL DEFAULT '',
           attrs_json TEXT NOT NULL DEFAULT '{}'
         );
         CREATE INDEX IF NOT EXISTS idx_blocks_page_position ON blocks(page_id, parent_block_id, position);
         CREATE TABLE IF NOT EXISTS page_links (
           source_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
           target_id TEXT REFERENCES pages(id) ON DELETE SET NULL, label TEXT NOT NULL,
           PRIMARY KEY(source_id, label, target_id)
         );
         CREATE INDEX IF NOT EXISTS idx_page_links_target ON page_links(target_id);
         CREATE TABLE IF NOT EXISTS page_revisions (
           id TEXT PRIMARY KEY, page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
           revision INTEGER NOT NULL, action TEXT NOT NULL, timestamp TEXT NOT NULL,
           title TEXT NOT NULL, document_json TEXT NOT NULL,
           UNIQUE(page_id, revision, action)
         );
         CREATE INDEX IF NOT EXISTS idx_page_revisions_page ON page_revisions(page_id, revision DESC);
         CREATE TABLE IF NOT EXISTS attachments (
           id TEXT PRIMARY KEY, page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
           relative_path TEXT NOT NULL UNIQUE, mime TEXT NOT NULL, size INTEGER NOT NULL,
           created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS databases (
           id TEXT PRIMARY KEY, page_id TEXT NOT NULL UNIQUE REFERENCES pages(id) ON DELETE CASCADE,
           title TEXT NOT NULL DEFAULT '', config_json TEXT NOT NULL DEFAULT '{}'
         );
         CREATE TABLE IF NOT EXISTS database_fields (
           id TEXT PRIMARY KEY, database_id TEXT NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
           name TEXT NOT NULL, field_type TEXT NOT NULL, position INTEGER NOT NULL, config_json TEXT NOT NULL DEFAULT '{}'
         );
         CREATE TABLE IF NOT EXISTS database_rows (
           id TEXT PRIMARY KEY, database_id TEXT NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
           page_id TEXT REFERENCES pages(id) ON DELETE CASCADE, position INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS property_values (
           row_id TEXT NOT NULL REFERENCES database_rows(id) ON DELETE CASCADE,
           field_id TEXT NOT NULL REFERENCES database_fields(id) ON DELETE CASCADE,
           value_json TEXT NOT NULL, PRIMARY KEY(row_id, field_id)
         );
         CREATE TABLE IF NOT EXISTS conversations (
           id TEXT PRIMARY KEY, thread_id TEXT, title TEXT NOT NULL, scope_json TEXT NOT NULL DEFAULT '{}',
           created_at TEXT NOT NULL, updated_at TEXT NOT NULL, active_turn_id TEXT
         );
         CREATE TABLE IF NOT EXISTS conversation_events (
           id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
           sequence INTEGER NOT NULL, event_type TEXT NOT NULL, payload_json TEXT NOT NULL, created_at TEXT NOT NULL,
           UNIQUE(conversation_id, sequence)
         );
         CREATE TABLE IF NOT EXISTS conversation_attachments (
           id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
           name TEXT NOT NULL DEFAULT '', relative_path TEXT NOT NULL, mime TEXT NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts USING fts5(block_id UNINDEXED, page_id UNINDEXED, text, tokenize='trigram');
         UPDATE schema_version SET version = 2 WHERE version < 2;"
    ).map_err(|error| AppError::new("sqlite_schema_failed", error.to_string()))?;
    ensure_column(connection, "conversations", "active_turn_id", "TEXT")?;
    ensure_column(
        connection,
        "conversation_attachments",
        "name",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    let version: i64 = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .map_err(|error| AppError::new("sqlite_schema_failed", error.to_string()))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::new(
            "sqlite_schema_too_new",
            "数据库版本高于当前应用支持版本",
        ));
    }
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AppError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| AppError::new("sqlite_schema_failed", error.to_string()))?;
    let exists = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| AppError::new("sqlite_schema_failed", error.to_string()))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    if !exists {
        connection
            .execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))
            .map_err(|error| AppError::new("sqlite_schema_failed", error.to_string()))?;
    }
    Ok(())
}

fn migrate_or_initialize(vault: &Path, database_path: &Path) -> Result<(), AppError> {
    let legacy_files = markdown_files(vault)?;
    let legacy_database_files = legacy_database_files(vault)?;
    if legacy_files.is_empty() && legacy_database_files.is_empty() {
        let connection = open_connection(database_path)?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|error| AppError::new("sqlite_init_failed", error.to_string()))?;
        return Ok(());
    }

    let data_root = database_path
        .parent()
        .ok_or_else(|| AppError::new("migration_failed", "数据库路径没有父目录"))?;
    let backup = data_root
        .join("migration-backups")
        .join(format!("{}-{}", now(), Uuid::new_v4()));
    crate::portable::copy_tree_verified(vault, &backup)
        .map_err(|error| AppError::new("migration_backup_failed", error.to_string()))?;
    let temporary = data_root.join(format!(".qingyu.sqlite3.migrating-{}", Uuid::new_v4()));
    let legacy_assets = vault.join(".noteapp").join("assets");
    let attachments = data_root.join("attachments");
    let temporary_attachments =
        data_root.join(format!(".attachments.migrating-{}", Uuid::new_v4()));
    let result = (|| {
        if legacy_assets.exists() && !attachments.exists() {
            crate::portable::copy_tree_verified(&legacy_assets, &temporary_attachments)
                .map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))?;
        }
        let mut connection = open_connection(&temporary)?;
        connection.execute_batch("PRAGMA defer_foreign_keys=ON;")
            .map_err(|error| AppError::new("migration_failed", error.to_string()))?;
        let transaction = connection
            .transaction()
            .map_err(|error| AppError::new("migration_failed", error.to_string()))?;
        let mut pages = Vec::new();
        for path in &legacy_files {
            let text = fs::read_to_string(path)
                .map_err(|error| AppError::new("migration_read_failed", error.to_string()))?;
            pages.push(parse_markdown(&text)?);
        }
        for page in &pages {
            insert_page(&transaction, page, 0)?;
        }
        let (definition_count, field_count) =
            migrate_database_definitions(&transaction, &legacy_database_files)?;
        let (record_count, property_value_count) =
            migrate_database_records(&transaction, &pages)?;
        let attachment_count = migrate_attachment_metadata(&transaction, &legacy_assets, &pages)?;
        let page_ids = pages.iter().map(|page| page.id.clone()).collect::<Vec<_>>();
        let history_count = migrate_legacy_page_histories(&transaction, vault, &page_ids)?;
        for page in &pages {
            replace_blocks_and_links(&transaction, page, &content_document(page, 0))?;
            record_revision(&transaction, page, 0, "migration")?;
        }
        validate_import(
            &transaction,
            pages.len(),
            definition_count,
            field_count,
            record_count,
            property_value_count,
            attachment_count,
            pages.len() + history_count,
        )?;
        transaction.execute("INSERT OR REPLACE INTO migration_state(key,completed_at) VALUES('legacy_databases',?1)", [now()]).map_err(sqlite_write_error)?;
        transaction.execute("INSERT OR REPLACE INTO migration_state(key,completed_at) VALUES('legacy_page_history',?1)", [now()]).map_err(sqlite_write_error)?;
        transaction
            .commit()
            .map_err(|error| AppError::new("migration_failed", error.to_string()))?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|error| AppError::new("migration_failed", error.to_string()))?;
        drop(connection);
        switch_migration_files(
            &temporary,
            database_path,
            &temporary_attachments,
            &attachments,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = remove_sqlite_files(&temporary);
        let _ = fs::remove_dir_all(&temporary_attachments);
    }
    result
}

pub(super) fn switch_migration_files(
    temporary_database: &Path,
    database_path: &Path,
    temporary_attachments: &Path,
    attachments: &Path,
) -> Result<(), AppError> {
    move_path_cross_device(temporary_database, database_path)?;
    if temporary_attachments.exists() {
        if let Err(error) = move_path_cross_device(temporary_attachments, attachments) {
            // 正式库此前不存在，附件切换失败时必须撤销刚落位的数据库。
            remove_sqlite_files(database_path).map_err(|rollback| {
                AppError::new(
                    "migration_rollback_failed",
                    format!("附件切换失败：{}；数据库回滚失败：{rollback}", error.message),
                )
            })?;
            return Err(error);
        }
    }
    Ok(())
}

fn move_path_cross_device(source: &Path, destination: &Path) -> Result<(), AppError> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(17) => {
            if source.is_dir() {
                crate::portable::copy_tree_verified(source, destination)
                    .map_err(|copy_error| AppError::new("migration_switch_failed", copy_error.to_string()))?;
                fs::remove_dir_all(source)
                    .map_err(|remove_error| AppError::new("migration_switch_failed", remove_error.to_string()))?;
            } else {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|create_error| AppError::new("migration_switch_failed", create_error.to_string()))?;
                }
                fs::copy(source, destination)
                    .map_err(|copy_error| AppError::new("migration_switch_failed", copy_error.to_string()))?;
                fs::remove_file(source)
                    .map_err(|remove_error| AppError::new("migration_switch_failed", remove_error.to_string()))?;
            }
            Ok(())
        }
        Err(error) => Err(AppError::new("migration_switch_failed", error.to_string())),
    }
}

fn remove_sqlite_files(database_path: &Path) -> std::io::Result<()> {
    for path in [
        database_path.to_path_buf(),
        sqlite_sidecar(database_path, "-wal"),
        sqlite_sidecar(database_path, "-shm"),
    ] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn sqlite_sidecar(database_path: &Path, suffix: &str) -> PathBuf {
    let mut value = database_path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    fn visit(root: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                if entry.file_name() != ".noteapp" {
                    visit(&path, output)?;
                }
            } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
                output.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, &mut files)
        .map_err(|error| AppError::new("migration_read_failed", error.to_string()))?;
    let trash = root.join(".noteapp").join("trash");
    if trash.exists() {
        visit(&trash, &mut files)
            .map_err(|error| AppError::new("migration_read_failed", error.to_string()))?;
    }
    files.sort();
    Ok(files)
}

fn legacy_database_files(vault: &Path) -> Result<Vec<PathBuf>, AppError> {
    let directory = vault.join(".noteapp").join("databases");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut files = fs::read_dir(directory)
        .map_err(|error| AppError::new("migration_read_failed", error.to_string()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| AppError::new("migration_read_failed", error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("json"));
    files.sort();
    Ok(files)
}

fn validate_import(
    transaction: &Transaction<'_>,
    expected_pages: usize,
    expected_databases: usize,
    expected_fields: usize,
    expected_records: usize,
    expected_property_values: usize,
    expected_attachments: usize,
    expected_revisions: usize,
) -> Result<(), AppError> {
    let page_count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM pages", [], |row| row.get(0))
        .map_err(|error| AppError::new("migration_validation_failed", error.to_string()))?;
    let broken_parents: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM pages child LEFT JOIN pages parent ON parent.id = child.parent_id WHERE child.parent_id IS NOT NULL AND parent.id IS NULL",
        [], |row| row.get(0))
        .map_err(|error| AppError::new("migration_validation_failed", error.to_string()))?;
    let broken_links: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM page_links link LEFT JOIN pages target ON target.id = link.target_id WHERE link.target_id IS NOT NULL AND target.id IS NULL",
        [], |row| row.get(0))
        .map_err(|error| AppError::new("migration_validation_failed", error.to_string()))?;
    let database_count: i64 = transaction.query_row("SELECT COUNT(*) FROM databases", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    let field_count: i64 = transaction.query_row("SELECT COUNT(*) FROM database_fields", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    let record_count: i64 = transaction.query_row("SELECT COUNT(*) FROM database_rows", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    let property_value_count: i64 = transaction.query_row("SELECT COUNT(*) FROM property_values", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    let attachment_count: i64 = transaction.query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    let revision_count: i64 = transaction.query_row("SELECT COUNT(*) FROM page_revisions", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    let foreign_key_errors: i64 = transaction.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| row.get(0)).map_err(sqlite_read_error)?;
    if page_count != expected_pages as i64
        || database_count != expected_databases as i64
        || field_count != expected_fields as i64
        || record_count != expected_records as i64
        || property_value_count != expected_property_values as i64
        || attachment_count != expected_attachments as i64
        || revision_count != expected_revisions as i64
        || broken_parents != 0
        || broken_links != 0
        || foreign_key_errors != 0
    {
        return Err(AppError::new(
            "migration_validation_failed",
            "迁移后的页面、数据库、父子关系或外键校验失败",
        ));
    }
    Ok(())
}

fn migrate_legacy_page_histories(
    transaction: &Transaction<'_>,
    vault: &Path,
    page_ids: &[String],
) -> Result<usize, AppError> {
    let history_root = vault.join(".noteapp").join("history");
    if !history_root.exists() {
        return Ok(0);
    }
    let mut migrated = 0;
    for page_id in page_ids {
        let path = history_root.join(format!("{page_id}.jsonl"));
        if !path.exists() {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| AppError::new("migration_history_read_failed", error.to_string()))?;
        let entries = source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<PageHistoryEntry>(line).map_err(|error| {
                    AppError::new("migration_invalid_history", error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, entry) in entries.iter().enumerate() {
            if entry.page_id != *page_id {
                return Err(AppError::new(
                    "migration_invalid_history",
                    format!("历史记录 {} 的页面 ID 不匹配", entry.id),
                ));
            }
            let revision = index as i64 - entries.len() as i64;
            let document = PageDocument {
                page_id: entry.page_id.clone(),
                title: entry.title.clone(),
                blocks: vec![BlockNode {
                    id: format!("legacy-history-{}", entry.id),
                    block_type: "markdown".into(),
                    text: entry.content.clone(),
                    attrs: BTreeMap::new(),
                    children: Vec::new(),
                }],
                revision,
            };
            transaction.execute(
                "INSERT INTO page_revisions(id,page_id,revision,action,timestamp,title,document_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![entry.id,entry.page_id,revision,entry.action,entry.timestamp,entry.title,serde_json::to_string(&document).unwrap_or_else(|_| "{}".into())],
            ).map_err(sqlite_write_error)?;
            migrated += 1;
        }
    }
    Ok(migrated)
}

fn backfill_legacy_page_histories(
    connection: &mut Connection,
    vault: &Path,
) -> Result<(), AppError> {
    let completed = connection
        .query_row(
            "SELECT 1 FROM migration_state WHERE key='legacy_page_history'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(sqlite_read_error)?
        .is_some();
    if completed {
        return Ok(());
    }
    let page_ids = {
        let mut statement = connection
            .prepare("SELECT id FROM pages ORDER BY id")
            .map_err(sqlite_read_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sqlite_read_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_read_error)?;
        rows
    };
    let transaction = connection.transaction().map_err(sqlite_write_error)?;
    migrate_legacy_page_histories(&transaction, vault, &page_ids)?;
    transaction.execute(
        "INSERT INTO migration_state(key,completed_at) VALUES('legacy_page_history',?1)",
        [now()],
    ).map_err(sqlite_write_error)?;
    transaction.commit().map_err(sqlite_write_error)
}

fn migrate_database_definitions(
    transaction: &Transaction<'_>,
    files: &[PathBuf],
) -> Result<(usize, usize), AppError> {
    let mut field_count = 0;
    for path in files {
        let definition: DatabaseDefinition = serde_json::from_slice(&fs::read(path).map_err(|error| AppError::new("migration_read_failed", error.to_string()))?)
            .map_err(|error| AppError::new("migration_invalid_database", error.to_string()))?;
        field_count += definition.fields.len();
        write_database_definition_tx(transaction, &definition)?;
    }
    Ok((files.len(), field_count))
}

pub(crate) fn write_database_definition_tx(transaction: &Transaction<'_>, definition: &DatabaseDefinition) -> Result<(), AppError> {
    let title = transaction
        .query_row(
            "SELECT title FROM pages WHERE id=?1",
            [&definition.page_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sqlite_read_error)?
        .unwrap_or_default();
    transaction.execute("INSERT INTO databases(id,page_id,title,config_json) VALUES(?1,?2,?3,?4)", params![definition.id,definition.page_id,title,serde_json::to_string(definition).unwrap()]).map_err(sqlite_write_error)?;
    for (position, field) in definition.fields.iter().enumerate() {
        transaction.execute("INSERT INTO database_fields(id,database_id,name,field_type,position,config_json) VALUES(?1,?2,?3,?4,?5,?6)", params![field.id,definition.id,field.name,serde_json::to_string(&field.field_type).unwrap(),position as i64,serde_json::to_string(&field.options).unwrap()]).map_err(sqlite_write_error)?;
    }
    Ok(())
}

fn migrate_database_records(
    transaction: &Transaction<'_>,
    pages: &[Page],
) -> Result<(usize, usize), AppError> {
    let mut record_count = 0;
    let mut property_value_count = 0;
    for page in pages.iter().filter(|page| page.kind == PageKind::DatabaseRecord) {
        let database_id = page.database_id.as_deref().ok_or_else(|| AppError::new("migration_invalid_record", "数据库记录缺少 database_id"))?;
        transaction.execute("INSERT INTO database_rows(id,database_id,page_id,position) VALUES(?1,?2,?3,?4)", params![page.id,database_id,page.id,page.sort_order]).map_err(sqlite_write_error)?;
        for (field_id, value) in &page.properties {
            transaction.execute("INSERT INTO property_values(row_id,field_id,value_json) VALUES(?1,?2,?3)", params![page.id,field_id,serde_json::to_string(value).unwrap()]).map_err(sqlite_write_error)?;
            property_value_count += 1;
        }
        record_count += 1;
    }
    Ok((record_count, property_value_count))
}

fn migrate_attachment_metadata(
    transaction: &Transaction<'_>,
    assets: &Path,
    pages: &[Page],
) -> Result<usize, AppError> {
    if !assets.exists() { return Ok(0); }
    let page_ids = pages.iter().map(|page| page.id.as_str()).collect::<std::collections::HashSet<_>>();
    let mut count = 0;
    for page_directory in fs::read_dir(assets).map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))? {
        let page_directory = page_directory.map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))?;
        if !page_directory.file_type().map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))?.is_dir() { continue; }
        let page_id = page_directory.file_name().to_string_lossy().to_string();
        if !page_ids.contains(page_id.as_str()) { continue; }
        for file in fs::read_dir(page_directory.path()).map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))? {
            let file = file.map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))?;
            if !file.file_type().map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))?.is_file() { continue; }
            let name = file.file_name().to_string_lossy().to_string();
            let extension = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("");
            let size = file.metadata().map_err(|error| AppError::new("migration_attachment_failed", error.to_string()))?.len() as i64;
            transaction.execute(
                "INSERT INTO attachments(id,page_id,relative_path,mime,size,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![Uuid::new_v4().to_string(),page_id,format!("attachments/{page_id}/{name}"),attachment_mime(extension),size,now()],
            ).map_err(sqlite_write_error)?;
            count += 1;
        }
    }
    Ok(count)
}

pub(crate) fn shared_vault_lock(root: &Path) -> Arc<Mutex<()>> {
    let key = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let locks = VAULT_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

impl PageRepository {
    pub fn default_root() -> PathBuf {
        PortablePaths::current()
            .map(|paths| paths.vault_root)
            .unwrap_or_else(|_| PathBuf::from("data").join("vault"))
    }

    pub fn open(root: PathBuf) -> Result<Self, AppError> {
        validate_vault_root(&root)?;
        fs::create_dir_all(&root).map_err(|e| AppError::new("vault_init_failed", e.to_string()))?;
        crate::transfer::recover_interrupted_import(&root)?;
        let database_path = if root.file_name().and_then(|value| value.to_str()) == Some("vault") {
            root.parent().unwrap_or(&root).join("qingyu.sqlite3")
        } else {
            root.join("qingyu.sqlite3")
        };
        if !database_path.exists() {
            migrate_or_initialize(&root, &database_path)?;
        }
        fs::create_dir_all(root.parent().unwrap_or(&root).join("attachments"))
            .map_err(|e| AppError::new("vault_init_failed", e.to_string()))?;
        let mut connection = open_connection(&database_path)?;
        backfill_legacy_page_histories(&mut connection, &root)?;
        let write_lock = shared_vault_lock(&root);
        Ok(Self {
            root,
            database_path,
            connection: Arc::new(Mutex::new(connection)),
            write_lock,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub(crate) fn with_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let _guard = self.lock_writes()?;
        let mut connection = self.connection.lock().map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        let value = operation(&transaction)?;
        transaction.commit().map_err(sqlite_write_error)?;
        Ok(value)
    }

    pub(crate) fn with_write_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let _guard = self.lock_writes()?;
        operation()
    }

    pub(crate) fn create_typed_tx(
        &self, transaction: &Transaction<'_>, title: String, parent_id: Option<String>,
        content: String, kind: PageKind, database_id: Option<String>, properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let parent_kind = if let Some(parent) = parent_id.as_deref() {
            Some(transaction.query_row("SELECT kind FROM pages WHERE id=?1 AND trashed_at IS NULL", [parent], |row| row.get::<_,String>(0)).optional().map_err(sqlite_read_error)?
                .ok_or_else(|| AppError::new("page_not_found", "父页面不存在"))?)
        } else { None };
        validate_parent_kind(&kind, database_id.as_deref(), parent_id.as_deref(), parent_kind.as_deref().map(kind_from_str))?;
        let timestamp = now();
        let page = Page { id: Uuid::new_v4().to_string(), title, parent_id, kind, content, created_at: timestamp.clone(), updated_at: timestamp, trashed_at: None, sort_order: 0, properties, database_id };
        insert_page(transaction, &page, 0)?;
        replace_blocks_and_links(transaction, &page, &content_document(&page, 0))?;
        record_revision(transaction, &page, 0, "create")?;
        Ok(page)
    }

    pub(crate) fn update_properties_tx(
        &self, transaction: &Transaction<'_>, mut page: Page, title: String, properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        page.title = title;
        page.properties = properties;
        page.updated_at = now();
        let revision = current_revision(transaction, &page.id)? + 1;
        update_page_row(transaction, &page, revision)?;
        let document = content_document(&page, revision);
        replace_blocks_and_links(transaction, &page, &document)?;
        record_revision_document(transaction, &page, &document, "update_properties")?;
        Ok(page)
    }

    pub fn snapshot_database(&self, destination: &Path) -> Result<(), AppError> {
        if destination.exists() {
            fs::remove_file(destination)
                .map_err(|error| AppError::new("sqlite_backup_failed", error.to_string()))?;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| AppError::new("sqlite_backup_failed", error.to_string()))?;
        }
        let source = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let mut target = Connection::open(destination)
            .map_err(|error| AppError::new("sqlite_backup_failed", error.to_string()))?;
        rusqlite::backup::Backup::new(&source, &mut target)
            .and_then(|backup| backup.run_to_completion(64, Duration::from_millis(5), None))
            .map_err(|error| AppError::new("sqlite_backup_failed", error.to_string()))
    }

    pub fn restore_database(&self, source_path: &Path) -> Result<(), AppError> {
        let source = Connection::open(source_path)
            .map_err(|error| AppError::new("sqlite_restore_failed", error.to_string()))?;
        let mut target = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        rusqlite::backup::Backup::new(&source, &mut target)
            .and_then(|backup| backup.run_to_completion(64, Duration::from_millis(5), None))
            .map_err(|error| AppError::new("sqlite_restore_failed", error.to_string()))?;
        initialize_schema(&target)?;
        super::retired_knowledge::remove(&mut target, &self.database_path)
    }

    pub fn list(&self, include_trash: bool) -> Result<Vec<PageSummary>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let sql = if include_trash {
            "SELECT id,title,parent_id,kind,sort_order,trashed_at FROM pages ORDER BY sort_order,title"
        } else {
            "SELECT id,title,parent_id,kind,sort_order,trashed_at FROM pages WHERE trashed_at IS NULL ORDER BY sort_order,title"
        };
        let mut statement = connection.prepare(sql).map_err(sqlite_read_error)?;
        let rows = statement
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                Ok(PageSummary {
                    path: format!("{}-{}.md", safe_name(&title), id),
                    id,
                    title,
                    parent_id: row.get(2)?,
                    kind: kind_from_str(&row.get::<_, String>(3)?),
                    sort_order: row.get(4)?,
                    trashed_at: row.get(5)?,
                })
            })
            .map_err(sqlite_read_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_read_error)
    }

    fn walk(
        &self,
        dir: &Path,
        out: &mut Vec<PageSummary>,
        include_trash: bool,
    ) -> Result<(), AppError> {
        for entry in
            fs::read_dir(dir).map_err(|e| AppError::new("vault_read_failed", e.to_string()))?
        {
            let entry = entry.map_err(|e| AppError::new("vault_read_failed", e.to_string()))?;
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(".noteapp") {
                continue;
            }
            if path.is_dir() {
                self.walk(&path, out, include_trash)?;
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let page = self.read_file(&path)?;
            if include_trash || page.trashed_at.is_none() {
                out.push(self.summary(&page, &path));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str, include_trash: bool) -> Result<Page, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let page = load_page(&connection, id)?;
        if !include_trash && page.trashed_at.is_some() {
            return Err(AppError::new("page_not_found", "页面不在当前视图"));
        }
        Ok(page)
    }

    pub fn create(
        &self,
        title: String,
        parent_id: Option<String>,
        content: String,
    ) -> Result<Page, AppError> {
        self.create_typed(
            title,
            parent_id,
            content,
            PageKind::Page,
            None,
            BTreeMap::new(),
        )
    }

    pub fn create_typed(
        &self,
        title: String,
        parent_id: Option<String>,
        content: String,
        kind: PageKind,
        database_id: Option<String>,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        self.create_typed_unlocked(title, parent_id, content, kind, database_id, properties)
    }

    pub(crate) fn create_typed_unlocked(
        &self,
        title: String,
        parent_id: Option<String>,
        content: String,
        kind: PageKind,
        database_id: Option<String>,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let parent_kind = parent_id.as_deref().map(|parent| self.get(parent, false)).transpose()?.map(|page| page.kind);
        validate_parent_kind(&kind, database_id.as_deref(), parent_id.as_deref(), parent_kind)?;
        let now = now();
        let page = Page {
            id: Uuid::new_v4().to_string(),
            title,
            parent_id,
            kind,
            content,
            created_at: now.clone(),
            updated_at: now,
            trashed_at: None,
            sort_order: 0,
            properties,
            database_id,
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        insert_page(&transaction, &page, 0)?;
        replace_blocks_and_links(&transaction, &page, &content_document(&page, 0))?;
        transaction
            .execute(
                "UPDATE page_links SET target_id=?1 WHERE target_id IS NULL AND lower(label)=lower(?2)",
                params![page.id, page.title],
            )
            .map_err(sqlite_write_error)?;
        record_revision(&transaction, &page, 0, "create")?;
        transaction.commit().map_err(sqlite_write_error)?;
        Ok(page)
    }

    pub(crate) fn update_properties_unlocked(
        &self,
        id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        page.title = title;
        page.properties = properties;
        page.updated_at = now();
        self.persist_page_update(&page, "update_properties")?;
        Ok(page)
    }

    pub fn update(&self, id: &str, title: String, content: String) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        self.update_unlocked(id, title, content)
    }

    pub fn update_checked(
        &self,
        id: &str,
        title: String,
        content: String,
        expected_title: &str,
        expected_content: &str,
    ) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        let current = self.get(id, false)?;
        if current.title != expected_title || current.content != expected_content {
            return Err(AppError::new(
                "page_write_conflict",
                "磁盘页面已发生变化，未覆盖现有内容",
            ));
        }
        self.update_unlocked(id, title, content)
    }

    fn update_unlocked(&self, id: &str, title: String, content: String) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        page.title = title;
        page.content = content;
        page.updated_at = now();
        self.persist_page_update(&page, "update")?;
        Ok(page)
    }

    pub fn store_asset(
        &self,
        page_id: &str,
        original_name: &str,
        bytes: &[u8],
    ) -> Result<String, AppError> {
        let _guard = self.lock_writes()?;
        if bytes.len() > 100 * 1024 * 1024 {
            return Err(AppError::new("asset_too_large", "附件不能超过 100 MiB"));
        }
        let name_path = Path::new(original_name);
        if original_name.is_empty()
            || name_path.components().count() != 1
            || name_path.file_name().and_then(|name| name.to_str()) != Some(original_name)
        {
            return Err(AppError::new("asset_invalid_name", "附件文件名不安全"));
        }
        let extension = attachment_extension(original_name, bytes)
            .ok_or_else(|| AppError::new("asset_invalid_type", "不支持此附件格式"))?;
        let _ = self.get(page_id, false)?;
        let directory = self
            .root
            .parent()
            .unwrap_or(&self.root)
            .join("attachments")
            .join(page_id);
        fs::create_dir_all(&directory)
            .map_err(|error| AppError::new("asset_write_failed", error.to_string()))?;
        let file_name = format!("{}.{}", Uuid::new_v4(), extension);
        let destination = directory.join(&file_name);
        let temporary = directory.join(format!("{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, bytes)
            .map_err(|error| AppError::new("asset_write_failed", error.to_string()))?;
        fs::rename(&temporary, &destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            AppError::new("asset_write_failed", error.to_string())
        })?;
        let mime = attachment_mime(extension);
        let relative = format!("attachments/{page_id}/{file_name}");
        if let Err(error) = self.connection.lock().map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?
            .execute("INSERT INTO attachments(id,page_id,relative_path,mime,size,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![Uuid::new_v4().to_string(), page_id, relative, mime, bytes.len() as i64, now()]) {
            let _ = fs::remove_file(&destination);
            return Err(sqlite_write_error(error));
        }
        Ok(format!("/.noteapp/assets/{page_id}/{file_name}"))
    }

    pub fn read_asset(&self, relative_path: &str) -> Result<(String, Vec<u8>), AppError> {
        if relative_path.contains('\\') {
            return Err(AppError::new("asset_invalid_path", "附件路径不安全"));
        }
        let parts = relative_path
            .trim_start_matches('/')
            .split('/')
            .collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != ".noteapp" || parts[1] != "assets" {
            return Err(AppError::new("asset_invalid_path", "附件路径不安全"));
        }
        let page_id = parts[2];
        let file = Path::new(parts[3]);
        let stem = file
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::new("asset_invalid_path", "附件路径不安全"))?;
        Uuid::parse_str(stem).map_err(|_| AppError::new("asset_invalid_path", "附件路径不安全"))?;
        let _ = self.get(page_id, false)?;
        let bytes = fs::read(
            self.root
                .parent()
                .unwrap_or(&self.root)
                .join("attachments")
                .join(page_id)
                .join(parts[3]),
        )
        .map_err(|error| AppError::new("asset_not_found", error.to_string()))?;
        if bytes.len() > 100 * 1024 * 1024 {
            return Err(AppError::new("asset_too_large", "附件不能超过 100 MiB"));
        }
        let extension = parts[3]
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .ok_or_else(|| AppError::new("asset_invalid_type", "附件格式无效"))?;
        let mime = attachment_mime(extension);
        Ok((mime.to_owned(), bytes))
    }

    pub fn move_page(
        &self,
        id: &str,
        parent_id: Option<String>,
        sort_order: i64,
    ) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        let mut page = self.get(id, false)?;
        if parent_id.as_deref() == Some(id) || self.is_descendant(parent_id.as_deref(), id)? {
            return Err(AppError::new(
                "page_invalid_parent",
                "不能将页面移动到自身或子页面下",
            ));
        }
        let parent_kind = parent_id.as_deref().map(|parent| self.get(parent, false)).transpose()?.map(|parent| parent.kind);
        validate_parent_kind(&page.kind, page.database_id.as_deref(), parent_id.as_deref(), parent_kind)?;
        page.parent_id = parent_id;
        let mut sibling_ids = self
            .list(false)?
            .into_iter()
            .filter(|sibling| sibling.id != id && sibling.parent_id == page.parent_id)
            .map(|sibling| sibling.id)
            .collect::<Vec<_>>();
        let target_index = sort_order.max(0) as usize;
        let target_index = target_index.min(sibling_ids.len());
        sibling_ids.insert(target_index, id.to_owned());
        page.sort_order = target_index as i64;
        page.updated_at = now();
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        for (index, sibling_id) in sibling_ids.iter().enumerate() {
            transaction
                .execute(
                    "UPDATE pages SET sort_order=?1,updated_at=?2 WHERE id=?3",
                    params![index as i64, page.updated_at, sibling_id],
                )
                .map_err(sqlite_write_error)?;
        }
        transaction
            .execute(
                "UPDATE pages SET parent_id=?1 WHERE id=?2",
                params![page.parent_id, id],
            )
            .map_err(sqlite_write_error)?;
        let revision = current_revision(&transaction, id)? + 1;
        transaction
            .execute(
                "UPDATE pages SET revision=?1 WHERE id=?2",
                params![revision, id],
            )
            .map_err(sqlite_write_error)?;
        record_revision(&transaction, &page, revision, "move")?;
        transaction.commit().map_err(sqlite_write_error)?;
        Ok(page)
    }

    pub fn trash(&self, id: &str) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        self.trash_unlocked(id)
    }

    pub fn trash_recursive(&self, id: &str) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        let page = self.get(id, false)?;
        let timestamp = now();
        let mut connection = self.connection.lock().map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        transaction.execute(
            "WITH RECURSIVE subtree(id) AS (SELECT ?1 UNION ALL SELECT pages.id FROM pages JOIN subtree ON pages.parent_id=subtree.id) UPDATE pages SET trashed_at=?2,updated_at=?2 WHERE id IN (SELECT id FROM subtree)",
            params![id, timestamp],
        ).map_err(sqlite_write_error)?;
        transaction.commit().map_err(sqlite_write_error)?;
        Ok(page)
    }

    pub fn trash_checked(&self, id: &str, expected: &Page) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        let current = self.get(id, false)?;
        if &current != expected {
            return Err(AppError::new(
                "approval_stale_data",
                "页面在审批期间已发生变化，请重新发起操作",
            ));
        }
        self.trash_unlocked(id)
    }

    fn trash_unlocked(&self, id: &str) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        page.trashed_at = Some(now());
        page.updated_at = now();
        self.persist_page_update(&page, "trash")?;
        Ok(page)
    }

    pub(crate) fn rollback_created_unlocked(&self, id: &str) -> Result<(), AppError> {
        self.connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?
            .execute("DELETE FROM pages WHERE id=?1", [id])
            .map_err(|error| AppError::new("page_rollback_failed", error.to_string()))?;
        let assets = self
            .root
            .parent()
            .unwrap_or(&self.root)
            .join("attachments")
            .join(id);
        if assets.exists() {
            fs::remove_dir_all(assets)
                .map_err(|error| AppError::new("page_rollback_failed", error.to_string()))?;
        }
        Ok(())
    }

    pub fn restore(&self, id: &str) -> Result<Page, AppError> {
        let _guard = self.lock_writes()?;
        let mut page = self.get(id, true)?;
        if page.trashed_at.is_none() {
            return Err(AppError::new("page_not_trashed", "页面不在回收站"));
        }
        page.trashed_at = None;
        page.updated_at = now();
        self.persist_page_update(&page, "restore")?;
        Ok(page)
    }

    pub fn purge(&self, id: &str) -> Result<(), AppError> {
        let _guard = self.lock_writes()?;
        let target = self.get(id, true)?;
        if target.trashed_at.is_none() {
            return Err(AppError::new("page_not_trashed", "页面必须先移入回收站"));
        }
        let ids = {
            let connection = self
                .connection
                .lock()
                .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
            let mut statement = connection.prepare("WITH RECURSIVE descendants(id) AS (SELECT ?1 UNION ALL SELECT pages.id FROM pages JOIN descendants ON pages.parent_id=descendants.id) SELECT id FROM descendants")
                .map_err(sqlite_read_error)?;
            let rows = statement
                .query_map([id], |row| row.get::<_, String>(0))
                .map_err(sqlite_read_error)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(sqlite_read_error)?
        };
        for page_id in &ids {
            let assets = self
                .root
                .parent()
                .unwrap_or(&self.root)
                .join("attachments")
                .join(&page_id);
            if assets.exists() {
                fs::remove_dir_all(assets)
                    .map_err(|error| AppError::new("page_purge_failed", error.to_string()))?;
            }
        }
        self.connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?
            .execute("DELETE FROM pages WHERE id=?1", [id])
            .map_err(sqlite_write_error)?;
        Ok(())
    }

    pub fn history(&self, id: &str) -> Result<Vec<PageHistoryEntry>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let mut statement = connection.prepare("SELECT id,page_id,action,timestamp,title,document_json FROM page_revisions WHERE page_id=?1 ORDER BY revision, rowid")
            .map_err(sqlite_read_error)?;
        let rows = statement
            .query_map([id], |row| {
                let document_json: String = row.get(5)?;
                let document: PageDocument =
                    serde_json::from_str(&document_json).unwrap_or(PageDocument {
                        page_id: row.get(1)?,
                        title: row.get(4)?,
                        blocks: Vec::new(),
                        revision: 0,
                    });
                Ok(PageHistoryEntry {
                    id: row.get(0)?,
                    page_id: row.get(1)?,
                    action: row.get(2)?,
                    timestamp: row.get(3)?,
                    title: row.get(4)?,
                    content: document_to_content(&document),
                })
            })
            .map_err(sqlite_read_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_read_error)
    }

    pub fn links(&self, id: &str) -> Result<Vec<PageLink>, AppError> {
        self.query_links(
            "SELECT source_id,target_id,label FROM page_links WHERE source_id=?1 ORDER BY label",
            id,
        )
    }

    pub fn backlinks(&self, id: &str) -> Result<Vec<PageLink>, AppError> {
        self.query_links("SELECT source_id,target_id,label FROM page_links WHERE target_id=?1 ORDER BY source_id,label", id)
    }

    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, AppError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let fts_query = format!("\"{}\"", query.trim().replace('"', "\"\""));
        let sql = if query.trim().chars().count() >= 3 {
            "SELECT p.id,p.title,p.parent_id,p.kind,p.sort_order,p.trashed_at,b.text
             FROM blocks_fts f JOIN blocks b ON b.id=f.block_id JOIN pages p ON p.id=f.page_id
             WHERE blocks_fts MATCH ?1 AND p.trashed_at IS NULL
             UNION SELECT p.id,p.title,p.parent_id,p.kind,p.sort_order,p.trashed_at,''
             FROM pages p WHERE p.trashed_at IS NULL AND lower(p.title) LIKE lower(?2)
             ORDER BY 5,2 LIMIT 50"
        } else {
            "SELECT p.id,p.title,p.parent_id,p.kind,p.sort_order,p.trashed_at,b.text
             FROM blocks b JOIN pages p ON p.id=b.page_id
             WHERE p.trashed_at IS NULL AND lower(b.text) LIKE lower(?2) AND ?1 IS NOT NULL
             UNION SELECT p.id,p.title,p.parent_id,p.kind,p.sort_order,p.trashed_at,''
             FROM pages p WHERE p.trashed_at IS NULL AND lower(p.title) LIKE lower(?2)
             ORDER BY 5,2 LIMIT 50"
        };
        let mut statement = connection.prepare(sql).map_err(sqlite_read_error)?;
        let like = format!("%{}%", query.trim());
        let rows = statement
            .query_map(params![fts_query, like], |row| {
                let title: String = row.get(1)?;
                let text: String = row.get(6)?;
                let id: String = row.get(0)?;
                Ok(SearchResult {
                    page: PageSummary {
                        path: format!("{}-{}.md", safe_name(&title), id),
                        id,
                        title: title.clone(),
                        parent_id: row.get(2)?,
                        kind: kind_from_str(&row.get::<_, String>(3)?),
                        sort_order: row.get(4)?,
                        trashed_at: row.get(5)?,
                    },
                    snippet: search_snippet(&format!("{title}
{text}"), query).unwrap_or(title),
                })
            })
            .map_err(sqlite_read_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_read_error)
    }

    pub fn open_bundle(&self, id: &str) -> Result<PageOpenBundle, AppError> {
        let page = self.get(id, false)?;
        let document = self.load_document(&page)?;
        Ok(PageOpenBundle {
            page,
            document,
            links: self.links(id)?,
            backlinks: self.backlinks(id)?,
        })
    }

    pub fn save_document(
        &self,
        document: PageDocument,
        expected_revision: i64,
    ) -> Result<PageDocument, AppError> {
        let _guard = self.lock_writes()?;
        let mut page = self.get(&document.page_id, false)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        let current = current_revision(&transaction, &document.page_id)?;
        if current != expected_revision {
            return Err(AppError::new(
                "page_write_conflict",
                "页面版本已变化，未覆盖现有内容",
            ));
        }
        let mut saved = document;
        saved.revision = current + 1;
        page.title = saved.title.clone();
        page.content = document_to_content(&saved);
        page.updated_at = now();
        update_page_row(&transaction, &page, saved.revision)?;
        replace_blocks_and_links(&transaction, &page, &saved)?;
        record_revision_document(&transaction, &page, &saved, "save")?;
        transaction.commit().map_err(sqlite_write_error)?;
        Ok(saved)
    }

    pub(crate) fn restore_document(
        &self,
        document: PageDocument,
    ) -> Result<PageDocument, AppError> {
        let mut page = self.get(&document.page_id, true)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        let mut restored = document;
        restored.revision = current_revision(&transaction, &restored.page_id)? + 1;
        page.title = restored.title.clone();
        page.content = document_to_content(&restored);
        page.trashed_at = None;
        page.updated_at = now();
        update_page_row(&transaction, &page, restored.revision)?;
        replace_blocks_and_links(&transaction, &page, &restored)?;
        record_revision_document(&transaction, &page, &restored, "restore")?;
        transaction.commit().map_err(sqlite_write_error)?;
        Ok(restored)
    }

    fn load_document(&self, page: &Page) -> Result<PageDocument, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        load_document(&connection, page)
    }

    fn persist_page_update(&self, page: &Page, action: &str) -> Result<(), AppError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let transaction = connection.transaction().map_err(sqlite_write_error)?;
        let revision = current_revision(&transaction, &page.id)? + 1;
        update_page_row(&transaction, page, revision)?;
        let document = content_document(page, revision);
        replace_blocks_and_links(&transaction, page, &document)?;
        record_revision_document(&transaction, page, &document, action)?;
        transaction.commit().map_err(sqlite_write_error)
    }

    fn query_links(&self, sql: &str, id: &str) -> Result<Vec<PageLink>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::new("sqlite_lock_failed", "数据库锁已损坏"))?;
        let mut statement = connection.prepare(sql).map_err(sqlite_read_error)?;
        let rows = statement
            .query_map([id], |row| {
                Ok(PageLink {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    label: row.get(2)?,
                })
            })
            .map_err(sqlite_read_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_read_error)
    }

    fn summary(&self, page: &Page, path: &Path) -> PageSummary {
        PageSummary {
            id: page.id.clone(),
            title: page.title.clone(),
            parent_id: page.parent_id.clone(),
            kind: page.kind.clone(),
            path: path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/"),
            sort_order: page.sort_order,
            trashed_at: page.trashed_at.clone(),
        }
    }
    fn path_for(&self, page: &Page) -> Result<PathBuf, AppError> {
        let mut dir = self.root.clone();
        if let Some(parent) = page.parent_id.as_deref() {
            let parent_path = self.find_path(parent)?;
            dir = parent_path.with_extension("");
            fs::create_dir_all(&dir)
                .map_err(|e| AppError::new("vault_write_failed", e.to_string()))?;
        }
        self.available_path(&dir, page)
    }

    fn available_path(&self, dir: &Path, page: &Page) -> Result<PathBuf, AppError> {
        let base = safe_name(&page.title);
        for suffix in 1_u32.. {
            let name = if suffix == 1 {
                base.clone()
            } else {
                format!("{base} ({suffix})")
            };
            let candidate = dir.join(format!("{name}.md"));
            if !candidate.exists() {
                return Ok(candidate);
            }
            if self
                .read_file(&candidate)
                .map(|existing| existing.id == page.id)
                .unwrap_or(false)
            {
                return Ok(candidate);
            }
        }
        unreachable!()
    }
    fn find_path(&self, id: &str) -> Result<PathBuf, AppError> {
        if self
            .root
            .join(".noteapp")
            .join("trash")
            .join(format!("{}.md", id))
            .exists()
        {
            return Ok(self
                .root
                .join(".noteapp")
                .join("trash")
                .join(format!("{}.md", id)));
        }
        let mut all = Vec::new();
        self.walk(&self.root, &mut all, true)?;
        all.into_iter()
            .find(|p| p.id == id)
            .map(|p| self.root.join(&p.path))
            .ok_or_else(|| AppError::new("page_not_found", "页面不存在"))
    }
    fn lock_writes(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.write_lock
            .lock()
            .map_err(|_| AppError::new("vault_write_lock_failed", "笔记库写入锁已损坏"))
    }
    fn read_file(&self, path: &Path) -> Result<Page, AppError> {
        let text = fs::read_to_string(path)
            .map_err(|e| AppError::new("page_read_failed", e.to_string()))?;
        parse_markdown(&text)
    }
    fn write_atomic(&self, path: &Path, page: &Page) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::new("page_write_failed", e.to_string()))?;
        }
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, serialize_markdown(page))
            .map_err(|e| AppError::new("page_write_failed", e.to_string()))?;
        fs::rename(tmp, path).map_err(|e| AppError::new("page_write_failed", e.to_string()))
    }
    fn record(&self, page: &Page, action: &str) -> Result<(), AppError> {
        let entry = PageHistoryEntry {
            id: Uuid::new_v4().to_string(),
            page_id: page.id.clone(),
            action: action.to_owned(),
            timestamp: now(),
            title: page.title.clone(),
            content: page.content.clone(),
        };
        let path = self
            .root
            .join(".noteapp")
            .join("history")
            .join(format!("{}.jsonl", page.id));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| AppError::new("history_write_failed", e.to_string()))?;
        writeln!(file, "{}", serde_json::to_string(&entry).unwrap())
            .map_err(|e| AppError::new("history_write_failed", e.to_string()))
    }
    fn is_descendant(&self, candidate: Option<&str>, ancestor: &str) -> Result<bool, AppError> {
        let mut current = candidate.map(str::to_owned);
        while let Some(id) = current {
            if id == ancestor {
                return Ok(true);
            }
            current = self.get(&id, false)?.parent_id;
        }
        Ok(false)
    }

    fn rewrite_links(
        &self,
        target_id: &str,
        old_title: &str,
        new_title: &str,
    ) -> Result<(), AppError> {
        let pages = self.list(false)?;
        let old_plain = format!("[[{}]]", old_title);
        let old_stable = format!("[[{}|{}]]", old_title, target_id);
        let replacement = format!("[[{}|{}]]", new_title, target_id);
        for summary in pages {
            if summary.id == target_id {
                continue;
            }
            let mut page = self.get(&summary.id, false)?;
            let updated = page
                .content
                .replace(&old_stable, &replacement)
                .replace(&old_plain, &replacement);
            if updated != page.content {
                page.content = updated;
                page.updated_at = now();
                let path = self.find_path(&page.id)?;
                self.write_atomic(&path, &page)?;
            }
        }
        Ok(())
    }

    fn rename_child_directory(
        &self,
        old_path: &Path,
        new_path: &Path,
        _old_title: &str,
        _new_title: &str,
    ) -> Result<(), AppError> {
        let old_dir = old_path.with_extension("");
        let new_dir = new_path.with_extension("");
        if old_dir.exists() && old_dir != new_dir && !new_dir.exists() {
            fs::rename(old_dir, new_dir)
                .map_err(|e| AppError::new("page_move_failed", e.to_string()))?;
        }
        Ok(())
    }
}

fn sqlite_read_error(error: rusqlite::Error) -> AppError {
    AppError::new("sqlite_read_failed", error.to_string())
}

fn sqlite_write_error(error: rusqlite::Error) -> AppError {
    AppError::new("sqlite_write_failed", error.to_string())
}

fn kind_as_str(kind: &PageKind) -> &'static str {
    match kind {
        PageKind::Page => "page",
        PageKind::Database => "database",
        PageKind::DatabaseRecord => "databaseRecord",
    }
}

fn kind_from_str(value: &str) -> PageKind {
    match value {
        "database" => PageKind::Database,
        "databaseRecord" => PageKind::DatabaseRecord,
        _ => PageKind::Page,
    }
}

fn validate_parent_kind(
    kind: &PageKind,
    database_id: Option<&str>,
    parent_id: Option<&str>,
    parent_kind: Option<PageKind>,
) -> Result<(), AppError> {
    if *kind == PageKind::DatabaseRecord {
        if parent_kind != Some(PageKind::Database)
            || parent_id.is_none()
            || database_id != parent_id
        {
            return Err(AppError::new(
                "page_invalid_parent_kind",
                "数据库条目必须保留在所属数据库中",
            ));
        }
        return Ok(());
    }
    if parent_kind == Some(PageKind::Database) {
        return Err(AppError::new(
            "page_invalid_parent_kind",
            "数据库只能直接包含条目页面",
        ));
    }
    Ok(())
}

fn insert_page(transaction: &Transaction<'_>, page: &Page, revision: i64) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO pages(id,title,parent_id,kind,created_at,updated_at,trashed_at,sort_order,properties_json,database_id,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![page.id,page.title,page.parent_id,kind_as_str(&page.kind),page.created_at,page.updated_at,page.trashed_at,page.sort_order,serde_json::to_string(&page.properties).unwrap_or_else(|_| "{}".into()),page.database_id,revision]
    ).map_err(sqlite_write_error)?;
    Ok(())
}

fn update_page_row(
    transaction: &Transaction<'_>,
    page: &Page,
    revision: i64,
) -> Result<(), AppError> {
    transaction.execute(
        "UPDATE pages SET title=?1,parent_id=?2,kind=?3,updated_at=?4,trashed_at=?5,sort_order=?6,properties_json=?7,database_id=?8,revision=?9 WHERE id=?10",
        params![page.title,page.parent_id,kind_as_str(&page.kind),page.updated_at,page.trashed_at,page.sort_order,serde_json::to_string(&page.properties).unwrap_or_else(|_| "{}".into()),page.database_id,revision,page.id]
    ).map_err(sqlite_write_error)?;
    Ok(())
}

fn load_page(connection: &Connection, id: &str) -> Result<Page, AppError> {
    connection.query_row(
        "SELECT id,title,parent_id,kind,created_at,updated_at,trashed_at,sort_order,properties_json,database_id FROM pages WHERE id=?1",
        [id], |row| {
            let properties: String = row.get(8)?;
            Ok(Page { id: row.get(0)?, title: row.get(1)?, parent_id: row.get(2)?, kind: kind_from_str(&row.get::<_,String>(3)?), content: String::new(), created_at: row.get(4)?, updated_at: row.get(5)?, trashed_at: row.get(6)?, sort_order: row.get(7)?, properties: serde_json::from_str(&properties).unwrap_or_default(), database_id: row.get(9)? })
        }).optional().map_err(sqlite_read_error)?
        .ok_or_else(|| AppError::new("page_not_found", "页面不存在"))
        .and_then(|mut page| { page.content = document_to_content(&load_document(connection, &page)?); Ok(page) })
}

fn content_document(page: &Page, revision: i64) -> PageDocument {
    PageDocument {
        page_id: page.id.clone(),
        title: page.title.clone(),
        blocks: vec![BlockNode {
            id: Uuid::new_v4().to_string(),
            block_type: "markdown".into(),
            text: page.content.clone(),
            attrs: BTreeMap::new(),
            children: Vec::new(),
        }],
        revision,
    }
}

fn document_to_content(document: &PageDocument) -> String {
    fn append(blocks: &[BlockNode], output: &mut Vec<String>) {
        for block in blocks {
            output.push(block.text.clone());
            append(&block.children, output);
        }
    }
    let mut parts = Vec::new();
    append(&document.blocks, &mut parts);
    parts.join("
")
}

fn load_document(connection: &Connection, page: &Page) -> Result<PageDocument, AppError> {
    let revision: i64 = connection
        .query_row(
            "SELECT revision FROM pages WHERE id=?1",
            [&page.id],
            |row| row.get(0),
        )
        .map_err(sqlite_read_error)?;
    let mut statement = connection.prepare("SELECT id,parent_block_id,position,block_type,text,attrs_json FROM blocks WHERE page_id=?1 ORDER BY parent_block_id,position,id").map_err(sqlite_read_error)?;
    let rows = statement
        .query_map([&page.id], |row| {
            let attrs: String = row.get(5)?;
            Ok((
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
                BlockNode {
                    id: row.get(0)?,
                    block_type: row.get(3)?,
                    text: row.get(4)?,
                    attrs: serde_json::from_str(&attrs).unwrap_or_default(),
                    children: Vec::new(),
                },
            ))
        })
        .map_err(sqlite_read_error)?;
    let mut grouped: HashMap<Option<String>, Vec<(i64, BlockNode)>> = HashMap::new();
    for row in rows {
        let (parent_id, position, block) = row.map_err(sqlite_read_error)?;
        grouped.entry(parent_id).or_default().push((position, block));
    }

    fn assemble(
        parent_id: Option<String>,
        grouped: &mut HashMap<Option<String>, Vec<(i64, BlockNode)>>,
    ) -> Vec<BlockNode> {
        let mut siblings = grouped.remove(&parent_id).unwrap_or_default();
        siblings.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.id.cmp(&right.1.id)));
        siblings
            .into_iter()
            .map(|(_, mut block)| {
                block.children = assemble(Some(block.id.clone()), grouped);
                block
            })
            .collect()
    }
    let blocks = assemble(None, &mut grouped);
    Ok(PageDocument {
        page_id: page.id.clone(),
        title: page.title.clone(),
        blocks,
        revision,
    })
}

fn current_revision(transaction: &Transaction<'_>, id: &str) -> Result<i64, AppError> {
    transaction
        .query_row("SELECT revision FROM pages WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .map_err(sqlite_read_error)
}

fn replace_blocks_and_links(
    transaction: &Transaction<'_>,
    page: &Page,
    document: &PageDocument,
) -> Result<(), AppError> {
    transaction
        .execute("DELETE FROM blocks_fts WHERE page_id=?1", [&page.id])
        .map_err(sqlite_write_error)?;
    transaction
        .execute("DELETE FROM blocks WHERE page_id=?1", [&page.id])
        .map_err(sqlite_write_error)?;
    transaction
        .execute("DELETE FROM page_links WHERE source_id=?1", [&page.id])
        .map_err(sqlite_write_error)?;
    insert_block_tree(transaction, &page.id, None, &document.blocks)?;
    let content = document_to_content(document);
    let mut link_tokens = extract_link_tokens(&content);
    collect_block_links(&document.blocks, &mut link_tokens);
    for (label, explicit_target) in link_tokens {
        let target = if let Some(target) = explicit_target {
            transaction
                .query_row("SELECT id FROM pages WHERE id=?1", [&target], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
                .map_err(sqlite_read_error)?
        } else {
            transaction
                .query_row(
                    "SELECT id FROM pages WHERE lower(title)=lower(?1) LIMIT 1",
                    [&label],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(sqlite_read_error)?
        };
        transaction
            .execute(
                "INSERT OR IGNORE INTO page_links(source_id,target_id,label) VALUES(?1,?2,?3)",
                params![page.id, target, label],
            )
            .map_err(sqlite_write_error)?;
    }
    Ok(())
}

fn insert_block_tree(
    transaction: &Transaction<'_>,
    page_id: &str,
    parent_id: Option<&str>,
    blocks: &[BlockNode],
) -> Result<(), AppError> {
    for (position, block) in blocks.iter().enumerate() {
        transaction.execute("INSERT INTO blocks(id,page_id,parent_block_id,position,block_type,text,attrs_json) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![block.id,page_id,parent_id,position as i64,block.block_type,block.text,serde_json::to_string(&block.attrs).unwrap_or_else(|_| "{}".into())]).map_err(sqlite_write_error)?;
        transaction
            .execute(
                "INSERT INTO blocks_fts(block_id,page_id,text) VALUES(?1,?2,?3)",
                params![block.id, page_id, block.text],
            )
            .map_err(sqlite_write_error)?;
        insert_block_tree(transaction, page_id, Some(&block.id), &block.children)?;
    }
    Ok(())
}

fn collect_block_links(blocks: &[BlockNode], output: &mut Vec<(String, Option<String>)>) {
    for block in blocks {
        collect_rich_text_links(&block.attrs, output);
        collect_block_links(&block.children, output);
    }
}

fn collect_rich_text_links(
    attrs: &BTreeMap<String, Value>,
    output: &mut Vec<(String, Option<String>)>,
) {
    fn visit(
        value: &Value,
        inherited_text: Option<&str>,
        output: &mut Vec<(String, Option<String>)>,
    ) {
        match value {
            Value::Object(object) => {
                let text = object
                    .get("text")
                    .and_then(Value::as_str)
                    .or(inherited_text);
                if let Some(href) = object.get("href").and_then(Value::as_str) {
                    if let Some(target) = href
                        .strip_prefix("qingyu://page/")
                        .and_then(|value| value.split('#').next())
                    {
                        if !target.is_empty() {
                            output
                                .push((text.unwrap_or("页面").to_owned(), Some(target.to_owned())));
                        }
                    }
                }
                for child in object.values() {
                    visit(child, text, output);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, inherited_text, output);
                }
            }
            _ => {}
        }
    }
    visit(
        &Value::Object(attrs.clone().into_iter().collect()),
        None,
        output,
    );
}

fn record_revision(
    transaction: &Transaction<'_>,
    page: &Page,
    revision: i64,
    action: &str,
) -> Result<(), AppError> {
    record_revision_document(transaction, page, &content_document(page, revision), action)
}

fn record_revision_document(
    transaction: &Transaction<'_>,
    page: &Page,
    document: &PageDocument,
    action: &str,
) -> Result<(), AppError> {
    transaction.execute("INSERT INTO page_revisions(id,page_id,revision,action,timestamp,title,document_json) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![Uuid::new_v4().to_string(),page.id,document.revision,action,now(),page.title,serde_json::to_string(document).unwrap_or_else(|_| "{}".into())]).map_err(sqlite_write_error)?;
    Ok(())
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
fn safe_name(title: &str) -> String {
    let value: String = title
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = value.trim().trim_matches('.');
    if trimmed.is_empty() {
        "未命名页面".to_owned()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn validate_vault_root(root: &Path) -> Result<(), AppError> {
    let sensitive = root.components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            value.as_str(),
            ".codex" | "ccswitch" | "cc-switch" | "cc_switch"
        )
    });
    if sensitive {
        return Err(AppError::new(
            "vault_sensitive_path",
            "不能把 Codex 或 CCSwitch 配置目录用作笔记库",
        ));
    }
    Ok(())
}

fn search_snippet(haystack: &str, query: &str) -> Option<String> {
    let original = haystack.chars().collect::<Vec<_>>();
    let mut lowered = Vec::new();
    let mut source_indexes = Vec::new();
    for (index, character) in original.iter().enumerate() {
        for lowered_character in character.to_lowercase() {
            lowered.push(lowered_character);
            source_indexes.push(index);
        }
    }
    let needle = query.to_lowercase().chars().collect::<Vec<_>>();
    if needle.is_empty() || needle.len() > lowered.len() {
        return None;
    }
    let match_start = lowered
        .windows(needle.len())
        .position(|window| window == needle)?;
    let source_start = source_indexes[match_start];
    let source_end = source_indexes[match_start + needle.len() - 1] + 1;
    let snippet_start = source_start.saturating_sub(48);
    let snippet_end = (source_end + 80).min(original.len());
    Some(
        original[snippet_start..snippet_end]
            .iter()
            .collect::<String>()
            .replace('\n', " "),
    )
}
fn extract_link_tokens(content: &str) -> Vec<(String, Option<String>)> {
    let mut found = Vec::new();
    let mut rest = content;
    let mut offset = 0;
    while let Some(start) = rest.find("[[") {
        let tail = &rest[start + 2..];
        if let Some(end) = tail.find("]]") {
            let raw = tail[..end].trim();
            let (label, target) = raw
                .split_once('|')
                .map(|(label, target)| (label.trim(), Some(target.trim().to_owned())))
                .unwrap_or((raw, None));
            if !label.is_empty() {
                found.push((offset + start, label.to_owned(), target));
            }
            offset += start + 2 + end + 2;
            rest = &tail[end + 2..];
        } else {
            break;
        }
    }

    let marker = "](qingyu://page/";
    let mut search_from = 0;
    while let Some(relative) = content[search_from..].find(marker) {
        let marker_start = search_from + relative;
        let Some(label_start) = content[..marker_start].rfind('[') else {
            search_from = marker_start + marker.len();
            continue;
        };
        if label_start > 0 && content.as_bytes()[label_start - 1] == b'!' {
            search_from = marker_start + marker.len();
            continue;
        }
        let target_start = marker_start + marker.len();
        let Some(target_end) = content[target_start..].find(')') else {
            break;
        };
        let label = content[label_start + 1..marker_start].trim();
        let target = content[target_start..target_start + target_end]
            .split('#')
            .next()
            .unwrap_or("")
            .trim();
        if !label.is_empty() && !target.is_empty() {
            found.push((label_start, label.to_owned(), Some(target.to_owned())));
        }
        search_from = target_start + target_end + 1;
    }
    found.sort_by_key(|item| item.0);
    found
        .into_iter()
        .map(|(_, label, target)| (label, target))
        .collect()
}
fn serialize_markdown(page: &Page) -> String {
    let kind = match page.kind {
        PageKind::Page => "page",
        PageKind::Database => "database",
        PageKind::DatabaseRecord => "databaseRecord",
    };
    let properties = serde_json::to_string(&page.properties).unwrap_or_else(|_| "{}".to_owned());
    format!("---
id: {}
title: {}
parent_id: {}
kind: {}
created_at: {}
updated_at: {}
trashed_at: {}
sort_order: {}
database_id: {}
properties: {}
---
{}", page.id, page.title.replace('\n', " "), page.parent_id.as_deref().unwrap_or(""), kind, page.created_at, page.updated_at, page.trashed_at.as_deref().unwrap_or(""), page.sort_order, page.database_id.as_deref().unwrap_or(""), properties, page.content)
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn attachment_extension<'a>(name: &'a str, bytes: &[u8]) -> Option<&'a str> {
    if let Some(detected) = image_extension(bytes) {
        return Some(detected);
    }
    let extension = Path::new(name).extension()?.to_str()?;
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "pdf"
            | "doc"
            | "docx"
            | "ppt"
            | "pptx"
            | "xls"
            | "xlsx"
            | "md"
            | "txt"
            | "csv"
            | "json"
            | "zip"
            | "7z"
            | "rar"
    )
    .then_some(extension)
}

fn attachment_mime(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "md" | "csv" => "text/plain",
        "json" => "application/json",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}
fn parse_markdown(text: &str) -> Result<Page, AppError> {
    let (front, content) = text
        .strip_prefix("---
")
        .and_then(|s| s.split_once("
---
"))
        .ok_or_else(|| AppError::new("page_invalid_metadata", "Markdown 页面缺少元数据"))?;
    let mut fields = HashMap::new();
    for line in front.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim(), value.trim());
        }
    }
    let required = |key: &str| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| AppError::new("page_invalid_metadata", format!("缺少字段 {}", key)))
    };
    Ok(Page {
        id: required("id")?.to_owned(),
        title: required("title")?.to_owned(),
        parent_id: fields
            .get("parent_id")
            .filter(|v| !v.is_empty())
            .map(|v| (*v).to_owned()),
        kind: match fields.get("kind").copied().unwrap_or("page") {
            "database" => PageKind::Database,
            "databaseRecord" => PageKind::DatabaseRecord,
            _ => PageKind::Page,
        },
        content: content.to_owned(),
        created_at: required("created_at")?.to_owned(),
        updated_at: required("updated_at")?.to_owned(),
        trashed_at: fields
            .get("trashed_at")
            .filter(|v| !v.is_empty())
            .map(|v| (*v).to_owned()),
        sort_order: fields
            .get("sort_order")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        properties: fields
            .get("properties")
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default(),
        database_id: fields
            .get("database_id")
            .filter(|value| !value.is_empty())
            .map(|value| (*value).to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_names_are_sanitized() {
        assert_eq!(safe_name("a:b?.md"), "a_b_.md");
    }
    #[test]
    fn links_are_extracted() {
        assert_eq!(
            extract_link_tokens("[[A]] and [[ B ]] and [新标题](qingyu://page/page-uuid)."),
            vec![
                ("A".into(), None),
                ("B".into(), None),
                ("新标题".into(), Some("page-uuid".into()))
            ]
        );
    }

    #[test]
    fn stable_page_links_survive_target_rename_and_move() {
        let root = std::env::temp_dir().join(format!("qingyu-link-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let folder = repo.create("目录".into(), None, String::new()).unwrap();
        let target = repo.create("旧标题".into(), None, String::new()).unwrap();
        let source = repo
            .create(
                "来源".into(),
                None,
                format!("[旧标题](qingyu://page/{})", target.id),
            )
            .unwrap();

        assert_eq!(
            repo.links(&source.id).unwrap()[0].target_id,
            Some(target.id.clone())
        );
        assert_eq!(repo.backlinks(&target.id).unwrap()[0].source_id, source.id);
        repo.update(&target.id, "新标题".into(), String::new())
            .unwrap();
        repo.move_page(&target.id, Some(folder.id), 0).unwrap();
        assert_eq!(
            repo.links(&source.id).unwrap()[0].target_id,
            Some(target.id)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nested_blocks_round_trip_through_sqlite() {
        let root = std::env::temp_dir().join(format!("qingyu-nested-blocks-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let page = repo.create("嵌套区块".into(), None, String::new()).unwrap();
        let mut document = repo.open_bundle(&page.id).unwrap().document;
        document.blocks = vec![BlockNode {
            id: "parent".into(),
            block_type: "bulletList".into(),
            text: "父区块".into(),
            attrs: BTreeMap::new(),
            children: vec![BlockNode {
                id: "child".into(),
                block_type: "listItem".into(),
                text: "子区块".into(),
                attrs: BTreeMap::new(),
                children: vec![BlockNode {
                    id: "grandchild".into(),
                    block_type: "paragraph".into(),
                    text: "深层正文".into(),
                    attrs: BTreeMap::new(),
                    children: Vec::new(),
                }],
            }],
        }];

        repo.save_document(document, 0).unwrap();
        let reopened = repo.open_bundle(&page.id).unwrap().document;

        assert_eq!(reopened.blocks[0].children[0].id, "child");
        assert_eq!(reopened.blocks[0].children[0].children[0].text, "深层正文");
        assert_eq!(repo.search("深层正文").unwrap()[0].page.id, page.id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_jsonl_page_history_is_migrated_into_revisions() {
        let base = std::env::temp_dir().join(format!("qingyu-history-migration-{}", Uuid::new_v4()));
        let root = base.join("vault");
        let history_root = root.join(".noteapp").join("history");
        fs::create_dir_all(&history_root).unwrap();
        let page = Page {
            id: "legacy-page".into(),
            title: "当前标题".into(),
            parent_id: None,
            kind: PageKind::Page,
            content: "当前正文".into(),
            created_at: "100".into(),
            updated_at: "300".into(),
            trashed_at: None,
            sort_order: 0,
            properties: BTreeMap::new(),
            database_id: None,
        };
        fs::write(root.join("当前标题.md"), serialize_markdown(&page)).unwrap();
        let entries = [
            PageHistoryEntry {
                id: "history-1".into(),
                page_id: page.id.clone(),
                action: "create".into(),
                timestamp: "100".into(),
                title: "旧标题".into(),
                content: "第一版".into(),
            },
            PageHistoryEntry {
                id: "history-2".into(),
                page_id: page.id.clone(),
                action: "update".into(),
                timestamp: "200".into(),
                title: "当前标题".into(),
                content: "第二版".into(),
            },
        ];
        let jsonl = entries
            .iter()
            .map(|entry| serde_json::to_string(entry).unwrap())
            .collect::<Vec<_>>()
            .join("
");
        fs::write(history_root.join("legacy-page.jsonl"), format!("{jsonl}
")).unwrap();

        let repo = PageRepository::open(root).unwrap();
        let history = repo.history("legacy-page").unwrap();

        assert_eq!(history.len(), 3);
        assert_eq!(history[0], entries[0]);
        assert_eq!(history[1], entries[1]);
        assert_eq!(history[2].action, "migration");
        assert_eq!(history[2].content, "当前正文");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn page_lifecycle_preserves_uuid_and_supports_trash_restore() {
        let root = std::env::temp_dir().join(format!("qingyu-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let parent = repo.create("项目".into(), None, "[[记录]]".into()).unwrap();
        let child = repo
            .create("记录".into(), Some(parent.id.clone()), "正文".into())
            .unwrap();
        let updated = repo
            .update(&child.id, "记录2".into(), "更新".into())
            .unwrap();
        assert_eq!(updated.id, child.id);
        assert_eq!(repo.get(&child.id, false).unwrap().title, "记录2");
        assert!(repo
            .search("更新")
            .unwrap()
            .iter()
            .any(|result| result.page.id == child.id));
        assert_eq!(repo.backlinks(&child.id).unwrap().len(), 1);
        repo.trash(&child.id).unwrap();
        assert_eq!(repo.list(true).unwrap().len(), 2);
        repo.restore(&child.id).unwrap();
        assert!(repo.get(&child.id, false).is_ok());
        assert!(repo.history(&child.id).unwrap().len() >= 4);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pages_support_deep_nesting_and_reject_parent_cycles() {
        let root = std::env::temp_dir().join(format!("qingyu-nested-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let level_one = repo.create("一级".into(), None, String::new()).unwrap();
        let level_two = repo
            .create("二级".into(), Some(level_one.id.clone()), String::new())
            .unwrap();
        let level_three = repo
            .create("三级".into(), Some(level_two.id.clone()), String::new())
            .unwrap();
        let level_four = repo
            .create("四级".into(), Some(level_three.id.clone()), "正文".into())
            .unwrap();

        assert_eq!(repo.get(&level_four.id, false).unwrap().content, "正文");
        let error = repo
            .move_page(&level_one.id, Some(level_four.id), 0)
            .unwrap_err();
        assert_eq!(error.code, "page_invalid_parent");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn database_parent_rejects_regular_pages_and_nested_databases() {
        let root = std::env::temp_dir().join(format!("qingyu-kind-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let database = repo
            .create_typed(
                "数据库".into(),
                None,
                String::new(),
                PageKind::Database,
                None,
                BTreeMap::new(),
            )
            .unwrap();
        let regular = repo
            .create("非法页面".into(), Some(database.id.clone()), String::new())
            .unwrap_err();
        assert_eq!(regular.code, "page_invalid_parent_kind");
        let nested = repo
            .create_typed(
                "非法数据库".into(),
                Some(database.id),
                String::new(),
                PageKind::Database,
                None,
                BTreeMap::new(),
            )
            .unwrap_err();
        assert_eq!(nested.code, "page_invalid_parent_kind");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn move_preserves_database_parent_rules_and_record_membership() {
        let root = std::env::temp_dir().join(format!("qingyu-move-kind-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let database = repo.create_typed("数据库".into(), None, String::new(), PageKind::Database, None, BTreeMap::new()).unwrap();
        let other_database = repo.create_typed("其他数据库".into(), None, String::new(), PageKind::Database, None, BTreeMap::new()).unwrap();
        let record = repo.create_typed("记录".into(), Some(database.id.clone()), String::new(), PageKind::DatabaseRecord, Some(database.id.clone()), BTreeMap::new()).unwrap();
        let page = repo.create("普通页".into(), None, String::new()).unwrap();

        assert_eq!(repo.move_page(&page.id, Some(database.id.clone()), 0).unwrap_err().code, "page_invalid_parent_kind");
        assert_eq!(repo.move_page(&other_database.id, Some(database.id.clone()), 0).unwrap_err().code, "page_invalid_parent_kind");
        assert_eq!(repo.move_page(&record.id, None, 0).unwrap_err().code, "page_invalid_parent_kind");
        assert_eq!(repo.move_page(&record.id, Some(other_database.id), 0).unwrap_err().code, "page_invalid_parent_kind");

        let child_page = repo.create("条目子页".into(), Some(record.id.clone()), String::new()).unwrap();
        let child_database = repo.create_typed("条目数据库".into(), Some(record.id), String::new(), PageKind::Database, None, BTreeMap::new()).unwrap();
        assert_eq!(child_page.kind, PageKind::Page);
        assert_eq!(child_database.kind, PageKind::Database);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opening_a_vault_preserves_existing_manifest_settings() {
        let root = std::env::temp_dir().join(format!("qingyu-vault-open-test-{}", Uuid::new_v4()));
        let noteapp = root.join(".noteapp");
        fs::create_dir_all(&noteapp).unwrap();
        let manifest = r#"{"version":1,"backupRoot":"D:\\QingYuBackups"}
"#;
        fs::write(noteapp.join("vault.json"), manifest).unwrap();

        PageRepository::open(root.clone()).unwrap();

        assert_eq!(
            fs::read_to_string(noteapp.join("vault.json")).unwrap(),
            manifest
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sensitive_configuration_tree_cannot_be_opened_as_a_vault() {
        let root = std::env::temp_dir()
            .join(format!("qingyu-sensitive-test-{}", Uuid::new_v4()))
            .join(".codex")
            .join("notes");

        let error = match PageRepository::open(root) {
            Ok(_) => panic!("敏感配置目录不应被打开为笔记库"),
            Err(error) => error,
        };

        assert_eq!(error.code, "vault_sensitive_path");
    }

    #[test]
    fn pages_with_the_same_title_coexist_in_one_parent() {
        let root =
            std::env::temp_dir().join(format!("qingyu-duplicate-title-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();

        let first = repo.create("同名".into(), None, "第一篇".into()).unwrap();
        let second = repo.create("同名".into(), None, "第二篇".into()).unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(repo.get(&first.id, false).unwrap().content, "第一篇");
        assert_eq!(repo.get(&second.id, false).unwrap().content, "第二篇");
        let paths = repo
            .list(false)
            .unwrap()
            .into_iter()
            .filter(|page| page.title == "同名")
            .map(|page| page.path)
            .collect::<Vec<_>>();
        assert_eq!(paths.len(), 2);
        assert_ne!(paths[0], paths[1]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_builds_unicode_snippets_on_character_boundaries() {
        let root =
            std::env::temp_dir().join(format!("qingyu-unicode-search-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let page = repo
            .create(
                "中文搜索".into(),
                None,
                format!("{}é关键字后文", "羽".repeat(16)),
            )
            .unwrap();

        let result = repo.search("关键字").unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].page.id, page.id);
        assert!(result[0].snippet.contains("关键字"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_returns_only_the_first_fifty_results() {
        let root =
            std::env::temp_dir().join(format!("qingyu-search-limit-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        for index in 0..60 {
            repo.create(
                format!("性能页面 {index:02}"),
                None,
                "统一性能关键字".into(),
            )
            .unwrap();
        }

        assert_eq!(repo.search("性能关键字").unwrap().len(), 50);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn moving_page_normalizes_sibling_order_at_target_index() {
        let root = std::env::temp_dir().join(format!("qingyu-order-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        repo.create("A".into(), None, String::new()).unwrap();
        repo.create("B".into(), None, String::new()).unwrap();
        let third = repo.create("C".into(), None, String::new()).unwrap();

        repo.move_page(&third.id, None, 0).unwrap();

        let pages = repo.list(false).unwrap();
        assert_eq!(
            pages
                .iter()
                .map(|page| page.title.as_str())
                .collect::<Vec<_>>(),
            vec!["C", "A", "B"]
        );
        assert_eq!(
            pages.iter().map(|page| page.sort_order).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn permanent_delete_requires_trash_and_removes_descendant_data() {
        let root = std::env::temp_dir().join(format!("qingyu-purge-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let parent = repo.create("父页面".into(), None, "正文".into()).unwrap();
        let child = repo
            .create("子页面".into(), Some(parent.id.clone()), "子正文".into())
            .unwrap();
        repo.store_asset(
            &child.id,
            "image.png",
            &[137, 80, 78, 71, 13, 10, 26, 10, 1],
        )
        .unwrap();

        let error = repo.purge(&parent.id).unwrap_err();
        assert_eq!(error.code, "page_not_trashed");
        repo.trash(&parent.id).unwrap();
        repo.purge(&parent.id).unwrap();

        assert!(repo.get(&parent.id, true).is_err());
        assert!(repo.get(&child.id, true).is_err());
        assert!(!root
            .join(".noteapp")
            .join("assets")
            .join(&child.id)
            .exists());
        assert!(!root
            .join(".noteapp")
            .join("history")
            .join(format!("{}.jsonl", child.id))
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn checked_update_rejects_a_changed_disk_version() {
        let root = std::env::temp_dir().join(format!("qingyu-cas-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let page = repo.create("标题".into(), None, "读取版本".into()).unwrap();
        repo.update(&page.id, "标题".into(), "外部版本".into())
            .unwrap();

        let error = repo
            .update_checked(
                &page.id,
                "本地标题".into(),
                "本地版本".into(),
                "标题",
                "读取版本",
            )
            .unwrap_err();

        assert_eq!(error.code, "page_write_conflict");
        assert_eq!(repo.get(&page.id, false).unwrap().content, "外部版本");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn image_assets_are_validated_and_stored_inside_the_page_directory() {
        let root = std::env::temp_dir().join(format!("qingyu-asset-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let page = repo.create("图片".into(), None, String::new()).unwrap();
        let png = [137, 80, 78, 71, 13, 10, 26, 10, 0];

        let relative = repo.store_asset(&page.id, "image.png", &png).unwrap();
        assert!(relative.starts_with(&format!("/.noteapp/assets/{}/", page.id)));
        assert!(relative.ends_with(".png"));
        let file_name = Path::new(&relative).file_name().unwrap();
        assert_eq!(
            fs::read(
                root.parent()
                    .unwrap()
                    .join("attachments")
                    .join(&page.id)
                    .join(file_name)
            )
            .unwrap(),
            png
        );
        let (mime, loaded) = repo.read_asset(&relative).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(loaded, png);
        assert_eq!(
            repo.read_asset("/.noteapp/assets/../vault.json")
                .unwrap_err()
                .code,
            "asset_invalid_path"
        );

        assert_eq!(
            repo.store_asset(&page.id, "../image.png", &png)
                .unwrap_err()
                .code,
            "asset_invalid_name"
        );
        assert_eq!(
            repo.store_asset(&page.id, "image.png", b"not an image")
                .unwrap_err()
                .code,
            "asset_invalid_type"
        );
        assert_eq!(
            repo.store_asset("missing-page", "image.png", &png)
                .unwrap_err()
                .code,
            "page_not_found"
        );
        assert_eq!(
            repo.store_asset(&page.id, "large.png", &vec![0; 100 * 1024 * 1024 + 1])
                .unwrap_err()
                .code,
            "asset_too_large"
        );
        let _ = fs::remove_dir_all(root);
    }
}
