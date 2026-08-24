use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::protocol::CodexEvent;
use crate::error::AppError;

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

#[derive(Debug, Clone)]
pub struct ConversationStore {
    root: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl ConversationStore {
    pub fn open(vault_root: &Path) -> Result<Self, AppError> {
        let root = vault_root.join(".noteapp").join("conversations");
        fs::create_dir_all(&root)
            .map_err(|error| AppError::new("conversation_init_failed", error.to_string()))?;
        let store = Self {
            root,
            write_lock: Arc::new(Mutex::new(())),
        };
        if !store.index_path().exists() {
            store.write_index(&[])?;
        }
        Ok(store)
    }

    pub fn create(
        &self,
        title: String,
        scope: ConversationScope,
    ) -> Result<ConversationMetadata, AppError> {
        let _guard = self.lock()?;
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
        let mut conversations = self.list_unlocked()?;
        conversations.push(metadata.clone());
        self.write_index(&conversations)?;
        Ok(metadata)
    }

    pub fn list(&self) -> Result<Vec<ConversationMetadata>, AppError> {
        let _guard = self.lock()?;
        self.list_unlocked()
    }

    fn list_unlocked(&self) -> Result<Vec<ConversationMetadata>, AppError> {
        let text = fs::read_to_string(self.index_path())
            .map_err(|error| AppError::new("conversation_read_failed", error.to_string()))?;
        serde_json::from_str(&text)
            .map_err(|error| AppError::new("conversation_invalid_data", error.to_string()))
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

    pub fn append_user_message(&self, id: &str, text: &str) -> Result<(), AppError> {
        self.append(
            id,
            ConversationRecord {
                kind: "userMessage".into(),
                timestamp: now(),
                data: json!({ "text": text }),
            },
        )
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
        let path = self.log_path(id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path)
            .map_err(|error| AppError::new("conversation_read_failed", error.to_string()))?;
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .map_err(|error| AppError::new("conversation_invalid_data", error.to_string()))
            })
            .collect()
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

    fn append_for_thread(&self, thread_id: &str, kind: &str, data: Value) -> Result<(), AppError> {
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
        let mut conversations = self.list_unlocked()?;
        let conversation = conversations
            .iter_mut()
            .find(|conversation| conversation.id == id)
            .ok_or_else(|| AppError::new("conversation_not_found", "对话不存在"))?;
        mutate(conversation);
        conversation.updated_at = now();
        let result = conversation.clone();
        self.write_index(&conversations)?;
        Ok(result)
    }

    fn append(&self, id: &str, record: ConversationRecord) -> Result<(), AppError> {
        let _guard = self.lock()?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path(id))
            .map_err(|error| AppError::new("conversation_write_failed", error.to_string()))?;
        writeln!(file, "{}", serde_json::to_string(&record).unwrap())
            .map_err(|error| AppError::new("conversation_write_failed", error.to_string()))
    }

    fn write_index(&self, conversations: &[ConversationMetadata]) -> Result<(), AppError> {
        let path = self.index_path();
        let temp = self.root.join(format!("index.{}.json.tmp", Uuid::new_v4()));
        fs::write(
            &temp,
            format!("{}\n", serde_json::to_string_pretty(conversations).unwrap()),
        )
        .map_err(|error| AppError::new("conversation_write_failed", error.to_string()))?;
        fs::rename(temp, path)
            .map_err(|error| AppError::new("conversation_write_failed", error.to_string()))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    fn log_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{}.jsonl", id))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.write_lock
            .lock()
            .map_err(|_| AppError::new("conversation_lock_failed", "对话存储锁已损坏"))
    }
}

fn sanitized_turn(turn: &Value) -> Value {
    json!({
        "id": turn.get("id"),
        "status": turn.get("status"),
        "error": turn.get("error")
    })
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
            .append_user_message(&conversation.id, "请总结当前页面")
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
        assert!(!fs::read_to_string(store.log_path(&conversation.id))
            .unwrap()
            .contains("隐藏推理"));
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
        let store = Arc::new(ConversationStore::open(&root).unwrap());
        let barrier = Arc::new(Barrier::new(8));
        let handles = (0..8)
            .map(|index| {
                let store = store.clone();
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
