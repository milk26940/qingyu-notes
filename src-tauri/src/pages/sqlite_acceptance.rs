use std::fs;
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::database::model::DatabaseQuery;
use crate::database::repository::DatabaseRepository;
use super::model::{BlockNode, PageDocument};
use super::repository::{switch_migration_files, PageRepository};

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("qingyu-sqlite-test-{}", Uuid::new_v4()))
}

#[test]
fn initializes_authoritative_schema_with_wal_and_foreign_keys() {
    let root = temp_root();
    let vault = root.join("vault");
    let repository = PageRepository::open(vault).unwrap();
    let database_path = root.join("qingyu.sqlite3");
    assert_eq!(repository.database_path(), database_path.as_path());

    let connection = Connection::open(database_path).unwrap();
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    let schema_version: i64 = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(schema_version, 2);
    for table in [
        "pages", "blocks", "page_links", "page_revisions", "attachments",
        "databases", "database_fields", "database_rows", "property_values",
        "conversations", "conversation_events", "conversation_attachments", "blocks_fts",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "缺少表 {table}");
    }
    drop(repository);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn saves_documents_and_updates_links_search_and_history_incrementally() {
    let root = temp_root();
    let repository = PageRepository::open(root.join("vault")).unwrap();
    let target = repository.create("目标".into(), None, "正文".into()).unwrap();
    let source = repository.create("来源".into(), None, "旧内容".into()).unwrap();
    let document = PageDocument {
        page_id: source.id.clone(),
        title: "来源".into(),
        blocks: vec![BlockNode {
            id: "block-1".into(),
            block_type: "paragraph".into(),
            text: format!("新内容 [[目标|{}]]", target.id),
            attrs: [("node".into(), serde_json::json!({
                "type": "paragraph",
                "content": [{ "type": "text", "text": "目标", "marks": [{ "type": "link", "attrs": { "href": format!("qingyu://page/{}", target.id) } }] }]
            }))].into_iter().collect(),
            children: Vec::new(),
        }],
        revision: 1,
    };

    let saved = repository.save_document(document, 0).unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(repository.get(&source.id, false).unwrap().content, format!("新内容 [[目标|{}]]", target.id));
    assert!(repository.links(&source.id).unwrap().iter().any(|link| link.target_id.as_deref() == Some(target.id.as_str())));
    assert_eq!(repository.backlinks(&target.id).unwrap()[0].source_id, source.id);
    assert_eq!(repository.search("新内容").unwrap().len(), 1);
    assert!(repository.history(&saved.page_id).unwrap().len() >= 2);
    assert_eq!(repository.open_bundle(&saved.page_id).unwrap().document.revision, 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn migrates_legacy_markdown_through_a_backup_and_keeps_the_source() {
    let root = temp_root();
    let vault = root.join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::write(
        vault.join("旧页面.md"),
        "---\nid: legacy-page\ntitle: 旧页面\nparent_id: \nkind: page\ncreated_at: 1\nupdated_at: 1\ntrashed_at: \nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n迁移正文",
    ).unwrap();

    let repository = PageRepository::open(vault.clone()).unwrap();
    assert_eq!(repository.get("legacy-page", false).unwrap().content, "迁移正文");
    assert!(vault.join("旧页面.md").exists());
    let backups = root.join("migration-backups");
    assert_eq!(fs::read_dir(backups).unwrap().count(), 1);
    assert!(root.join("qingyu.sqlite3").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn migrates_out_of_order_hierarchy_database_definition_and_record_values() {
    let root = temp_root();
    let vault = root.join("vault");
    let definitions = vault.join(".noteapp").join("databases");
    fs::create_dir_all(&definitions).unwrap();
    fs::write(vault.join("00-child.md"), "---\nid: child\ntitle: 子页\nparent_id: parent\nkind: page\ncreated_at: 1\nupdated_at: 1\ntrashed_at: \nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n子页").unwrap();
    fs::write(vault.join("10-parent.md"), "---\nid: parent\ntitle: 父页\nparent_id: \nkind: page\ncreated_at: 1\nupdated_at: 1\ntrashed_at: \nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n父页").unwrap();
    fs::write(vault.join("20-db.md"), "---\nid: db\ntitle: 数据库\nparent_id: \nkind: database\ncreated_at: 1\nupdated_at: 1\ntrashed_at: \nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n").unwrap();
    fs::write(vault.join("30-row.md"), "---\nid: row\ntitle: 记录\nparent_id: db\nkind: databaseRecord\ncreated_at: 1\nupdated_at: 1\ntrashed_at: \nsort_order: 0\ndatabase_id: db\nproperties: {\"field\":\"值\"}\n---\n").unwrap();
    fs::write(definitions.join("db.json"), r#"{"id":"db","pageId":"db","fields":[{"id":"field","name":"字段","fieldType":"text","options":[]}],"views":[],"createdAt":"1","updatedAt":"1"}"#).unwrap();
    let assets = vault.join(".noteapp").join("assets").join("parent");
    fs::create_dir_all(&assets).unwrap();
    fs::write(assets.join("image.png"), [137, 80, 78, 71, 13, 10, 26, 10]).unwrap();

    PageRepository::open(vault).unwrap();
    let connection = Connection::open(root.join("qingyu.sqlite3")).unwrap();
    for (table, expected) in [("pages", 4), ("databases", 1), ("database_fields", 1), ("database_rows", 1), ("property_values", 1), ("attachments", 1)] {
        let actual: i64 = connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
        assert_eq!(actual, expected, "{table}");
    }
    assert_eq!(connection.query_row("PRAGMA foreign_key_check", [], |_| Ok(1)).optional().unwrap(), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_database_migration_is_complete_without_the_json_source() {
    let root = temp_root();
    let vault = root.join("vault");
    let definitions = vault.join(".noteapp").join("databases");
    fs::create_dir_all(&definitions).unwrap();
    fs::write(vault.join("database.md"), "---\nid: legacy-db\ntitle: 项目库\nparent_id: \nkind: database\ncreated_at: 1\nupdated_at: 2\ntrashed_at: \nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n").unwrap();
    fs::write(vault.join("record.md"), "---\nid: legacy-row\ntitle: 第一条\nparent_id: legacy-db\nkind: databaseRecord\ncreated_at: 1\nupdated_at: 2\ntrashed_at: \nsort_order: 3\ndatabase_id: legacy-db\nproperties: {\"status\":\"进行中\",\"score\":7}\n---\n记录正文").unwrap();
    let definition_path = definitions.join("legacy-db.json");
    fs::write(&definition_path, r#"{"id":"legacy-db","pageId":"legacy-db","fields":[{"id":"status","name":"状态","fieldType":"select","options":["待办","进行中"]},{"id":"score","name":"分数","fieldType":"number","options":[]}],"views":[{"id":"table-main","name":"主视图","viewType":"table","filters":[],"sorts":[]}],"createdAt":"1","updatedAt":"2"}"#).unwrap();

    let repository = DatabaseRepository::open(vault).unwrap();
    fs::remove_file(definition_path).unwrap();

    let definition = repository.get("legacy-db").unwrap();
    assert_eq!(definition.fields.len(), 2);
    assert_eq!(definition.views[0].name, "主视图");
    let records = repository
        .query("legacy-db", DatabaseQuery::default())
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].title, "第一条");
    assert_eq!(records[0].content, "记录正文");
    assert_eq!(records[0].properties["status"], "进行中");
    assert_eq!(records[0].properties["score"], 7);

    let connection = Connection::open(root.join("qingyu.sqlite3")).unwrap();
    let migrated_title: String = connection
        .query_row(
            "SELECT title FROM databases WHERE id='legacy-db'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migrated_title, "项目库");
    for (table, expected) in [
        ("databases", 1),
        ("database_fields", 2),
        ("database_rows", 1),
        ("property_values", 2),
    ] {
        let actual: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .unwrap();
        assert_eq!(actual, expected, "{table}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_legacy_database_data_does_not_switch_the_temporary_sqlite() {
    let root = temp_root();
    let vault = root.join("vault");
    let definitions = vault.join(".noteapp").join("databases");
    fs::create_dir_all(&definitions).unwrap();
    fs::write(
        definitions.join("orphan.json"),
        r#"{"id":"orphan","pageId":"missing-page","fields":[{"id":"field","name":"字段","fieldType":"text","options":[]}],"views":[],"createdAt":"1","updatedAt":"1"}"#,
    )
    .unwrap();

    let error = match PageRepository::open(vault) {
        Ok(_) => panic!("孤立数据库定义不应完成迁移"),
        Err(error) => error,
    };

    assert!(matches!(
        error.code.as_str(),
        "migration_validation_failed" | "sqlite_write_failed"
    ));
    assert!(!root.join("qingyu.sqlite3").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn migrates_legacy_trash_pages_and_keeps_them_out_of_active_list() {
    let root = temp_root();
    let vault = root.join("vault");
    let trash = vault.join(".noteapp").join("trash");
    fs::create_dir_all(&trash).unwrap();
    fs::write(vault.join("active.md"), "---\nid: active\ntitle: 活动页\nparent_id: \nkind: page\ncreated_at: 1\nupdated_at: 1\ntrashed_at: \nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n活动").unwrap();
    fs::write(trash.join("trashed.md"), "---\nid: trashed\ntitle: 回收站页\nparent_id: \nkind: page\ncreated_at: 1\nupdated_at: 2\ntrashed_at: 2\nsort_order: 0\ndatabase_id: \nproperties: {}\n---\n回收站").unwrap();

    let repository = PageRepository::open(vault).unwrap();
    assert_eq!(repository.list(false).unwrap().len(), 1);
    assert_eq!(repository.list(true).unwrap().len(), 2);
    assert_eq!(repository.get("trashed", true).unwrap().trashed_at.as_deref(), Some("2"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_attachment_switch_removes_the_new_database() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let temporary_database = root.join("temporary.sqlite3");
    let database = root.join("qingyu.sqlite3");
    let temporary_attachments = root.join("temporary-attachments");
    let blocked_parent = root.join("blocked-parent");
    let attachments = blocked_parent.join("attachments");
    fs::write(&temporary_database, "new database").unwrap();
    fs::create_dir_all(&temporary_attachments).unwrap();
    fs::write(temporary_attachments.join("image.png"), "image").unwrap();
    fs::write(&blocked_parent, "blocks directory rename").unwrap();

    let error = switch_migration_files(
        &temporary_database,
        &database,
        &temporary_attachments,
        &attachments,
    )
    .unwrap_err();

    assert_eq!(error.code, "migration_switch_failed");
    assert!(!database.exists());
    assert!(temporary_attachments.exists());
    assert_eq!(fs::read_to_string(&blocked_parent).unwrap(), "blocks directory rename");
    let _ = fs::remove_dir_all(root);
}
