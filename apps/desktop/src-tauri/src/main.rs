// Mencegah jendela konsol tambahan di Windows saat rilis.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    // Mode penjaga keep-awake: tanpa jendela, sebelum Tauri dan plugin dimulai.
    if mandor_desktop_lib::keep_awake::requested(std::env::args()) {
        return mandor_desktop_lib::keep_awake::run();
    }
    // Pembersihan WSL oleh hook uninstall NSIS.
    if mandor_desktop_lib::uninstall::requested(std::env::args()) {
        return mandor_desktop_lib::run_uninstall_cleanup();
    }
    mandor_desktop_lib::run()
}
