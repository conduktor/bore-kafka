// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            
            // let window = tauri::WindowBuilder::new(app, "label",  tauri::WindowUrl::App("https://demo.conduktor.io/index.html".into()),)
            let window = tauri::WindowBuilder::new(app, "label",  tauri::WindowUrl::App("https://conduktor.conduktor.app/".into()),)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        // .setup(|app| {
        //     let window = app.get_window("main").unwrap();
        //     window.eval("window.location.replace('https://google.com')")
        //     Ok(())
        //   })
        //   .run(tauri::generate_context!())
        .expect("error while running tauri application")
}
