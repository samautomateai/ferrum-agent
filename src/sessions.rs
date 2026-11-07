use super::*;
pub(super) fn save_session(path: &Path, messages: &[Value]) -> Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    atomic_replace(path, &serde_json::to_vec_pretty(messages)?)?;
    Ok(())
}


pub(super) fn open_store(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE IF NOT EXISTS messages (session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        ordinal INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(session_id, ordinal));
        CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, ordinal);")?;
    Ok(conn)
}

pub(super) fn load_sqlite(conn: &Connection, id: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT payload FROM messages WHERE session_id=?1 ORDER BY ordinal")?;
    let values = stmt.query_map(params![id], |row| row.get::<_, String>(0))?
        .map(|r| -> Result<Value> { Ok(serde_json::from_str(&r?)?) })
        .collect::<Result<Vec<_>>>()?;
    Ok(values)
}

pub(super) fn save_sqlite(conn: &mut Connection, id: &str, messages: &[Value]) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO sessions(id) VALUES(?1) ON CONFLICT(id) DO UPDATE SET updated_at=CURRENT_TIMESTAMP",
        params![id])?;
    tx.execute("DELETE FROM messages WHERE session_id=?1", params![id])?;
    {
        let mut stmt = tx.prepare("INSERT INTO messages(session_id, ordinal, payload) VALUES(?1, ?2, ?3)")?;
        for (i, msg) in messages.iter().enumerate() {
            stmt.execute(params![id, i as i64, serde_json::to_string(msg)?])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn persist(args: &Args, db: &mut Option<Connection>, messages: &[Value]) -> Result<()> {
    if let Some(conn) = db { save_sqlite(conn, &args.session_id, messages)?; }
    if let Some(path) = &args.session { save_session(path, messages)?; }
    Ok(())
}

