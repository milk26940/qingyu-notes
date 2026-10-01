use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexInitializeResult {
    pub user_agent: String,
    pub codex_home: String,
    pub platform_family: String,
    pub platform_os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexRpcError {
    pub code: Value,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexServerRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexItemSummary {
    pub id: Option<String>,
    pub item_type: Option<String>,
    pub status: Option<String>,
    pub tool: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CodexEvent {
    AgentMessageDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        delta: String,
    },
    TurnPlanUpdated {
        thread_id: String,
        turn_id: String,
        explanation: Option<String>,
        plan: Value,
    },
    TurnDiffUpdated {
        thread_id: String,
        turn_id: String,
        diff: String,
    },
    TurnStarted {
        thread_id: String,
        turn: Value,
    },
    TurnCompleted {
        thread_id: String,
        turn: Value,
    },
    ItemStarted {
        thread_id: String,
        turn_id: String,
        item: CodexItemSummary,
    },
    ItemCompleted {
        thread_id: String,
        turn_id: String,
        item: CodexItemSummary,
    },
    Other {
        method: String,
        params: Value,
    },
}

impl CodexEvent {
    pub fn thread_id(&self) -> Option<&str> {
        match self {
            Self::AgentMessageDelta { thread_id, .. }
            | Self::TurnPlanUpdated { thread_id, .. }
            | Self::TurnDiffUpdated { thread_id, .. }
            | Self::TurnStarted { thread_id, .. }
            | Self::TurnCompleted { thread_id, .. }
            | Self::ItemStarted { thread_id, .. }
            | Self::ItemCompleted { thread_id, .. } => Some(thread_id),
            Self::Other { params, .. } => params.get("threadId").and_then(Value::as_str),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RpcMessage {
    Response {
        id: i64,
        result: Option<Value>,
        error: Option<CodexRpcError>,
    },
    Event(CodexEvent),
    ServerRequest(CodexServerRequest),
}

#[derive(Debug, Deserialize)]
struct InitializeResponse {
    id: Option<i64>,
    result: Option<CodexInitializeResult>,
}

pub fn parse_initialize_response(line: &str) -> Result<CodexInitializeResult, AppError> {
    let response: InitializeResponse = serde_json::from_str(line)
        .map_err(|error| AppError::new("codex_invalid_json", error.to_string()))?;

    if response.id != Some(0) {
        return Err(AppError::new(
            "codex_invalid_handshake",
            "Codex returned an unexpected response id",
        ));
    }

    response.result.ok_or_else(|| {
        AppError::new(
            "codex_invalid_handshake",
            "Codex did not return initialization data",
        )
    })
}

pub fn initialize_request(id: i64) -> Value {
    json!({
        "method": "initialize",
        "id": id,
        "params": {
            "clientInfo": {
                "name": "qingyu_notes",
                "title": "轻羽",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true,
                "requestAttestation": false
            }
        }
    })
}

pub fn rpc_request(id: i64, method: &str, params: Value) -> Value {
    json!({ "method": method, "id": id, "params": params })
}

pub fn rpc_notification(method: &str, params: Value) -> Value {
    json!({ "method": method, "params": params })
}

pub fn rpc_response(id: Value, result: Option<Value>, error: Option<CodexRpcError>) -> Value {
    match error {
        Some(error) => json!({ "id": id, "error": error }),
        None => json!({ "id": id, "result": result.unwrap_or_else(|| json!({})) }),
    }
}

pub fn parse_rpc_message(line: &str) -> Result<RpcMessage, AppError> {
    let value: Value = serde_json::from_str(line)
        .map_err(|error| AppError::new("codex_invalid_json", error.to_string()))?;
    if let Some(method) = value.get("method").and_then(Value::as_str) {
        let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
        if let Some(id) = value.get("id") {
            return Ok(RpcMessage::ServerRequest(CodexServerRequest {
                id: id.clone(),
                method: method.to_owned(),
                params,
            }));
        }
        return Ok(RpcMessage::Event(parse_event(method, params)));
    }
    let id = value
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::new("codex_invalid_message", "Codex 响应缺少数字请求 ID"))?;
    let error = value
        .get("error")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| AppError::new("codex_invalid_message", error.to_string()))?;
    Ok(RpcMessage::Response {
        id,
        result: value.get("result").cloned(),
        error,
    })
}

fn parse_event(method: &str, params: Value) -> CodexEvent {
    let string = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match method {
        "item/agentMessage/delta" => CodexEvent::AgentMessageDelta {
            thread_id: string("threadId"),
            turn_id: string("turnId"),
            item_id: string("itemId"),
            delta: string("delta"),
        },
        "turn/plan/updated" => CodexEvent::TurnPlanUpdated {
            thread_id: string("threadId"),
            turn_id: string("turnId"),
            explanation: params
                .get("explanation")
                .and_then(Value::as_str)
                .map(str::to_owned),
            plan: params.get("plan").cloned().unwrap_or_else(|| json!([])),
        },
        "turn/diff/updated" => CodexEvent::TurnDiffUpdated {
            thread_id: string("threadId"),
            turn_id: string("turnId"),
            diff: string("diff"),
        },
        "turn/started" | "turn/completed" => {
            let thread_id = string("threadId");
            let turn = params.get("turn").cloned().unwrap_or_else(|| json!({}));
            if method == "turn/started" {
                CodexEvent::TurnStarted { thread_id, turn }
            } else {
                CodexEvent::TurnCompleted { thread_id, turn }
            }
        }
        "item/started" | "item/completed" => {
            let thread_id = string("threadId");
            let turn_id = string("turnId");
            let item = summarize_item(params.get("item"));
            if method == "item/started" {
                CodexEvent::ItemStarted {
                    thread_id,
                    turn_id,
                    item,
                }
            } else {
                CodexEvent::ItemCompleted {
                    thread_id,
                    turn_id,
                    item,
                }
            }
        }
        _ => CodexEvent::Other {
            method: method.to_owned(),
            params,
        },
    }
}

fn summarize_item(item: Option<&Value>) -> CodexItemSummary {
    let item = item.unwrap_or(&Value::Null);
    CodexItemSummary {
        id: item.get("id").and_then(Value::as_str).map(str::to_owned),
        item_type: item.get("type").and_then(Value::as_str).map(str::to_owned),
        status: item
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
        tool: item.get("tool").and_then(Value::as_str).map(str::to_owned),
        namespace: item
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

pub fn qingyu_dynamic_tools() -> Value {
    let page_tools = [
        (
            "search",
            "搜索笔记页面",
            json!({ "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] }),
        ),
        ("read", "读取 UUID 对应的页面", id_schema("id")),
        ("read_document", "读取页面结构化区块文档", id_schema("id")),
        (
            "create",
            "创建 Markdown 页面",
            json!({ "type": "object", "properties": { "title": { "type": "string" }, "parentId": { "type": ["string", "null"] }, "content": { "type": "string" } }, "required": ["title", "content"] }),
        ),
        (
            "update",
            "更新页面标题或正文",
            json!({ "type": "object", "properties": { "id": { "type": "string" }, "title": { "type": "string" }, "content": { "type": "string" } }, "required": ["id", "title", "content"] }),
        ),
        ("create_block", "在页面或父区块中创建区块", json!({ "type": "object", "properties": { "pageId": { "type": "string" }, "revision": { "type": "integer" }, "parentId": { "type": ["string", "null"] }, "position": { "type": "integer" }, "block": { "type": "object" } }, "required": ["pageId", "revision", "block"] })),
        ("update_block", "按稳定 ID 更新区块", json!({ "type": "object", "properties": { "pageId": { "type": "string" }, "revision": { "type": "integer" }, "block": { "type": "object" } }, "required": ["pageId", "revision", "block"] })),
        ("move_block", "移动页面内区块或调整父区块", json!({ "type": "object", "properties": { "pageId": { "type": "string" }, "revision": { "type": "integer" }, "blockId": { "type": "string" }, "parentId": { "type": ["string", "null"] }, "position": { "type": "integer" } }, "required": ["pageId", "revision", "blockId", "position"] })),
        ("delete_block", "删除页面内区块", json!({ "type": "object", "properties": { "pageId": { "type": "string" }, "revision": { "type": "integer" }, "blockId": { "type": "string" } }, "required": ["pageId", "revision", "blockId"] })),
        (
            "move",
            "移动页面到新的父页面",
            json!({ "type": "object", "properties": { "id": { "type": "string" }, "parentId": { "type": ["string", "null"] }, "sortOrder": { "type": "integer" } }, "required": ["id", "sortOrder"] }),
        ),
        ("archive", "将页面移入回收站", id_schema("id")),
    ];
    let database_tools = [
        (
            "query",
            "按组合筛选和多字段排序查询数据库记录",
            json!({ "type": "object", "properties": { "databaseId": { "type": "string" }, "filters": { "type": "array", "items": { "type": "object" } }, "sorts": { "type": "array", "items": { "type": "object" } } }, "required": ["databaseId"] }),
        ),
        (
            "update_schema",
            "修改数据库字段模型",
            json!({ "type": "object", "properties": { "databaseId": { "type": "string" }, "schema": { "type": "object" } }, "required": ["databaseId", "schema"] }),
        ),
        (
            "create_record",
            "创建数据库记录页面",
            json!({ "type": "object", "properties": { "databaseId": { "type": "string" }, "title": { "type": "string" }, "properties": { "type": "object" } }, "required": ["databaseId", "title"] }),
        ),
        (
            "update_record",
            "更新数据库记录属性",
            json!({ "type": "object", "properties": { "id": { "type": "string" }, "title": { "type": "string" }, "properties": { "type": "object" } }, "required": ["id", "properties"] }),
        ),
    ];
    let relation_tools = [
        ("link", "建立页面 UUID 关系", relation_schema()),
        ("unlink", "移除页面 UUID 关系", relation_schema()),
        ("backlinks", "读取页面反向链接", id_schema("id")),
    ];
    let attachment_tools = [("read", "读取当前对话中附件的文本内容", id_schema("path"))];
    json!([
        namespace("page", "操作轻羽结构化页面和区块", &page_tools),
        namespace("database", "操作轻羽数据库和记录", &database_tools),
        namespace("relation", "操作页面 UUID 关系", &relation_tools),
        namespace("attachment", "只读访问用户添加到对话的附件", &attachment_tools)
    ])
}

fn id_schema(name: &str) -> Value {
    json!({ "type": "object", "properties": { name: { "type": "string" } }, "required": [name] })
}

fn relation_schema() -> Value {
    json!({ "type": "object", "properties": { "sourceId": { "type": "string" }, "fieldId": { "type": "string" }, "targetId": { "type": "string" } }, "required": ["sourceId", "fieldId", "targetId"] })
}

fn namespace(name: &str, description: &str, tools: &[(&str, &str, Value)]) -> Value {
    json!({
        "type": "namespace",
        "name": name,
        "description": description,
        "tools": tools.iter().map(|(name, description, input_schema)| json!({
            "type": "function",
            "name": name,
            "description": description,
            "inputSchema": input_schema
        })).collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_initialize_response() {
        let line = r#"{"id":0,"result":{"userAgent":"Codex Desktop/0.146.0","codexHome":"C:\\Users\\26940\\.codex","platformFamily":"windows","platformOs":"windows"}}"#;
        let response = parse_initialize_response(line).unwrap();
        assert_eq!(response.codex_home, r"C:\Users\26940\.codex");
        assert_eq!(response.platform_family, "windows");
    }

    #[test]
    fn rejects_a_non_handshake_message() {
        let error =
            parse_initialize_response(r#"{"method":"turn/started","params":{}}"#).unwrap_err();
        assert_eq!(error.code, "codex_invalid_handshake");
    }

    #[test]
    fn classifies_client_response_by_request_id() {
        let message = parse_rpc_message(r#"{"id":42,"result":{"ok":true}}"#).unwrap();
        assert_eq!(
            message,
            RpcMessage::Response {
                id: 42,
                result: Some(serde_json::json!({ "ok": true })),
                error: None,
            }
        );
    }

    #[test]
    fn classifies_typed_notification() {
        let message = parse_rpc_message(
            r#"{"method":"item/agentMessage/delta","params":{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","delta":"你好"}}"#,
        )
        .unwrap();
        assert_eq!(
            message,
            RpcMessage::Event(CodexEvent::AgentMessageDelta {
                thread_id: "thread-1".into(),
                turn_id: "turn-1".into(),
                item_id: "item-1".into(),
                delta: "你好".into(),
            })
        );
    }

    #[test]
    fn preserves_server_request_id_and_arguments() {
        let message = parse_rpc_message(
            r#"{"method":"item/tool/call","id":"server-7","params":{"threadId":"thread-1","turnId":"turn-1","callId":"call-1","namespace":"page","tool":"read","arguments":{"id":"page-1"}}}"#,
        )
        .unwrap();
        assert_eq!(
            message,
            RpcMessage::ServerRequest(CodexServerRequest {
                id: serde_json::json!("server-7"),
                method: "item/tool/call".into(),
                params: serde_json::json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "callId": "call-1",
                    "namespace": "page",
                    "tool": "read",
                    "arguments": { "id": "page-1" }
                }),
            })
        );
    }

    #[test]
    fn builds_initialize_with_experimental_api() {
        let request = initialize_request(9);
        assert_eq!(request["id"], 9);
        assert_eq!(request["params"]["capabilities"]["experimentalApi"], true);
        assert_eq!(
            request["params"]["capabilities"]["requestAttestation"],
            false
        );
    }
}
