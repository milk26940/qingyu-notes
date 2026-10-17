use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::protocol::CodexEvent;
use crate::error::AppError;
use crate::pages::repository::{shared_vault_lock, PageRepository};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConversationScope {
    Vault,
    Page {
        #[serde(rename = "pageId", alias = "page_id")]
        page_id: String,
    },
    Database {
        #[serde(rename = "databaseId", alias = "database_id")]
        database_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMetadata {
    pub id: String,
    pub thread_id: Option<String>,
    pub title: String,
    pub scope: ConversationScope,
    pub created_at: String,
    pub updated_at: String,
    pub active_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub kind: String,
    pub timestamp: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAttachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub relative_path: String,
}

#[derive(Debug, Clone)]
pub struct ConversationStore {
    root: PathBuf,
    database_path: PathBuf,
    write_lock: Arc<Mutex<()>>,
    vault_lock: Arc<Mutex<()>>,
}

impl ConversationStore {
    pub fn open(vault_root: &Path) -> Result<Self, AppError> {
        let _ = PageRepository::open(vault_root.to_path_buf())?;
        let data_root = if vault_root.file_name().and_then(|value| value.to_str()) == Some("vault") {
            vault_root.parent().unwrap_or(vault_root)
        } else {
            vault_root
        };
        let root = data_root.join("attachments").join("conversations");
        fs::create_dir_all(&root)
            .map_err(|error| AppError::new("conversation_init_failed", error.to_string()))?;
        let store = Self {
            root,
            database_path: data_root.join("qingyu.sqlite3"),
            write_lock: Arc::new(Mutex::new(())),
            vault_lock: shared_vault_lock(vault_root),
        };
        let legacy_root = vault_root.join(".noteapp").join("conversations");
        fs::create_dir_all(&legacy_root)
            .map_err(|error| AppError::new("conversation_init_failed", error.to_string()))?;
        store.migrate_legacy(&legacy_root)?;
        Ok(store)
    }

    pub fn create(
        &self,
        title: String,
        scope: ConversationScope,
    ) -> Result<ConversationMetadata, AppError> {
        let _guard = self.lock()?;
        let _vault_guard = self.lock_vault()?;
        let timestamp = now();
        let metadata = ConversationMetadata {
            id: Uuid::new_v4().to_string(),
            thread_id: None,
            title,
            scope,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            active_turn_id: None,
        };
        let connection = self.connection()?;
        connection.execute("INSERT INTO conversations(id,thread_id,title,scope_json,created_at,updated_at,active_turn_id) VALUES(?1,NULL,?2,?3,?4,?5,NULL)", params![metadata.id,metadata.title,serde_json::to_string(&metadata.scope).unwrap(),metadata.created_at,metadata.updated_at]).map_err(sqlite_write)?;
        Ok(metadata)
    }

    pub fn list(&self) -> Result<Vec<ConversationMetadata>, AppError> {
        let _guard = self.lock()?;
        self.list_unlocked()
    }

    fn list_unlocked(&self) -> Result<Vec<ConversationMetadata>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id,thread_id,title,scope_json,created_at,updated_at,active_turn_id FROM conversations ORDER BY updated_at DESC").map_err(sqlite_read)?;
        let rows = statement.query_map([], |row| {
            let scope: String = row.get(3)?;
            Ok(ConversationMetadata { id: row.get(0)?, thread_id: row.get(1)?, title: row.get(2)?, scope: serde_json::from_str(&scope).unwrap_or(ConversationScope::Vault), created_at: row.get(4)?, updated_at: row.get(5)?, active_turn_id: row.get(6)? })
        }).map_err(sqlite_read)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sqlite_read)
    }

    pub fn get(&self, id: &str) -> Result<ConversationMetadata, AppError> {
        self.list()?
            .into_iter()
            .find(|conversation| conversation.id == id)
            .ok_or_else(|| AppError::new("conversation_not_found", "对话不存在"))
    }

    pub fn attach_thread(
        &self,
        id: &str,
        thread_id: String,
    ) -> Result<ConversationMetadata, AppError> {
        self.update(id, |conversation| {
            conversation.thread_id = Some(thread_id);
        })
    }

    pub fn set_active_turn(
        &self,
        id: &str,
        turn_id: Option<String>,
    ) -> Result<ConversationMetadata, AppError> {
        self.update(id, |conversation| {
            conversation.active_turn_id = turn_id;
        })
    }

    pub fn append_user_message(
        &self,
        id: &str,
        request_text: &str,
        display_text: Option<&str>,
        selected_text: Option<&str>,
        attachments: &[ConversationAttachment],
    ) -> Result<(), AppError> {
        self.append(
            id,
            ConversationRecord {
                kind: "userMessage".into(),
                timestamp: now(),
                data: json!({
                    "text": display_text.unwrap_or(request_text),
                    "requestText": request_text,
                    "displayText": display_text,
                    "selectedText": selected_text,
                    "attachments": attachments,
                }),
            },
        )
    }

    pub fn store_attachment(&self, conversation_id: &str, original_name: &str, mime: &str, bytes: &[u8]) -> Result<ConversationAttachment, AppError> {
        self.get(conversation_id)?;
        if bytes.len() > 50 * 1024 * 1024 {
            return Err(AppError::new("attachment_too_large", "单个附件不能超过 50 MB"));
        }
        let name = Path::new(original_name).file_name().and_then(|value| value.to_str()).filter(|value| !value.trim().is_empty()).ok_or_else(|| AppError::new("attachment_invalid_name", "附件名称无效"))?.to_owned();
        let extension = Path::new(&name).extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
        let allowed = ["png", "jpg", "jpeg", "gif", "webp", "pdf", "docx", "pptx", "md", "txt", "json", "csv", "log", "xml"];
        if !allowed.contains(&extension.as_str()) {
            return Err(AppError::new("attachment_unsupported_type", "暂不支持这种附件格式"));
        }
        let id = Uuid::new_v4().to_string();
        let directory = self.root.join("attachments").join(conversation_id);
        fs::create_dir_all(&directory).map_err(|error| AppError::new("attachment_write_failed", error.to_string()))?;
        let file_name = if extension.is_empty() { id.clone() } else { format!("{}.{}", id, extension) };
        fs::write(directory.join(&file_name), bytes).map_err(|error| AppError::new("attachment_write_failed", error.to_string()))?;
        let relative_path = format!("attachments/{}/{}", conversation_id, file_name);
        self.connection()?.execute("INSERT INTO conversation_attachments(id,conversation_id,name,relative_path,mime,size,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id,conversation_id,name,relative_path,mime,bytes.len() as i64,now()]).map_err(sqlite_write)?;
        Ok(ConversationAttachment { id, name, mime: mime.to_owned(), size: bytes.len() as u64, relative_path })
    }

    pub fn read_attachment_text(&self, relative_path: &str) -> Result<String, AppError> {
        let path = self.attachment_path(relative_path)?;
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
        let text = match extension.as_str() {
            "md" | "txt" | "json" | "csv" | "log" | "xml" => fs::read_to_string(&path).map_err(|error| AppError::new("attachment_read_failed", error.to_string()))?,
            "pdf" => pdf_extract::extract_text(&path).map_err(|error| AppError::new("attachment_parse_failed", error.to_string()))?,
            "docx" | "pptx" => extract_office_text(&path, &extension)?,
            _ => return Err(AppError::new("attachment_not_text", "该附件应作为图片输入读取")),
        };
        Ok(text.chars().take(200_000).collect())
    }

    pub fn attachment_path(&self, relative_path: &str) -> Result<PathBuf, AppError> {
        let relative = Path::new(relative_path);
        let components = relative.components().collect::<Vec<_>>();
        let inside_attachments = matches!(components.first(), Some(std::path::Component::Normal(value)) if *value == std::ffi::OsStr::new("attachments"));
        if relative.is_absolute() || components.len() < 3 || !inside_attachments || components.iter().any(|component| !matches!(component, std::path::Component::Normal(_))) {
            return Err(AppError::new("attachment_invalid_path", "附件路径不安全"));
        }
        let path = self.root.join(relative);
        if !path.is_file() { return Err(AppError::new("attachment_not_found", "附件不存在")); }
        Ok(path)
    }

    pub fn read_attachment_bytes(&self, relative_path: &str) -> Result<Vec<u8>, AppError> {
        let path = self.attachment_path(relative_path)?;
        fs::read(path).map_err(|error| AppError::new("attachment_read_failed", error.to_string()))
    }

    pub fn append_server_request(
        &self,
        thread_id: &str,
        method: &str,
        params: &Value,
    ) -> Result<(), AppError> {
        let Some(conversation) = self.by_thread(thread_id)? else {
            return Ok(());
        };
        let data = if method == "item/tool/call" {
            json!({
                "method": method,
                "threadId": params.get("threadId"),
                "turnId": params.get("turnId"),
                "callId": params.get("callId"),
                "namespace": params.get("namespace"),
                "tool": params.get("tool"),
                "arguments": params.get("arguments")
            })
        } else {
            json!({ "method": method })
        };
        self.append(
            &conversation.id,
            ConversationRecord {
                kind: "toolRequest".into(),
                timestamp: now(),
                data,
            },
        )
    }

    pub fn append_event(&self, thread_id: &str, event: &CodexEvent) -> Result<(), AppError> {
        let Some(conversation) = self.by_thread(thread_id)? else {
            return Ok(());
        };
        let record = match event {
            CodexEvent::AgentMessageDelta {
                turn_id,
                item_id,
                delta,
                ..
            } => Some((
                "agentMessageDelta",
                json!({ "turnId": turn_id, "itemId": item_id, "delta": delta }),
            )),
            CodexEvent::TurnPlanUpdated {
                turn_id,
                explanation,
                plan,
                ..
            } => Some((
                "planUpdated",
                json!({ "turnId": turn_id, "explanation": explanation, "plan": plan }),
            )),
            CodexEvent::TurnDiffUpdated { turn_id, diff, .. } => {
                Some(("diffUpdated", json!({ "turnId": turn_id, "diff": diff })))
            }
            CodexEvent::TurnStarted { turn, .. } => Some(("turnStarted", sanitized_turn(turn))),
            CodexEvent::TurnCompleted { turn, .. } => Some(("turnCompleted", sanitized_turn(turn))),
            CodexEvent::ItemStarted { turn_id, item, .. } => {
                Some(("itemStarted", json!({ "turnId": turn_id, "item": item })))
            }
            CodexEvent::ItemCompleted { turn_id, item, .. } => {
                Some(("itemCompleted", json!({ "turnId": turn_id, "item": item })))
            }
            CodexEvent::Other { .. } => None,
        };
        if let Some((kind, data)) = record {
            self.append(
                &conversation.id,
                ConversationRecord {
                    kind: kind.into(),
                    timestamp: now(),
                    data,
                },
            )?;
        }
        Ok(())
    }

    pub fn append_tool_outcome(
        &self,
        thread_id: &str,
        call_id: &str,
        namespace: &str,
        tool: &str,
        success: bool,
    ) -> Result<(), AppError> {
        self.append_for_thread(
            thread_id,
            "toolCompleted",
            json!({
                "callId": call_id,
                "namespace": namespace,
                "tool": tool,
                "success": success
            }),
        )
    }

    pub fn append_approval(
        &self,
        thread_id: &str,
        call_id: &str,
        namespace: &str,
        tool: &str,
        diff: &str,
        approved: Option<bool>,
    ) -> Result<(), AppError> {
        self.append_for_thread(
            thread_id,
            if approved.is_some() {
                "approvalResolved"
            } else {
                "approvalRequested"
            },
            json!({
                "callId": call_id,
                "namespace": namespace,
                "tool": tool,
                "diff": diff,
                "approved": approved
            }),
        )
    }

    pub fn append_change_summary(
        &self,
        thread_id: &str,
        call_id: &str,
        namespace: &str,
        tool: &str,
        diff: &str,
    ) -> Result<(), AppError> {
        self.append_for_thread(
            thread_id,
            "changeSummary",
            json!({
                "callId": call_id,
                "namespace": namespace,
                "tool": tool,
                "diff": diff
            }),
        )
    }

    pub fn history(&self, id: &str) -> Result<Vec<ConversationRecord>, AppError> {
        self.get(id)?;
        let _guard = self.lock()?;
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT event_type,created_at,payload_json FROM conversation_events WHERE conversation_id=?1 ORDER BY sequence").map_err(sqlite_read)?;
        let rows = statement.query_map([id], |row| { let payload: String=row.get(2)?; Ok(ConversationRecord { kind: row.get(0)?, timestamp: row.get(1)?, data: serde_json::from_str(&payload).unwrap_or(Value::Null) }) }).map_err(sqlite_read)?;
        rows.collect::<Result<Vec<_>,_>>().map_err(sqlite_read)
    }

    pub fn registered_thread_ids(&self) -> Result<Vec<String>, AppError> {
        Ok(self
            .list()?
            .into_iter()
            .filter_map(|conversation| conversation.thread_id)
            .collect())
    }

    pub fn set_active_turn_for_thread(
        &self,
        thread_id: &str,
        turn_id: Option<String>,
    ) -> Result<(), AppError> {
        if let Some(conversation) = self.by_thread(thread_id)? {
            self.set_active_turn(&conversation.id, turn_id)?;
        }
        Ok(())
    }

    fn by_thread(&self, thread_id: &str) -> Result<Option<ConversationMetadata>, AppError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|conversation| conversation.thread_id.as_deref() == Some(thread_id)))
    }

    pub(crate) fn append_for_thread(&self, thread_id: &str, kind: &str, data: Value) -> Result<(), AppError> {
        let Some(conversation) = self.by_thread(thread_id)? else {
            return Ok(());
        };
        self.append(
            &conversation.id,
            ConversationRecord {
                kind: kind.into(),
                timestamp: now(),
                data,
            },
        )
    }

    fn update(
        &self,
        id: &str,
        mutate: impl FnOnce(&mut ConversationMetadata),
    ) -> Result<ConversationMetadata, AppError> {
        let _guard = self.lock()?;
        let _vault_guard = self.lock_vault()?;
        let mut conversation = self.list_unlocked()?
            .into_iter()
            .find(|conversation| conversation.id == id)
            .ok_or_else(|| AppError::new("conversation_not_found", "对话不存在"))?;
        mutate(&mut conversation);
        conversation.updated_at = now();
        self.connection()?.execute("UPDATE conversations SET thread_id=?2,title=?3,scope_json=?4,updated_at=?5,active_turn_id=?6 WHERE id=?1", params![conversation.id,conversation.thread_id,conversation.title,serde_json::to_string(&conversation.scope).unwrap(),conversation.updated_at,conversation.active_turn_id]).map_err(sqlite_write)?;
        Ok(conversation)
    }

    fn append(&self, id: &str, record: ConversationRecord) -> Result<(), AppError> {
        let _guard = self.lock()?;
        let _vault_guard = self.lock_vault()?;
        let connection = self.connection()?;
        let sequence: i64 = connection.query_row("SELECT COALESCE(MAX(sequence),-1)+1 FROM conversation_events WHERE conversation_id=?1", [id], |row| row.get(0)).map_err(sqlite_read)?;
        connection.execute("INSERT INTO conversation_events(id,conversation_id,sequence,event_type,payload_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![Uuid::new_v4().to_string(),id,sequence,record.kind,serde_json::to_string(&record.data).unwrap(),record.timestamp]).map_err(sqlite_write)?;
        connection.execute("UPDATE conversations SET updated_at=?2 WHERE id=?1", params![id,now()]).map_err(sqlite_write)?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, AppError> {
        let connection = Connection::open(&self.database_path).map_err(sqlite_read)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;").map_err(sqlite_read)?;
        Ok(connection)
    }

    fn migrate_legacy(&self, legacy_root: &Path) -> Result<(), AppError> {
        let index = legacy_root.join("index.json");
        if !index.exists() {
            return Ok(());
        }
        let metadata: Vec<ConversationMetadata> = serde_json::from_slice(&fs::read(&index)
            .map_err(|error| AppError::new("conversation_read_failed", error.to_string()))?)
            .map_err(|error| AppError::new("conversation_invalid_data", error.to_string()))?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(sqlite_write)?;
        for conversation in &metadata {
            transaction.execute("INSERT OR IGNORE INTO conversations(id,thread_id,title,scope_json,created_at,updated_at,active_turn_id) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![conversation.id,conversation.thread_id,conversation.title,serde_json::to_string(&conversation.scope).unwrap(),conversation.created_at,conversation.updated_at,conversation.active_turn_id]).map_err(sqlite_write)?;
            let log = legacy_root.join(format!("{}.jsonl", conversation.id));
            if log.exists() {
                for (sequence, line) in fs::read_to_string(log).map_err(|error| AppError::new("conversation_read_failed", error.to_string()))?.lines().filter(|line| !line.trim().is_empty()).enumerate() {
                    let record: ConversationRecord = serde_json::from_str(line).map_err(|error| AppError::new("conversation_invalid_data", error.to_string()))?;
                    transaction.execute("INSERT OR IGNORE INTO conversation_events(id,conversation_id,sequence,event_type,payload_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![format!("legacy-{}-{sequence}",conversation.id),conversation.id,sequence as i64,record.kind,serde_json::to_string(&record.data).unwrap(),record.timestamp]).map_err(sqlite_write)?;
                }
            }
        }
        transaction.commit().map_err(sqlite_write)?;
        let legacy_attachments = legacy_root.join("attachments");
        let destination = self.root.join("attachments");
        if legacy_attachments.exists() && !destination.exists() {
            crate::portable::copy_tree_verified(&legacy_attachments, &destination)
                .map_err(|error| AppError::new("conversation_migration_failed", error.to_string()))?;
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.write_lock
            .lock()
            .map_err(|_| AppError::new("conversation_lock_failed", "对话存储锁已损坏"))
    }

    fn lock_vault(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.vault_lock
            .lock()
            .map_err(|_| AppError::new("vault_write_lock_failed", "笔记库写入锁已损坏"))
    }
}

fn sqlite_read(error: rusqlite::Error) -> AppError {
    AppError::new("conversation_read_failed", error.to_string())
}

fn sqlite_write(error: rusqlite::Error) -> AppError {
    AppError::new("conversation_write_failed", error.to_string())
}

pub(crate) fn extract_office_text(path: &Path, extension: &str) -> Result<String, AppError> {
    let file = fs::File::open(path).map_err(|error| AppError::new("attachment_read_failed", error.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| AppError::new("attachment_parse_failed", error.to_string()))?;
    let mut names = (0..archive.len()).filter_map(|index| archive.by_index(index).ok().map(|entry| entry.name().to_owned())).filter(|name| if extension == "docx" { name == "word/document.xml" } else { name.starts_with("ppt/slides/slide") && name.ends_with(".xml") }).collect::<Vec<_>>();
    names.sort();
    let mut output = String::new();
    for name in names {
        let mut entry = archive.by_name(&name).map_err(|error| AppError::new("attachment_parse_failed", error.to_string()))?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml).map_err(|error| AppError::new("attachment_parse_failed", error.to_string()))?;
        output.push_str(&xml_to_text(&xml));
        output.push('\n');
    }
    Ok(output)
}

fn xml_to_text(xml: &str) -> String {
    let normalized = xml.replace("</w:p>", "\n").replace("</a:p>", "\n").replace("<w:tab/>", "\t").replace("<a:br/>", "\n");
    let mut output = String::new();
    let mut in_tag = false;
    for character in normalized.chars() {
        match character { '<' => in_tag = true, '>' => in_tag = false, _ if !in_tag => output.push(character), _ => {} }
    }
    output.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

fn sanitized_turn(turn: &Value) -> Value {
    let mut result = json!({
        "id": turn.get("id"),
        "status": turn.get("status"),
        "error": turn.get("error")
    });
    if let Some(usage) = turn.get("usage").or_else(|| turn.get("tokenUsage")) {
        result["usage"] = usage.clone();
    }
    result
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn conversation_scope_accepts_camel_and_legacy_fields() {
        let current_page: ConversationScope =
            serde_json::from_value(json!({ "kind": "page", "pageId": "page-1" })).unwrap();
        let legacy_page: ConversationScope =
            serde_json::from_value(json!({ "kind": "page", "page_id": "page-1" })).unwrap();
        assert_eq!(current_page, legacy_page);
        assert_eq!(
            serde_json::to_value(current_page).unwrap()["pageId"],
            "page-1"
        );

        let current_database: ConversationScope =
            serde_json::from_value(json!({ "kind": "database", "databaseId": "database-1" }))
                .unwrap();
        let legacy_database: ConversationScope =
            serde_json::from_value(json!({ "kind": "database", "database_id": "database-1" }))
                .unwrap();
        assert_eq!(current_database, legacy_database);
        assert_eq!(
            serde_json::to_value(current_database).unwrap()["databaseId"],
            "database-1"
        );
    }

    #[test]
    fn stores_metadata_and_visible_records_only() {
        let root = std::env::temp_dir().join(format!("qingyu-conversation-{}", Uuid::new_v4()));
        let store = ConversationStore::open(&root).unwrap();
        let conversation = store
            .create(
                "整理项目".into(),
                ConversationScope::Page {
                    page_id: "page-1".into(),
                },
            )
            .unwrap();
        store
            .attach_thread(&conversation.id, "thread-1".into())
            .unwrap();
        store
            .append_user_message(&conversation.id, "请总结当前页面", None, None, &[])
            .unwrap();
        store
            .append_event(
                "thread-1",
                &CodexEvent::AgentMessageDelta {
                    thread_id: "thread-1".into(),
                    turn_id: "turn-1".into(),
                    item_id: "item-1".into(),
                    delta: "摘要".into(),
                },
            )
            .unwrap();
        store
            .append_event(
                "thread-1",
                &CodexEvent::Other {
                    method: "item/reasoning/textDelta".into(),
                    params: json!({ "delta": "隐藏推理" }),
                },
            )
            .unwrap();

        let records = store.history(&conversation.id).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].kind, "userMessage");
        assert_eq!(records[1].kind, "agentMessageDelta");
        assert!(!serde_json::to_string(&records).unwrap().contains("隐藏推理"));
        assert_eq!(
            store.list().unwrap()[0].thread_id.as_deref(),
            Some("thread-1")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn history_rejects_ids_outside_the_conversation_index() {
        let root = std::env::temp_dir().join(format!("qingyu-conversation-{}", Uuid::new_v4()));
        let store = ConversationStore::open(&root).unwrap();
        fs::write(
            root.join(".noteapp").join("escaped.jsonl"),
            r#"{"kind":"userMessage","timestamp":"0","data":{"text":"不应读取"}}"#,
        )
        .unwrap();

        let error = store.history("../escaped").unwrap_err();

        assert_eq!(error.code, "conversation_not_found");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_creates_keep_every_conversation() {
        let root = std::env::temp_dir().join(format!("qingyu-conversation-{}", Uuid::new_v4()));
        let store = ConversationStore::open(&root).unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let handles = (0..8)
            .map(|index| {
                let store = ConversationStore::open(&root).unwrap();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.create(format!("对话 {index}"), ConversationScope::Vault)
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(store.list().unwrap().len(), 8);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persists_tool_approval_and_change_projection() {
        let root = std::env::temp_dir().join(format!("qingyu-conversation-{}", Uuid::new_v4()));
        let store = ConversationStore::open(&root).unwrap();
        let conversation = store
            .create("修改页面".into(), ConversationScope::Vault)
            .unwrap();
        store
            .attach_thread(&conversation.id, "thread-1".into())
            .unwrap();
        store
            .append_approval("thread-1", "call-1", "page", "update", "+ 新正文", None)
            .unwrap();
        store
            .append_approval(
                "thread-1",
                "call-1",
                "page",
                "update",
                "+ 新正文",
                Some(true),
            )
            .unwrap();
        store
            .append_tool_outcome("thread-1", "call-1", "page", "update", true)
            .unwrap();
        store
            .append_change_summary("thread-1", "call-1", "page", "update", "+ 新正文")
            .unwrap();

        let kinds = store
            .history(&conversation.id)
            .unwrap()
            .into_iter()
            .map(|record| record.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "approvalRequested",
                "approvalResolved",
                "toolCompleted",
                "changeSummary"
            ]
        );
        let _ = fs::remove_dir_all(root);
    }
}
