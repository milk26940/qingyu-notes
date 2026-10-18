use std::path::PathBuf;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use rig_core::providers::openai::{OpenAI, OpenAIConfig, Route};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    encrypted_key: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettings {
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
    pub workspace_root: String,
}

pub struct ModelConnection {
    pub client: OpenAI,
    pub model: String,
    pub api_key: String,
}

pub fn workspace_root() -> Result<PathBuf, AppError> {
    std::env::current_exe().ok().and_then(|path| path.parent().map(PathBuf::from))
        .ok_or_else(|| AppError::new("agent_path_failed", "无法定位当前软件目录"))
}

fn settings_path() -> Result<PathBuf, AppError> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .ok_or_else(|| AppError::new("agent_settings_failed", "无法定位本机加密配置目录"))?;
    Ok(base.join("QingYu").join("Agent").join("settings.json"))
}

fn load() -> Result<StoredSettings, AppError> {
    let path = settings_path()?;
    if !path.exists() { return Ok(StoredSettings::default()); }
    serde_json::from_slice(&std::fs::read(path).map_err(settings_error)?)
        .map_err(|_| AppError::new("agent_settings_failed", "Agent 配置文件格式不正确"))
}

fn settings_error(error: std::io::Error) -> AppError {
    AppError::new("agent_settings_failed", error.to_string())
}

pub fn get() -> Result<AgentSettings, AppError> {
    let settings = load()?;
    Ok(AgentSettings {
        base_url: settings.base_url,
        model: settings.model,
        has_api_key: !settings.encrypted_key.is_empty(),
        workspace_root: workspace_root()?.to_string_lossy().into_owned(),
    })
}

pub fn save(base_url: String, model: String, api_key: Option<String>, clear_api_key: bool) -> Result<AgentSettings, AppError> {
    let url = reqwest::Url::parse(base_url.trim()).map_err(|_| AppError::new("agent_invalid_url", "请输入有效的 API Base URL"))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::new("agent_invalid_url", "API 地址仅支持 HTTP/HTTPS，不能包含账号、查询参数或片段"));
    }
    let base_url = url.as_str().trim_end_matches('/').to_owned();
    if base_url.ends_with("/chat/completions") || base_url.ends_with("/responses") || base_url.ends_with("/models") {
        return Err(AppError::new("agent_invalid_url", "请填写 API 基础地址，例如以 /v1 结尾，不要填写具体接口路径"));
    }
    let mut settings = load()?;
    settings.base_url = base_url;
    settings.model = model.trim().to_owned();
    if clear_api_key { settings.encrypted_key.clear(); }
    if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
        settings.encrypted_key = STANDARD.encode(protect(key.trim().as_bytes(), true)?);
    }
    let path = settings_path()?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(settings_error)?;
    let temporary = path.with_extension("pending");
    std::fs::write(&temporary, serde_json::to_vec_pretty(&settings).map_err(|_| AppError::new("agent_settings_failed", "无法保存 Agent 配置"))?).map_err(settings_error)?;
    // 配置不包含明文密钥；替换失败时保留原来的配置。
    std::fs::rename(&temporary, &path).map_err(settings_error)?;
    get()
}

pub fn connection(model: Option<&str>) -> Result<ModelConnection, AppError> {
    let settings = load()?;
    if settings.base_url.is_empty() { return Err(AppError::new("agent_not_configured", "请先在 Agent 设置中填写 API 地址、Key 和模型")); }
    let model = model.filter(|model| !model.trim().is_empty()).unwrap_or(&settings.model).trim().to_owned();
    let api_key = if settings.encrypted_key.is_empty() { String::new() } else {
        let bytes = STANDARD.decode(&settings.encrypted_key).map_err(|_| AppError::new("agent_key_failed", "已保存的密钥格式不正确，请重新填写"))?;
        String::from_utf8(protect(&bytes, false)?).map_err(|_| AppError::new("agent_key_failed", "无法读取已保存的密钥"))?
    };
    let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20)).timeout(Duration::from_secs(300));
    // Windows 使用系统 TLS 和证书验证，与桌面网络环境保持一致。
    #[cfg(windows)]
    let http = http.tls_backend_native();
    let http = http.build().map_err(|_| AppError::new("agent_http_failed", "无法初始化模型网络连接"))?;
    let client = OpenAIConfig::new(api_key.clone()).with_base_url(settings.base_url).with_route(Route::Chat).connect(rig_reqwest::ReqwestClient::from(http));
    Ok(ModelConnection { client, model, api_key })
}

pub fn model_error(error: rig_core::error::ProviderError, key: &str) -> AppError {
    let mut message = error.to_string();
    // Rig 的 HTTP 错误没有向标准 source 暴露内部错误，需从传输包装继续追踪。
    let mut source: Option<&dyn std::error::Error> = match &error {
        rig_core::error::ProviderError::Http(http) => Some(http.as_ref()),
        _ => std::error::Error::source(&error),
    };
    // 保留传输层的实际原因，避免只显示无法定位问题的顶层发送失败。
    while let Some(cause) = source {
        let detail = cause.to_string();
        if !detail.is_empty() && !message.contains(&detail) {
            message.push_str("；原因：");
            message.push_str(&detail);
        }
        source = cause.source();
    }
    let message = if key.is_empty() { message } else { message.replace(key, "[已隐藏密钥]") };
    AppError::new("agent_api_failed", message)
}


