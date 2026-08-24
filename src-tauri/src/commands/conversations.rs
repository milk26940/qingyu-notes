use serde_json::Value;
use tauri::{AppHandle, State};

use super::pages::PageState;
use crate::codex::conversation::{
    ConversationMetadata, ConversationRecord, ConversationScope, ConversationStore,
};
use crate::codex::runtime::{CodexRuntime, PendingApproval};
use crate::error::AppError;

async fn setup(
    app: &AppHandle,
    pages: &State<'_, PageState>,
    runtime: &State<'_, CodexRuntime>,
) -> Result<ConversationStore, AppError> {
    let repository = super::pages::repo(pages).await?;
    let store = runtime.configure_vault(repository).await?;
    runtime.start(app).await?;
    Ok(store)
}

fn scope_instruction(scope: &ConversationScope) -> String {
    let focus = match scope {
        ConversationScope::Vault => {
            "当前上下文是整个轻羽笔记库。请按需使用 page.search 和 database.query。".to_owned()
        }
        ConversationScope::Page { page_id } => format!(
            "当前上下文聚焦页面 UUID {}。开始工作前优先调用 page.read。",
            page_id
        ),
        ConversationScope::Database { database_id } => format!(
            "当前上下文聚焦数据库 UUID {}。开始工作前优先调用 database.query。",
            database_id
        ),
    };
    format!("你运行在轻羽笔记软件中。{} 所有笔记业务读取和写入只能使用轻羽提供的 page、database、relation 动态工具；不得用 shell 或原始文件写入绕过历史、审批与索引。", focus)
}

#[tauri::command]
pub async fn conversation_create(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    title: String,
    scope: ConversationScope,
) -> Result<ConversationMetadata, AppError> {
    let repository = super::pages::repo(&pages).await?;
    let root = repository.root().to_path_buf();
    let store = setup(&app, &pages, &runtime).await?;
    let conversation = store.create(title, scope.clone())?;
    let thread_id = runtime
        .start_thread(&root, &scope_instruction(&scope))
        .await?;
    store.attach_thread(&conversation.id, thread_id)
}

#[tauri::command]
pub async fn conversation_list(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
) -> Result<Vec<ConversationMetadata>, AppError> {
    setup(&app, &pages, &runtime).await?.list()
}

#[tauri::command]
pub async fn conversation_history(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    id: String,
) -> Result<Vec<ConversationRecord>, AppError> {
    setup(&app, &pages, &runtime).await?.history(&id)
}

#[tauri::command]
pub async fn conversation_send(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    id: String,
    text: String,
) -> Result<Value, AppError> {
    let repository = super::pages::repo(&pages).await?;
    let root = repository.root().to_path_buf();
    let store = setup(&app, &pages, &runtime).await?;
    let conversation = store.get(&id)?;
    let thread_id = match conversation.thread_id {
        Some(thread_id) => {
            runtime.resume_thread(&thread_id).await?;
            thread_id
        }
        None => {
            let thread_id = runtime
                .start_thread(&root, &scope_instruction(&conversation.scope))
                .await?;
            store.attach_thread(&id, thread_id.clone())?;
            thread_id
        }
    };
    store.append_user_message(&id, &text)?;
    let turn_id = runtime.start_turn(&thread_id, &text, &root).await?;
    store.set_active_turn(&id, Some(turn_id.clone()))?;
    Ok(serde_json::json!({ "threadId": thread_id, "turnId": turn_id }))
}

#[tauri::command]
pub async fn conversation_interrupt(
    runtime: State<'_, CodexRuntime>,
    thread_id: String,
    turn_id: String,
) -> Result<(), AppError> {
    runtime.interrupt_turn(&thread_id, &turn_id).await
}

#[tauri::command]
pub async fn approval_list(
    runtime: State<'_, CodexRuntime>,
) -> Result<Vec<PendingApproval>, AppError> {
    Ok(runtime.approvals().await)
}

#[tauri::command]
pub async fn approval_resolve(
    app: AppHandle,
    runtime: State<'_, CodexRuntime>,
    id: String,
    approved: bool,
) -> Result<(), AppError> {
    runtime.resolve_approval(&app, &id, approved).await
}
