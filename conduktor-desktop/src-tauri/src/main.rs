// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use conduktor_kafka_proxy::proxy_state::add_connection;
use conduktor_kafka_proxy::utils::parse_bootstrap_server;
use conduktor_kafka_proxy::{
    proxy_state::{ProxyState}};

use std::sync::{Arc, RwLock};

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let proxy_state = Arc::new(RwLock::new(ProxyState::new(None)));
    add_connection(&proxy_state, parse_bootstrap_server("localhost:9092".into())).await;

   start_tauri().await;
}


async fn  start_tauri() {
    tauri::Builder::default()
    .setup(|app| {
             let window = tauri::WindowBuilder::new(app, "label",  tauri::WindowUrl::App("https://conduktor.conduktor.app/".into()),)
            .build()?;
        Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application")
}