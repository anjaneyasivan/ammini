use anyhow::{Context, Result};
use grammers_session::storages::SqliteSession;
use std::path::PathBuf;
use std::sync::Arc;

/// Returns the default path for the Telegram session file.
pub fn session_path() -> PathBuf {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("min-mpv");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("telegram.session")
}

/// Load or create a SQLite session (async, opens the database).
pub async fn load_or_create_session() -> Result<Arc<SqliteSession>> {
    let path = session_path();
    tracing::debug!("session: opening SQLite session at {}", path.display());
    let session = SqliteSession::open(&path)
        .await
        .context("Failed to load or create session")?;
    tracing::debug!("session: loaded successfully");
    Ok(Arc::new(session))
}

/// SqliteSession auto-persists all changes to the SQLite database file.
/// No explicit save call is needed. This function is kept for API compatibility.
pub fn save_session(_session: &SqliteSession) -> Result<()> {
    tracing::info!("Session is auto-persisted to {}", session_path().display());
    Ok(())
}

/// Delete the session file from disk.
pub fn delete_session() -> Result<()> {
    let path = session_path();
    if path.exists() {
        std::fs::remove_file(&path).context("Failed to delete session file")?;
        tracing::info!("Session deleted from {}", path.display());
    }
    Ok(())
}
