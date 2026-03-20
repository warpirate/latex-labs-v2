use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabSession {
    pub id: String,
    pub project_dir: String,
    pub host_name: String,
    pub port: u16,
    pub active: bool,
    pub connected_users: Vec<String>,
}

static SESSIONS: LazyLock<Mutex<HashMap<String, CollabSession>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub async fn start_collab_session(
    project_dir: String,
    host_name: String,
    port: Option<u16>,
) -> Result<CollabSession, String> {
    let port = port.unwrap_or(9090);
    let session = CollabSession {
        id: Uuid::new_v4().to_string(),
        project_dir,
        host_name,
        port,
        active: true,
        connected_users: vec![],
    };
    let mut sessions = SESSIONS.lock().await;
    sessions.insert(session.id.clone(), session.clone());
    // TODO: Start actual WebSocket server with tokio-tungstenite
    // For now, just track the session metadata
    Ok(session)
}

#[tauri::command]
pub async fn stop_collab_session(session_id: String) -> Result<(), String> {
    let mut sessions = SESSIONS.lock().await;
    if let Some(session) = sessions.get_mut(&session_id) {
        session.active = false;
    }
    sessions.remove(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn get_collab_status() -> Result<Option<CollabSession>, String> {
    let sessions = SESSIONS.lock().await;
    Ok(sessions.values().find(|s| s.active).cloned())
}

#[tauri::command]
pub async fn generate_collab_invite(session_id: String) -> Result<String, String> {
    let sessions = SESSIONS.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;
    // Generate a simple invite URL/token
    Ok(format!(
        "latexlabs://collab?host=localhost&port={}&session={}",
        session.port, session.id
    ))
}