/// 对当前模型分别发送文字、无副作用工具和图片请求，保留各项实际结果。
pub async fn test_connection(model: Option<&str>) -> Result<serde_json::Value, AppError> {
    use rig_core::completion::{CompletionRequest, ToolDefinition};
    use rig_core::message::{DocumentSourceKind, Image, ImageMediaType, Message, ToolChoice, ToolName, UserContent};
    use serde_json::json;
    use futures::StreamExt;
    let connection = connection(model)?;
    if connection.model.is_empty() { return Err(AppError::new("agent_model_required", "请填写模型名称")); }
    let model = connection.client.completion(connection.model.clone());
    let text_probe = async {
        let mut stream = model.stream(CompletionRequest::new("Reply with exactly QINGYU_OK."))?;
        let mut received_text = false;
        while let Some(item) = stream.next().await {
            if let rig_core::streaming::Item::Event(rig_core::streaming::StreamEvent::Text { text, .. }) = item? { received_text |= !text.is_empty(); }
        }
        let response = stream.finish().await?;
        Ok::<_, rig_core::error::ProviderError>((response, received_text))
    };
    let mut tool_request = CompletionRequest::new("Call qingyu_probe with value QINGYU_OK. This is a capability test; no note data is accessed or changed.")
        .tools(vec![ToolDefinition::new(ToolName::new("qingyu_probe").unwrap(), "无副作用的能力探测", json!({ "type": "object", "properties": { "value": { "type": "string" } }, "required": ["value"] }))]);
    tool_request.tool_choice = Some(ToolChoice::Required);
    let image_request = CompletionRequest::new(Message::User { content: vec![
        UserContent::text("Inspect the image. Reply with only the left half color, a comma, and the right half color, in English."),
        UserContent::Image(Image { data: DocumentSourceKind::base64("iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAK0lEQVR4nO3NsQkAAAzDsPz/dHpDpi4CjwalydS4d90BAAAAAAAAAADAG3A2h/wuHtDxTwAAAABJRU5ErkJggg=="), media_type: Some(ImageMediaType::PNG), ..Default::default() }),
    ] });
    let (text_probe, tool_probe, image_probe) = tokio::join!(text_probe, model.call(tool_request), model.call(image_request));
    let mut details = Vec::<String>::new();
    let text = match text_probe {
        Ok((response, received_text)) => { let passed = received_text && response.error.is_none() && response.text().contains("QINGYU_OK"); details.push(if passed { "文字：流式探测通过".into() } else { "文字：流式接口未返回约定正文".into() }); passed }
        Err(error) => { details.push(format!("文字：{}", model_error(error, &connection.api_key).message)); false }
    };
    let tools = match tool_probe {
        Ok(response) => { let passed = response.error.is_none() && response.tool_calls().any(|call| call.function.name.as_str() == "qingyu_probe" && call.function.arguments.get("value").and_then(serde_json::Value::as_str) == Some("QINGYU_OK")); details.push(if passed { "工具：探测通过，未执行笔记操作".into() } else { "工具：模型未返回有效的约定工具调用".into() }); passed }
        Err(error) => { details.push(format!("工具：{}", model_error(error, &connection.api_key).message)); false }
    };
    let images = match image_probe {
        Ok(response) => { let answer = response.text().to_lowercase(); let passed = response.error.is_none() && answer.find("red").zip(answer.find("blue")).is_some_and(|(red, blue)| red < blue); details.push(if passed { "图片：实际识别左右颜色通过".into() } else { "图片：模型未正确识别测试图片".into() }); passed }
        Err(error) => { details.push(format!("图片：{}", model_error(error, &connection.api_key).message)); false }
    };
    Ok(json!({ "model": connection.model, "text": text, "tools": tools, "images": images, "details": details }))
}

#[cfg(windows)]
fn protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, AppError> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN};
    use windows_sys::Win32::Foundation::LocalFree;
    let input = CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    let ok = unsafe {
        if encrypt { CryptProtectData(&input, null(), null(), null_mut(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
        else { CryptUnprotectData(&input, null_mut(), null(), null_mut(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
    };
    if ok == 0 { return Err(AppError::new("agent_key_failed", "系统无法加密或解密 API Key，请使用保存密钥的 Windows 账号")); }
    let result = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData as _); }
    Ok(result)
}

#[cfg(not(windows))]
fn protect(_bytes: &[u8], _encrypt: bool) -> Result<Vec<u8>, AppError> {
    Err(AppError::new("agent_key_failed", "此版本的系统密钥存储暂仅支持 Windows"))
}

#[cfg(test)]
mod connection_diagnostics {
    use super::*;

    #[tokio::test]
    async fn network_error_preserves_cause_and_redacts_key() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let key = "qingyu-test-secret";
        let error = reqwest::Client::builder().no_proxy().build().unwrap()
            .get(format!("http://{address}/{key}")).send().await.unwrap_err();
        let mut root: &dyn std::error::Error = &error;
        while let Some(source) = root.source() { root = source; }
        let cause = root.to_string();
        let error = rig_core::error::ProviderError::Http(std::sync::Arc::new(rig_core::http_client::Error::instance(error)));
        let message = model_error(error, key).message;
        assert!(message.contains(&cause), "未显示实际连接失败原因：{message}");
        assert!(!message.contains(key));
    }

    #[tokio::test]
    #[ignore = "手动检查本机已配置的模型接口，不读取笔记内容"]
    async fn configured_service() {
        match super::super::runtime::CodexRuntime::default().list_models().await {
            Ok(models) => println!("模型列表成功：{} 个模型", models.len()),
            Err(error) => panic!("{}", error.message),
        }
        let result = test_connection(None).await.unwrap();
        println!("{}", result);
        assert_eq!(result["text"], true);
        assert_eq!(result["tools"], true);
    }
}

