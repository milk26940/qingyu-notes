use crate::error::AppError;
use std::path::Path;

pub fn text(path: &Path) -> Result<String, AppError> {
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    let value = match extension.as_str() {
        "txt" | "md" | "markdown" | "csv" | "json" => std::fs::read_to_string(path)
            .map_err(|e| AppError::new("attachment_parse_failed", e.to_string()))?,
        "pdf" => pdf_extract::extract_text(path)
            .map_err(|e| AppError::new("attachment_parse_failed", e.to_string()))?,
        "docx" | "pptx" => crate::codex::conversation::extract_office_text(path, &extension)?,
        _ => {
            return Err(AppError::new(
                "attachment_unsupported",
                "暂不支持此附件正文格式，请转换为 PDF、DOCX、PPTX 或文本",
            ))
        }
    };
    if value.trim().is_empty() {
        return Err(AppError::new(
            "attachment_empty",
            "未提取到正文，扫描文件可能需要 OCR",
        ));
    }
    Ok(value)
}

pub fn chunks(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(1800)
        .map(|part| part.iter().collect())
        .collect()
}
