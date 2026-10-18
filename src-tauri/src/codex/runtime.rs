use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::conversation::{ConversationAttachment, ConversationStore};
use super::protocol::{qingyu_dynamic_tools, CodexEvent};
use crate::backup::model::BackupScope;
use crate::backup::repository::BackupRepository;
use crate::database::model::{DatabaseDefinition, DatabaseFieldType};
use crate::database::repository::DatabaseRepository;
use crate::error::AppError;
use crate::pages::model::{BlockNode, Page, PageLink};
use crate::pages::repository::PageRepository;

#[path = "native.rs"]
mod native;

fn find_block<'a>(blocks: &'a [BlockNode], id: &str) -> Option<&'a BlockNode> {
    blocks.iter().find_map(|block| {
        if block.id == id {
            Some(block)
        } else {
            find_block(&block.children, id)
        }
    })
}

fn find_block_mut<'a>(blocks: &'a mut [BlockNode], id: &str) -> Option<&'a mut BlockNode> {
    blocks.iter_mut().find_map(|block| {
        if block.id == id {
            Some(block)
        } else {
            find_block_mut(&mut block.children, id)
        }
    })
}

fn validate_block_ids(blocks: &[BlockNode]) -> Result<(), AppError> {
    fn collect(blocks: &[BlockNode], ids: &mut HashSet<String>) -> Result<(), AppError> {
        for block in blocks {
            if block.id.trim().is_empty() {
                return Err(AppError::new("block_invalid_id", "区块 ID 不能为空"));
            }
            if !ids.insert(block.id.clone()) {
                return Err(AppError::new("block_duplicate_id", "区块 ID 已存在"));
            }
            collect(&block.children, ids)?;
        }
        Ok(())
    }

    collect(blocks, &mut HashSet::new())
}

fn insert_block(
    blocks: &mut Vec<BlockNode>,
    parent_id: Option<&str>,
    position: usize,
    block: BlockNode,
) -> Result<(), AppError> {
    let mut candidate = blocks.clone();
    let siblings = match parent_id {
        Some(parent_id) => &mut find_block_mut(&mut candidate, parent_id)
            .ok_or_else(|| AppError::new("block_parent_not_found", "父区块不存在"))?
            .children,
        None => &mut candidate,
    };
    siblings.insert(position.min(siblings.len()), block);
    validate_block_ids(&candidate)?;
    *blocks = candidate;
    Ok(())
}

fn replace_block(blocks: &mut Vec<BlockNode>, replacement: BlockNode) -> Result<(), AppError> {
    let mut candidate = blocks.clone();
    let target = find_block_mut(&mut candidate, &replacement.id)
        .ok_or_else(|| AppError::new("block_not_found", "区块不存在"))?;
    *target = replacement;
    validate_block_ids(&candidate)?;
    *blocks = candidate;
    Ok(())
}

fn remove_block(blocks: &mut Vec<BlockNode>, id: &str) -> Option<BlockNode> {
    if let Some(index) = blocks.iter().position(|block| block.id == id) {
        return Some(blocks.remove(index));
    }
    blocks
        .iter_mut()
        .find_map(|block| remove_block(&mut block.children, id))
}

fn move_block(
    blocks: &mut Vec<BlockNode>,
    block_id: &str,
    parent_id: Option<&str>,
    position: usize,
) -> Result<(), AppError> {
    let source = find_block(blocks, block_id)
        .ok_or_else(|| AppError::new("block_not_found", "区块不存在"))?;
    if parent_id == Some(block_id)
        || parent_id.is_some_and(|parent_id| find_block(&source.children, parent_id).is_some())
    {
        return Err(AppError::new(
            "block_invalid_parent",
            "不能把区块移动到自身或其后代中",
        ));
    }
    if parent_id.is_some_and(|parent_id| find_block(blocks, parent_id).is_none()) {
        return Err(AppError::new("block_parent_not_found", "父区块不存在"));
    }

    let mut candidate = blocks.clone();
    let block = remove_block(&mut candidate, block_id)
        .ok_or_else(|| AppError::new("block_not_found", "区块不存在"))?;
    let siblings = match parent_id {
        Some(parent_id) => &mut find_block_mut(&mut candidate, parent_id)
            .ok_or_else(|| AppError::new("block_parent_not_found", "父区块不存在"))?
            .children,
        None => &mut candidate,
    };
    siblings.insert(position.min(siblings.len()), block);
    validate_block_ids(&candidate)?;
    *blocks = candidate;
    Ok(())
}

