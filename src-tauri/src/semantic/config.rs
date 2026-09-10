use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingConfig {
    pub base_url: String,
    pub model: String,
    pub dimension: usize,
    pub enabled: bool,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

fn default_concurrency() -> usize { 6 }

impl Default for EmbeddingConfig {
    fn default() -> Self { Self { base_url: "https://dashscope.aliyuncs.com/api/v1".into(), model: "qwen3-vl-embedding".into(), dimension: 1024, enabled: false, concurrency: default_concurrency() } }
}

pub fn config_path(data_root: &Path) -> PathBuf { data_root.join("settings").join("embedding.json") }
pub fn key_path(data_root: &Path) -> PathBuf { data_root.join("settings").join("embedding.key") }

pub fn load(data_root: &Path) -> Result<EmbeddingConfig, String> {
    let path = config_path(data_root);
    if !path.exists() { return Ok(EmbeddingConfig::default()); }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn save(data_root: &Path, config: &EmbeddingConfig) -> Result<(), String> {
    let dir = data_root.join("settings");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    fs::write(config_path(data_root), bytes).map_err(|e| e.to_string())
}

pub fn save_key(data_root: &Path, key: &str) -> Result<(), String> {
    let dir = data_root.join("settings");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    { return crate::semantic::config::windows_protect_write(&key_path(data_root), key.as_bytes()); }
    #[cfg(not(windows))]
    { fs::write(key_path(data_root), key.as_bytes()).map_err(|e| e.to_string()) }
}

pub fn load_key(data_root: &Path) -> Result<Option<String>, String> {
    let path = key_path(data_root);
    if !path.exists() { return Ok(None); }
    #[cfg(windows)]
    { return Ok(Some(String::from_utf8(windows_protect_read(&path)?).map_err(|e| e.to_string())?)); }
    #[cfg(not(windows))]
    { Ok(Some(String::from_utf8(fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?)) }
}

pub fn clear_key(data_root: &Path) -> Result<(), String> {
    let path = key_path(data_root);
    if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
    Ok(())
}

#[cfg(windows)]
fn windows_protect_write(path: &Path, input: &[u8]) -> Result<(), String> {
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let mut data = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe { CryptProtectData(&mut data, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut output) };
    if ok == 0 { return Err(std::io::Error::last_os_error().to_string()); }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { windows_sys::Win32::Foundation::LocalFree(output.pbData.cast()); }
    fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn windows_protect_read(path: &Path) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut input = CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe { CryptUnprotectData(&mut input, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut output) };
    if ok == 0 { return Err(std::io::Error::last_os_error().to_string()); }
    let result = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { windows_sys::Win32::Foundation::LocalFree(output.pbData.cast()); }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::EmbeddingConfig;

    #[test]
    fn legacy_config_uses_default_concurrency() {
        let config: EmbeddingConfig = serde_json::from_str(
            r#"{"baseUrl":"https://example.com","model":"embedding","dimension":1024,"enabled":true}"#,
        )
        .unwrap();

        assert_eq!(config.concurrency, 6);
    }
}
