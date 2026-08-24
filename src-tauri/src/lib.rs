mod codex;
mod commands;
mod database;
mod error;
mod pages;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = codex::runtime::CodexRuntime::default();
    let pages = commands::pages::PageState::default();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(runtime)
        .manage(pages)
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::codex::codex_status,
            commands::codex::codex_start,
            commands::codex::codex_stop,
            commands::pages::vault_init,
            commands::pages::page_list,
            commands::pages::page_get,
            commands::pages::page_create,
            commands::pages::page_update,
            commands::pages::page_move,
            commands::pages::page_trash,
            commands::pages::page_restore,
            commands::pages::page_history,
            commands::pages::page_search,
            commands::pages::page_links,
            commands::pages::page_backlinks,
            commands::database::database_create,
            commands::database::database_get,
            commands::database::database_add_field,
            commands::database::database_create_record,
            commands::database::database_update_record,
            commands::database::database_query,
            commands::conversations::conversation_create,
            commands::conversations::conversation_list,
            commands::conversations::conversation_history,
            commands::conversations::conversation_send,
            commands::conversations::conversation_interrupt,
            commands::conversations::approval_list,
            commands::conversations::approval_resolve
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