fn delete_block(blocks: &mut Vec<BlockNode>, block_id: &str) -> Result<(), AppError> {
    let mut candidate = blocks.clone();
    remove_block(&mut candidate, block_id)
        .ok_or_else(|| AppError::new("block_not_found", "区块不存在"))?;
    *blocks = candidate;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum CodexStatus {
    Stopped,
    Starting,
    Ready {
        user_agent: String,
        codex_home: String,
    },
    Failed {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub id: String,
    pub request_id: Value,
    pub thread_id: String,
    pub turn_id: String,
    pub namespace: String,
    pub tool: String,
    pub arguments: Value,
    pub diff: String,
    #[serde(skip)]
    pub connection_generation: u64,
    #[serde(skip)]
    pub vault_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelOption {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub is_default: bool,
    pub input_modalities: Vec<String>,
}

struct ActiveTurn {
    thread_id: String,
    cancel: CancellationToken,
}

#[derive(Clone)]
pub struct CodexRuntime {
    status: Arc<RwLock<CodexStatus>>,
    pages: Arc<RwLock<Option<PageRepository>>>,
    conversations: Arc<RwLock<Option<ConversationStore>>>,
    active: Arc<Mutex<HashMap<String, ActiveTurn>>>,
    instructions: Arc<RwLock<HashMap<String, String>>>,
    generation: Arc<AtomicU64>,
    vault_root: Arc<RwLock<Option<String>>>,
}

impl Default for CodexRuntime {
    fn default() -> Self {
        Self {
            status: Arc::new(RwLock::new(CodexStatus::Stopped)),
            pages: Arc::new(RwLock::new(None)),
            conversations: Arc::new(RwLock::new(None)),
            active: Arc::new(Mutex::new(HashMap::new())),
            instructions: Arc::new(RwLock::new(HashMap::new())),
            generation: Arc::new(AtomicU64::new(0)),
            vault_root: Arc::new(RwLock::new(None)),
        }
    }
}

impl CodexRuntime {
    pub async fn status(&self) -> CodexStatus { self.status.read().await.clone() }

    pub async fn configure_vault(&self, pages: PageRepository) -> Result<ConversationStore, AppError> {
        let root = pages.root().to_string_lossy().to_string();
        let mut current = self.vault_root.write().await;
        if current.as_deref() != Some(&root) {
            for turn in self.active.lock().await.values() { turn.cancel.cancel(); }
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.instructions.write().await.clear();
            *current = Some(root);
        }
        let store = ConversationStore::open(pages.root())?;
        *self.pages.write().await = Some(pages);
        *self.conversations.write().await = Some(store.clone());
        Ok(store)
    }

    pub async fn start(&self, app: &AppHandle) -> Result<CodexStatus, AppError> {
        let settings = super::settings::get()?;
        let status = if settings.base_url.is_empty() {
            CodexStatus::Failed { code: "agent_not_configured".into(), message: "请先配置内置 Agent 的 API 地址、Key 和模型".into() }
        } else {
            CodexStatus::Ready { user_agent: "轻羽内置 Agent · Rig 0.44.0".into(), codex_home: settings.workspace_root }
        };
        self.set_status(app, status.clone()).await;
        Ok(status)
    }

    pub async fn stop(&self, app: &AppHandle) -> Result<CodexStatus, AppError> {
        for turn in self.active.lock().await.values() { turn.cancel.cancel(); }
        self.set_status(app, CodexStatus::Stopped).await;
        Ok(CodexStatus::Stopped)
    }

    pub async fn list_models(&self) -> Result<Vec<CodexModelOption>, AppError> {
        let connection = super::settings::connection(None)?;
        let result = match connection.client.list_models().await {
            Err(rig_core::error::ProviderError::Http(_)) => {
                // 模型列表是只读请求，仅为临时传输故障重试一次；认证和接口错误直接返回。
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                connection.client.list_models().await
            }
            result => result,
        };
        let models = result.map_err(|error| super::settings::model_error(error, &connection.api_key))?;
        let mut options = models.into_iter().map(|model| CodexModelOption {
            display_name: model.display_name().to_owned(), description: model.description.unwrap_or_default(),
            is_default: model.id == connection.model, id: model.id.clone(), model: model.id,
            input_modalities: vec!["text".into()],
        }).collect::<Vec<_>>();
        // 自定义部署可能不在列表里；保留用户配置的模型名，不替换成其他模型。
        if !connection.model.is_empty() && !options.iter().any(|item| item.model == connection.model) {
            options.insert(0, CodexModelOption { id: connection.model.clone(), model: connection.model.clone(), display_name: connection.model, description: "手动配置的默认模型".into(), is_default: true, input_modalities: vec!["text".into()] });
        }
        Ok(options)
    }

    pub async fn start_thread(&self, _cwd: &Path, instructions: &str, _model: Option<&str>) -> Result<String, AppError> {
        let id = format!("rig-{}", uuid::Uuid::new_v4());
        self.instructions.write().await.insert(id.clone(), instructions.to_owned());
        Ok(id)
    }

    pub async fn resume_thread(&self, thread_id: &str, instructions: Option<&str>) -> Result<(), AppError> {
        // 旧线程 ID 仅作为本地对话标识使用，不连接旧的外部运行时。
        if let Some(instructions) = instructions { self.instructions.write().await.insert(thread_id.into(), instructions.into()); }
        Ok(())
    }

    pub async fn start_turn(&self, app: &AppHandle, thread_id: &str, text: &str, _cwd: &Path, attachments: &[ConversationAttachment], model: Option<&str>, user_data: Value) -> Result<String, AppError> {
        self.launch_turn(app, thread_id, text, attachments, model, user_data).await
    }

    pub async fn delete_conversation(&self, store: &ConversationStore, id: &str) -> Result<(), AppError> {
        let conversation = store.get(id)?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let active = self.active.lock().await;
            let turns = active.values().filter(|turn| Some(turn.thread_id.as_str()) == conversation.thread_id.as_deref()).collect::<Vec<_>>();
            if turns.is_empty() {
                // 同启动执行共用互斥锁，删除期间不能再启动新任务。
                store.set_active_turn(id, None)?;
                return store.delete(id);
            }
            for turn in turns { turn.cancel.cancel(); }
            drop(active);
            if tokio::time::Instant::now() >= deadline { return Err(AppError::new("conversation_stop_timeout", "仍在等待当前操作结束，对话尚未删除，请稍后重试")); }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<(), AppError> {
        if let Some(turn) = self.active.lock().await.get(turn_id).filter(|turn| turn.thread_id == thread_id) { turn.cancel.cancel(); }
        Ok(())
    }

    pub async fn approvals(&self) -> Vec<PendingApproval> { Vec::new() }
    pub async fn resolve_approval(&self, _app: &AppHandle, _id: &str, _approved: bool) -> Result<(), AppError> {
        Err(AppError::new("approval_not_found", "内置 Agent 默认自动执行，没有待处理审批"))
    }

    async fn set_status(&self, app: &AppHandle, status: CodexStatus) {
        *self.status.write().await = status.clone();
        let _ = app.emit("codex://status", status);
    }

    async fn emit_activity(&self, app: &AppHandle, thread_id: &str, data: Value) {
        if let Some(store) = self.conversations.read().await.clone() {
            if let Err(error) = store.append_for_thread(thread_id, "toolActivity", data.clone()) {
                let _ = app.emit("codex://persistence-error", error);
            }
        }
        let _ = app.emit("codex://event", CodexEvent::Other {
            method: "qingyu/toolActivity".into(),
            params: json!({ "threadId": thread_id, "data": data }),
        });
    }

    async fn change_snapshot(&self, namespace: &str, arguments: &Value) -> Option<Value> {
        let pages = self.pages.read().await.clone()?;
        if namespace == "database" && arguments.get("schema").is_some() {
            let id = arguments.get("databaseId")?.as_str()?;
            let schema = DatabaseRepository::from_pages(pages.clone()).ok()?.get(id).ok()?;
            let page = pages.get(&schema.page_id, false).ok()?;
            return Some(json!({ "pageId": page.id, "title": page.title, "content": serde_json::to_string_pretty(&schema).ok()? }));
        }
        let id = arguments.get("pageId").or_else(|| arguments.get("id")).or_else(|| arguments.get("sourceId"))?.as_str()?;
        let page = pages.get(id, false).ok()?;
        let content = if page.database_id.is_some() {
            format!("{}\n\n{}", page.content, serde_json::to_string_pretty(&page.properties).ok()?)
        } else { page.content };
        let content = if arguments.get("sortOrder").is_some() {
            format!("父页面：{}\n排序位置：{}\n\n{}", page.parent_id.as_deref().unwrap_or("根目录"), page.sort_order, content)
        } else { content };
        Some(json!({ "pageId": page.id, "title": page.title, "content": content }))
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_activity(&self, app: &AppHandle, thread_id: &str, turn_id: &str, call_id: &str, namespace: &str, tool: &str, arguments: &Value, before: Option<Value>, result: &Result<Value, AppError>) {
        let mut data = json!({ "turnId": turn_id, "callId": call_id, "namespace": namespace, "tool": tool, "stage": "completed", "success": result.is_ok() });
        if let Err(error) = result { data["error"] = json!(error.message); }
        if result.is_ok() && is_write_tool(namespace, tool) {
            let mut target = arguments.clone();
            if let Ok(value) = result {
                if let Some(id) = value.get("id").or_else(|| value.get("pageId")).and_then(Value::as_str) {
                    target["id"] = json!(id);
                }
            }
            let after = self.change_snapshot(namespace, &target).await;
            data["change"] = json!({ "before": before, "after": after, "blockId": arguments.get("blockId").or_else(|| arguments.pointer("/block/id")), "operation": tool });
        }
        self.emit_activity(app, thread_id, data).await;
    }

    async fn execute_tool(
        &self,
        namespace: &str,
        tool: &str,
        arguments: &Value,
    ) -> Result<Value, AppError> {
        let pages = self
            .pages
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::new("vault_not_initialized", "笔记库尚未初始化"))?;
        let string = |key: &str| {
            arguments
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::new("tool_invalid_arguments", format!("缺少参数 {}", key)))
        };
        let value = match (namespace, tool) {
            ("page", "search") => serde_json::to_value(pages.search(string("query")?)?).unwrap(),
            ("page", "read") => serde_json::to_value(pages.get(string("id")?, false)?).unwrap(),
            ("page", "read_document") => serde_json::to_value(pages.open_bundle(string("id")?)?.document).unwrap(),
            ("attachment", "read") => {
                let store = self.conversations.read().await.clone().ok_or_else(|| AppError::new("conversation_not_initialized", "对话存储尚未初始化"))?;
                json!({ "text": store.read_attachment_text(string("path")?)? })
            }
            ("page", "create") => serde_json::to_value(
                pages.create(
                    string("title")?.to_owned(),
                    arguments
                        .get("parentId")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    string("content")?.to_owned(),
                )?,
            )
            .unwrap(),
            ("page", "update") => serde_json::to_value(pages.update_checked(
                string("id")?,
                string("title")?.to_owned(),
                string("content")?.to_owned(),
                string("expectedTitle")?,
                string("expectedContent")?,
            )?)
            .unwrap(),
            ("page", "create_block") | ("page", "update_block") | ("page", "move_block") | ("page", "delete_block") => {
                let page_id = string("pageId")?;
                let revision = arguments.get("revision").and_then(Value::as_i64).ok_or_else(|| AppError::new("tool_invalid_arguments", "缺少 revision"))?;
                let mut document = pages.open_bundle(page_id)?.document;
                match tool {
                    "create_block" => {
                        let block = serde_json::from_value(arguments.get("block").cloned().ok_or_else(|| AppError::new("tool_invalid_arguments", "缺少 block"))?).map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                        let position = arguments.get("position").and_then(Value::as_u64).unwrap_or(document.blocks.len() as u64) as usize;
                        insert_block(
                            &mut document.blocks,
                            arguments.get("parentId").and_then(Value::as_str),
                            position,
                            block,
                        )?;
                    }
                    "update_block" => {
                        let block: crate::pages::model::BlockNode = serde_json::from_value(arguments.get("block").cloned().ok_or_else(|| AppError::new("tool_invalid_arguments", "缺少 block"))?).map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                        replace_block(&mut document.blocks, block)?;
                    }
                    "move_block" => {
                        let block_id = string("blockId")?;
                        let position = arguments.get("position").and_then(Value::as_u64).unwrap_or(0) as usize;
                        move_block(
                            &mut document.blocks,
                            block_id,
                            arguments.get("parentId").and_then(Value::as_str),
                            position,
                        )?;
                    }
                    "delete_block" => {
                        let block_id = string("blockId")?;
                        delete_block(&mut document.blocks, block_id)?;
                    }
                    _ => unreachable!(),
                }
                serde_json::to_value(pages.save_document(document, revision)?).unwrap()
            }
            ("page", "move") => serde_json::to_value(
                pages.move_page(
                    string("id")?,
                    arguments
                        .get("parentId")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    arguments
                        .get("sortOrder")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                )?,
            )
            .unwrap(),
            ("page", "archive") => {
                let expected: Page =
                    serde_json::from_value(arguments.get("expectedPage").cloned().ok_or_else(
                        || AppError::new("tool_invalid_arguments", "缺少 expectedPage"),
                    )?)
                    .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                serde_json::to_value(pages.trash_checked(string("id")?, &expected)?).unwrap()
            }
            ("relation", "backlinks") => {
                let target_id = string("id")?;
                let mut links = pages.backlinks(target_id)?;
                let databases = DatabaseRepository::from_pages(pages.clone())?;
                for summary in pages.list(false)? {
                    let record = pages.get(&summary.id, false)?;
                    let Some(database_id) = record.database_id.as_deref() else {
                        continue;
                    };
                    let definition = databases.get(database_id)?;
                    for field in definition
                        .fields
                        .iter()
                        .filter(|field| matches!(field.field_type, DatabaseFieldType::Relation))
                    {
                        let contains_target = record
                            .properties
                            .get(&field.id)
                            .and_then(Value::as_array)
                            .is_some_and(|values| {
                                values.iter().any(|value| value.as_str() == Some(target_id))
                            });
                        if contains_target {
                            links.push(PageLink {
                                source_id: record.id.clone(),
                                target_id: Some(target_id.to_owned()),
                                label: field.name.clone(),
                            });
                        }
                    }
                }
                serde_json::to_value(links).unwrap()
            }
            ("relation", "link" | "unlink") => {
                let record = pages.get(string("sourceId")?, false)?;
                let field_id = string("fieldId")?;
                let target_id = string("targetId")?;
                pages.get(target_id, false)?;
                let mut properties = record.properties.clone();
                let mut ids: Vec<String> = properties
                    .get(field_id)
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                if tool == "link" {
                    if !ids.iter().any(|id| id == target_id) {
                        ids.push(target_id.to_owned());
                    }
                } else {
                    ids.retain(|id| id != target_id);
                }
                properties.insert(field_id.to_owned(), json!(ids));
                let repo = DatabaseRepository::from_pages(pages.clone())?;
                let database_id = record.database_id.clone().ok_or_else(|| {
                    AppError::new("database_record_not_found", "源页面不是数据库记录")
                })?;
                let expected: Page = serde_json::from_value(
                    arguments.get("expectedPage").cloned().ok_or_else(|| {
                        AppError::new("tool_invalid_arguments", "缺少 expectedPage")
                    })?,
                ).map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                let updated = repo.update_record_checked(
                    &database_id, &record.id, record.title, properties, &expected,
                )?;
                serde_json::to_value(updated).unwrap()
            }
            ("database", "read") => {
                let repo = DatabaseRepository::from_pages(pages.clone())?;
                serde_json::to_value(repo.get(string("databaseId")?)?).unwrap()
            }
            ("database", "query") => {
                let repo = DatabaseRepository::from_pages(pages.clone())?;
                let query = serde_json::from_value(json!({
                    "filters": arguments.get("filters").cloned().unwrap_or_else(|| json!([])),
                    "sorts": arguments.get("sorts").cloned().unwrap_or_else(|| json!([]))
                }))
                .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                serde_json::to_value(repo.query(string("databaseId")?, query)?).unwrap()
            }
            ("database", "create_record") => {
                let repo = DatabaseRepository::from_pages(pages.clone())?;
                let properties = serde_json::from_value(
                    arguments
                        .get("properties")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )
                .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                serde_json::to_value(repo.create_record(
                    string("databaseId")?,
                    string("title")?.to_owned(),
                    properties,
                )?)
                .unwrap()
            }
            ("database", "update_record") => {
                let repo = DatabaseRepository::from_pages(pages.clone())?;
                let record = pages.get(string("id")?, false)?;
                let database_id = record.database_id.clone().ok_or_else(|| {
                    AppError::new("database_record_not_found", "记录不属于数据库")
                })?;
                let properties = serde_json::from_value(
                    arguments
                        .get("properties")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )
                .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                let expected: Page =
                    serde_json::from_value(arguments.get("expectedPage").cloned().ok_or_else(
                        || AppError::new("tool_invalid_arguments", "缺少 expectedPage"),
                    )?)
                    .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                serde_json::to_value(
                    repo.update_record_checked(
                        &database_id,
                        &record.id,
                        arguments
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or(&record.title)
                            .to_owned(),
                        properties,
                        &expected,
                    )?,
                )
                .unwrap()
            }
            ("database", "update_schema") => {
                let repo = DatabaseRepository::from_pages(pages.clone())?;
                let schema = arguments
                    .get("schema")
                    .ok_or_else(|| AppError::new("tool_invalid_arguments", "缺少 schema"))?;
                let field_type: DatabaseFieldType =
                    serde_json::from_value(schema.get("fieldType").cloned().ok_or_else(|| {
                        AppError::new("tool_invalid_arguments", "缺少 fieldType")
                    })?)
                    .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                let expected: DatabaseDefinition = serde_json::from_value(
                    arguments
                        .get("expectedDefinition")
                        .cloned()
                        .ok_or_else(|| {
                            AppError::new("tool_invalid_arguments", "缺少 expectedDefinition")
                        })?,
                )
                .map_err(|error| AppError::new("tool_invalid_arguments", error.to_string()))?;
                serde_json::to_value(
                    repo.add_field_checked(
                        string("databaseId")?,
                        &expected,
                        schema
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("新字段")
                            .to_owned(),
                        field_type,
                        schema
                            .get("options")
                            .and_then(Value::as_array)
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    )?,
                )
                .unwrap()
            }
            _ => {
                return Err(AppError::new(
                    "tool_not_supported",
                    format!("不支持工具 {}.{}", namespace, tool),
                ))
            }
        };
        Ok(value)
    }

    async fn describe_change(
        &self,
        namespace: &str,
        tool: &str,
        arguments: &Value,
    ) -> Result<String, AppError> {
        let pages = self
            .pages
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::new("vault_not_initialized", "笔记库尚未初始化"))?;
        let string = |key: &str| {
            arguments
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::new("tool_invalid_arguments", format!("缺少参数 {}", key)))
        };
        match (namespace, tool) {
            ("page", "update") => {
                let current = pages.get(string("id")?, false)?;
                let title = string("title")?;
                let content = string("content")?;
                Ok(format!(
                    "页面 {} ({})\n\n标题\n- {}\n+ {}\n\n正文\n--- 当前内容\n+++ Agent 内容\n{}",
                    current.title,
                    current.id,
                    current.title,
                    title,
                    line_diff(&current.content, content)
                ))
            }
            ("page", "archive") => {
                let current = pages.get(string("id")?, false)?;
                Ok(format!(
                    "归档页面\n- 标题：{}\n- UUID：{}\n- 当前正文：{} 个字符",
                    current.title,
                    current.id,
                    current.content.chars().count()
                ))
            }
            ("database", "update_schema") => {
                let repo = DatabaseRepository::from_pages(pages)?;
                let current = repo.get(string("databaseId")?)?;
                let proposed = arguments
                    .get("schema")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Ok(format!(
                    "修改数据库字段\n--- 当前结构\n{}\n+++ Agent 结构\n{}",
                    serde_json::to_string_pretty(&current).unwrap_or_default(),
                    serde_json::to_string_pretty(&proposed).unwrap_or_default()
                ))
            }
            ("relation", "unlink") => {
                let source = pages.get(string("sourceId")?, false)?;
                let target = pages.get(string("targetId")?, false)?;
                Ok(format!(
                    "移除关系\n- 来源：{} ({})\n- 字段：{}\n- 目标：{} ({})",
                    source.title,
                    source.id,
                    string("fieldId")?,
                    target.title,
                    target.id
                ))
            }
            _ if is_write_tool(namespace, tool) => Ok(format!(
                "执行 {}.{}\n{}",
                namespace,
                tool,
                serde_json::to_string_pretty(arguments).unwrap_or_default()
            )),
            _ => Err(AppError::new(
                "tool_preview_unsupported",
                "无法为此危险操作生成变更预览",
            )),
        }
    }

}

fn is_dangerous(namespace: &str, tool: &str) -> bool {
    matches!(
        (namespace, tool),
        ("page", "update" | "archive" | "create_block" | "update_block" | "move_block" | "delete_block")
            | ("database", "update_schema" | "update_record")
            | ("relation", "unlink")
    )
}

fn attach_read_baseline(
    reads: &HashMap<String, Value>,
    namespace: &str,
    tool: &str,
    arguments: &mut Value,
) -> Result<(), AppError> {
    let (id_key, snapshot_key) = match (namespace, tool) {
        ("page", "update") => ("id", ""),
        ("page", "archive") | ("database", "update_record") => ("id", "expectedPage"),
        ("relation", "link" | "unlink") => ("sourceId", "expectedPage"),
        ("database", "update_schema") => ("databaseId", "expectedDefinition"),
        _ => return Ok(()),
    };
    let id = arguments.get(id_key).and_then(Value::as_str)
        .ok_or_else(|| AppError::new("tool_invalid_arguments", format!("缺少参数 {id_key}")))?;
    let key = if snapshot_key == "expectedDefinition" { format!("database:{id}") } else { id.to_owned() };
    let previous = reads.get(&key).ok_or_else(|| {
        if snapshot_key == "expectedDefinition" {
            AppError::new("database_read_required", "修改数据库结构前必须先调用 database_read")
        } else {
            AppError::new("page_read_required", "修改页面或记录前必须先调用 page_read 或 database_query")
        }
    })?;
    if snapshot_key.is_empty() {
        arguments["expectedTitle"] = previous["title"].clone();
        arguments["expectedContent"] = previous["content"].clone();
    } else {
        arguments[snapshot_key] = previous.clone();
    }
    Ok(())
}
fn is_write_tool(namespace: &str, tool: &str) -> bool {
    matches!(
        (namespace, tool),
        ("page", "create" | "update" | "move" | "archive" | "create_block" | "update_block" | "move_block" | "delete_block")
            | (
                "database",
                "update_schema" | "create_record" | "update_record"
            )
            | ("relation", "link" | "unlink")
    )
}

#[cfg(test)]
fn thread_start_params(cwd: &Path, scope_instruction: &str, model: Option<&str>, dynamic_tools: bool) -> Value {
    let mut params = json!({
        "cwd": cwd.to_string_lossy(), "approvalPolicy": "never", "sandbox": "read-only",
        "serviceName": "qingyu_notes", "developerInstructions": scope_instruction,
        "environments": [], "selectedCapabilityRoots": [],
        "config": {
            "features": { "shell_tool": false, "apps": false, "plugins": false },
            "web_search": "disabled", "agents": { "enabled": false }, "mcp_servers": {}
        }
    });
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        params["model"] = json!(model);
    }
    if dynamic_tools {
        params["dynamicTools"] = qingyu_dynamic_tools();
    } else {
        params["developerInstructions"] = json!(format!(
            "{}\n当前 App Server 不支持轻羽动态工具。只能回答只读问题，不得尝试修改文件。",
            scope_instruction
        ));
    }
    params
}

