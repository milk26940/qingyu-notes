// Windows 开发版和发布版都使用图形子系统，避免显示额外命令行窗口。
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    qingyu_notes_lib::run()
}
