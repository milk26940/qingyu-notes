use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::task::JoinHandle;

#[cfg(windows)]
use super::command::codex_creation_flags;
use super::command::resolve_codex_launch;
use super::conversation::{ConversationAttachment, ConversationStore};
use super::protocol::{
    initialize_request, parse_initialize_response, parse_rpc_message, qingyu_dynamic_tools,
    rpc_notification, rpc_request, rpc_response, CodexEvent, CodexInitializeResult,
    CodexServerRequest, RpcMessage,
};
use crate::backup::model::BackupScope;
use crate::backup::repository::BackupRepository;
use crate::database::model::{DatabaseDefinition, DatabaseFieldType};
use crate::database::repository::DatabaseRepository;
use crate::error::AppError;
use crate::pages::model::{BlockNode, Page, PageLink};
use crate::pages::repository::PageRepository;

type PendingResponse = oneshot::Sender<Result<Value, AppError>>;

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

struct CodexProcess {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    reader_task: JoinHandle<()>,
}

impl Drop for CodexProcess {
    fn drop(&mut self) {
        self.reader_task.abort();
        let _ = self.child.start_kill();
    }
}

#[derive(Clone)]
pub struct CodexRuntime {
    status: Arc<RwLock<CodexStatus>>,
    process: Arc<Mutex<Option<CodexProcess>>>,
    pending: Arc<Mutex<HashMap<i64, PendingResponse>>>,
    next_id: Arc<AtomicI64>,
    pages: Arc<RwLock<Option<PageRepository>>>,
    conversations: Arc<RwLock<Option<ConversationStore>>>,
    approvals: Arc<Mutex<HashMap<String, PendingApproval>>>,
    lifecycle: Arc<Mutex<()>>,
    generation: Arc<AtomicU64>,
    vault_root: Arc<RwLock<Option<String>>>,
    knowledge_queries: Arc<Mutex<HashMap<String, String>>>,
}