#[cfg(test)]
fn dynamic_tools_unavailable(error: &AppError) -> bool {
    let message = error.message.to_lowercase();
    message.contains("dynamictool")
        || message.contains("experimental")
        || message.contains("unsupported")
        || message.contains("unknown field")
}

#[cfg(test)]
fn approval_matches(approval: &PendingApproval, generation: u64, vault_root: &str) -> bool {
    approval.connection_generation == generation && approval.vault_root == vault_root
}

#[cfg(test)]
fn codex_app_server_args() -> [&'static str; 15] {
    [
        "-c",
        "features.shell_tool=false",
        "-c",
        "features.apps=false",
        "-c",
        "features.plugins=false",
        "-c",
        "web_search=\"disabled\"",
        "-c",
        "agents.enabled=false",
        "-c",
        "mcp_servers={}",
        "app-server",
        "--listen",
        "stdio://",
    ]
}

fn line_diff(current: &str, proposed: &str) -> String {
    let removed = current
        .lines()
        .map(|line| format!("- {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let added = proposed
        .lines()
        .map(|line| format!("+ {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{removed}\n{added}")
}

fn emit_vault_change(app: &AppHandle, namespace: &str, tool: &str, arguments: &Value) {
    let page_ids = ["id", "pageId", "sourceId", "targetId"]
        .iter()
        .filter_map(|key| arguments.get(key).and_then(Value::as_str))
        .collect::<Vec<_>>();
    let database_ids = arguments
        .get("databaseId")
        .and_then(Value::as_str)
        .into_iter()
        .collect::<Vec<_>>();
    let _ = app.emit(
        "vault://changed",
        json!({
            "source": "agent",
            "namespace": namespace,
            "tool": tool,
            "pageIds": page_ids,
            "databaseIds": database_ids
        }),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use uuid::Uuid;

    use super::*;

    fn test_block(id: &str, children: Vec<crate::pages::model::BlockNode>) -> crate::pages::model::BlockNode {
        crate::pages::model::BlockNode {
            id: id.into(),
            block_type: "paragraph".into(),
            text: id.into(),
            attrs: BTreeMap::new(),
            children,
        }
    }

    #[test]
    fn structured_block_operations_recurse_through_nested_children() {
        let mut blocks = vec![test_block(
            "parent-a",
            vec![test_block("nested", Vec::new())],
        ), test_block("parent-b", Vec::new())];

        insert_block(
            &mut blocks,
            Some("parent-a"),
            1,
            test_block("created", Vec::new()),
        )
        .unwrap();
        replace_block(
            &mut blocks,
            test_block("nested", vec![test_block("nested-child", Vec::new())]),
        )
        .unwrap();
        move_block(&mut blocks, "nested", Some("parent-b"), 0).unwrap();
        delete_block(&mut blocks, "created").unwrap();

        assert_eq!(blocks[0].children.len(), 0);
        assert_eq!(blocks[1].children[0].id, "nested");
        assert_eq!(blocks[1].children[0].children[0].id, "nested-child");
    }

    #[test]
    fn moving_a_block_into_its_descendant_is_rejected_without_mutating_document() {
        let original = vec![test_block(
            "parent",
            vec![test_block("child", Vec::new())],
        )];
        let mut blocks = original.clone();

        let error = move_block(&mut blocks, "parent", Some("child"), 0).unwrap_err();

        assert_eq!(error.code, "block_invalid_parent");
        assert_eq!(blocks, original);
    }

    #[test]
    fn app_server_is_started_with_dynamic_tools_only() {
        let args = codex_app_server_args();
        assert!(args.contains(&"features.shell_tool=false"));
        assert!(args.contains(&"features.apps=false"));
        assert!(args.contains(&"features.plugins=false"));
        assert!(args.contains(&"web_search=\"disabled\""));
        assert!(args.contains(&"agents.enabled=false"));
        assert!(args.contains(&"mcp_servers={}"));
    }

    #[test]
    fn dangerous_tools_require_approval() {
        assert!(is_dangerous("page", "update"));
        assert!(is_dangerous("page", "archive"));
        assert!(is_dangerous("page", "create_block"));
        assert!(is_dangerous("page", "update_block"));
        assert!(is_dangerous("page", "move_block"));
        assert!(is_dangerous("page", "delete_block"));
        assert!(is_dangerous("database", "update_schema"));
        assert!(is_dangerous("database", "update_record"));
        assert!(is_dangerous("relation", "unlink"));
        assert!(!is_dangerous("page", "search"));
        assert!(!is_dangerous("relation", "link"));
        assert!(is_write_tool("page", "create"));
        assert!(is_write_tool("page", "update_block"));
        assert!(is_write_tool("database", "update_record"));
        assert!(!is_write_tool("page", "read"));
    }

    #[test]
    fn unsupported_dynamic_tools_fall_back_to_read_only_thread() {
        let error = AppError::new("codex_rpc_error", "unsupported dynamicTools capability");
        assert!(dynamic_tools_unavailable(&error));
        let params = thread_start_params(Path::new(r"C:\notes"), "读取当前页面", None, false);
        assert!(params.get("dynamicTools").is_none());
        assert!(params["developerInstructions"]
            .as_str()
            .unwrap()
            .contains("只能回答只读问题"));
    }

    #[test]
    fn approvals_are_bound_to_connection_and_vault() {
        let approval = PendingApproval {
            id: "call-1".into(),
            request_id: json!(1),
            thread_id: "thread-1".into(),
            turn_id: "turn-1".into(),
            namespace: "page".into(),
            tool: "update".into(),
            arguments: json!({}),
            diff: String::new(),
            connection_generation: 7,
            vault_root: r"C:\notes-a".into(),
        };
        assert!(approval_matches(&approval, 7, r"C:\notes-a"));
        assert!(!approval_matches(&approval, 8, r"C:\notes-a"));
        assert!(!approval_matches(&approval, 7, r"C:\notes-b"));
    }

    #[test]
    fn relation_backlinks_include_database_relation_fields() {
        tauri::async_runtime::block_on(async {
            let root =
                std::env::temp_dir().join(format!("qingyu-agent-relation-{}", Uuid::new_v4()));
            let pages = PageRepository::open(root.clone()).unwrap();
            let databases = DatabaseRepository::from_pages(pages.clone()).unwrap();
            let (database, _) = databases.create_database("项目".into(), None).unwrap();
            let relation = databases
                .add_field(
                    &database.id,
                    "关联".into(),
                    DatabaseFieldType::Relation,
                    vec![],
                )
                .unwrap();
            let source = databases
                .create_record(&database.id, "来源".into(), BTreeMap::new())
                .unwrap();
            let target = pages.create("目标".into(), None, String::new()).unwrap();
            let runtime = CodexRuntime::default();
            *runtime.pages.write().await = Some(pages);

            runtime
                .execute_tool(
                    "relation",
                    "link",
                    &json!({ "sourceId": source.id, "fieldId": relation.id, "targetId": target.id, "expectedPage": source }),
                )
                .await
                .unwrap();
            let stale = runtime
                .execute_tool(
                    "relation", "unlink",
                    &json!({ "sourceId": source.id, "fieldId": relation.id, "targetId": target.id, "expectedPage": source }),
                )
                .await
                .unwrap_err();
            assert_eq!(stale.code, "approval_stale_data");
            let backlinks = runtime
                .execute_tool("relation", "backlinks", &json!({ "id": target.id }))
                .await
                .unwrap();

            assert_eq!(backlinks.as_array().unwrap().len(), 1);
            assert_eq!(backlinks[0]["sourceId"], source.id);
            assert_eq!(backlinks[0]["label"], "关联");
            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn page_update_preview_contains_current_and_proposed_content() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!("qingyu-agent-diff-{}", Uuid::new_v4()));
            let pages = PageRepository::open(root.clone()).unwrap();
            let page = pages
                .create("旧标题".into(), None, "旧正文".into())
                .unwrap();
            let runtime = CodexRuntime::default();
            *runtime.pages.write().await = Some(pages);

            let preview = runtime
                .describe_change(
                    "page",
                    "update",
                    &json!({ "id": page.id, "title": "新标题", "content": "新正文" }),
                )
                .await
                .unwrap();

            assert!(preview.contains("- 旧标题"));
            assert!(preview.contains("+ 新标题"));
            assert!(preview.contains("- 旧正文"));
            assert!(preview.contains("+ 新正文"));
            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn database_record_update_rejects_changes_made_after_approval() {
        tauri::async_runtime::block_on(async {
            let root =
                std::env::temp_dir().join(format!("qingyu-agent-record-cas-{}", Uuid::new_v4()));
            let pages = PageRepository::open(root.clone()).unwrap();
            let databases = DatabaseRepository::from_pages(pages.clone()).unwrap();
            let (database, _) = databases.create_database("任务".into(), None).unwrap();
            let record = databases
                .create_record(&database.id, "原记录".into(), BTreeMap::new())
                .unwrap();
            let mut arguments = json!({ "id": record.id, "title": "Agent 修改", "properties": {} });
            assert_eq!(
                attach_read_baseline(&HashMap::new(), "database", "update_record", &mut arguments).unwrap_err().code,
                "page_read_required"
            );
            let reads = HashMap::from([(record.id.clone(), json!(record))]);
            pages
                .update_checked(&record.id, record.title.clone(), "用户正文".into(), &record.title, &record.content)
                .unwrap();
            attach_read_baseline(&reads, "database", "update_record", &mut arguments).unwrap();
            let runtime = CodexRuntime::default();
            *runtime.pages.write().await = Some(pages.clone());

            let error = runtime
                .execute_tool(
                    "database",
                    "update_record",
                    &arguments,
                )
                .await
                .unwrap_err();

            assert_eq!(error.code, "approval_stale_data");
            assert_eq!(pages.get(&record.id, false).unwrap().content, "用户正文");
            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn page_archive_rejects_changes_made_after_approval() {
        tauri::async_runtime::block_on(async {
            let root =
                std::env::temp_dir().join(format!("qingyu-agent-archive-cas-{}", Uuid::new_v4()));
            let pages = PageRepository::open(root.clone()).unwrap();
            let page = pages
                .create("原页面".into(), None, "原正文".into())
                .unwrap();
            pages
                .update_checked(
                    &page.id,
                    "用户修改".into(),
                    page.content.clone(),
                    &page.title,
                    &page.content,
                )
                .unwrap();
            let runtime = CodexRuntime::default();
            *runtime.pages.write().await = Some(pages.clone());

            let error = runtime
                .execute_tool(
                    "page",
                    "archive",
                    &json!({ "id": page.id, "expectedPage": page }),
                )
                .await
                .unwrap_err();

            assert_eq!(error.code, "approval_stale_data");
            assert!(pages.get(&page.id, false).is_ok());
            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn database_schema_update_rejects_changes_made_after_approval() {
        tauri::async_runtime::block_on(async {
            let root =
                std::env::temp_dir().join(format!("qingyu-agent-schema-cas-{}", Uuid::new_v4()));
            let pages = PageRepository::open(root.clone()).unwrap();
            let databases = DatabaseRepository::from_pages(pages.clone()).unwrap();
            let (database, definition) = databases.create_database("任务".into(), None).unwrap();
            databases
                .add_field(
                    &database.id,
                    "用户字段".into(),
                    DatabaseFieldType::Text,
                    vec![],
                )
                .unwrap();
            let runtime = CodexRuntime::default();
            *runtime.pages.write().await = Some(pages);

            let error = runtime
                .execute_tool(
                    "database",
                    "update_schema",
                    &json!({
                        "databaseId": database.id,
                        "schema": { "name": "Agent 字段", "fieldType": "text" },
                        "expectedDefinition": definition
                    }),
                )
                .await
                .unwrap_err();

            assert_eq!(error.code, "approval_stale_data");
            assert_eq!(databases.get(&database.id).unwrap().fields.len(), 1);
            let _ = std::fs::remove_dir_all(root);
        });
    }
}
