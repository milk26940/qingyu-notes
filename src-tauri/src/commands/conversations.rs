use serde_json::Value;
use tauri::{AppHandle, State};
use serde::Serialize;

use super::pages::PageState;
use crate::codex::conversation::{
    ConversationAttachment, ConversationMetadata, ConversationRecord, ConversationScope, ConversationStore,
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

fn codex_root(vault_root: &std::path::Path) -> std::path::PathBuf {
    vault_root.parent().and_then(|data| data.parent()).unwrap_or(vault_root).to_path_buf()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAttachmentData { mime: String, bytes: Vec<u8> }

fn scope_instruction(scope: &ConversationScope) -> String {
    let focus = match scope {
        ConversationScope::Vault => {
            "当前上下文是整个轻羽笔记库。请按需使用 knowledge.search、page.search 和 database.query。".to_owned()
        }
        ConversationScope::Page { page_id } => format!(
            "当前上下文聚焦页面 UUID {}。需要页面正文时再调用 page.read 或 page.read_document。",
            page_id
        ),
        ConversationScope::Database { database_id } => format!(
            "当前上下文聚焦数据库 UUID {}。开始工作前优先调用 database.query。",
            database_id
        ),
    };
    format!("你运行在轻羽笔记软件中。{} 用户询问笔记中的事实、概念或关系时，即使没有点名工具，也应先调用 knowledge.search，再按需读取 Wiki 或原始页面；普通闲聊无需强制检索。回答必须区分关键词、语义、Wiki 和图谱证据，引用使用可点击格式 [页面标题](qingyu://page/<pageId>)，不得把无来源的常识写成笔记证据。所有笔记业务读取和写入只能使用轻羽提供的 page、database、relation、knowledge 动态工具；不得用 shell 或原始文件写入绕过历史、审批与索引。", focus)
}

fn resumable_thread_id(
    thread_id: String,
    resume_result: Result<(), AppError>,
) -> Result<Option<String>, AppError> {
    match resume_result {
        Ok(()) => Ok(Some(thread_id)),
        Err(error) => {
            let message = error.message.to_lowercase();
            if message.contains("no rollout found for thread id")
                || message.contains("thread not found")
            {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

#[tauri::command]
pub async fn conversation_create(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    title: String,
    scope: ConversationScope,
    model: Option<String>,
) -> Result<ConversationMetadata, AppError> {
    let repository = super::pages::repo(&pages).await?;
    let root = codex_root(repository.root());
    let store = setup(&app, &pages, &runtime).await?;
    let conversation = store.create(title, scope.clone())?;
    let thread_id = runtime
        .start_thread(&root, &scope_instruction(&scope), model.as_deref())
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
    display_text: Option<String>,
    selected_text: Option<String>,
    attachments: Vec<ConversationAttachment>,
    model: Option<String>,
) -> Result<Value, AppError> {
    let repository = super::pages::repo(&pages).await?;
    let root = codex_root(repository.root());
    let store = setup(&app, &pages, &runtime).await?;
    let conversation = store.get(&id)?;
    let resumable_thread = match conversation.thread_id {
        Some(thread_id) => {
            let resume_result = runtime.resume_thread(&thread_id).await;
            resumable_thread_id(thread_id, resume_result)?
        }
        None => None,
    };
    let thread_id = match resumable_thread {
        Some(thread_id) => thread_id,
        None => {
            let thread_id = runtime
                .start_thread(&root, &scope_instruction(&conversation.scope), model.as_deref())
                .await?;
            store.attach_thread(&id, thread_id.clone())?;
            thread_id
        }
    };
    store.append_user_message(
        &id,
        &text,
        display_text.as_deref(),
        selected_text.as_deref(),
        &attachments,
    )?;
    let attachment_context = if attachments.is_empty() { String::new() } else { format!("\n\n本轮附件（使用 attachment.read 读取非图片附件）：\n{}", attachments.iter().map(|attachment| format!("- {} | {} | {}", attachment.name, attachment.mime, attachment.relative_path)).collect::<Vec<_>>().join("\n")) };
    let retrieval_rule = "\n\n轻羽检索规则：调用 knowledge.search 时，query 必须使用本轮用户问题原文，不得自行翻译、补充或扩展检索词。回答中的‘实际拆词’只能逐字复制工具返回的 queryTerms；关系名称和 evidence 必须逐字使用工具返回值，不得改写、拼接或声称工具未返回的验证结果。轻羽界面会单独展示权威检索依据。";
    let knowledge_query = display_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| selected_text.as_deref().map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(&text)
        .to_owned();
    let evidence_rule = "\n关键词结果按页面去重，必须使用 matchedTerms 与 matches 说明该页全部命中词和片段，不能把 matchedTerm 当作全部命中。图谱关系只能引用 graph.relations 中通过校验的关系；Wiki 是 AI 整理内容，Wiki 正文中的关系或用户保留的旧关系不等同于已验证图谱证据，需读取原始笔记核对后再作为结论。";
    let turn_id = runtime.start_turn(&thread_id, &format!("{}{}{}{}", text, attachment_context, retrieval_rule, evidence_rule), &knowledge_query, &root, &attachments, model.as_deref()).await?;
    store.set_active_turn(&id, Some(turn_id.clone()))?;
    Ok(serde_json::json!({ "threadId": thread_id, "turnId": turn_id }))
}

#[tauri::command]
pub async fn conversation_store_attachment(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    id: String,
    original_name: String,
    mime: String,
    bytes: Vec<u8>,
) -> Result<ConversationAttachment, AppError> {
    setup(&app, &pages, &runtime).await?.store_attachment(&id, &original_name, &mime, &bytes)
}

#[tauri::command]
pub async fn conversation_store_attachment_path(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    id: String,
    source_path: String,
) -> Result<ConversationAttachment, AppError> {
    let path = std::path::PathBuf::from(source_path);
    let metadata = std::fs::metadata(&path).map_err(|error| AppError::new("attachment_read_failed", error.to_string()))?;
    if !metadata.is_file() || metadata.len() > 50 * 1024 * 1024 { return Err(AppError::new("attachment_invalid_file", "附件不是文件或超过 50 MB")); }
    let name = path.file_name().and_then(|value| value.to_str()).ok_or_else(|| AppError::new("attachment_invalid_name", "附件名称无效"))?;
    let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match extension.as_str() { "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "webp" => "image/webp", "pdf" => "application/pdf", "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation", "md" => "text/markdown", "txt" | "log" => "text/plain", "json" => "application/json", "csv" => "text/csv", "xml" => "application/xml", _ => "application/octet-stream" };
    let bytes = std::fs::read(&path).map_err(|error| AppError::new("attachment_read_failed", error.to_string()))?;
    setup(&app, &pages, &runtime).await?.store_attachment(&id, name, mime, &bytes)
}

#[tauri::command]
pub async fn conversation_read_attachment(
    app: AppHandle,
    pages: State<'_, PageState>,
    runtime: State<'_, CodexRuntime>,
    relative_path: String,
    mime: String,
) -> Result<ConversationAttachmentData, AppError> {
    let store = setup(&app, &pages, &runtime).await?;
    Ok(ConversationAttachmentData { mime, bytes: store.read_attachment_bytes(&relative_path)? })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_rollout_replaces_the_stale_thread() {
        let missing = AppError::new("codex_rpc_error", "no rollout found for thread id 01a033e9");
        assert_eq!(
            resumable_thread_id("old-thread".into(), Err(missing)).unwrap(),
            None
        );

        let retained = resumable_thread_id("old-thread".into(), Ok(())).unwrap();
        assert_eq!(retained.as_deref(), Some("old-thread"));
    }
}
