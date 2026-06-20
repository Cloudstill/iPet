//! `chat_messages` table — chat history persistence, scoped per session.

use super::{AppResult, ChatRecord, Storage};
use chrono::Utc;
use rusqlite::params;

impl Storage {
    pub fn save_chat_message(&self, session_id: i64, role: &str, content: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO chat_messages (session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role, content, now],
        )?;
        // Denormalize recency onto the session so the list view sorts without a
        // join. `updated_at` also bumps so a session with a new message drifts
        // toward the top of a recency-sorted list.
        conn.execute(
            "UPDATE sessions SET last_message_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;
        Ok(())
    }

    /// Most recent `limit` messages of `session_id`, oldest-first.
    pub fn recent_messages(&self, session_id: i64, limit: usize) -> AppResult<Vec<ChatRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, role, content, created_at
             FROM chat_messages
             WHERE session_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            Ok(ChatRecord {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        records.reverse();
        Ok(records)
    }

    /// Delete every message belonging to a session (used when deleting a
    /// session — the FK has no ON DELETE CASCADE, so we clean up explicitly).
    pub fn clear_session_messages(&self, session_id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM chat_messages WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }
}
