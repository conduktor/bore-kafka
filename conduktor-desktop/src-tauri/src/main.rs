// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use conduktor_kafka_proxy::proxy_state::add_connection;
use conduktor_kafka_proxy::proxy_state::ProxyState;
use conduktor_kafka_proxy::utils::parse_bootstrap_server;
use tauri::SystemTray;
use tauri::SystemTrayEvent;
use tauri::{CustomMenuItem, Menu, MenuItem, Submenu, SystemTrayMenu, SystemTrayMenuItem};
use tracing::info;

use std::sync::{Arc, RwLock};

#[tauri::command]
async fn start_proxy(host: String, port:String) -> u16 {
    info!("Starting proxy on {}:{}", host, port);

    let proxy_state = Arc::new(RwLock::new(ProxyState::new(None)));
    let p = add_connection(
        &proxy_state,
        parse_bootstrap_server("localhost:9092".into()),
    )
    .await;

    p
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    start_tauri().await;
}

async fn start_tauri() {
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let close = CustomMenuItem::new("close".to_string(), "Close");

    let submenu = Submenu::new(
        "Menu",
        Menu::new().add_item(quit.clone()).add_item(close.clone()),
    );
    let menu = Menu::new()
        .add_native_item(MenuItem::Copy)
        .add_submenu(submenu);

    let tray_menu = SystemTrayMenu::new()
        .add_item(quit)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(close);

    let system_tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .setup(|app| {
            let window = tauri::WindowBuilder::new(
                app,
                "label",
                tauri::WindowUrl::App("https://conduktor.conduktor.app/".into()),
            )
            .build()?;
            Ok(())
        })
        .menu(menu)
        .system_tray(system_tray)
        .on_menu_event(|event| match event.menu_item_id() {
            "quit" => {
                std::process::exit(0);
            }
            "close" => {
                event.window().close().unwrap();
            }
            _ => {}
        })
        .on_menu_event(|event| match event.menu_item_id() {
            "quit" => {
                std::process::exit(0);
            }
            "close" => {
                event.window().close().unwrap();
            }
            _ => {}
        })
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
            _ => {}
        })
        // .manage(plop.clone())
        .invoke_handler(tauri::generate_handler![start_proxy])
        .run(tauri::generate_context!())
        .expect("error while running tauri application")
}
