// PSX Studio - editeur desktop (Tauri).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    psx_studio_editor_lib::run()
}
