mod codex;
mod commands;
mod error;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = codex::runtime::CodexRuntime::default();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![greet, commands::codex::codex_status, commands::codex::codex_start, commands::codex::codex_stop])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
