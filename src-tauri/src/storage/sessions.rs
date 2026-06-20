//! `sessions` table — multi-session chat management.

use super::{AppResult, ChatSession, Storage};
use chrono::Utc;
use rusqlite::params;

impl Storage {
    /// Create a session and return it. `title` is trimmed; an empty title
    /// becomes "新会话" so the list never shows a blank entry.
    pub fn create_session(&self, title: Option<&str>) -> AppResult<ChatSession> {
        let now = Utc::now().to_rfc3339();
        let title = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "新会话".to_string());
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO sessions (title, created_at, updated_at)
             VALUES (?1, ?2, ?2)",
            params![title, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(ChatSession {
            id,
            title,
            created_at: now.clone(),
            updated_at: now,
            last_message_at: None,
        })
    }

    /// Ensure at least one session exists and return its id. Used at startup so
    /// the app always has a "current session" to write messages into, even on
    /// a fresh DB (the v3 migration back-fills one for pre-v3 DBs, but a brand
    /// new DB has none until this runs).
    pub fn ensure_default_session(&self) -> AppResult<i64> {
        let conn = self.lock()?;
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM sessions ORDER BY last_message_at DESC NULLS LAST, id ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(id) = existing {
            return Ok(id);
        }
        drop(conn);
        let session = self.create_session(Some("默认会话"))?;
        Ok(session.id)
    }

    /// All sessions, most-recently-active first.
    pub fn list_sessions(&self) -> AppResult<Vec<ChatSession>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, last_message_at
             FROM sessions
             ORDER BY
               (last_message_at IS NULL),
               last_message_at DESC,
               updated_at DESC",
        )?;
        let rows = stmt.query_map([], read_session_row)?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    pub fn get_session(&self, id: i64) -> AppResult<Option<ChatSession>> {
        let conn = self.lock()?;
        let session = conn
            .query_row(
                "SELECT id, title, created_at, updated_at, last_message_at
                 FROM sessions WHERE id = ?1",
                params![id],
                read_session_row,
            )
            .ok();
        Ok(session)
    }

    pub fn rename_session(&self, id: i64, title: &str) -> AppResult<Option<ChatSession>> {
        let title = title.trim();
        if title.is_empty() {
            return self.get_session(id);
        }
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;
        let touched = conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id],
        )?;
        if touched == 0 {
            return Ok(None);
        }
        drop(conn);
        self.get_session(id)
    }

    /// Delete a session and all its messages. The FK on chat_messages has no
    /// ON DELETE CASCADE, so messages are removed explicitly first.
    pub fn delete_session(&self, id: i64) -> AppResult<bool> {
        self.clear_session_messages(id)?;
        let conn = self.lock()?;
        let removed = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }
}

fn read_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatSession> {
    Ok(ChatSession {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        last_message_at: row.get(4)?,
    })
}
