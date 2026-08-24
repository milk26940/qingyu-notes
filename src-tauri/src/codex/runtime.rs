use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

use super::command::resolve_codex_launch;
use super::protocol::{parse_initialize_response, CodexInitializeResult};
use crate::error::AppError;

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

struct CodexProcess {
    child: Child,
}

impl Drop for CodexProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[derive(Clone)]
pub struct CodexRuntime {
    status: Arc<RwLock<CodexStatus>>,
    process: Arc<Mutex<Option<CodexProcess>>>,
}

impl Default for CodexRuntime {
    fn default() -> Self {
        Self {
            status: Arc::new(RwLock::new(CodexStatus::Stopped)),
            process: Arc::new(Mutex::new(None)),
        }
    }
}

impl CodexRuntime {
    pub async fn status(&self) -> CodexStatus {
        self.status.read().await.clone()
    }

    pub async fn start(&self, app: &AppHandle) -> Result<CodexStatus, AppError> {
        if matches!(
            *self.status.read().await,
            CodexStatus::Ready { .. } | CodexStatus::Starting
        ) {
            return Ok(self.status().await);
        }
        self.set_status(app, CodexStatus::Starting).await;
        let launch = match resolve_codex_launch() {
            Ok(launch) => launch,
            Err(error) => {
                self.set_status(
                    app,
                    CodexStatus::Failed {
                        code: error.code.clone(),
                        message: error.message.clone(),
                    },
                )
                .await;
                return Err(error);
            }
        };
        let mut command = Command::new(launch.program);
        command
            .args(launch.prefix_args)
            .args(["app-server", "--listen", "stdio://"]);
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|error| AppError::new("codex_start_failed", error.to_string()))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::new("codex_start_failed", "无法打开 Codex 标准输入"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::new("codex_start_failed", "无法打开 Codex 标准输出"))?;
        let initialize = r#"{"method":"initialize","id":0,"params":{"clientInfo":{"name":"qingyu_notes","title":"轻羽","version":"0.1.0"}}}
{"method":"initialized","params":{}}
"#;
        stdin
            .write_all(initialize.as_bytes())
            .await
            .map_err(|error| AppError::new("codex_start_failed", error.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|error| AppError::new("codex_start_failed", error.to_string()))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
            .await
            .map_err(|_| AppError::new("codex_handshake_timeout", "Codex 初始化超时"))?
            .map_err(|error| AppError::new("codex_handshake_failed", error.to_string()))?;
        let result: CodexInitializeResult = parse_initialize_response(line.trim_end())?;
        *self.process.lock().await = Some(CodexProcess { child });
        self.set_status(
            app,
            CodexStatus::Ready {
                user_agent: result.user_agent.clone(),
                codex_home: result.codex_home.clone(),
            },
        )
        .await;
        let event_app = app.clone();
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let _ = event_app.emit("codex://event", line.trim_end().to_owned());
                    }
                }
            }
        });
        Ok(self.status().await)
    }

    pub async fn stop(&self, app: &AppHandle) -> Result<CodexStatus, AppError> {
        if let Some(mut process) = self.process.lock().await.take() {
            process
                .child
                .kill()
                .await
                .map_err(|error| AppError::new("codex_stop_failed", error.to_string()))?;
            let _ = process.child.wait().await;
        }
        self.set_status(app, CodexStatus::Stopped).await;
        Ok(CodexStatus::Stopped)
    }

    async fn set_status(&self, app: &AppHandle, status: CodexStatus) {
        *self.status.write().await = status.clone();
        let _ = app.emit("codex://status", status);
    }
}
