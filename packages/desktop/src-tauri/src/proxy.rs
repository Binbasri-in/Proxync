use tauri::Emitter;
use proxync_core::events::{create_event_channel, ProxyncEvent};

#[tauri::command]
pub async fn start_proxy(app: tauri::AppHandle, local_port: u16) -> Result<u16, String> {
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
                ProxyncEvent::TunnelStatusChanged { port, status } => {
                    let _ = app_clone.emit("tunnel:status-changed", serde_json::json!({
                        "port": port,
                        "status": status,
                    }));
                }
                _ => {}
            }
        }
    });

    proxync_core::proxy::start_proxy(tx, local_port).await
}
