use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexInitializeResult {
    pub user_agent: String,
    pub codex_home: String,
    pub platform_family: String,
    pub platform_os: String,
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
}
