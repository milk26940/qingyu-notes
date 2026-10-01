mod backup;
mod codex;
mod commands;
mod database;
mod error;
mod pages;
mod portable;
mod projection;
mod transfer;

use tauri::{Emitter, Manager};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn window_refresh_ime_position(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::ptr::null_mut;
        use windows_sys::Win32::Foundation::{HWND, RECT};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOREDRAW,
            SWP_NOSIZE, SWP_NOZORDER,
        };

        let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as HWND;
        let mut rect = RECT::default();
        let flags =
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOREDRAW;

        // WebView2 在宿主窗口缩放后可能丢失 IME 光标的屏幕坐标，
        // 无重绘地微移并复位宿主窗口可强制其重新同步。
        unsafe {
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if SetWindowPos(
                hwnd,
                null_mut(),
                rect.left.saturating_add(1),
                rect.top,
                0,
                0,
                flags,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if SetWindowPos(hwnd, null_mut(), rect.left, rect.top, 0, 0, flags) == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
    }

    #[cfg(not(windows))]
    let _ = window;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    #[cfg(debug_assertions)]
    let context = {
        let mut context = context;
        // 开发版使用独立标识，避免单实例机制唤起正在运行的正式版。
        context.config_mut().identifier = "com.qingyu.notes.dev".into();
        if let Some(window) = context.config_mut().app.windows.first_mut() {
            window.title = "轻羽 · 开发版".into();
        }
        context
    };
    let portable_paths = portable::prepare().unwrap_or_else(|error| {
        if let Ok(executable) = std::env::current_exe() {
            if let Some(install_dir) = executable.parent() {
                let _ = std::fs::write(
                    install_dir.join("portable-migration-error.log"),
                    format!("{error}\n"),
                );
            }
        }
        panic!("无法准备轻羽便携数据目录：{error}");
    });
    if let Ok(executable) = std::env::current_exe() {
        if let Some(install_dir) = executable.parent() {
            let _ = std::fs::remove_file(install_dir.join("portable-migration-error.log"));
        }
    }
    let runtime = codex::runtime::CodexRuntime::default();
    let pages = commands::pages::PageState::default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(runtime)
        .manage(pages)
        .setup(move |app| {
            let event_app = app.handle().clone();
            if let Ok(watcher) = codex::config_watcher::CodexConfigWatcher::start(move || {
                let _ = event_app.emit("codex://config-changed", ());
            }) {
                app.manage(watcher);
            }
            let window_config = app
                .config()
                .app
                .windows
                .first()
                .ok_or("轻羽主窗口配置不存在")?;
            let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?
                .data_directory(portable_paths.webview_root.clone());
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone())?;
            }
            builder.build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            window_refresh_ime_position,
            commands::backup::backup_checkpoint,
            commands::backup::backup_list,
            commands::backup::backup_location,
            commands::backup::backup_restore,
            commands::backup::backup_export_zip,
            commands::backup::backup_import_zip,
            commands::transfer::transfer_export_lossless,
            commands::transfer::transfer_export_markdown,
            commands::transfer::transfer_preview_content,
            commands::transfer::transfer_import_content,
            commands::codex::codex_status,
            commands::codex::codex_start,
            commands::codex::codex_stop,
            commands::codex::codex_restart,
            commands::codex::codex_models,
            commands::pages::vault_init,
            commands::pages::page_list,
            commands::pages::page_get,
            commands::pages::page_open_bundle,
            commands::pages::page_save_document,
            commands::pages::page_create,
            commands::pages::page_update,
            commands::pages::page_store_asset,
            commands::pages::page_read_asset,
            commands::pages::page_move,
            commands::pages::page_trash,
            commands::pages::page_trash_recursive,
            commands::pages::page_restore,
            commands::pages::page_purge,
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
            commands::database::database_update_table_view,
            commands::conversations::conversation_create,
            commands::conversations::conversation_list,
            commands::conversations::conversation_history,
            commands::conversations::conversation_send,
            commands::conversations::conversation_store_attachment,
            commands::conversations::conversation_store_attachment_path,
            commands::conversations::conversation_read_attachment,
            commands::conversations::conversation_interrupt,
            commands::conversations::approval_list,
            commands::conversations::approval_resolve
        ])
        .run(context)
        .expect("error while running tauri application");
}
