use tauri::Emitter;
use proxync_core::events::{create_event_channel, EventSender, ProxyncEvent};

#[tauri::command]
pub async fn close_tunnel(tunnel_id: String, local_port: Option<u16>) -> Result<(), String> {
    proxync_core::tunnel::close_tunnel(tunnel_id, local_port).await
}

#[tauri::command]
pub async fn close_all_tunnels() -> Result<(), String> {
    proxync_core::tunnel::close_all_tunnels().await
}

fn spawn_event_bridge(app: tauri::AppHandle) -> EventSender {
    let (tx, mut rx) = create_event_channel();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                ProxyncEvent::RequestLog { id, method, path, port, headers, body_preview, tunnel_id, timestamp } => {
                    let _ = app_clone.emit("request:log", serde_json::json!({
                        "requestId": id,
                        "method": method,
                        "path": path,
                        "port": port,
                        "headers": headers,
                        "bodyPreview": body_preview,
                        "tunnelId": tunnel_id,
                        "timestamp": timestamp,
                    }));
                }
                ProxyncEvent::ResponseLog { id, request_id, status, duration_ms, response_headers, response_body_preview, timestamp } => {
                    let _ = app_clone.emit("request:log:response", serde_json::json!({
                        "id": id,
                        "requestId": request_id,
                        "status": status,
                        "durationMs": duration_ms,
                        "responseHeaders": response_headers,
                        "responseBodyPreview": response_body_preview,
                        "timestamp": timestamp,
                    }));
                }
                ProxyncEvent::TunnelAutoClosed { tunnel_id } => {
                    let _ = app_clone.emit("tunnel:auto-closed", serde_json::json!({
                        "tunnelId": tunnel_id,
                    }));
                }
                ProxyncEvent::TunnelStatusChanged { port, status } => {
                    let _ = app_clone.emit("tunnel:status-changed", serde_json::json!({
                        "port": port,
                        "status": status,
                    }));
                }
            }
        }
    });
    tx
}

#[tauri::command]
pub async fn open_tunnel(
    app: tauri::AppHandle,
    tunnel_id: String,
    local_port: u16,
    token: String,
    workspace_id: String,
    relay_url: Option<String>,
) -> Result<(), String> {
    let tx = spawn_event_bridge(app);
    proxync_core::tunnel::open_tunnel(tx, tunnel_id, local_port, token, workspace_id, relay_url).await
}

#[tauri::command]
pub async fn open_cloudflare_tunnel(
    app: tauri::AppHandle,
    tunnel_id: String,
    local_port: u16,
) -> Result<String, String> {
    let tx = spawn_event_bridge(app);
    proxync_core::tunnel::open_cloudflare_tunnel(tx, tunnel_id, local_port).await
}

#[tauri::command]
pub async fn open_native_tunnel(
    app: tauri::AppHandle,
    tunnel_id: String,
    local_port: u16,
    subdomain: String,
) -> Result<String, String> {
    let tx = spawn_event_bridge(app);
    proxync_core::tunnel::open_native_tunnel(tx, tunnel_id, local_port, subdomain).await
}