impl Default for CodexRuntime {
    fn default() -> Self {
        Self {
            status: Arc::new(RwLock::new(CodexStatus::Stopped)),
            process: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicI64::new(1)),
            pages: Arc::new(RwLock::new(None)),
            conversations: Arc::new(RwLock::new(None)),
            approvals: Arc::new(Mutex::new(HashMap::new())),
            lifecycle: Arc::new(Mutex::new(())),
            generation: Arc::new(AtomicU64::new(0)),
            vault_root: Arc::new(RwLock::new(None)),
            knowledge_queries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl CodexRuntime {
    pub async fn status(&self) -> CodexStatus {
        self.status.read().await.clone()
    }

    pub async fn configure_vault(
        &self,
        pages: PageRepository,
    ) -> Result<ConversationStore, AppError> {
        let store = ConversationStore::open(pages.root())?;
        let root = pages.root().to_string_lossy().to_string();
        let changed = self.vault_root.read().await.as_deref() != Some(root.as_str());
        if changed {
            self.approvals.lock().await.clear();
            *self.vault_root.write().await = Some(root);
        }
        *self.pages.write().await = Some(pages);
        *self.conversations.write().await = Some(store.clone());
        if changed && matches!(*self.status.read().await, CodexStatus::Ready { .. }) {
            self.resume_registered_threads().await;
        }
        Ok(store)
    }

    pub async fn start(&self, app: &AppHandle) -> Result<CodexStatus, AppError> {
        let _lifecycle = self.lifecycle.lock().await;
        if matches!(
            *self.status.read().await,
            CodexStatus::Ready { .. } | CodexStatus::Starting
        ) {
            return Ok(self.status().await);
        }
        self.set_status(app, CodexStatus::Starting).await;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let launch = resolve_codex_launch().inspect_err(|error| {
            let failure = error.clone();
            let status = self.status.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                *status.write().await = CodexStatus::Failed {
                    code: failure.code.clone(),
                    message: failure.message.clone(),
                };
                let _ = app.emit("codex://status", status.read().await.clone());
            });
        })?;
        let mut command = Command::new(launch.program);
        command
            .args(launch.prefix_args)
            .args(codex_app_server_args())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        command.creation_flags(codex_creation_flags());
        let mut child = command
            .spawn()
            .map_err(|error| AppError::new("codex_start_failed", error.to_string()))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or_else(|| {
            AppError::new("codex_start_failed", "无法打开 Codex 标准输入")
        })?));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::new("codex_start_failed", "无法打开 Codex 标准输出"))?;
        let mut reader = BufReader::new(stdout);
        write_json(&stdin, &initialize_request(0)).await?;
        write_json(&stdin, &rpc_notification("initialized", json!({}))).await?;
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
            .await
            .map_err(|_| AppError::new("codex_handshake_timeout", "Codex 初始化超时"))?
            .map_err(|error| AppError::new("codex_handshake_failed", error.to_string()))?;
        let result: CodexInitializeResult = parse_initialize_response(line.trim_end())?;

        let runtime = self.clone();
        let event_app = app.clone();
        let reader_stdin = stdin.clone();
        let reader_task = tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        runtime
                            .route_message(&event_app, &reader_stdin, line.trim_end())
                            .await
                    }
                }
            }
            if runtime.generation.load(Ordering::SeqCst) == generation {
                runtime.fail_pending("Codex App Server 已断开").await;
                runtime.approvals.lock().await.clear();
                runtime
                    .set_status(
                        &event_app,
                        CodexStatus::Failed {
                            code: "codex_disconnected".into(),
                            message: "Codex App Server 已断开，正在等待重连".into(),
                        },
                    )
                    .await;
            }
        });
        *self.process.lock().await = Some(CodexProcess {
            child,
            stdin,
            reader_task,
        });
        self.set_status(
            app,
            CodexStatus::Ready {
                user_agent: result.user_agent,
                codex_home: result.codex_home,
            },
        )
        .await;
        self.resume_registered_threads().await;
        Ok(self.status().await)
    }

    pub async fn stop(&self, app: &AppHandle) -> Result<CodexStatus, AppError> {
        let _lifecycle = self.lifecycle.lock().await;
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(mut process) = self.process.lock().await.take() {
            process.reader_task.abort();
            process
                .child
                .kill()
                .await
                .map_err(|error| AppError::new("codex_stop_failed", error.to_string()))?;
            let _ = process.child.wait().await;
        }
        self.fail_pending("Codex 已停止").await;
        self.approvals.lock().await.clear();
        self.set_status(app, CodexStatus::Stopped).await;
        Ok(CodexStatus::Stopped)
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, AppError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let stdin = self.stdin().await?;
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        if let Err(error) = write_json(&stdin, &rpc_request(id, method, params)).await {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(Duration::from_secs(60), receiver).await {
            Ok(result) => result
                .map_err(|_| AppError::new("codex_request_cancelled", "Codex 请求通道已关闭"))?,
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(AppError::new(
                    "codex_request_timeout",
                    format!("{} 请求超时", method),
                ))
            }
        }
    }

    pub async fn list_models(&self) -> Result<Vec<CodexModelOption>, AppError> {
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let result = self
                .request(
                    "model/list",
                    json!({ "cursor": cursor, "limit": 100, "includeHidden": false }),
                )
                .await?;
            let page = result
                .get("data")
                .cloned()
                .ok_or_else(|| AppError::new("codex_invalid_models", "Codex 未返回模型列表"))?;
            models.extend(
                serde_json::from_value::<Vec<CodexModelOption>>(page)
                    .map_err(|error| AppError::new("codex_invalid_models", error.to_string()))?,
            );
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(models)
    }

    pub async fn start_thread(
        &self,
        cwd: &Path,
        scope_instruction: &str,
        model: Option<&str>,
    ) -> Result<String, AppError> {
        let result = match self
            .request(
                "thread/start",
                thread_start_params(cwd, scope_instruction, model, true),
            )
            .await
        {
            Ok(result) => result,
            Err(error) if dynamic_tools_unavailable(&error) => {
                self.request(
                    "thread/start",
                    thread_start_params(cwd, scope_instruction, model, false),
                )
                .await?
            }
            Err(error) => return Err(error),
        };
        result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| AppError::new("codex_invalid_thread", "Codex 未返回 threadId"))
    }

    pub async fn resume_thread(&self, thread_id: &str) -> Result<(), AppError> {
        self.request("thread/resume", json!({ "threadId": thread_id }))
            .await
            .map(|_| ())
    }

    pub async fn start_turn(
        &self,
        thread_id: &str,
        text: &str,
        original_user_query: &str,
        cwd: &Path,
        attachments: &[ConversationAttachment],
        model: Option<&str>,
    ) -> Result<String, AppError> {
        let mut input = vec![json!({ "type": "text", "text": text })];
        if let Some(store) = self.conversations.read().await.clone() {
            for attachment in attachments.iter().filter(|attachment| attachment.mime.starts_with("image/")) {
                let path = store.attachment_path(&attachment.relative_path)?;
                input.push(json!({ "type": "localImage", "path": path.to_string_lossy() }));
            }
        }
        let mut params = json!({
            "threadId": thread_id, "input": input,
            "cwd": cwd.to_string_lossy(), "approvalPolicy": "never",
            "sandboxPolicy": { "type": "readOnly", "networkAccess": false }
        });
        if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
            params["model"] = json!(model);
        }
        self.knowledge_queries
            .lock()
            .await
            .insert(thread_id.to_owned(), original_user_query.to_owned());
        let result = self
            .request(
                "turn/start",
                params,
            )
            .await;
        if result.is_err() {
            self.knowledge_queries.lock().await.remove(thread_id);
        }
        let result = result?;
        result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| AppError::new("codex_invalid_turn", "Codex 未返回 turnId"))
    }

    pub async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<(), AppError> {
        self.request(
            "turn/interrupt",
            json!({ "threadId": thread_id, "turnId": turn_id }),
        )
        .await
        .map(|_| ())
    }

    pub async fn approvals(&self) -> Vec<PendingApproval> {
        self.approvals.lock().await.values().cloned().collect()
    }

    pub async fn resolve_approval(
        &self,
        app: &AppHandle,
        id: &str,
        approved: bool,
    ) -> Result<(), AppError> {
        let approval = self
            .approvals
            .lock()
            .await
            .remove(id)
            .ok_or_else(|| AppError::new("approval_not_found", "审批请求不存在"))?;
        let current_generation = self.generation.load(Ordering::SeqCst);
        let current_root = self.vault_root.read().await.clone().unwrap_or_default();
        if !approval_matches(&approval, current_generation, &current_root) {
            return Err(AppError::new(
                "approval_stale",
                "审批所属连接或笔记库已变化，请重新发起操作",
            ));
        }
        let stdin = self.stdin().await?;
        let result = if approved {
            let pages = self
                .pages
                .read()
                .await
                .clone()
                .ok_or_else(|| AppError::new("vault_not_initialized", "笔记库尚未初始化"));
            match pages.and_then(|pages| {
                BackupRepository::open(pages.root())?
                    .checkpoint(BackupScope::Vault, "agentDangerousWrite")
            }) {
                Ok(_) => {
                    self.execute_tool(&approval.namespace, &approval.tool, &approval.arguments)
                        .await
                }
                Err(error) => Err(AppError::new(
                    "agent_backup_failed",
                    format!("危险操作已阻止：备份失败：{}", error.message),
                )),
            }
        } else {
            Err(AppError::new("tool_rejected", "用户拒绝了此操作"))
        };
        if let Some(store) = self.conversations.read().await.clone() {
            let _ = store.append_approval(
                &approval.thread_id,
                &approval.id,
                &approval.namespace,
                &approval.tool,
                &approval.diff,
                Some(approved),
            );
            let _ = store.append_tool_outcome(
                &approval.thread_id,
                &approval.id,
                &approval.namespace,
                &approval.tool,
                result.is_ok(),
            );
            if result.is_ok() {
                let _ = store.append_change_summary(
                    &approval.thread_id,
                    &approval.id,
                    &approval.namespace,
                    &approval.tool,
                    &approval.diff,
                );
            }
        }
        if result.is_ok() {
            emit_vault_change(
                app,
                &approval.namespace,
                &approval.tool,
                &approval.arguments,
            );
        }
        write_tool_result(&stdin, approval.request_id, result).await
    }

    async fn stdin(&self) -> Result<Arc<Mutex<ChildStdin>>, AppError> {
        self.process
            .lock()
            .await
            .as_ref()
            .map(|process| process.stdin.clone())
            .ok_or_else(|| AppError::new("codex_not_running", "Codex 尚未连接"))
    }

    async fn route_message(&self, app: &AppHandle, stdin: &Arc<Mutex<ChildStdin>>, line: &str) {
        match parse_rpc_message(line) {
            Ok(RpcMessage::Response { id, result, error }) => {
                if let Some(sender) = self.pending.lock().await.remove(&id) {
                    let value = match error {
                        Some(error) => Err(AppError::new("codex_rpc_error", error.message)),
                        None => Ok(result.unwrap_or_else(|| json!({}))),
                    };
                    let _ = sender.send(value);
                }
            }
            Ok(RpcMessage::Event(event)) => {
                if let Some(thread_id) = event.thread_id() {
                    if let Some(store) = self.conversations.read().await.clone() {
                        if let Err(error) = store.append_event(thread_id, &event) {
                            let _ = app.emit("codex://persistence-error", error);
                        }
                        match &event {
                            CodexEvent::TurnStarted { turn, .. } => {
                                if let Err(error) = store.set_active_turn_for_thread(
                                    thread_id,
                                    turn.get("id").and_then(Value::as_str).map(str::to_owned),
                                ) {
                                    let _ = app.emit("codex://persistence-error", error);
                                }
                            }
                            CodexEvent::TurnCompleted { .. } => {
                                self.knowledge_queries.lock().await.remove(thread_id);
                                if let Err(error) =
                                    store.set_active_turn_for_thread(thread_id, None)
                                {
                                    let _ = app.emit("codex://persistence-error", error);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let _ = app.emit("codex://event", event);
            }
            Ok(RpcMessage::ServerRequest(request)) => {
                self.handle_server_request(app, stdin, request).await
            }
            Err(error) => {
                let _ = app.emit("codex://warning", error);
            }
        }
    }

    async fn handle_server_request(
        &self,
        app: &AppHandle,
        stdin: &Arc<Mutex<ChildStdin>>,
        request: CodexServerRequest,
    ) {
        if request.method != "item/tool/call" {
            let _ = write_tool_result(
                stdin,
                request.id,
                Err(AppError::new(
                    "unsupported_server_request",
                    "轻羽不支持此服务端请求",
                )),
            )
            .await;
            return;
        }
        let namespace = request
            .params
            .get("namespace")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let tool = request
            .params
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut arguments = request
            .params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if is_dangerous(&namespace, &tool) {
            if let Some(pages) = self.pages.read().await.clone() {
                if let Err(error) =
                    attach_approval_baseline(&pages, &namespace, &tool, &mut arguments)
                {
                    let _ = write_tool_result(stdin, request.id, Err(error)).await;
                    return;
                }
            }
        }
        let call_id = request
            .params
            .get("callId")
            .and_then(Value::as_str)
            .unwrap_or("tool-call")
            .to_owned();
        let thread_id = request
            .params
            .get("threadId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let turn_id = request
            .params
            .get("turnId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let original_query = self.knowledge_queries.lock().await.get(&thread_id).cloned();
        force_knowledge_query(
            &namespace,
            &tool,
            &mut arguments,
            original_query.as_deref(),
        );
        if let Some(store) = self.conversations.read().await.clone() {
            if let Err(error) =
                store.append_server_request(&thread_id, &request.method, &request.params)
            {
                let _ = app.emit("codex://persistence-error", error);
            }
        }
        if is_dangerous(&namespace, &tool) {
            let diff = match self.describe_change(&namespace, &tool, &arguments).await {
                Ok(diff) => diff,
                Err(error) => {
                    let _ = write_tool_result(stdin, request.id, Err(error)).await;
                    return;
                }
            };
            let approval = PendingApproval {
                id: call_id.clone(),
                request_id: request.id,
                thread_id,
                turn_id,
                namespace,
                tool,
                diff,
                arguments,
                connection_generation: self.generation.load(Ordering::SeqCst),
                vault_root: self.vault_root.read().await.clone().unwrap_or_default(),
            };
            self.approvals
                .lock()
                .await
                .insert(call_id, approval.clone());
            if let Some(store) = self.conversations.read().await.clone() {
                let _ = store.append_approval(
                    &approval.thread_id,
                    &approval.id,
                    &approval.namespace,
                    &approval.tool,
                    &approval.diff,
                    None,
                );
            }
            let _ = app.emit(
                "codex://event",
                CodexEvent::TurnDiffUpdated {
                    thread_id: approval.thread_id.clone(),
                    turn_id: approval.turn_id.clone(),
                    diff: approval.diff.clone(),
                },
            );
            // 自动执行仍经过既有的路径校验、版本检查和变更记录。
            if let Err(error) = self.resolve_approval(app, &approval.id, true).await {
                let _ = app.emit("codex://persistence-error", error);
            }
        } else {
            let diff = if is_write_tool(&namespace, &tool) {
                self.describe_change(&namespace, &tool, &arguments)
                    .await
                    .ok()
            } else {
                None
            };
            if let Some(diff) = &diff {
                let event = CodexEvent::TurnDiffUpdated {
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    diff: diff.clone(),
                };
                if let Some(store) = self.conversations.read().await.clone() {
                    let _ = store.append_event(&thread_id, &event);
                }
                let _ = app.emit("codex://event", event);
            }
            let result = self.execute_tool(&namespace, &tool, &arguments).await;
            if namespace == "knowledge" && tool == "search" {
                if let Ok(value) = &result {
                    let event = CodexEvent::KnowledgeSearchResult {
                        thread_id: thread_id.clone(),
                        turn_id: turn_id.clone(),
                        call_id: call_id.clone(),
                        result: value.clone(),
                    };
                    if let Some(store) = self.conversations.read().await.clone() {
                        let _ = store.append_event(&thread_id, &event);
                    }
                    let _ = app.emit("codex://event", event);
                }
            }
            if let Some(store) = self.conversations.read().await.clone() {
                let _ = store.append_tool_outcome(
                    &thread_id,
                    &call_id,
                    &namespace,
                    &tool,
                    result.is_ok(),
                );
                if result.is_ok() {
                    if let Some(diff) = &diff {
                        let _ = store
                            .append_change_summary(&thread_id, &call_id, &namespace, &tool, diff);
                    }
                }
            }
            if result.is_ok() {
                emit_vault_change(app, &namespace, &tool, &arguments);
            }
            let _ = write_tool_result(stdin, request.id, result).await;
        }
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
            ("knowledge", "search") => {
                let query=string("query")?;
                let knowledge=crate::semantic::knowledge::search(&pages,query)?;
                let effective_query = knowledge["effectiveQuery"].as_str().unwrap_or_default();
                let semantic_result = if effective_query.is_empty() {
                    Ok(Vec::new())
                } else {
                    crate::commands::semantic::search_repository(&pages,effective_query).await
                };
                match semantic_result {
                    Ok(semantic)=>{
                        let semantic = semantic.into_iter().map(|item| json!({
                            "pageId":item.page_id,
                            "blockId":item.block_id,
                            "pageTitle":item.page_title,
                            "snippet":item.snippet,
                            "score":item.score,
                            "link":format!("qingyu://page/{}#{}", item.page_id, item.block_id)
                        })).collect::<Vec<_>>();
                        json!({
                        "originalQuery":knowledge["originalQuery"],
                        "effectiveQuery":knowledge["effectiveQuery"],
                        "queryTerms":knowledge["queryTerms"],
                        "keyword":knowledge["keyword"],
                        "semantic":semantic,
                        "wiki":knowledge["wiki"],
                        "graph":{"relations":knowledge["relations"],"matchedEntities":knowledge["matchedEntities"]}
                    })},
                    Err(error)=>json!({
                        "originalQuery":knowledge["originalQuery"],
                        "effectiveQuery":knowledge["effectiveQuery"],
                        "queryTerms":knowledge["queryTerms"],
                        "keyword":knowledge["keyword"],
                        "semantic":[],
                        "wiki":knowledge["wiki"],
                        "graph":{"relations":knowledge["relations"],"matchedEntities":knowledge["matchedEntities"]},
                        "semanticUnavailable":error.message
                    }),
                }
            }
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
                let expected: Option<Page> = if tool == "unlink" {
                    Some(
                        serde_json::from_value(arguments.get("expectedPage").cloned().ok_or_else(
                            || AppError::new("tool_invalid_arguments", "缺少 expectedPage"),
                        )?)
                        .map_err(|error| {
                            AppError::new("tool_invalid_arguments", error.to_string())
                        })?,
                    )
                } else {
                    None
                };
                let updated = if let Some(expected) = expected.as_ref() {
                    repo.update_record_checked(
                        &database_id,
                        &record.id,
                        record.title,
                        properties,
                        expected,
                    )?
                } else {
                    repo.update_record(&database_id, &record.id, record.title, properties)?
                };
                serde_json::to_value(updated).unwrap()
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

    async fn resume_registered_threads(&self) {
        let Some(store) = self.conversations.read().await.clone() else {
            return;
        };
        if let Ok(ids) = store.registered_thread_ids() {
            for id in ids {
                let _ = self.resume_thread(&id).await;
            }
        }
    }

    async fn fail_pending(&self, message: &str) {
        for (_, sender) in self.pending.lock().await.drain() {
            let _ = sender.send(Err(AppError::new("codex_disconnected", message)));
        }
    }

    async fn set_status(&self, app: &AppHandle, status: CodexStatus) {
        *self.status.write().await = status.clone();
        let _ = app.emit("codex://status", status);
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

fn attach_approval_baseline(
    pages: &PageRepository,
    namespace: &str,
    tool: &str,
    arguments: &mut Value,
) -> Result<(), AppError> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| AppError::new("tool_invalid_arguments", "工具参数必须是对象"))?;
    let id = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| AppError::new("tool_invalid_arguments", format!("缺少参数 {key}")))
    };
    match (namespace, tool) {
        ("page", "update") => {
            let current = pages.get(&id("id")?, false)?;
            object.insert("expectedTitle".into(), json!(current.title));
            object.insert("expectedContent".into(), json!(current.content));
        }
        ("page", "archive") | ("database", "update_record") => {
            let current = pages.get(&id("id")?, false)?;
            object.insert(
                "expectedPage".into(),
                serde_json::to_value(current).unwrap(),
            );
        }
        ("database", "update_schema") => {
            let database_id = id("databaseId")?;
            let definition = DatabaseRepository::from_pages(pages.clone())?.get(&database_id)?;
            object.insert(
                "expectedDefinition".into(),
                serde_json::to_value(definition).unwrap(),
            );
        }
        ("relation", "unlink") => {
            let current = pages.get(&id("sourceId")?, false)?;
            object.insert(
                "expectedPage".into(),
                serde_json::to_value(current).unwrap(),
            );
        }
        _ => {}
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

fn dynamic_tools_unavailable(error: &AppError) -> bool {
    let message = error.message.to_lowercase();
    message.contains("dynamictool")
        || message.contains("experimental")
        || message.contains("unsupported")
        || message.contains("unknown field")
}

fn force_knowledge_query(
    namespace: &str,
    tool: &str,
    arguments: &mut Value,
    original_query: Option<&str>,
) {
    if namespace == "knowledge" && tool == "search" {
        if let Some(query) = original_query {
            arguments["query"] = json!(query);
        }
    }
}

fn approval_matches(approval: &PendingApproval, generation: u64, vault_root: &str) -> bool {
    approval.connection_generation == generation && approval.vault_root == vault_root
}

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

async fn write_json(stdin: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<(), AppError> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|error| AppError::new("codex_invalid_request", error.to_string()))?;
    bytes.push(b'\n');
    let mut writer = stdin.lock().await;
    writer
        .write_all(&bytes)
        .await
        .map_err(|error| AppError::new("codex_write_failed", error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| AppError::new("codex_write_failed", error.to_string()))
}

async fn write_tool_result(
    stdin: &Arc<Mutex<ChildStdin>>,
    id: Value,
    result: Result<Value, AppError>,
) -> Result<(), AppError> {
    let response = match result {
        Ok(value) => rpc_response(
            id,
            Some(
                json!({ "success": true, "contentItems": [{ "type": "inputText", "text": serde_json::to_string(&value).unwrap_or_default() }] }),
            ),
            None,
        ),
        Err(error) => rpc_response(
            id,
            Some(
                json!({ "success": false, "contentItems": [{ "type": "inputText", "text": error.message }] }),
            ),
            None,
        ),
    };
    write_json(stdin, &response).await
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
    fn knowledge_search_uses_the_original_user_question() {
        let mut arguments = json!({ "query": "device tree Platform bus" });

        force_knowledge_query(
            "knowledge",
            "search",
            &mut arguments,
            Some("设备树和 Platform 总线是什么关系？"),
        );

        assert_eq!(arguments["query"], "设备树和 Platform 总线是什么关系？");
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
                    &json!({ "sourceId": source.id, "fieldId": relation.id, "targetId": target.id }),
                )
                .await
                .unwrap();
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
            databases
                .update_record(&database.id, &record.id, "用户修改".into(), BTreeMap::new())
                .unwrap();
            let runtime = CodexRuntime::default();
            *runtime.pages.write().await = Some(pages.clone());

            let error = runtime
                .execute_tool(
                    "database",
                    "update_record",
                    &json!({
                        "id": record.id,
                        "title": "Agent 修改",
                        "properties": {},
                        "expectedPage": record
                    }),
                )
                .await
                .unwrap_err();

            assert_eq!(error.code, "approval_stale_data");
            assert_eq!(pages.get(&record.id, false).unwrap().title, "用户修改");
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
