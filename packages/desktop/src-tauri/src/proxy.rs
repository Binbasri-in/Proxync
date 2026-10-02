#[tauri::command]
pub async fn start_proxy(app: tauri::AppHandle, local_port: u16) -> Result<u16, String> {
    let tx = crate::tunnel::get_or_init_event_bridge(&app);
    proxync_core::proxy::start_proxy(tx, local_port).await
}
