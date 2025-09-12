use log::error;

use crate::globals::SHINKAI_NODE_MANAGER_INSTANCE;
use crate::local_shinkai_node::shinkai_node_manager::ShinkaiNodeManager;
use crate::local_shinkai_node::shinkai_node_options::ShinkaiNodeOptions;
use crate::windows::{recreate_window, Window};

#[tauri::command]
pub async fn show_shinkai_node_manager_window(app_handle: tauri::AppHandle) {
    let _ = recreate_window(app_handle, Window::ShinkaiNodeManager, true);
}

#[tauri::command]
pub async fn shinkai_node_is_running() -> Result<bool, String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().read().await;
    let is_running = shinkai_node_manager_guard.is_running().await;
    Ok(is_running)
}

#[tauri::command]
pub async fn shinkai_node_set_options(
    options: ShinkaiNodeOptions,
) -> Result<ShinkaiNodeOptions, String> {
    let mut shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().write().await;
    let options = shinkai_node_manager_guard
        .set_shinkai_node_options(options)
        .await;
    Ok(options)
}

#[tauri::command]
pub async fn shinkai_node_get_options() -> Result<ShinkaiNodeOptions, String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().read().await;
    let options = shinkai_node_manager_guard.get_shinkai_node_options().await;
    Ok(options)
}

#[tauri::command]
pub async fn shinkai_node_spawn() -> Result<(), String> {
    let mut shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().write().await;
    match shinkai_node_manager_guard.spawn().await {
        Ok(_) => Ok(()),
        Err(message) => {
            error!("error spawning shinkai node: {}", message);
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn shinkai_node_kill() -> Result<(), String> {
    let mut shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().write().await;
    shinkai_node_manager_guard.kill().await;
    Ok(())
}

#[tauri::command]
pub async fn shinkai_node_remove_storage(preserve_keys: bool) -> Result<(), String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().write().await;
    match shinkai_node_manager_guard
        .remove_storage(preserve_keys)
        .await
    {
        Ok(_) => Ok(()),
        Err(_) => Ok(()),
    }
}

#[tauri::command]
pub async fn shinkai_node_set_default_options() -> Result<ShinkaiNodeOptions, String> {
    let mut shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().write().await;
    let options = shinkai_node_manager_guard
        .set_default_shinkai_node_options()
        .await;
    Ok(options)
}

#[tauri::command]
pub async fn shinkai_node_get_ollama_api_url() -> Result<String, String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().read().await;
    let ollama_api_url = shinkai_node_manager_guard.get_ollama_api_url();
    Ok(ollama_api_url)
}

#[tauri::command]
pub async fn shinkai_node_get_default_model() -> Result<String, String> {
    Ok("shinkai-backend:FREE_TEXT_INFERENCE".to_string())
}

#[tauri::command]
pub async fn shinkai_node_get_default_embedding_model() -> Result<String, String> {
    let default_options = ShinkaiNodeOptions::default();
    Ok(default_options.default_embedding_model.unwrap_or_else(|| "embeddinggemma:300m".to_string()))
}

#[tauri::command]
pub async fn shinkai_node_get_ollama_version(
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    match ShinkaiNodeManager::get_ollama_version(app_handle).await {
        Ok(version) => Ok(version),
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
pub async fn shinkai_node_open_storage_location() -> Result<(), String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().read().await;
    match shinkai_node_manager_guard.open_storage_location() {
        Ok(_) => Ok(()),
        Err(message) => Err(message),
    }
}

#[tauri::command]
pub async fn shinkai_node_open_storage_location_with_path(relative_path: String) -> Result<(), String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().read().await;
    match shinkai_node_manager_guard.open_storage_location_with_path(&relative_path) {
        Ok(_) => Ok(()),
        Err(message) => Err(message),
    }
}

#[tauri::command]
pub async fn shinkai_node_open_chat_folder(storage_location: &str, chat_folder_name: &str) -> Result<(), String> {
    let shinkai_node_manager_guard = SHINKAI_NODE_MANAGER_INSTANCE.get().unwrap().read().await;
    match shinkai_node_manager_guard.open_chat_folder(storage_location, chat_folder_name) {
        Ok(_) => Ok(()),
        Err(message) => Err(message),
    }
}
